//! Builds the tray's native menu and keeps its checks and labels current.

use anyhow::Result;
use tray_icon::menu::{CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu};

use super::APP_LABEL;
use crate::{
    config::{Config, Profile, hotkey, presets::POPULAR_STRETCH},
    display::{DisplayState, ModeKind},
};

/// A menu choice or hotkey press for the tray app to carry out.
pub(super) enum Command {
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

pub(super) fn toggle_label(config: &Config) -> String {
    if config.hotkey_enabled && !config.hotkey.trim().is_empty() {
        match hotkey::canonical(&config.hotkey) {
            Ok(canonical) => format!("Toggle mode\t{canonical}"),
            Err(_) => format!("Toggle mode\t{}", config.hotkey.trim()),
        }
    } else {
        "Toggle mode".to_owned()
    }
}

pub(super) struct TrayMenu {
    pub(super) root: Menu,
    pub(super) status: MenuItem,
    pub(super) native: CheckMenuItem,
    pub(super) stretch: CheckMenuItem,
    pub(super) presets: Vec<(Profile, CheckMenuItem)>,
    pub(super) toggle: MenuItem,
    pub(super) auto: CheckMenuItem,
    pub(super) restore_on_alt_tab: CheckMenuItem,
    pub(super) startup: CheckMenuItem,
    pub(super) hotkey_enabled: CheckMenuItem,
    pub(super) exit: MenuItem,
}

impl TrayMenu {
    pub(super) fn new(config: &Config) -> Result<Self> {
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

    pub(super) fn command(&self, id: &MenuId) -> Option<Command> {
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

    pub(super) fn sync(&self, config: &Config, display: &DisplayState) {
        let kind = display.kind(&config.stretch);
        let native = display.native;
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
            hotkey::canonical(&config.hotkey)
                .map(|hotkey| format!("Enable hotkey\t{hotkey}"))
                .unwrap_or_else(|_| "Enable hotkey".into()),
        );
    }
}

/// Muda's text() omits everything after the tab. Read the native caption to
/// verify the shortcut and resolution columns Windows actually displays.
#[cfg(test)]
pub(super) fn menu_caption(
    menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    position: u32,
) -> String {
    use windows::Win32::UI::WindowsAndMessaging::{GetMenuStringW, MF_BYPOSITION};
    let mut buffer = [0u16; 256];
    // SAFETY: the menu is owned by the test and the output buffer is writable.
    let length = unsafe { GetMenuStringW(menu, position, Some(&mut buffer), MF_BYPOSITION) };
    String::from_utf16_lossy(&buffer[..length as usize])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::Mode;
    use tray_icon::menu::ContextMenu;
    use windows::Win32::UI::WindowsAndMessaging::HMENU;

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
        menu.sync(
            &config,
            &DisplayState {
                current: Some(native),
                panel: Some(native),
                native: Some(native),
            },
        );
        assert!(menu.native.is_checked());
        assert!(!menu.stretch.is_checked());
        menu.sync(
            &config,
            &DisplayState {
                current: Some(stretch),
                panel: Some(native),
                native: Some(native),
            },
        );
        assert!(!menu.native.is_checked());
        assert!(menu.stretch.is_checked());
        assert_eq!(
            menu.presets
                .iter()
                .filter(|(_, item)| item.is_checked())
                .count(),
            1
        );
        menu.sync(
            &config,
            &DisplayState {
                current: None,
                panel: Some(native),
                native: Some(native),
            },
        );
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
        menu.sync(
            &disabled,
            &DisplayState {
                current: None,
                panel: None,
                native: None,
            },
        );
        assert!(!menu.hotkey_enabled.is_checked());
        assert_eq!(menu.toggle.text(), "Toggle mode");
        assert_eq!(toggle_label(&disabled), "Toggle mode");

        let custom = Config {
            hotkey: "Ctrl+Shift+F9".into(),
            ..Config::default()
        };
        menu.sync(
            &custom,
            &DisplayState {
                current: None,
                panel: None,
                native: None,
            },
        );
        assert_eq!(menu_caption(root, 3), "Toggle mode\tCtrl+Shift+F9");
    }
}
