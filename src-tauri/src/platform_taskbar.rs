#[cfg(windows)]
mod windows_taskbar {
    use std::fmt;
    use std::mem::size_of;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    use tauri::async_runtime;
    use tauri::{AppHandle, Emitter, WebviewWindow};
    use tokio::time::sleep;
    use windows::core::{IUnknown, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{
        DefSubclassProc, ITaskbarList3, RemoveWindowSubclass, SetWindowSubclass, TaskbarList,
        THBF_DISABLED, THBF_ENABLED, THBN_CLICKED, THB_FLAGS, THB_ICON, THB_TOOLTIP, THUMBBUTTON,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateIcon, GetAncestor, RegisterWindowMessageW, GA_ROOT, HICON, WM_COMMAND,
    };

    use crate::{DesktopMusicPlayerCommand, DesktopMusicPlayerState};

    const BUTTON_FAVORITE: u32 = 4101;
    const BUTTON_PREVIOUS: u32 = 4102;
    const BUTTON_PLAY: u32 = 4103;
    const BUTTON_NEXT: u32 = 4104;
    const SUBCLASS_ID: usize = 4100;
    const TASKBAR_ICON_SIZE: usize = 20;

    static TASKBAR_APP: OnceLock<Mutex<Option<AppHandle>>> = OnceLock::new();
    static TASKBAR_RUNTIME: OnceLock<Mutex<TaskbarRuntime>> = OnceLock::new();
    static TASKBAR_STATE: OnceLock<Mutex<DesktopMusicPlayerState>> = OnceLock::new();
    static TASKBAR_BUTTON_MESSAGE: OnceLock<u32> = OnceLock::new();
    static TASKBAR_ICONS: OnceLock<[isize; 5]> = OnceLock::new();
    static TASKBAR_STARTUP_RETRIES: OnceLock<()> = OnceLock::new();
    static TASKBAR_DIAGNOSTIC_COUNT: AtomicU8 = AtomicU8::new(0);

    #[derive(Default)]
    struct TaskbarRuntime {
        hwnd: isize,
        toolbar_added: bool,
        subclass_installed: bool,
    }

    pub fn install(app: &AppHandle, window: &WebviewWindow) {
        let Ok(hwnd) = window.hwnd() else {
            return;
        };
        let app_slot = TASKBAR_APP.get_or_init(|| Mutex::new(None));
        if let Ok(mut slot) = app_slot.lock() {
            *slot = Some(app.clone());
        }

        let hwnd = root_hwnd(HWND(hwnd.0 as _));
        let _ = taskbar_button_created_message();
        let runtime = TASKBAR_RUNTIME.get_or_init(|| Mutex::new(TaskbarRuntime::default()));
        if let Ok(mut runtime) = runtime.lock() {
            let next_hwnd = hwnd.0 as isize;
            if runtime.hwnd != next_hwnd {
                runtime.hwnd = next_hwnd;
                runtime.toolbar_added = false;
                runtime.subclass_installed = false;
            }
            if !runtime.subclass_installed {
                // SAFETY: hwnd принадлежит главному окну текущего процесса; callback статический.
                let installed =
                    unsafe { SetWindowSubclass(hwnd, Some(taskbar_subclass_proc), SUBCLASS_ID, 0) }
                        .as_bool();
                runtime.subclass_installed = installed;
            }
        }

        update_music_controls(app, &current_state());
        schedule_startup_retries(app.clone());
    }

    pub fn update_music_controls(_app: &AppHandle, state: &DesktopMusicPlayerState) {
        if let Ok(mut current) = TASKBAR_STATE
            .get_or_init(|| Mutex::new(DesktopMusicPlayerState::default()))
            .lock()
        {
            *current = state.clone();
        }

        let Some((hwnd, toolbar_added)) = current_runtime_target() else {
            return;
        };
        let buttons = taskbar_buttons(state);
        let toolbar_added = if toolbar_added {
            match thumbbar_update_buttons(hwnd, &buttons) {
                Ok(()) => true,
                Err(error) => {
                    log_taskbar_error("ThumbBarUpdateButtons", error);
                    install_toolbar(hwnd, &buttons)
                }
            }
        } else {
            install_toolbar(hwnd, &buttons)
        };

        if let Some(runtime) = TASKBAR_RUNTIME.get() {
            if let Ok(mut runtime) = runtime.lock() {
                if runtime.hwnd == hwnd.0 as isize {
                    runtime.toolbar_added = toolbar_added;
                }
            }
        }
    }

    unsafe extern "system" fn taskbar_subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _subclass_id: usize,
        _ref_data: usize,
    ) -> LRESULT {
        if msg == taskbar_button_created_message() {
            if let Some(runtime) = TASKBAR_RUNTIME.get() {
                if let Ok(mut runtime) = runtime.lock() {
                    runtime.toolbar_added = false;
                }
            }
            if let Some(app) = current_app_handle() {
                update_music_controls(&app, &current_state());
            }
        } else if msg == WM_COMMAND && high_word(wparam) == THBN_CLICKED {
            if let Some(command) = command_for_button(low_word(wparam)) {
                if let Some(app) = current_app_handle() {
                    let _ = app.emit("stem://music-player-command", command);
                }
            }
        }

        if msg == 0x0002 {
            let _ = RemoveWindowSubclass(hwnd, Some(taskbar_subclass_proc), SUBCLASS_ID);
        }

        DefSubclassProc(hwnd, msg, wparam, lparam)
    }

    fn current_app_handle() -> Option<AppHandle> {
        TASKBAR_APP
            .get()
            .and_then(|slot| slot.lock().ok().and_then(|slot| slot.clone()))
    }

    fn current_state() -> DesktopMusicPlayerState {
        TASKBAR_STATE
            .get()
            .and_then(|state| state.lock().ok().map(|state| state.clone()))
            .unwrap_or_default()
    }

    fn current_runtime_target() -> Option<(HWND, bool)> {
        let runtime = TASKBAR_RUNTIME.get()?;
        let runtime = runtime.lock().ok()?;
        if runtime.hwnd == 0 {
            return None;
        }
        Some((HWND(runtime.hwnd as _), runtime.toolbar_added))
    }

    fn root_hwnd(hwnd: HWND) -> HWND {
        // SAFETY: GetAncestor только нормализует HWND до root-окна текущего процесса.
        let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
        if root.0.is_null() {
            hwnd
        } else {
            root
        }
    }

    fn command_for_button(id: u32) -> Option<DesktopMusicPlayerCommand> {
        let command = match id {
            BUTTON_FAVORITE => "favorite",
            BUTTON_PREVIOUS => "previous",
            BUTTON_PLAY => "playPause",
            BUTTON_NEXT => "next",
            _ => return None,
        };
        Some(DesktopMusicPlayerCommand {
            command: command.to_string(),
            position_sec: None,
        })
    }

    fn taskbar_button_created_message() -> u32 {
        *TASKBAR_BUTTON_MESSAGE.get_or_init(|| {
            let wide = wide_null("TaskbarButtonCreated");
            // SAFETY: строка завершается нулём и живёт до завершения вызова.
            unsafe { RegisterWindowMessageW(PCWSTR(wide.as_ptr())) }
        })
    }

    fn thumbbar_add_buttons(hwnd: HWND, buttons: &[THUMBBUTTON; 4]) -> windows::core::Result<()> {
        with_taskbar_list(|taskbar| unsafe { taskbar.ThumbBarAddButtons(hwnd, buttons) })
    }

    fn thumbbar_update_buttons(
        hwnd: HWND,
        buttons: &[THUMBBUTTON; 4],
    ) -> windows::core::Result<()> {
        with_taskbar_list(|taskbar| unsafe { taskbar.ThumbBarUpdateButtons(hwnd, buttons) })
    }

    fn with_taskbar_list(
        f: impl FnOnce(&ITaskbarList3) -> windows::core::Result<()>,
    ) -> windows::core::Result<()> {
        // SAFETY: COM инициализируется только на время операции; typed binding освобождает интерфейс через Drop.
        unsafe {
            let init_hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let should_uninit = init_hr.is_ok();
            let result = CoCreateInstance::<_, ITaskbarList3>(
                &TaskbarList,
                None::<&IUnknown>,
                CLSCTX_INPROC_SERVER,
            )
            .and_then(|taskbar| {
                taskbar.HrInit()?;
                f(&taskbar)
            });
            if should_uninit {
                CoUninitialize();
            }
            result
        }
    }

    fn install_toolbar(hwnd: HWND, buttons: &[THUMBBUTTON; 4]) -> bool {
        match thumbbar_add_buttons(hwnd, buttons) {
            Ok(()) => {
                if let Err(error) = thumbbar_update_buttons(hwnd, buttons) {
                    log_taskbar_error("ThumbBarUpdateButtons after add", error);
                }
                true
            }
            Err(add_error) => match thumbbar_update_buttons(hwnd, buttons) {
                Ok(()) => true,
                Err(update_error) => {
                    log_taskbar_error("ThumbBarAddButtons", add_error);
                    log_taskbar_error("ThumbBarUpdateButtons fallback", update_error);
                    false
                }
            },
        }
    }

    fn taskbar_buttons(state: &DesktopMusicPlayerState) -> [THUMBBUTTON; 4] {
        let icons = taskbar_icons();
        [
            thumb_button(
                BUTTON_FAVORITE,
                HICON(icons[0] as _),
                if state.favorite {
                    "Убрать из избранного"
                } else {
                    "Добавить в избранное"
                },
                true,
            ),
            thumb_button(
                BUTTON_PREVIOUS,
                HICON(icons[1] as _),
                "Предыдущий трек",
                true,
            ),
            thumb_button(
                BUTTON_PLAY,
                if state.playing {
                    HICON(icons[3] as _)
                } else {
                    HICON(icons[2] as _)
                },
                if state.playing {
                    "Пауза"
                } else {
                    "Проиграть"
                },
                true,
            ),
            thumb_button(BUTTON_NEXT, HICON(icons[4] as _), "Следующий трек", true),
        ]
    }

    fn thumb_button(id: u32, icon: HICON, tip: &str, enabled: bool) -> THUMBBUTTON {
        let mut button = THUMBBUTTON {
            dwMask: THB_ICON | THB_TOOLTIP | THB_FLAGS,
            iId: id,
            iBitmap: 0,
            hIcon: icon,
            szTip: [0; 260],
            dwFlags: if enabled { THBF_ENABLED } else { THBF_DISABLED },
        };
        let tip = wide_null(tip);
        let len = tip.len().min(button.szTip.len());
        button.szTip[..len].copy_from_slice(&tip[..len]);
        button
    }

    fn taskbar_icons() -> &'static [isize; 5] {
        TASKBAR_ICONS.get_or_init(|| {
            [
                create_icon(&[
                    "00000011111100000000",
                    "00001111111111000000",
                    "00011100000011100000",
                    "00111000000001110000",
                    "01110000000000111000",
                    "01100000000000011000",
                    "11100000110000011100",
                    "11100000110000011100",
                    "11000011111100001100",
                    "11000011111100001100",
                    "11000011111100001100",
                    "11000011111100001100",
                    "11100000110000011100",
                    "11100000110000011100",
                    "01100000000000011000",
                    "01110000000000111000",
                    "00111000000001110000",
                    "00011100000011100000",
                    "00001111111111000000",
                    "00000011111100000000",
                ])
                .0 as isize,
                create_icon(&[
                    "00000000000000000000",
                    "00000000000000000000",
                    "00000001000010000000",
                    "00011001100011000000",
                    "00011001110011100000",
                    "00011001111011110000",
                    "00011001111111111000",
                    "00011001111111111100",
                    "00011001111111111110",
                    "00011001111111111110",
                    "00011001111111111110",
                    "00011001111111111110",
                    "00011001111111111100",
                    "00011001111111111000",
                    "00011001111011110000",
                    "00011001110011100000",
                    "00011001100011000000",
                    "00000001000010000000",
                    "00000000000000000000",
                    "00000000000000000000",
                ])
                .0 as isize,
                create_icon(&[
                    "00000000000000000000",
                    "00000000000000000000",
                    "00000011000000000000",
                    "00000011100000000000",
                    "00000011110000000000",
                    "00000011111000000000",
                    "00000011111100000000",
                    "00000011111110000000",
                    "00000011111111000000",
                    "00000011111111100000",
                    "00000011111111110000",
                    "00000011111111110000",
                    "00000011111111100000",
                    "00000011111111000000",
                    "00000011111110000000",
                    "00000011111100000000",
                    "00000011111000000000",
                    "00000011110000000000",
                    "00000000000000000000",
                    "00000000000000000000",
                ])
                .0 as isize,
                create_icon(&[
                    "00000000000000000000",
                    "00000000000000000000",
                    "00000000000000000000",
                    "00000011100011100000",
                    "00000011100011100000",
                    "00000011100011100000",
                    "00000011100011100000",
                    "00000011100011100000",
                    "00000011100011100000",
                    "00000011100011100000",
                    "00000011100011100000",
                    "00000011100011100000",
                    "00000011100011100000",
                    "00000011100011100000",
                    "00000011100011100000",
                    "00000011100011100000",
                    "00000011100011100000",
                    "00000000000000000000",
                    "00000000000000000000",
                    "00000000000000000000",
                ])
                .0 as isize,
                create_icon(&[
                    "00000000000000000000",
                    "00000000000000000000",
                    "00000001000010000000",
                    "00000011000110001100",
                    "00000111001110001100",
                    "00001111011110001100",
                    "00011111111110001100",
                    "00111111111110001100",
                    "01111111111110001100",
                    "01111111111110001100",
                    "01111111111110001100",
                    "01111111111110001100",
                    "00111111111110001100",
                    "00011111111110001100",
                    "00001111011110001100",
                    "00000111001110001100",
                    "00000011000110001100",
                    "00000001000010000000",
                    "00000000000000000000",
                    "00000000000000000000",
                ])
                .0 as isize,
            ]
        })
    }

    fn create_icon(rows: &[&str; TASKBAR_ICON_SIZE]) -> HICON {
        let (and_bits, xor_bits) = icon_bits(rows);
        // SAFETY: обе битовые маски рассчитаны под 1 bpp с word-aligned stride.
        unsafe {
            CreateIcon(
                None,
                TASKBAR_ICON_SIZE as i32,
                TASKBAR_ICON_SIZE as i32,
                1,
                1,
                and_bits.as_ptr(),
                xor_bits.as_ptr(),
            )
            .unwrap_or_default()
        }
    }

    fn icon_bits(rows: &[&str; TASKBAR_ICON_SIZE]) -> (Vec<u8>, Vec<u8>) {
        let stride = ((TASKBAR_ICON_SIZE + 15) / 16) * 2;
        let mut and_bits = vec![0xffu8; stride * TASKBAR_ICON_SIZE];
        let mut xor_bits = vec![0u8; stride * TASKBAR_ICON_SIZE];
        for (y, row) in rows.iter().enumerate() {
            let bytes = row.as_bytes();
            for x in 0..TASKBAR_ICON_SIZE {
                if bytes.get(x) == Some(&b'1') {
                    let index = y * stride + x / 8;
                    let mask = 0x80 >> (x % 8);
                    and_bits[index] &= !mask;
                    xor_bits[index] |= mask;
                }
            }
        }
        (and_bits, xor_bits)
    }

    fn low_word(value: WPARAM) -> u32 {
        (value.0 & 0xffff) as u32
    }

    fn high_word(value: WPARAM) -> u32 {
        ((value.0 >> 16) & 0xffff) as u32
    }

    fn schedule_startup_retries(app: AppHandle) {
        if TASKBAR_STARTUP_RETRIES.set(()).is_err() {
            return;
        }

        async_runtime::spawn(async move {
            for delay_ms in [250, 1000, 2500] {
                sleep(Duration::from_millis(delay_ms)).await;
                let app_for_call = app.clone();
                let app_for_update = app.clone();
                if let Err(error) = app_for_call.run_on_main_thread(move || {
                    update_music_controls(&app_for_update, &current_state());
                }) {
                    log_taskbar_error("run_on_main_thread", error);
                }
            }
        });
    }

    fn log_taskbar_error(operation: &str, error: impl fmt::Display) {
        if TASKBAR_DIAGNOSTIC_COUNT.fetch_add(1, Ordering::Relaxed) < 8 {
            eprintln!("StemRed taskbar controls: {operation} failed: {error}");
        }
    }

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    #[allow(dead_code)]
    fn _assert_thumbbutton_size() {
        let _ = size_of::<THUMBBUTTON>();
    }
}

#[cfg(windows)]
pub use windows_taskbar::{install, update_music_controls};

#[cfg(not(windows))]
pub fn install(_app: &tauri::AppHandle, _window: &tauri::WebviewWindow) {}

#[cfg(not(windows))]
pub fn update_music_controls(_app: &tauri::AppHandle, _state: &crate::DesktopMusicPlayerState) {}
