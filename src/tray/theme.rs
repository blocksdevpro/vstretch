//! Opts native tray popups into Windows' current application theme.

use std::sync::OnceLock;

use anyhow::{Result, ensure};
use tray_icon::TrayIcon;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::{
            GetModuleHandleW, GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
        },
        UI::{
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{WM_NCDESTROY, WM_SETTINGCHANGE, WM_THEMECHANGED},
        },
    },
    core::{PCSTR, s, w},
};

type AllowDarkModeForWindow = unsafe extern "system" fn(HWND, bool) -> bool;
type ThemeRefresh = unsafe extern "system" fn();

struct ThemeApi {
    allow_window: AllowDarkModeForWindow,
    refresh_policy: ThemeRefresh,
    flush_menus: ThemeRefresh,
}

static THEME_API: OnceLock<Option<ThemeApi>> = OnceLock::new();
const SUBCLASS_ID: usize = 1;
const ALLOW_WINDOW_ORDINAL: usize = 133;
const REFRESH_POLICY_ORDINAL: usize = 104;
const FLUSH_MENUS_ORDINAL: usize = 136;
const APP_MODE_ORDINAL: usize = 135;

impl ThemeApi {
    fn load() -> Option<Self> {
        let build = windows_build()?;
        if build < 17763 {
            return None;
        }
        // Native popup menus need UxTheme's dark-mode opt-in. Muda's MenuTheme
        // only changes window menu bars. Resolve these private exports at runtime
        // so Windows versions without them can still use their default menus.
        // SAFETY: these exports use the declared system ABIs on the checked
        // Windows builds. The version check below handles ordinal 135's ABI change.
        unsafe {
            // Keep this DLL loaded for the process lifetime, like the function cache.
            let library =
                LoadLibraryExW(w!("uxtheme.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32).ok()?;
            let allow_window = std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                AllowDarkModeForWindow,
            >(GetProcAddress(
                library,
                PCSTR(ALLOW_WINDOW_ORDINAL as *const u8),
            )?);
            let refresh_policy =
                std::mem::transmute::<unsafe extern "system" fn() -> isize, ThemeRefresh>(
                    GetProcAddress(library, PCSTR(REFRESH_POLICY_ORDINAL as *const u8))?,
                );
            let flush_menus =
                std::mem::transmute::<unsafe extern "system" fn() -> isize, ThemeRefresh>(
                    GetProcAddress(library, PCSTR(FLUSH_MENUS_ORDINAL as *const u8))?,
                );
            let app_mode = GetProcAddress(library, PCSTR(APP_MODE_ORDINAL as *const u8))?;
            // Ordinal 135 changed signature in Windows 10 1903. AllowDark follows
            // Windows' light/dark preference and leaves high contrast to Windows.
            if build < 18362 {
                let allow_app = std::mem::transmute::<
                    unsafe extern "system" fn() -> isize,
                    unsafe extern "system" fn(bool) -> bool,
                >(app_mode);
                allow_app(true);
            } else {
                let set_app_mode = std::mem::transmute::<
                    unsafe extern "system" fn() -> isize,
                    unsafe extern "system" fn(i32) -> i32,
                >(app_mode);
                const ALLOW_DARK: i32 = 1;
                set_app_mode(ALLOW_DARK);
            }
            Some(Self {
                allow_window,
                refresh_policy,
                flush_menus,
            })
        }
    }

    fn refresh(&self) {
        // SAFETY: load resolved these functions from a DLL kept alive for the process.
        unsafe {
            (self.refresh_policy)();
            (self.flush_menus)();
        }
    }
}

fn windows_build() -> Option<u32> {
    // SAFETY: this named ntdll export has a fixed ABI. All three output pointers
    // stay writable throughout the call. It reports the version without a manifest.
    unsafe {
        let library = GetModuleHandleW(w!("ntdll.dll")).ok()?;
        let version = std::mem::transmute::<
            unsafe extern "system" fn() -> isize,
            unsafe extern "system" fn(*mut u32, *mut u32, *mut u32),
        >(GetProcAddress(library, s!("RtlGetNtVersionNumbers"))?);
        let (mut major, mut minor, mut build) = (0, 0, 0);
        version(&mut major, &mut minor, &mut build);
        (major == 10 && minor == 0).then_some(build & 0x0fff_ffff)
    }
}

pub(super) fn follow_system_theme(icon: &TrayIcon) -> Result<()> {
    let Some(api) = THEME_API.get_or_init(ThemeApi::load) else {
        return Ok(());
    };
    let hwnd = HWND(icon.window_handle());
    // SAFETY: the borrowed tray owns this window on the current thread. The
    // callback holds no pointer into App and uses only process-lifetime functions.
    unsafe {
        (api.allow_window)(hwnd, true);
        ensure!(
            SetWindowSubclass(hwnd, Some(theme_proc), SUBCLASS_ID, 0).as_bool(),
            "could not observe Windows theme changes"
        );
    }
    api.refresh();
    Ok(())
}

unsafe extern "system" fn theme_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    subclass_id: usize,
    _data: usize,
) -> LRESULT {
    // Settings broadcasts can arrive through SendMessage during GetMessageW,
    // so checking only the outer message loop would miss live theme changes.
    if matches!(message, WM_SETTINGCHANGE | WM_THEMECHANGED)
        && let Some(api) = THEME_API.get().and_then(Option::as_ref)
    {
        api.refresh();
    }
    // SAFETY: Windows invokes this callback for the window it was attached to.
    // Forward each message so tray-icon and muda still receive their events.
    unsafe {
        if message == WM_NCDESTROY {
            let _ = RemoveWindowSubclass(hwnd, Some(theme_proc), subclass_id);
        }
        DefSubclassProc(hwnd, message, wparam, lparam)
    }
}
