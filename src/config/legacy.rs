//! Reads the older on-disk format (a named profile map) into the current `Config`.

use std::collections::BTreeMap;

use anyhow::Result;
use serde::Deserialize;

use super::{Config, Profile};

/// Previous on-disk shape: a named profile map plus a string pointer.
#[derive(Debug, Deserialize)]
struct LegacyConfig {
    #[serde(alias = "default_profile")]
    default_stretch_profile: String,
    #[serde(default, alias = "native")]
    default_native_profile: Option<Profile>,
    #[serde(default)]
    profiles: BTreeMap<String, Profile>,
}

impl From<LegacyConfig> for Config {
    fn from(legacy: LegacyConfig) -> Self {
        let stretch = legacy
            .profiles
            .get(&legacy.default_stretch_profile)
            .cloned()
            .or_else(|| {
                parse_res_name(&legacy.default_stretch_profile).map(|(w, h)| Profile::new(w, h))
            })
            .unwrap_or_else(|| Config::default().stretch);
        Self {
            stretch,
            native: legacy.default_native_profile,
            auto_stretch: Config::default().auto_stretch,
            restore_on_alt_tab: false,
            start_with_windows: None,
            hotkey: Config::default().hotkey,
            hotkey_enabled: Config::default().hotkey_enabled,
        }
    }
}

fn parse_res_name(name: &str) -> Option<(u32, u32)> {
    let (w, h) = name.split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

/// Parses either format. The flag is true when the text used the legacy one.
pub(super) fn parse_config(text: &str) -> Result<(Config, bool)> {
    match toml::from_str::<Config>(text) {
        Ok(config) => Ok((config, false)),
        Err(new_err) => match toml::from_str::<LegacyConfig>(text) {
            Ok(legacy) => Ok((Config::from(legacy), true)),
            Err(_) => Err(new_err.into()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_profile_map_migrates() {
        let text = r#"
default_stretch_profile = "1440x1080"

[default_native_profile]
width = 2560
height = 1440
refresh = 180

[profiles.1024x768]
width = 1024
height = 768

[profiles.1440x1080]
width = 1440
height = 1080
"#;
        let (c, migrated) = parse_config(text).unwrap();
        assert!(migrated);
        assert_eq!(c.stretch, Profile::new(1440, 1080));
        let n = c.native.expect("native override");
        assert_eq!((n.width, n.height, n.refresh), (2560, 1440, Some(180)));
    }

    #[test]
    fn old_default_profile_key_still_loads() {
        let text = r#"
default_profile = "1280x960"
[profiles.1280x960]
width = 1280
height = 960
refresh = 144
"#;
        let (c, migrated) = parse_config(text).unwrap();
        assert!(migrated);
        assert_eq!(
            c.stretch,
            Profile {
                width: 1280,
                height: 960,
                refresh: Some(144),
            }
        );
        assert!(c.native.is_none());
    }

    #[test]
    fn legacy_native_table_still_loads() {
        let text = r#"
default_stretch_profile = "1440x1080"
[native]
width = 1920
height = 1080
refresh = 165
[profiles.1440x1080]
width = 1440
height = 1080
"#;
        let (c, migrated) = parse_config(text).unwrap();
        assert!(migrated);
        let n = c.native.expect("native override");
        assert_eq!((n.width, n.height, n.refresh), (1920, 1080, Some(165)));
    }
}
