use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};

/// Curated popular stretch resolutions (Valorant / FPS community).
/// Shared built-in list — not user config.
pub struct PopularPreset {
    pub width: u32,
    pub height: u32,
    pub aspect: &'static str,
    /// Only `Some("common")` or `Some("popular")` — keep it minimal.
    pub tag: Option<&'static str>,
}

impl PopularPreset {
    /// Display line, e.g. `1440 × 1080`.
    pub fn resolution_label(&self) -> String {
        format!("{:>4} × {:<4}", self.width, self.height)
    }

    #[cfg(test)]
    fn name(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }
}

/// Built-in stretch resolutions, ordered high → low (height, then width).
/// Exactly one `common` and one `popular` tag in the whole list.
pub const POPULAR_STRETCH: &[PopularPreset] = &[
    PopularPreset {
        width: 1920,
        height: 1440,
        aspect: "4:3",
        tag: None,
    },
    PopularPreset {
        width: 1920,
        height: 1200,
        aspect: "16:10",
        tag: None,
    },
    PopularPreset {
        width: 1600,
        height: 1200,
        aspect: "4:3",
        tag: None,
    },
    PopularPreset {
        width: 1728,
        height: 1080,
        aspect: "16:10",
        tag: None,
    },
    PopularPreset {
        width: 1440,
        height: 1080,
        aspect: "4:3",
        tag: Some("popular"),
    },
    PopularPreset {
        width: 1680,
        height: 1050,
        aspect: "16:10",
        tag: None,
    },
    PopularPreset {
        width: 1280,
        height: 1024,
        aspect: "5:4",
        tag: None,
    },
    PopularPreset {
        width: 1280,
        height: 960,
        aspect: "4:3",
        tag: Some("common"),
    },
    PopularPreset {
        width: 1440,
        height: 900,
        aspect: "16:10",
        tag: None,
    },
    PopularPreset {
        width: 1152,
        height: 864,
        aspect: "4:3",
        tag: None,
    },
    PopularPreset {
        width: 1024,
        height: 768,
        aspect: "4:3",
        tag: None,
    },
];

impl PopularPreset {
    #[cfg(test)]
    fn find(query: &str) -> Option<&'static PopularPreset> {
        let q = query.trim().to_lowercase().replace('×', "x");
        POPULAR_STRETCH
            .iter()
            .find(|p| p.name().eq_ignore_ascii_case(&q))
    }
}

const CONFIG_HEADER: &str = "# vstretch — omit [native] to auto-detect the panel\n\n";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub width: u32,
    pub height: u32,
    /// Optional refresh rate. If omitted, panel Hz is used when applying stretch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh: Option<u32>,
}

impl Profile {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            refresh: None,
        }
    }

    pub fn name(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }
}

fn validate_profile(p: &Profile) -> Result<()> {
    if p.width == 0 || p.height == 0 {
        bail!("invalid size {}x{}", p.width, p.height);
    }
    if p.refresh == Some(0) {
        bail!("invalid refresh 0");
    }
    Ok(())
}

/// User config: one stretch target and an optional native restore override.
///
/// Path (Windows): `%APPDATA%\vstretch\config.toml`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub stretch: Profile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<Profile>,
}

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

impl Default for Config {
    fn default() -> Self {
        Self {
            stretch: Profile::new(1440, 1080),
            native: None,
        }
    }
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
        }
    }
}

fn parse_res_name(name: &str) -> Option<(u32, u32)> {
    let (w, h) = name.split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

fn parse_config(text: &str) -> Result<(Config, bool)> {
    match toml::from_str::<Config>(text) {
        Ok(config) => Ok((config, false)),
        Err(new_err) => match toml::from_str::<LegacyConfig>(text) {
            Ok(legacy) => Ok((Config::from(legacy), true)),
            Err(_) => Err(new_err.into()),
        },
    }
}

impl Config {
    pub fn config_path() -> Result<PathBuf> {
        let dirs = BaseDirs::new().context("could not resolve home/config directories")?;
        Ok(dirs.config_dir().join("vstretch").join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            bail!(
                "no config found at {}\nrun `vstretch` once to set up",
                path.display()
            );
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let (config, migrated) =
            parse_config(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        config.validate()?;
        if migrated {
            let _ = config.save(&path);
        }
        Ok(config)
    }

    /// Load config, or create default if missing.
    pub fn load_or_init() -> Result<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            Self::load()
        } else {
            let config = Self::default();
            config.save(&path)?;
            println!("created {}", path.display());
            Ok(config)
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let body = toml::to_string_pretty(self).context("failed to serialize config")?;
        let text = format!("{CONFIG_HEADER}{body}");
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, &text).with_context(|| format!("failed to write {}", tmp.display()))?;
        if path.exists() {
            fs::remove_file(path)
                .with_context(|| format!("failed to replace {}", path.display()))?;
        }
        match fs::rename(&tmp, path) {
            Ok(()) => Ok(()),
            Err(_) => {
                let result = fs::write(path, &text)
                    .with_context(|| format!("failed to write {}", path.display()));
                let _ = fs::remove_file(&tmp);
                result
            }
        }
    }

    pub fn save_default_path(&self) -> Result<()> {
        let path = Self::config_path()?;
        self.save(&path)
    }

    pub fn validate(&self) -> Result<()> {
        validate_profile(&self.stretch).context("stretch")?;
        if let Some(n) = &self.native {
            validate_profile(n).context("native")?;
        }
        Ok(())
    }

    pub fn set_stretch(&mut self, width: u32, height: u32) -> Result<String> {
        self.stretch = Profile::new(width, height);
        self.validate()?;
        self.save_default_path()?;
        Ok(self.stretch.name())
    }

    pub fn set_native(&mut self, width: u32, height: u32, refresh: Option<u32>) -> Result<()> {
        self.native = Some(Profile {
            width,
            height,
            refresh,
        });
        self.validate()?;
        self.save_default_path()?;
        Ok(())
    }

    pub fn clear_native(&mut self) -> Result<()> {
        self.native = None;
        self.save_default_path()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_popular_preset() {
        let p = PopularPreset::find("1440x1080").unwrap();
        assert_eq!((p.width, p.height), (1440, 1080));
        assert!(PopularPreset::find("9999x9999").is_none());
    }

    #[test]
    fn default_config_is_valid() {
        let c = Config::default();
        assert!(c.native.is_none());
        assert_eq!(c.stretch, Profile::new(1440, 1080));
        c.validate().unwrap();
    }

    #[test]
    fn default_config_serializes_stretch_only() {
        let text = toml::to_string_pretty(&Config::default()).unwrap();
        assert!(text.contains("[stretch]"));
        assert!(!text.contains("[native]"));
        assert!(!text.contains("[profiles"));
        assert!(!text.contains("default_stretch_profile"));
    }

    #[test]
    fn new_format_deserializes() {
        let text = r#"
[stretch]
width = 1280
height = 960
[native]
width = 1920
height = 1080
refresh = 165
"#;
        let (c, migrated) = parse_config(text).unwrap();
        assert!(!migrated);
        assert_eq!(c.stretch, Profile::new(1280, 960));
        let n = c.native.as_ref().expect("native override");
        assert_eq!((n.width, n.height, n.refresh), (1920, 1080, Some(165)));
        c.validate().unwrap();
    }

    #[test]
    fn header_comment_is_ignored() {
        let text = format!("{CONFIG_HEADER}[stretch]\nwidth = 1440\nheight = 1080\n");
        let (c, migrated) = parse_config(&text).unwrap();
        assert!(!migrated);
        assert_eq!(c.stretch, Profile::new(1440, 1080));
    }

    #[test]
    fn native_override_round_trips() {
        let c = Config {
            stretch: Profile::new(1440, 1080),
            native: Some(Profile {
                width: 1920,
                height: 1080,
                refresh: Some(180),
            }),
        };
        let text = toml::to_string_pretty(&c).unwrap();
        assert!(text.contains("[stretch]"));
        assert!(text.contains("[native]"));
        let loaded: Config = toml::from_str(&text).unwrap();
        assert_eq!(loaded.stretch, c.stretch);
        assert_eq!(loaded.native, c.native);
    }

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

    #[test]
    fn native_override_rejects_zero_size() {
        let c = Config {
            native: Some(Profile {
                width: 0,
                height: 1080,
                refresh: None,
            }),
            ..Config::default()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn native_override_rejects_zero_refresh() {
        let c = Config {
            native: Some(Profile {
                width: 1920,
                height: 1080,
                refresh: Some(0),
            }),
            ..Config::default()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn stretch_rejects_zero_refresh() {
        let c = Config {
            stretch: Profile {
                width: 1440,
                height: 1080,
                refresh: Some(0),
            },
            native: None,
        };
        assert!(c.validate().is_err());
    }
}
