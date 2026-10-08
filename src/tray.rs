//! Runs menu commands and automatic display polling on one Windows UI thread.

mod theme;

use anyhow::{Context, Result, bail};
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu},
};
use windows::{
    Win32::UI::{
        Input::KeyboardAndMouse::{
            HOT_KEY_MODIFIERS, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
        },
        WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, KillTimer, MSG, SetTimer, TranslateMessage, WM_HOTKEY,
            WM_TIMER,
        },
    },
    core::Error as WinError,
};

use crate::{
    autostretch::{Action, AutoPolicy, AutoStretch, GameActivity},
    config::{Config, POPULAR_STRETCH, Profile},
    display::{self, Mode, ModeKind},
    platform, startup,
};

/// Thread-wide hotkey id for the Native ↔ Stretch toggle.
const HOTKEY_ID: i32 = 1;
const APP_LABEL: &str = concat!("Vstretch v", env!("CARGO_PKG_VERSION"));

enum Command {
    Native,
    Stretch,
    Toggle,
    Preset(Profile),
    AutoStretch,
    RestoreOnAltTab,
    Startup,
    HotkeyEnabled,
    Exit,
}

fn toggle_label(config: &Config) -> String {
    if config.hotkey_enabled && !config.hotkey.trim().is_empty() {
        match crate::config::canonical_hotkey(&config.hotkey) {
            Ok(canonical) => format!("Toggle mode\t{canonical}"),
            Err(_) => format!("Toggle mode\t{}", config.hotkey.trim()),
        }
    } else {
        "Toggle mode".to_owned()
    }
}

struct Hotkey {
    registered: Option<(u32, u32)>,
}

impl Hotkey {
    fn new() -> Self {
        Self { registered: None }
    }

    /// Registers the configured hotkey, re-registering when it changes.
    ///
    /// Returns an actionable error for the menu when the combo is invalid or
    /// already taken. Disabled or cleared hotkeys unregister and report no
    /// error. Never panics; the tray keeps running on conflict.
    fn sync(&mut self, config: &Config) -> Option<String> {
        if !config.hotkey_enabled || config.hotkey.trim().is_empty() {
            self.unregister();
            return None;
        }
        let (modifiers, vk) = match crate::config::parse_hotkey(&config.hotkey) {
            Ok(parsed) => parsed,
            Err(error) => {
                self.unregister();
                return Some(format!(
                    "Hotkey '{}': {error:#} (fix config.toml or disable)",
                    config.hotkey.trim()
                ));
            }
        };
        if self.registered == Some((modifiers, vk)) {
            return None;
        }
        self.unregister();
        let flags = HOT_KEY_MODIFIERS(modifiers | MOD_NOREPEAT.0);
        // SAFETY: None registers a thread hotkey on this tray UI thread;
        // WM_HOTKEY for it arrives through GetMessageW below.
        match unsafe { RegisterHotKey(None, HOTKEY_ID, flags, vk) } {
            Ok(()) => {
                self.registered = Some((modifiers, vk));
                None
            }
            Err(error) => {
                let label = crate::config::canonical_hotkey(&config.hotkey)
                    .unwrap_or_else(|_| config.hotkey.trim().to_owned());
                Some(format!(
                    "Hotkey '{label}' unavailable: {error:#} (in use? change or disable)"
                ))
            }
        }
    }

    fn unregister(&mut self) {
        if self.registered.is_some() {
            // SAFETY: paired with RegisterHotKey(None, HOTKEY_ID, ..) on this thread.
            let _ = unsafe { UnregisterHotKey(None, HOTKEY_ID) };
            self.registered = None;
        }
    }
}

impl Drop for Hotkey {
    fn drop(&mut self) {
        self.unregister();
    }
}

struct TrayMenu {
    root: Menu,
    status: MenuItem,
    native: CheckMenuItem,
    stretch: CheckMenuItem,
    presets: Vec<(Profile, CheckMenuItem)>,
    toggle: MenuItem,
    auto: CheckMenuItem,
    restore_on_alt_tab: CheckMenuItem,
    startup: CheckMenuItem,
    hotkey_enabled: CheckMenuItem,
    exit: MenuItem,
}

impl TrayMenu {
    fn new(config: &Config) -> Result<Self> {
        let root = Menu::new();
        let version = MenuItem::new(APP_LABEL, false, None);
        let status = MenuItem::new("Vstretch", false, None);
        let native = CheckMenuItem::new("Native", true, false, None);
        let stretch = CheckMenuItem::new("Stretch", true, false, None);
        let presets_menu = Submenu::new("Stretch presets", true);
        let mut presets = Vec::new();
        for preset in POPULAR_STRETCH {
            let profile = Profile::new(preset.width, preset.height);
            let item = CheckMenuItem::new(
                format!("{} × {}\t{}", preset.width, preset.height, preset.aspect),
                true,
                false,
                None,
            );
            presets_menu.append(&item)?;
            presets.push((profile, item));
        }
        if !presets
            .iter()
            .any(|(p, _)| p.width == config.stretch.width && p.height == config.stretch.height)
        {
            let profile = config.stretch.clone();
            presets_menu.append(&PredefinedMenuItem::separator())?;
            let item = CheckMenuItem::new(
                format!("{} × {}\tCustom", profile.width, profile.height),
                true,
                false,
                None,
            );
            presets_menu.append(&item)?;
            presets.push((profile, item));
        }
        let toggle = MenuItem::new(toggle_label(config), true, None);
        let auto = CheckMenuItem::new(
            "Auto-stretch Valorant && CS2",
            true,
            config.auto_stretch,
            None,
        );
        let restore_on_alt_tab = CheckMenuItem::new(
            "Restore desktop on Alt+Tab",
            true,
            config.restore_on_alt_tab,
            None,
        );
        let startup = CheckMenuItem::new("Start with Windows", true, false, None);
        let hotkey_enabled = CheckMenuItem::new("Enable hotkey", true, config.hotkey_enabled, None);
        let automatic_menu = Submenu::new("Automatic switching", true);
        automatic_menu.append_items(&[&auto, &restore_on_alt_tab])?;
        let settings_menu = Submenu::new("Settings", true);
        settings_menu.append_items(&[
            &hotkey_enabled,
            &PredefinedMenuItem::separator(),
            &startup,
        ])?;
        let exit = MenuItem::new("Exit", true, None);
        root.append_items(&[
            &version,
            &status,
            &PredefinedMenuItem::separator(),
            &toggle,
            &native,
            &stretch,
            &presets_menu,
            &PredefinedMenuItem::separator(),
            &automatic_menu,
            &settings_menu,
            &PredefinedMenuItem::separator(),
            &exit,
        ])?;
        Ok(Self {
            root,
            status,
            native,
            stretch,
            presets,
            toggle,
            auto,
            restore_on_alt_tab,
            startup,
            hotkey_enabled,
            exit,
        })
    }

    fn command(&self, id: &MenuId) -> Option<Command> {
        if id == self.native.id() {
            Some(Command::Native)
        } else if id == self.stretch.id() {
            Some(Command::Stretch)
        } else if id == self.toggle.id() {
            Some(Command::Toggle)
        } else if id == self.auto.id() {
            Some(Command::AutoStretch)
        } else if id == self.restore_on_alt_tab.id() {
            Some(Command::RestoreOnAltTab)
        } else if id == self.startup.id() {
            Some(Command::Startup)
        } else if id == self.hotkey_enabled.id() {
            Some(Command::HotkeyEnabled)
        } else if id == self.exit.id() {
            Some(Command::Exit)
        } else {
            self.presets
                .iter()
                .find(|(_, item)| id == item.id())
                .map(|(profile, _)| Command::Preset(profile.clone()))
        }
    }

    fn sync(
        &self,
        config: &Config,
        current: Option<Mode>,
        panel: Option<Mode>,
        native: Option<Mode>,
    ) {
        let kind = display::classify_mode(current, native, panel, Some(&config.stretch));
        self.native.set_text(
            native
                .map(|n| format!("Native\t{} × {}", n.width, n.height))
                .unwrap_or_else(|| "Native\tUnavailable".into()),
        );
        self.native.set_enabled(native.is_some());
        self.native.set_checked(kind == ModeKind::Native);
        self.stretch.set_text(format!(
            "Stretch\t{} × {}",
            config.stretch.width, config.stretch.height
        ));
        // Automatic sessions and manual stretch both need a known restore target.
        self.stretch.set_enabled(native.is_some());
        self.stretch.set_checked(kind == ModeKind::Stretch);
        for (profile, item) in &self.presets {
            item.set_checked(
                profile.width == config.stretch.width && profile.height == config.stretch.height,
            );
        }
        self.toggle.set_text(toggle_label(config));
        self.auto.set_checked(config.auto_stretch);
        self.restore_on_alt_tab
            .set_checked(config.restore_on_alt_tab);
        self.hotkey_enabled.set_checked(config.hotkey_enabled);
        self.hotkey_enabled.set_text(
            crate::config::canonical_hotkey(&config.hotkey)
                .map(|hotkey| format!("Enable hotkey\t{hotkey}"))
                .unwrap_or_else(|_| "Enable hotkey".into()),
        );
    }
}

struct App {
    config: Config,
    menu: TrayMenu,
    icon: TrayIcon,
    auto: AutoStretch,
    current: Option<Mode>,
    panel: Option<Mode>,
    native: Option<Mode>,
    activity: GameActivity,
    error: Option<String>,
    hotkey: Hotkey,
    hotkey_error: Option<String>,
}

impl App {
    fn new() -> Result<Self> {
        let config = Config::load_or_init()?;
        let menu = TrayMenu::new(&config)?;
        let icon = TrayIconBuilder::new()
            .with_icon(tray_icon()?)
            .with_tooltip(format!("{APP_LABEL} | Right-click for display modes"))
            .with_menu(Box::new(menu.root.clone()))
            .with_menu_on_left_click(true)
            .build()
            .context("could not create the system tray icon")?;
        theme::follow_system_theme(&icon)?;
        let mut hotkey = Hotkey::new();
        let hotkey_error = hotkey.sync(&config);
        let mut app = Self {
            config,
            menu,
            icon,
            auto: AutoStretch::default(),
            current: None,
            panel: None,
            native: None,
            activity: GameActivity::Stopped,
            error: None,
            hotkey,
            hotkey_error,
        };
        app.refresh_display();
        if let Err(error) = startup::initialize(&mut app.config) {
            app.error = Some(format!("Startup settings: {error:#}"));
        }
        app.refresh_startup();
        app.sync_hotkey();
        app.sync_menu();
        Ok(app)
    }

    fn sync_hotkey(&mut self) {
        self.hotkey_error = self.hotkey.sync(&self.config);
    }

    fn refresh_display(&mut self) {
        self.current = display::get_current_resolution().ok();
        self.panel = display::get_native_resolution().ok();
        self.native = display::resolve_native(self.config.native.as_ref(), self.panel).ok();
    }

    fn refresh_startup(&mut self) {
        match startup::enabled() {
            Ok(enabled) => {
                self.menu.startup.set_enabled(true);
                self.menu.startup.set_checked(enabled);
            }
            Err(error) => {
                self.menu.startup.set_checked(false);
                self.menu.startup.set_enabled(false);
                self.error = Some(format!("Startup settings: {error:#}"));
            }
        }
    }

    fn sync_menu(&mut self) {
        self.menu
            .sync(&self.config, self.current, self.panel, self.native);
        let status = {
            let mut problems = Vec::new();
            if let Some(error) = &self.error {
                problems.push(error.clone());
            }
            if let Some(error) = &self.hotkey_error {
                problems.push(error.clone());
            }
            if problems.is_empty() {
                self.current
                    .map(|mode| {
                        format!(
                            "Current display\t{} × {} @ {} Hz",
                            mode.width, mode.height, mode.refresh
                        )
                    })
                    .unwrap_or_else(|| "Current display unavailable".into())
            } else {
                problems.join(" | ")
            }
        };
        self.menu.status.set_text(
            status
                .chars()
                .take(110)
                .collect::<String>()
                .replace('&', "&&"),
        );
        let _ = self.icon.set_tooltip(Some(format!(
            "{APP_LABEL} | {}",
            status
                .chars()
                .take(100)
                .collect::<String>()
                .replace('\t', ": ")
        )));
    }

    fn tick(&mut self) {
        // --tui may have saved a preset or native override while the tray runs.
        // Invalid external edits leave the last valid config in use and visible.
        match Config::load() {
            Ok(config) => self.config = config,
            Err(error) => self.error = Some(format!("Config: {error:#}")),
        }
        self.refresh_display();
        if let Some(activity) = platform::game_activity() {
            self.activity = activity;
            self.run_auto();
        } else if !self.config.auto_stretch {
            self.run_auto();
        }
        self.refresh_startup();
        self.sync_hotkey();
        self.sync_menu();
    }

    fn run_auto(&mut self) {
        let Some(current) = self.current else { return };
        let policy = AutoPolicy {
            enabled: self.config.auto_stretch,
            restore_on_alt_tab: self.config.restore_on_alt_tab,
        };
        let Some(action) = self.auto.observe(policy, self.activity, current) else {
            return;
        };
        let result = match action {
            Action::Stretch { desktop } => {
                if self.native.is_none() {
                    self.error = Some("Auto-stretch paused: native resolution unavailable".into());
                    return;
                }
                display::apply_profile(
                    &self.config.stretch,
                    display::stretch_refresh(self.panel, self.native),
                )
                .map(|applied| self.auto.applied(desktop, applied))
            }
            Action::Restore { desktop } => display::change_resolution(desktop)
                .map(|()| self.auto.restored(policy, self.activity)),
        };
        match result {
            Ok(()) => self.error = None,
            Err(error) => self.error = Some(format!("Auto-stretch: {error:#}")),
        }
        self.refresh_display();
    }

    fn handle(&mut self, command: Command) -> Result<bool> {
        self.error = None;
        self.refresh_display();
        match command {
            Command::Native => {
                let native = self.native.context("could not detect native resolution")?;
                display::restore_native(native, &self.config.stretch)?;
                self.auto.manual_choice();
            }
            Command::Stretch => {
                self.native.context("could not detect native resolution")?;
                display::apply_profile(
                    &self.config.stretch,
                    display::stretch_refresh(self.panel, self.native),
                )?;
                self.auto.manual_choice();
            }
            Command::Toggle => {
                let native = self.native.context("could not detect native resolution")?;
                display::toggle_stretch(
                    &self.config.stretch,
                    native,
                    display::stretch_refresh(self.panel, Some(native)),
                )?;
                self.auto.manual_choice();
            }
            Command::Preset(profile) => {
                let was_stretch = display::classify_mode(
                    self.current,
                    self.native,
                    self.panel,
                    Some(&self.config.stretch),
                ) == ModeKind::Stretch;
                let mut next = self.config.clone();
                next.stretch = profile;
                next.save_default_path()?;
                self.config = next;
                if was_stretch {
                    display::apply_profile(
                        &self.config.stretch,
                        display::stretch_refresh(self.panel, self.native),
                    )?;
                    self.auto.manual_choice();
                }
            }
            Command::AutoStretch => {
                let mut next = self.config.clone();
                next.auto_stretch = !next.auto_stretch;
                next.save_default_path()?;
                self.config = next;
                if self.config.auto_stretch
                    && matches!(self.auto, AutoStretch::PausedUntilSessionEnds)
                {
                    self.auto = AutoStretch::Ready;
                }
                self.run_auto();
            }
            Command::RestoreOnAltTab => {
                let mut next = self.config.clone();
                next.restore_on_alt_tab = !next.restore_on_alt_tab;
                next.save_default_path()?;
                self.config = next;
                self.run_auto();
            }
            Command::Startup => {
                let enabled = !startup::enabled()?;
                let mut next = self.config.clone();
                next.start_with_windows = Some(enabled);
                next.save_default_path()?;
                self.config = next;
                startup::set_enabled(enabled)?;
            }
            Command::HotkeyEnabled => {
                let enabled = !self.config.hotkey_enabled;
                self.config.set_hotkey_enabled(enabled)?;
            }
            Command::Exit => {
                self.restore_on_exit()?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn restore_on_exit(&mut self) -> Result<()> {
        if let Some(desktop) = display::get_current_resolution()
            .ok()
            .and_then(|current| self.auto.restore_on_exit(current))
        {
            display::change_resolution(desktop)
                .context("could not restore the desktop; select Native before exiting")?;
            self.auto = AutoStretch::Ready;
        }
        Ok(())
    }
}

struct Timer(usize);

impl Timer {
    fn new() -> Result<Self> {
        // A thread timer keeps all display changes on the menu's UI thread.
        let id = unsafe { SetTimer(None, 0, 750, None) };
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
            if let Some(command) = app.menu.command(event.id()) {
                match app.handle(command) {
                    Ok(true) => return Ok(None),
                    Ok(false) => {}
                    Err(error) => {
                        app.error = Some(format!("{error:#}"));
                        platform::show_error(&format!("{error:#}"));
                    }
                }
                app.refresh_display();
                app.refresh_startup();
                app.sync_hotkey();
                app.sync_menu();
            }
        }
        // Mouse move/click events aren't needed, but drain the default channel.
        while TrayIconEvent::receiver().try_recv().is_ok() {}
        if message.message == WM_HOTKEY && message.wParam.0 == HOTKEY_ID as usize {
            match app.handle(Command::Toggle) {
                Ok(true) => return Ok(None),
                Ok(false) => {}
                Err(error) => {
                    app.error = Some(format!("{error:#}"));
                    platform::show_error(&format!("{error:#}"));
                }
            }
            app.refresh_display();
            app.refresh_startup();
            app.sync_hotkey();
            app.sync_menu();
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
    const PIXELS: &[u8] = include_bytes!("../assets/tray-icon-32.rgba");
    anyhow::ensure!(
        PIXELS.len() == (SIZE * SIZE * 4) as usize,
        "tray icon pixels do not match {SIZE}x{SIZE} RGBA"
    );
    Icon::from_rgba(PIXELS.to_vec(), SIZE, SIZE).context("could not create tray icon image")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tray_icon::menu::ContextMenu;
    use windows::Win32::UI::WindowsAndMessaging::{GetMenuStringW, HMENU, MF_BYPOSITION};

    // Muda's text() omits everything after the tab. Read the native caption to
    // verify the shortcut and resolution columns Windows actually displays.
    fn menu_caption(menu: HMENU, position: u32) -> String {
        let mut buffer = [0u16; 256];
        // SAFETY: the menu is owned by the test and the output buffer is writable.
        let length = unsafe { GetMenuStringW(menu, position, Some(&mut buffer), MF_BYPOSITION) };
        String::from_utf16_lossy(&buffer[..length as usize])
    }

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
                let _ = display::change_resolution(self.0);
            }
        }
        let original = display::get_current_resolution()?;
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
            display::get_current_resolution()?.matches_profile(&config.stretch),
            "automatic stretch did not reach the primary display"
        );
        let applied = display::get_current_resolution()?;
        app.activity = GameActivity::Background;
        app.run_auto();
        anyhow::ensure!(
            display::get_current_resolution()? == applied,
            "default Alt+Tab changed the resolution"
        );
        app.activity = GameActivity::Focused;
        app.run_auto();
        anyhow::ensure!(
            display::get_current_resolution()? == applied,
            "default return from Alt+Tab changed the resolution"
        );
        app.activity = GameActivity::Stopped;
        app.run_auto();
        anyhow::ensure!(
            display::get_current_resolution()? == original,
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
            display::get_current_resolution()? == original,
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
            display::get_current_resolution()? == original,
            "disabling automatic mode did not restore the desktop"
        );

        app.handle(Command::AutoStretch)?;
        anyhow::ensure!(
            display::get_current_resolution()?.matches_profile(&config.stretch),
            "reenabling automatic mode did not apply stretch"
        );
        anyhow::ensure!(
            app.handle(Command::Exit)?,
            "Exit did not finish the automatic session"
        );
        anyhow::ensure!(
            display::get_current_resolution()? == original,
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
        let manual_current = display::get_current_resolution()?;
        anyhow::ensure!(
            manual_current.matches_profile(&app.config.stretch)
                && app.menu.stretch.is_checked()
                && !app.menu.native.is_checked(),
            "manual stretch did not reach the display and menu: current={manual_current:?}, restore={original:?}, native={:?}, stretch_checked={}, native_checked={}",
            app.native,
            app.menu.stretch.is_checked(),
            app.menu.native.is_checked()
        );
        app.handle(Command::Native)?;
        app.refresh_display();
        app.sync_menu();
        anyhow::ensure!(
            display::get_current_resolution()? == original && app.menu.native.is_checked(),
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
        let original = display::get_current_resolution()?;
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
            display::get_current_resolution()? == original,
            "save-only menu actions changed the display"
        );
        println!(
            "Shell registered the tray icon; toggle, auto-stretch, Alt+Tab opt-in/out, preset, hotkey opt-out/in, and Exit events reached the app. Display unchanged."
        );
        Ok(())
    }

    #[test]
    fn mode_checks_follow_the_actual_display_and_presets_follow_saved_config() {
        let config = Config::default();
        let menu = TrayMenu::new(&config).unwrap();
        let native = Mode {
            width: 2560,
            height: 1440,
            refresh: 180,
        };
        let stretch = Mode {
            width: 1440,
            height: 1080,
            refresh: 180,
        };
        menu.sync(&config, Some(native), Some(native), Some(native));
        assert!(menu.native.is_checked());
        assert!(!menu.stretch.is_checked());
        menu.sync(&config, Some(stretch), Some(native), Some(native));
        assert!(!menu.native.is_checked());
        assert!(menu.stretch.is_checked());
        assert_eq!(
            menu.presets
                .iter()
                .filter(|(_, item)| item.is_checked())
                .count(),
            1
        );
        menu.sync(&config, None, Some(native), Some(native));
        assert!(!menu.native.is_checked());
        assert!(!menu.stretch.is_checked());
    }

    #[test]
    fn toggle_menu_shows_hotkey_and_enable_tracks_config() {
        let config = Config::default();
        let menu = TrayMenu::new(&config).unwrap();
        let root = HMENU(menu.root.hpopupmenu() as _);
        assert_eq!(menu_caption(root, 3), "Toggle mode\tCtrl+Alt+S");
        assert!(menu.hotkey_enabled.is_checked());
        assert!(matches!(
            menu.command(menu.toggle.id()),
            Some(Command::Toggle)
        ));
        assert!(matches!(
            menu.command(menu.hotkey_enabled.id()),
            Some(Command::HotkeyEnabled)
        ));

        let disabled = Config {
            hotkey_enabled: false,
            ..Config::default()
        };
        menu.sync(&disabled, None, None, None);
        assert!(!menu.hotkey_enabled.is_checked());
        assert_eq!(menu.toggle.text(), "Toggle mode");
        assert_eq!(toggle_label(&disabled), "Toggle mode");

        let custom = Config {
            hotkey: "Ctrl+Shift+F9".into(),
            ..Config::default()
        };
        menu.sync(&custom, None, None, None);
        assert_eq!(menu_caption(root, 3), "Toggle mode\tCtrl+Shift+F9");
    }

    #[test]
    fn hotkey_sync_handles_disabled_cleared_and_invalid_without_crash() {
        let mut hotkey = Hotkey::new();
        // Disabled and cleared hotkeys unregister quietly with no error.
        for config in [
            Config {
                hotkey_enabled: false,
                ..Config::default()
            },
            Config {
                hotkey: String::new(),
                ..Config::default()
            },
            Config {
                hotkey: "   ".into(),
                ..Config::default()
            },
        ] {
            assert!(hotkey.sync(&config).is_none());
            assert!(hotkey.registered.is_none());
        }
        // Invalid combos report an actionable error and stay unregistered.
        let invalid = Config {
            hotkey: "Ctrl+Alt".into(),
            ..Config::default()
        };
        let error = hotkey
            .sync(&invalid)
            .expect("invalid hotkey needs an error");
        assert!(error.contains("Ctrl+Alt"), "{error}");
        assert!(hotkey.registered.is_none());
    }

    #[test]
    fn tray_icon_uses_the_canonical_terracotta_v() {
        const SIZE: usize = 32;
        const PIXELS: &[u8] = include_bytes!("../assets/tray-icon-32.rgba");
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
            include_str!("../assets/icon.svg"),
            include_str!("../website/app/icon.svg"),
            "assets/icon.svg drifted from website/app/icon.svg"
        );
        assert!(
            include_str!("../assets/icon.svg").contains("#df755e"),
            "canonical icon lost its terracotta fill"
        );
        tray_icon().unwrap();
    }
}
