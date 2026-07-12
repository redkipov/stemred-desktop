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
    const TASKBAR_ICON_SIZE: usize = 24;
    const ICON_SUPERSAMPLE: usize = 4;

    static TASKBAR_APP: OnceLock<Mutex<Option<AppHandle>>> = OnceLock::new();
    static TASKBAR_RUNTIME: OnceLock<Mutex<TaskbarRuntime>> = OnceLock::new();
    static TASKBAR_STATE: OnceLock<Mutex<DesktopMusicPlayerState>> = OnceLock::new();
    static TASKBAR_BUTTON_MESSAGE: OnceLock<u32> = OnceLock::new();
    static TASKBAR_ICONS: OnceLock<[isize; 5]> = OnceLock::new();
    static TASKBAR_STARTUP_RETRIES: OnceLock<()> = OnceLock::new();
    static TASKBAR_DIAGNOSTIC_COUNT: AtomicU8 = AtomicU8::new(0);

    #[derive(Clone, Copy)]
    enum TaskbarIcon {
        FavoriteAdd,
        Previous,
        Play,
        Pause,
        Next,
    }

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
            volume: None,
            source: Some("taskbar".to_string()),
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
                create_icon(TaskbarIcon::FavoriteAdd).0 as isize,
                create_icon(TaskbarIcon::Previous).0 as isize,
                create_icon(TaskbarIcon::Play).0 as isize,
                create_icon(TaskbarIcon::Pause).0 as isize,
                create_icon(TaskbarIcon::Next).0 as isize,
            ]
        })
    }

    fn create_icon(icon: TaskbarIcon) -> HICON {
        let (and_bits, xor_bits) = icon_bits(icon);
        // SAFETY: AND-маска рассчитана под 1 bpp, XOR-слой под 32 bpp BGRA.
        unsafe {
            CreateIcon(
                None,
                TASKBAR_ICON_SIZE as i32,
                TASKBAR_ICON_SIZE as i32,
                1,
                32,
                and_bits.as_ptr(),
                xor_bits.as_ptr(),
            )
            .unwrap_or_default()
        }
    }

    fn icon_bits(icon: TaskbarIcon) -> (Vec<u8>, Vec<u8>) {
        let stride = TASKBAR_ICON_SIZE.div_ceil(16) * 2;
        let mut and_bits = vec![0xffu8; stride * TASKBAR_ICON_SIZE];
        let mut xor_bits = vec![0u8; TASKBAR_ICON_SIZE * TASKBAR_ICON_SIZE * 4];
        for y in 0..TASKBAR_ICON_SIZE {
            for x in 0..TASKBAR_ICON_SIZE {
                let alpha = icon_alpha(icon, x, y);
                if alpha > 0 {
                    let index = y * stride + x / 8;
                    let mask = 0x80 >> (x % 8);
                    and_bits[index] &= !mask;
                    let color_index = (y * TASKBAR_ICON_SIZE + x) * 4;
                    xor_bits[color_index] = alpha;
                    xor_bits[color_index + 1] = alpha;
                    xor_bits[color_index + 2] = alpha;
                    xor_bits[color_index + 3] = alpha;
                }
            }
        }
        (and_bits, xor_bits)
    }

    fn icon_alpha(icon: TaskbarIcon, x: usize, y: usize) -> u8 {
        let mut covered = 0;
        for sample_y in 0..ICON_SUPERSAMPLE {
            for sample_x in 0..ICON_SUPERSAMPLE {
                let px = x as f32 + (sample_x as f32 + 0.5) / ICON_SUPERSAMPLE as f32;
                let py = y as f32 + (sample_y as f32 + 0.5) / ICON_SUPERSAMPLE as f32;
                if icon_contains(icon, px, py) {
                    covered += 1;
                }
            }
        }
        ((covered * 255) / (ICON_SUPERSAMPLE * ICON_SUPERSAMPLE)) as u8
    }

    fn icon_contains(icon: TaskbarIcon, x: f32, y: f32) -> bool {
        match icon {
            TaskbarIcon::FavoriteAdd => favorite_add_icon_contains(x, y),
            TaskbarIcon::Previous => {
                rect_contains(x, y, 4.8, 5.0, 7.2, 19.0)
                    || triangle_contains(x, y, (18.8, 5.0), (18.8, 19.0), (7.2, 12.0))
            }
            TaskbarIcon::Play => triangle_contains(x, y, (7.0, 5.0), (7.0, 19.0), (19.0, 12.0)),
            TaskbarIcon::Pause => {
                rect_contains(x, y, 6.8, 5.0, 10.5, 19.0)
                    || rect_contains(x, y, 13.5, 5.0, 17.2, 19.0)
            }
            TaskbarIcon::Next => {
                triangle_contains(x, y, (5.2, 5.0), (5.2, 19.0), (16.8, 12.0))
                    || rect_contains(x, y, 16.8, 5.0, 19.2, 19.0)
            }
        }
    }

    fn favorite_add_icon_contains(x: f32, y: f32) -> bool {
        let dx = x - 12.0;
        let dy = y - 12.0;
        let distance = (dx * dx + dy * dy).sqrt();
        let ring = (7.2..=9.6).contains(&distance);
        let vertical = rect_contains(x, y, 11.0, 7.2, 13.0, 16.8);
        let horizontal = rect_contains(x, y, 7.2, 11.0, 16.8, 13.0);
        ring || vertical || horizontal
    }

    fn rect_contains(x: f32, y: f32, left: f32, top: f32, right: f32, bottom: f32) -> bool {
        x >= left && x <= right && y >= top && y <= bottom
    }

    fn triangle_contains(x: f32, y: f32, a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
        let d1 = triangle_sign(x, y, a, b);
        let d2 = triangle_sign(x, y, b, c);
        let d3 = triangle_sign(x, y, c, a);
        let has_negative = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
        let has_positive = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
        !(has_negative && has_positive)
    }

    fn triangle_sign(x: f32, y: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
        (x - b.0) * (a.1 - b.1) - (a.0 - b.0) * (y - b.1)
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
