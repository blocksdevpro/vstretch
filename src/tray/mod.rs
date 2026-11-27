//! Runs menu commands and automatic display polling on one Windows UI thread.
//!
//! - [`app`]: settings, the latest display reading, and automatic switching.
//! - [`menu`]: the native menu and the commands it produces.
//! - [`hotkey`]: global hotkey registration.
//! - [`theme`]: following the Windows light/dark setting.

mod app;
mod hotkey;
mod menu;
mod theme;

use anyhow::{Context, Result, bail};
use tray_icon::{Icon, TrayIconEvent, menu::MenuEvent};
use windows::{
    Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, KillTimer, MSG, SetTimer, TranslateMessage, WM_HOTKEY,
        WM_TIMER,
    },
    core::Error as WinError,
};

use crate::{config::Config, platform};
use app::App;
use hotkey::HOTKEY_ID;
use menu::Command;

const APP_LABEL: &str = concat!("Vstretch v", env!("CARGO_PKG_VERSION"));
/// How often the tray re-reads settings, the display, and game activity.
const POLL_INTERVAL_MS: u32 = 750;

struct Timer(usize);

impl Timer {
    fn new() -> Result<Self> {
        // A thread timer keeps all display changes on the menu's UI thread.
        let id = unsafe { SetTimer(None, 0, POLL_INTERVAL_MS, None) };
        if id == 0 {
            return Err(WinError::from_thread().into());
        }
        Ok(Self(id))
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        let _ = unsafe { KillTimer(None, self.0) };
    }
}

pub fn run() -> Result<Option<std::path::PathBuf>> {
    let Some(_instance) = platform::tray_instance(&Config::config_path()?)? else {
        return Ok(None);
    };
    let executable = crate::update::ExecutableWatch::new(std::env::current_exe()?)?;
    let mut app = App::new()?;
    let timer = Timer::new()?;
    let mut message = MSG::default();
    loop {
        // tray-icon/muda own the hidden window. Its Windows messages and the
        // foreground poll run on this one thread; no worker shares display state.
        // SAFETY: message is writable for the entire call. The window filter is null.
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 == -1 {
            app.restore_on_exit()?;
            bail!(
                "system tray message loop failed: {}",
                WinError::from_thread()
            );
        }
        if result.0 == 0 {
            break;
        }
        // SAFETY: GetMessageW returned a message for this thread; both functions
        // borrow it only for the duration of these calls.
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if let Some(command) = app.menu.command(event.id())
                && app.run_command(command)
            {
                return Ok(None);
            }
        }
        // Mouse move/click events aren't needed, but drain the default channel.
        while TrayIconEvent::receiver().try_recv().is_ok() {}
        if message.message == WM_HOTKEY
            && message.wParam.0 == HOTKEY_ID as usize
            && app.run_command(Command::Toggle)
        {
            return Ok(None);
        }
        if message.message == WM_TIMER && message.hwnd.is_invalid() && message.wParam.0 == timer.0 {
            if executable.changed() {
                // Restore first; the caller releases tray/hotkey ownership and
                // finishes the recovery journal before starting the new image.
                app.restore_on_exit()?;
                return Ok(Some(executable.path));
            }
            app.tick();
        }
    }
    app.restore_on_exit()?;
    Ok(None)
}

/// Canonical terracotta V from `assets/icon.svg`, baked to 32x32 RGBA.
///
/// The bytes are generated from the same rounded-square + white V drawing as
/// `assets/icon.ico`, then embedded with `include_bytes!` so the downloaded
/// executable stays portable with no sidecar image file.
fn tray_icon() -> Result<Icon> {
    const SIZE: u32 = 32;
    const PIXELS: &[u8] = include_bytes!("../../assets/tray-icon-32.rgba");
    anyhow::ensure!(
        PIXELS.len() == (SIZE * SIZE * 4) as usize,
        "tray icon pixels do not match {SIZE}x{SIZE} RGBA"
    );
    Icon::from_rgba(PIXELS.to_vec(), SIZE, SIZE).context("could not create tray icon image")
}

#[cfg(test)]
mod tests {
    //! End-to-end checks that drive the whole tray app on the real desktop.
    use super::*;
    use crate::{
        autostretch::GameActivity,
        config::Profile,
        display::{self, Mode},
    };
    use menu::menu_caption;

    #[test]
    #[ignore = "temporarily switches the real primary display; requires isolated VSTRETCH_CONFIG and restores the original mode"]
    fn automatic_session_applies_and_restores_display() -> Result<()> {
        anyhow::ensure!(
            std::env::var_os("VSTRETCH_CONFIG").is_some(),
            "set an isolated VSTRETCH_CONFIG before running"
        );
        struct RestoreDisplay(Mode);
        impl Drop for RestoreDisplay {
            fn drop(&mut self) {
                let _ = display::change_mode(self.0);
            }
        }
        let original = display::current_mode()?;
        let _restore = RestoreDisplay(original);
        let config = Config {
            stretch: Profile::new(1280, 960),
            start_with_windows: Some(false),
            native: Some(Profile {
                width: original.width,
                height: original.height,
                refresh: Some(original.refresh),
            }),
            ..Config::default()
        };
        config.save_default_path()?;
        let mut app = App::new()?;
        app.activity = GameActivity::Focused;
        app.run_auto();
        anyhow::ensure!(
            app.error.is_none(),
            "automatic apply failed: {:?}",
            app.error
        );
        anyhow::ensure!(
            display::current_mode()?.matches_profile(&config.stretch),
            "automatic stretch did not reach the primary display"
        );
        let applied = display::current_mode()?;
        app.activity = GameActivity::Background;
        app.run_auto();
        anyhow::ensure!(
            display::current_mode()? == applied,
            "default Alt+Tab changed the resolution"
        );
        app.activity = GameActivity::Focused;
        app.run_auto();
        anyhow::ensure!(
            display::current_mode()? == applied,
            "default return from Alt+Tab changed the resolution"
        );
        app.activity = GameActivity::Stopped;
        app.run_auto();
        anyhow::ensure!(
            display::current_mode()? == original,
            "game exit did not restore the desktop"
        );

        app.handle(Command::RestoreOnAltTab)?;
        anyhow::ensure!(
            Config::load()?.restore_on_alt_tab,
            "Alt+Tab opt-in did not persist"
        );
        app.activity = GameActivity::Focused;
        app.run_auto();
        app.activity = GameActivity::Background;
        app.run_auto();
        anyhow::ensure!(
            display::current_mode()? == original,
            "opted-in Alt+Tab did not restore the desktop"
        );
        app.handle(Command::RestoreOnAltTab)?;
        anyhow::ensure!(
            !Config::load()?.restore_on_alt_tab,
            "Alt+Tab opt-out did not persist"
        );

        app.activity = GameActivity::Focused;
        app.run_auto();
        anyhow::ensure!(
            app.error.is_none(),
            "second automatic apply failed: {:?}",
            app.error
        );
        app.handle(Command::AutoStretch)?;
        anyhow::ensure!(
            !Config::load()?.auto_stretch,
            "automatic toggle did not persist"
        );
        anyhow::ensure!(
            display::current_mode()? == original,
            "disabling automatic mode did not restore the desktop"
        );

        app.handle(Command::AutoStretch)?;
        anyhow::ensure!(
            display::current_mode()?.matches_profile(&config.stretch),
            "reenabling automatic mode did not apply stretch"
        );
        anyhow::ensure!(
            app.handle(Command::Exit)?,
            "Exit did not finish the automatic session"
        );
        anyhow::ensure!(
            display::current_mode()? == original,
            "Exit did not restore the desktop"
        );
        // The player may already be using 1440x1080. Pick a different size so
        // this check exercises a real manual switch and keeps native unambiguous.
        let manual_profile = if original.width == 1440 && original.height == 1080 {
            Profile::new(1280, 960)
        } else {
            Profile::new(1440, 1080)
        };
        app.handle(Command::Preset(manual_profile))?;
        app.handle(Command::Stretch)?;
        app.refresh_display();
        app.sync_menu();
        let manual_current = display::current_mode()?;
        anyhow::ensure!(
            manual_current.matches_profile(&app.config.stretch)
                && app.menu.stretch.is_checked()
                && !app.menu.native.is_checked(),
            "manual stretch did not reach the display and menu: current={manual_current:?}, restore={original:?}, native={:?}, stretch_checked={}, native_checked={}",
            app.display.native,
            app.menu.stretch.is_checked(),
            app.menu.native.is_checked()
        );
        app.handle(Command::Native)?;
        app.refresh_display();
        app.sync_menu();
        anyhow::ensure!(
            display::current_mode()? == original && app.menu.native.is_checked(),
            "manual Native did not restore the display and menu"
        );
        println!(
            "Default Alt+Tab kept the display unchanged; game exit restored it. Opted-in Alt+Tab restored it. Disabling auto-stretch, Exit, and manual Native also restored the display."
        );
        Ok(())
    }

    #[test]
    #[ignore = "requires the Windows shell and an isolated VSTRETCH_CONFIG; does not change display settings"]
    fn native_menu_events_save_config_and_update_checks() -> Result<()> {
        use tray_icon::menu::ContextMenu;
        use windows::Win32::{
            Foundation::{HWND, WPARAM},
            UI::WindowsAndMessaging::{GetMenuItemID, GetSubMenu, HMENU, SendMessageW, WM_COMMAND},
        };
        anyhow::ensure!(
            std::env::var_os("VSTRETCH_CONFIG").is_some(),
            "set VSTRETCH_CONFIG to an isolated absolute config path before running"
        );
        let initial = Config::load_or_init()?;
        anyhow::ensure!(
            initial.auto_stretch,
            "first-run config did not enable auto-stretch"
        );
        anyhow::ensure!(
            !initial.restore_on_alt_tab,
            "first-run config enabled Alt+Tab restoration"
        );
        anyhow::ensure!(
            initial.hotkey == "Ctrl+Alt+S" && initial.hotkey_enabled,
            "first-run config did not enable Ctrl+Alt+S hotkey"
        );
        let original = display::current_mode()?;
        // A native override equal to the active desktop makes preset selection
        // a save-only operation even if the test machine is already stretched.
        let config = Config {
            start_with_windows: Some(false),
            native: Some(Profile {
                width: original.width,
                height: original.height,
                refresh: Some(original.refresh),
            }),
            ..Config::default()
        };
        config.save_default_path()?;
        let mut app = App::new()?;
        anyhow::ensure!(
            app.icon.rect().is_some(),
            "Windows shell did not register the tray icon"
        );
        let hwnd = HWND(app.icon.window_handle());
        let root = HMENU(app.menu.root.hpopupmenu() as _);
        let items = app.menu.root.items();
        anyhow::ensure!(items.len() == 12, "tray menu grouping changed");
        let header = items[0]
            .as_menuitem()
            .context("app version header is missing")?;
        anyhow::ensure!(
            header.text() == format!("Vstretch v{}", env!("CARGO_PKG_VERSION"))
                && !header.is_enabled(),
            "header did not show the installed app version"
        );
        anyhow::ensure!(
            !app.menu.restore_on_alt_tab.is_checked(),
            "Alt+Tab checkbox was checked by default"
        );
        let toggle_id = unsafe { GetMenuItemID(root, 3) };
        unsafe { SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(toggle_id as usize)), None) };
        let event = MenuEvent::receiver()
            .try_recv()
            .context("no native toggle menu event")?;
        anyhow::ensure!(
            event.id() == app.menu.toggle.id(),
            "wrong toggle menu event"
        );
        anyhow::ensure!(
            app.menu.command(event.id()).is_some(),
            "toggle menu event did not map to a command"
        );
        // Toggle would change the display; mapping is enough for this save-only test.
        app.sync_menu();
        anyhow::ensure!(
            menu_caption(root, 3) == "Toggle mode\tCtrl+Alt+S",
            "toggle menu did not show the hotkey"
        );

        let settings = unsafe { GetSubMenu(root, 9) };
        anyhow::ensure!(!settings.is_invalid(), "Settings submenu is missing");
        let startup_id = unsafe { GetMenuItemID(settings, 2) };
        unsafe { SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(startup_id as usize)), None) };
        let event = MenuEvent::receiver()
            .try_recv()
            .context("no native startup menu event")?;
        anyhow::ensure!(
            matches!(app.menu.command(event.id()), Some(Command::Startup)),
            "startup menu event did not map to a command"
        );
        // Check the startup command without changing the user's sign-in settings.
        let automatic = unsafe { GetSubMenu(root, 8) };
        anyhow::ensure!(
            !automatic.is_invalid(),
            "Automatic switching submenu is missing"
        );
        let auto_id = unsafe { GetMenuItemID(automatic, 0) };
        unsafe { SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(auto_id as usize)), None) };
        let event = MenuEvent::receiver()
            .try_recv()
            .context("no native auto-stretch menu event")?;
        anyhow::ensure!(
            event.id() == app.menu.auto.id(),
            "wrong auto-stretch menu event"
        );
        app.handle(app.menu.command(event.id()).context("unknown menu event")?)?;
        app.sync_menu();
        anyhow::ensure!(
            !Config::load()?.auto_stretch && !app.menu.auto.is_checked(),
            "auto-stretch preference did not reach config and menu"
        );

        let alt_tab_id = unsafe { GetMenuItemID(automatic, 1) };
        for expected in [true, false] {
            unsafe { SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(alt_tab_id as usize)), None) };
            let event = MenuEvent::receiver()
                .try_recv()
                .context("no native Alt+Tab menu event")?;
            anyhow::ensure!(
                event.id() == app.menu.restore_on_alt_tab.id(),
                "wrong Alt+Tab menu event"
            );
            app.handle(
                app.menu
                    .command(event.id())
                    .context("unknown Alt+Tab event")?,
            )?;
            app.sync_menu();
            anyhow::ensure!(
                Config::load()?.restore_on_alt_tab == expected
                    && app.menu.restore_on_alt_tab.is_checked() == expected,
                "Alt+Tab preference did not reach config and menu"
            );
        }

        let presets = unsafe { GetSubMenu(root, 6) };
        let preset_id = unsafe { GetMenuItemID(presets, 0) };
        unsafe { SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(preset_id as usize)), None) };
        let event = MenuEvent::receiver()
            .try_recv()
            .context("no native preset menu event")?;
        app.handle(
            app.menu
                .command(event.id())
                .context("unknown preset event")?,
        )?;
        app.sync_menu();
        anyhow::ensure!(
            Config::load()?.stretch == app.menu.presets[0].0 && app.menu.presets[0].1.is_checked(),
            "preset did not reach config and menu"
        );

        let hotkey_id = unsafe { GetMenuItemID(settings, 0) };
        unsafe { SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(hotkey_id as usize)), None) };
        let event = MenuEvent::receiver()
            .try_recv()
            .context("no native hotkey menu event")?;
        anyhow::ensure!(
            event.id() == app.menu.hotkey_enabled.id(),
            "wrong hotkey menu event"
        );
        app.handle(
            app.menu
                .command(event.id())
                .context("unknown hotkey event")?,
        )?;
        app.sync_hotkey();
        app.sync_menu();
        anyhow::ensure!(
            !Config::load()?.hotkey_enabled && !app.menu.hotkey_enabled.is_checked(),
            "hotkey opt-out did not reach config and menu"
        );
        // Restore the default so later launches keep the global toggle.
        app.handle(Command::HotkeyEnabled)?;
        app.sync_hotkey();
        app.sync_menu();
        anyhow::ensure!(
            Config::load()?.hotkey_enabled && app.menu.hotkey_enabled.is_checked(),
            "hotkey opt-in did not reach config and menu"
        );

        let exit_id = unsafe { GetMenuItemID(root, 11) };
        unsafe { SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(exit_id as usize)), None) };
        let event = MenuEvent::receiver()
            .try_recv()
            .context("no native Exit menu event")?;
        anyhow::ensure!(
            app.handle(app.menu.command(event.id()).context("unknown Exit event")?)?,
            "Exit did not close the app"
        );
        anyhow::ensure!(
            display::current_mode()? == original,
            "save-only menu actions changed the display"
        );
        println!(
            "Shell registered the tray icon; toggle, auto-stretch, Alt+Tab opt-in/out, preset, hotkey opt-out/in, and Exit events reached the app. Display unchanged."
        );
        Ok(())
    }

    #[test]
    fn tray_icon_uses_the_canonical_terracotta_v() {
        const SIZE: usize = 32;
        const PIXELS: &[u8] = include_bytes!("../../assets/tray-icon-32.rgba");
        assert_eq!(PIXELS.len(), SIZE * SIZE * 4);
        // Rounded corners stay transparent so the square reads as a rounded tile.
        for (x, y) in [(0, 0), (31, 0), (0, 31), (31, 31)] {
            let alpha = PIXELS[(y * SIZE + x) * 4 + 3];
            assert_eq!(alpha, 0, "corner ({x},{y}) should stay transparent");
        }
        let mut terracotta = 0;
        let mut white = 0;
        let mut opaque = 0;
        for chunk in PIXELS.as_chunks::<4>().0 {
            if chunk[3] > 128 {
                opaque += 1;
            }
            if chunk[0] == 223 && chunk[1] == 117 && chunk[2] == 94 && chunk[3] == 255 {
                terracotta += 1;
            }
            // Anti-aliased edges blend the V, so count near-white as the V.
            if chunk[0] >= 240 && chunk[1] >= 240 && chunk[2] >= 240 && chunk[3] >= 200 {
                white += 1;
            }
        }
        assert!(terracotta > 100, "tray should keep the terracotta tile");
        assert!(white > 50, "tray should keep the white V");
        assert!(opaque > 500, "tray should stay mostly opaque");
        // The old navy/teal tray must not come back.
        assert!(
            !PIXELS.as_chunks::<4>().0.contains(&[22, 28, 42, 255]),
            "tray still uses the old navy background"
        );
        assert!(
            !PIXELS.as_chunks::<4>().0.contains(&[71, 222, 203, 255]),
            "tray still uses the old teal V"
        );
        // The Rust asset and the website favicon must stay byte-identical.
        assert_eq!(
            include_str!("../../assets/icon.svg"),
            include_str!("../../website/app/icon.svg"),
            "assets/icon.svg drifted from website/app/icon.svg"
        );
        assert!(
            include_str!("../../assets/icon.svg").contains("#df755e"),
            "canonical icon lost its terracotta fill"
        );
        tray_icon().unwrap();
    }
}
