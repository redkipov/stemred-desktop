#[cfg(windows)]
mod windows_taskbar {
    use std::mem::size_of;
    use std::ptr::{null, null_mut};
    use std::sync::{Mutex, OnceLock};

    use tauri::{AppHandle, Emitter, WebviewWindow};
    use windows_sys::core::{BOOL, GUID, HRESULT, PCWSTR};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows_sys::Win32::UI::Shell::{
        DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass, TaskbarList, THBF_DISABLED,
        THBF_ENABLED, THBN_CLICKED, THB_FLAGS, THB_ICON, THB_TOOLTIP, THUMBBUTTON,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateIcon, GetAncestor, RegisterWindowMessageW, GA_ROOT, HICON, WM_COMMAND,
    };

    use crate::{DesktopMusicPlayerCommand, DesktopMusicPlayerState};

    const BUTTON_FAVORITE: u32 = 4101;
    const BUTTON_PREVIOUS: u32 = 4102;
    const BUTTON_PLAY: u32 = 4103;
    const BUTTON_NEXT: u32 = 4104;
    const SUBCLASS_ID: usize = 4100;
    const IID_ITASKBAR_LIST3: GUID = GUID::from_u128(0xea1afb91_9e28_4b86_90e9_9e9f8a5eea84);

    static TASKBAR_APP: OnceLock<Mutex<Option<AppHandle>>> = OnceLock::new();
    static TASKBAR_RUNTIME: OnceLock<Mutex<TaskbarRuntime>> = OnceLock::new();
    static TASKBAR_STATE: OnceLock<Mutex<DesktopMusicPlayerState>> = OnceLock::new();
    static TASKBAR_BUTTON_MESSAGE: OnceLock<u32> = OnceLock::new();
    static TASKBAR_ICONS: OnceLock<[isize; 5]> = OnceLock::new();

    #[derive(Default)]
    struct TaskbarRuntime {
        hwnd: isize,
        toolbar_added: bool,
        subclass_installed: bool,
    }

    #[repr(C)]
    struct ITaskbarList3 {
        vtbl: *const ITaskbarList3Vtbl,
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    struct ITaskbarList3Vtbl {
        QueryInterface: unsafe extern "system" fn(
            this: *mut ITaskbarList3,
            riid: *const GUID,
            ppv: *mut *mut core::ffi::c_void,
        ) -> HRESULT,
        AddRef: unsafe extern "system" fn(this: *mut ITaskbarList3) -> u32,
        Release: unsafe extern "system" fn(this: *mut ITaskbarList3) -> u32,
        HrInit: unsafe extern "system" fn(this: *mut ITaskbarList3) -> HRESULT,
        AddTab: unsafe extern "system" fn(this: *mut ITaskbarList3, hwnd: HWND) -> HRESULT,
        DeleteTab: unsafe extern "system" fn(this: *mut ITaskbarList3, hwnd: HWND) -> HRESULT,
        ActivateTab: unsafe extern "system" fn(this: *mut ITaskbarList3, hwnd: HWND) -> HRESULT,
        SetActiveAlt: unsafe extern "system" fn(this: *mut ITaskbarList3, hwnd: HWND) -> HRESULT,
        MarkFullscreenWindow: unsafe extern "system" fn(
            this: *mut ITaskbarList3,
            hwnd: HWND,
            fullscreen: BOOL,
        ) -> HRESULT,
        SetProgressValue: unsafe extern "system" fn(
            this: *mut ITaskbarList3,
            hwnd: HWND,
            completed: u64,
            total: u64,
        ) -> HRESULT,
        SetProgressState:
            unsafe extern "system" fn(this: *mut ITaskbarList3, hwnd: HWND, flags: i32) -> HRESULT,
        RegisterTab:
            unsafe extern "system" fn(this: *mut ITaskbarList3, tab: HWND, mdi: HWND) -> HRESULT,
        UnregisterTab: unsafe extern "system" fn(this: *mut ITaskbarList3, tab: HWND) -> HRESULT,
        SetTabOrder: unsafe extern "system" fn(
            this: *mut ITaskbarList3,
            tab: HWND,
            insert_before: HWND,
        ) -> HRESULT,
        SetTabActive: unsafe extern "system" fn(
            this: *mut ITaskbarList3,
            tab: HWND,
            mdi: HWND,
            reserved: u32,
        ) -> HRESULT,
        ThumbBarAddButtons: unsafe extern "system" fn(
            this: *mut ITaskbarList3,
            hwnd: HWND,
            count: u32,
            buttons: *mut THUMBBUTTON,
        ) -> HRESULT,
        ThumbBarUpdateButtons: unsafe extern "system" fn(
            this: *mut ITaskbarList3,
            hwnd: HWND,
            count: u32,
            buttons: *mut THUMBBUTTON,
        ) -> HRESULT,
        ThumbBarSetImageList: unsafe extern "system" fn(
            this: *mut ITaskbarList3,
            hwnd: HWND,
            image_list: isize,
        ) -> HRESULT,
        SetOverlayIcon: unsafe extern "system" fn(
            this: *mut ITaskbarList3,
            hwnd: HWND,
            icon: HICON,
            description: PCWSTR,
        ) -> HRESULT,
        SetThumbnailTooltip:
            unsafe extern "system" fn(this: *mut ITaskbarList3, hwnd: HWND, tip: PCWSTR) -> HRESULT,
        SetThumbnailClip: unsafe extern "system" fn(
            this: *mut ITaskbarList3,
            hwnd: HWND,
            clip: *mut RECT,
        ) -> HRESULT,
    }

    pub fn install(app: &AppHandle, window: &WebviewWindow) {
        let Ok(hwnd) = window.hwnd() else {
            return;
        };
        let app_slot = TASKBAR_APP.get_or_init(|| Mutex::new(None));
        if let Ok(mut slot) = app_slot.lock() {
            *slot = Some(app.clone());
        }

        let hwnd = root_hwnd(hwnd.0 as HWND);
        let _ = taskbar_button_created_message();
        let runtime = TASKBAR_RUNTIME.get_or_init(|| Mutex::new(TaskbarRuntime::default()));
        if let Ok(mut runtime) = runtime.lock() {
            let next_hwnd = hwnd as isize;
            if runtime.hwnd != next_hwnd {
                runtime.hwnd = next_hwnd;
                runtime.toolbar_added = false;
                runtime.subclass_installed = false;
            }
            if !runtime.subclass_installed {
                // SAFETY: hwnd принадлежит главному окну текущего процесса; callback статический.
                let installed = unsafe {
                    SetWindowSubclass(hwnd, Some(taskbar_subclass_proc), SUBCLASS_ID, 0) != 0
                };
                runtime.subclass_installed = installed;
            }
        }

        update_music_controls(app, &current_state());
    }

    pub fn update_music_controls(_app: &AppHandle, state: &DesktopMusicPlayerState) {
        if let Ok(mut current) = TASKBAR_STATE
            .get_or_init(|| Mutex::new(DesktopMusicPlayerState::default()))
            .lock()
        {
            *current = state.clone();
        }

        let Some(runtime) = TASKBAR_RUNTIME.get() else {
            return;
        };
        let Ok(mut runtime) = runtime.lock() else {
            return;
        };
        if runtime.hwnd == 0 {
            return;
        }

        let hwnd = runtime.hwnd as HWND;
        let mut buttons = taskbar_buttons(state);
        if runtime.toolbar_added && thumbbar_update_buttons(hwnd, &mut buttons) {
            return;
        }

        runtime.toolbar_added = false;
        if thumbbar_add_buttons(hwnd, &mut buttons) {
            runtime.toolbar_added = true;
            let _ = thumbbar_update_buttons(hwnd, &mut buttons);
            return;
        }

        if thumbbar_update_buttons(hwnd, &mut buttons) {
            runtime.toolbar_added = true;
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

    fn root_hwnd(hwnd: HWND) -> HWND {
        // SAFETY: GetAncestor только нормализует HWND до root-окна текущего процесса.
        let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
        if root.is_null() {
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
            unsafe { RegisterWindowMessageW(wide.as_ptr()) }
        })
    }

    fn thumbbar_add_buttons(hwnd: HWND, buttons: &mut [THUMBBUTTON; 4]) -> bool {
        with_taskbar_list(|taskbar| unsafe {
            ((*(*taskbar).vtbl).HrInit)(taskbar);
            ((*(*taskbar).vtbl).ThumbBarAddButtons)(
                taskbar,
                hwnd,
                buttons.len() as u32,
                buttons.as_mut_ptr(),
            )
        })
    }

    fn thumbbar_update_buttons(hwnd: HWND, buttons: &mut [THUMBBUTTON; 4]) -> bool {
        with_taskbar_list(|taskbar| unsafe {
            ((*(*taskbar).vtbl).HrInit)(taskbar);
            ((*(*taskbar).vtbl).ThumbBarUpdateButtons)(
                taskbar,
                hwnd,
                buttons.len() as u32,
                buttons.as_mut_ptr(),
            )
        })
    }

    fn with_taskbar_list(f: impl FnOnce(*mut ITaskbarList3) -> HRESULT) -> bool {
        // SAFETY: COM инициализируется только на время операции; интерфейс освобождается через Release.
        unsafe {
            let init_hr = CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32);
            let should_uninit = init_hr >= 0;
            let mut raw = null_mut();
            let hr = CoCreateInstance(
                &TaskbarList,
                null_mut(),
                CLSCTX_INPROC_SERVER,
                &IID_ITASKBAR_LIST3,
                &mut raw,
            );
            if hr < 0 || raw.is_null() {
                if should_uninit {
                    CoUninitialize();
                }
                return false;
            }

            let taskbar = raw as *mut ITaskbarList3;
            let result = f(taskbar);
            ((*(*taskbar).vtbl).Release)(taskbar);
            if should_uninit {
                CoUninitialize();
            }
            result >= 0
        }
    }

    fn taskbar_buttons(state: &DesktopMusicPlayerState) -> [THUMBBUTTON; 4] {
        let icons = taskbar_icons();
        [
            thumb_button(
                BUTTON_FAVORITE,
                icons[0] as HICON,
                if state.favorite {
                    "Убрать из избранного"
                } else {
                    "Добавить в избранное"
                },
                true,
            ),
            thumb_button(BUTTON_PREVIOUS, icons[1] as HICON, "Предыдущий трек", true),
            thumb_button(
                BUTTON_PLAY,
                if state.playing {
                    icons[3] as HICON
                } else {
                    icons[2] as HICON
                },
                if state.playing {
                    "Пауза"
                } else {
                    "Проиграть"
                },
                true,
            ),
            thumb_button(BUTTON_NEXT, icons[4] as HICON, "Следующий трек", true),
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
                    "0000000000000000",
                    "0000010000100000",
                    "0000111001110000",
                    "0001111111111000",
                    "0011111111111100",
                    "0001111111111000",
                    "0000111111110000",
                    "0000011111100000",
                    "0000001111000000",
                    "0000011111100000",
                    "0000111001110000",
                    "0001110000111000",
                    "0011000000011100",
                    "0000000000000000",
                    "0000000000000000",
                    "0000000000000000",
                ]) as isize,
                create_icon(&[
                    "0000000000000000",
                    "0000001000100000",
                    "0000011001100000",
                    "0000111011100000",
                    "0001111111100000",
                    "0011111111100000",
                    "0111111111100000",
                    "1111111111100000",
                    "0111111111100000",
                    "0011111111100000",
                    "0001111111100000",
                    "0000111011100000",
                    "0000011001100000",
                    "0000001000100000",
                    "0000000000000000",
                    "0000000000000000",
                ]) as isize,
                create_icon(&[
                    "0000000000000000",
                    "0001100000000000",
                    "0001111000000000",
                    "0001111110000000",
                    "0001111111100000",
                    "0001111111111000",
                    "0001111111111100",
                    "0001111111111000",
                    "0001111111100000",
                    "0001111110000000",
                    "0001111000000000",
                    "0001100000000000",
                    "0000000000000000",
                    "0000000000000000",
                    "0000000000000000",
                    "0000000000000000",
                ]) as isize,
                create_icon(&[
                    "0000000000000000",
                    "0011100011100000",
                    "0011100011100000",
                    "0011100011100000",
                    "0011100011100000",
                    "0011100011100000",
                    "0011100011100000",
                    "0011100011100000",
                    "0011100011100000",
                    "0011100011100000",
                    "0011100011100000",
                    "0011100011100000",
                    "0000000000000000",
                    "0000000000000000",
                    "0000000000000000",
                    "0000000000000000",
                ]) as isize,
                create_icon(&[
                    "0000000000000000",
                    "0000010001000000",
                    "0000011001100000",
                    "0000011101110000",
                    "0000011111111000",
                    "0000011111111100",
                    "0000011111111110",
                    "0000011111111111",
                    "0000011111111110",
                    "0000011111111100",
                    "0000011111111000",
                    "0000011101110000",
                    "0000011001100000",
                    "0000010001000000",
                    "0000000000000000",
                    "0000000000000000",
                ]) as isize,
            ]
        })
    }

    fn create_icon(rows: &[&str; 16]) -> HICON {
        let and_bits = [0u8; 32];
        let xor_bits = icon_bits(rows);
        // SAFETY: обе битовые маски имеют размер 16x16 при 1 bpp.
        unsafe {
            CreateIcon(
                null_mut(),
                16,
                16,
                1,
                1,
                and_bits.as_ptr(),
                xor_bits.as_ptr(),
            )
        }
    }

    fn icon_bits(rows: &[&str; 16]) -> [u8; 32] {
        let mut bits = [0u8; 32];
        for (y, row) in rows.iter().enumerate() {
            let bytes = row.as_bytes();
            for x in 0..16 {
                if bytes.get(x) == Some(&b'1') {
                    bits[y * 2 + x / 8] |= 0x80 >> (x % 8);
                }
            }
        }
        bits
    }

    fn low_word(value: WPARAM) -> u32 {
        (value & 0xffff) as u32
    }

    fn high_word(value: WPARAM) -> u32 {
        ((value >> 16) & 0xffff) as u32
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
