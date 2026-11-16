//! Registers the configured global hotkey on the tray's UI thread.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
};

use crate::config::{Config, hotkey};

/// Thread-wide hotkey id for the Native ↔ Stretch toggle.
pub(super) const HOTKEY_ID: i32 = 1;

pub(super) struct Hotkey {
    registered: Option<(u32, u32)>,
}

impl Hotkey {
    pub(super) fn new() -> Self {
        Self { registered: None }
    }

    /// Registers the configured hotkey, re-registering when it changes.
    ///
    /// Returns an actionable error for the menu when the combo is invalid or
    /// already taken. Disabled or cleared hotkeys unregister and report no
    /// error. Never panics; the tray keeps running on conflict.
    pub(super) fn sync(&mut self, config: &Config) -> Option<String> {
        if !config.hotkey_enabled || config.hotkey.trim().is_empty() {
            self.unregister();
            return None;
        }
        let (modifiers, vk) = match hotkey::parse(&config.hotkey) {
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
                let label = hotkey::canonical(&config.hotkey)
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
