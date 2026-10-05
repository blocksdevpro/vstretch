//! Validates, migrates, and saves display profiles and tray preferences.

use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};

/// A built-in stretch preset shared by the tray and terminal pickers.
pub struct PopularPreset {
    pub width: u32,
    pub height: u32,
    pub aspect: &'static str,
    /// An optional label shown beside the resolution in the terminal picker.
    pub tag: Option<&'static str>,
}

impl PopularPreset {
    /// Pads the dimensions so resolution columns line up in the picker.
    pub fn resolution_label(&self) -> String {
        format!("{:>4} × {:<4}", self.width, self.height)
    }

    #[cfg(test)]
    fn name(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }
}

/// Built-in stretch resolutions, sorted by descending height, then width.
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

fn validate_profile(profile: &Profile) -> Result<()> {
    if profile.width == 0 || profile.height == 0 {
        bail!("invalid size {}x{}", profile.width, profile.height);
    }
    if profile.refresh == Some(0) {
        bail!("invalid refresh 0");
    }
    Ok(())
}

/// Display profiles and preferences shared by the tray, CLI, and terminal UI.
///
/// Path (Windows): `%APPDATA%\vstretch\config.toml`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub stretch: Profile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<Profile>,
    #[serde(default = "auto_stretch_default")]
    pub auto_stretch: bool,
    #[serde(default)]
    pub restore_on_alt_tab: bool,
    /// None until the tray applies the first-launch Windows startup default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_with_windows: Option<bool>,
}

fn auto_stretch_default() -> bool {
    true
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
            auto_stretch: auto_stretch_default(),
            restore_on_alt_tab: false,
            start_with_windows: None,
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
            auto_stretch: auto_stretch_default(),
            restore_on_alt_tab: false,
            start_with_windows: None,
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
        if let Some(path) = std::env::var_os("VSTRETCH_CONFIG") {
            let path = PathBuf::from(path);
            ensure!(
                path.is_absolute(),
                "VSTRETCH_CONFIG must be an absolute path"
            );
            return Ok(path);
        }
        let dirs = BaseDirs::new().context("could not resolve home/config directories")?;
        Ok(dirs.config_dir().join("vstretch").join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        Self::load_from_path(&path)
    }

    fn load_from_path(path: &Path) -> Result<Self> {
        let text = {
            // Drop the lock as soon as the file is read. Parsing and any legacy
            // migration happen afterward, so they cannot block another reader.
            let _file = crate::platform::config_file_lock(path)?;
            if !path.exists() {
                bail!(
                    "no config found at {}\nrun `vstretch` once to set up",
                    path.display()
                );
            }
            fs::read_to_string(path)
                .with_context(|| format!("failed to read {}", path.display()))?
        };
        let (config, migrated) =
            parse_config(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        config.validate()?;
        if migrated {
            // A read-only legacy file is still usable. Try the migration again
            // on the next load rather than blocking display controls.
            let _ = config.save(path);
        }
        Ok(config)
    }

    /// Creates defaults only when the file is missing; malformed files stay intact.
    pub fn load_or_init() -> Result<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            Self::load()
        } else {
            let config = Self::default();
            config.save(&path)?;
            Ok(config)
        }
    }

    /// Replaces the config only after a complete, valid file has been written.
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
        let body = toml::to_string_pretty(self).context("failed to serialize config")?;
        // Each writer gets its own temporary file on the destination filesystem.
        // The tray can keep reading the old config until the rename replaces it.
        let mut staged = tempfile::Builder::new()
            .prefix(".vstretch-config-")
            .tempfile_in(parent)
            .with_context(|| format!("failed to stage {}", path.display()))?;
        write!(staged, "{CONFIG_HEADER}{body}")
            .with_context(|| format!("failed to write {}", staged.path().display()))?;
        staged
            .as_file()
            .sync_all()
            .context("failed to flush configuration")?;
        // Windows can reject replacement during a read or another rename. Share
        // a lock with load_from_path only while publishing the finished file.
        let _file = crate::platform::config_file_lock(path)?;
        staged
            .into_temp_path()
            .persist(path)
            .with_context(|| format!("failed to replace {}", path.display()))?;
        Ok(())
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
        self.update_and_save(|next| next.stretch = Profile::new(width, height))?;
        Ok(self.stretch.name())
    }

    pub fn set_native(&mut self, width: u32, height: u32, refresh: Option<u32>) -> Result<()> {
        self.update_and_save(|next| {
            next.native = Some(Profile {
                width,
                height,
                refresh,
            });
        })
    }

    pub fn clear_native(&mut self) -> Result<()> {
        self.update_and_save(|next| next.native = None)
    }

    fn update_and_save(&mut self, change: impl FnOnce(&mut Self)) -> Result<()> {
        let mut next = self.clone();
        change(&mut next);
        next.save_default_path()?;
        // Keep the UI on the last saved settings if validation or disk I/O fails.
        *self = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_replaces_a_complete_config_and_cleans_up_staging() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        Config::default().save(&path)?;
        let updated = Config {
            stretch: Profile::new(1280, 960),
            auto_stretch: false,
            ..Config::default()
        };
        updated.save(&path)?;

        let (loaded, migrated) = parse_config(&fs::read_to_string(&path)?)?;
        assert!(!migrated);
        assert_eq!(loaded.stretch, updated.stretch);
        assert!(!loaded.auto_stretch);
        assert_eq!(fs::read_dir(directory.path())?.count(), 1);
        Ok(())
    }

    #[test]
    fn failed_save_keeps_the_previous_file_and_cleans_up_staging() -> Result<()> {
        use std::os::windows::fs::OpenOptionsExt;

        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        Config::default().save(&path)?;
        let original = fs::read(&path)?;
        // Deny deletion and replacement while this handle is open. This exercises
        // a real Windows sharing error without changing the user's configuration.
        let locked = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)?;
        let updated = Config {
            stretch: Profile::new(1280, 960),
            ..Config::default()
        };
        assert!(updated.save(&path).is_err());
        drop(locked);

        assert_eq!(fs::read(&path)?, original);
        assert_eq!(fs::read_dir(directory.path())?.count(), 1);
        Ok(())
    }

    #[test]
    fn invalid_save_and_setters_keep_the_last_valid_settings() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        Config::default().save(&path)?;
        let original = fs::read(&path)?;
        let invalid = Config {
            stretch: Profile::new(0, 960),
            ..Config::default()
        };
        assert!(invalid.save(&path).is_err());
        assert_eq!(fs::read(&path)?, original);

        let mut config = Config::default();
        let stretch = config.stretch.clone();
        assert!(config.set_stretch(0, 960).is_err());
        assert_eq!(config.stretch, stretch);
        assert!(config.set_native(1920, 1080, Some(0)).is_err());
        assert!(config.native.is_none());
        Ok(())
    }

    #[test]
    fn concurrent_saves_never_publish_partial_configuration() -> Result<()> {
        use std::{sync::Barrier, thread};

        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        Config::default().save(&path)?;
        let widths = [1280, 1440, 1600, 1920];
        let barrier = Barrier::new(widths.len() + 1);

        thread::scope(|scope| -> Result<()> {
            let mut writers = Vec::new();
            for width in widths {
                let path = &path;
                let barrier = &barrier;
                writers.push(scope.spawn(move || -> Result<()> {
                    let config = Config {
                        stretch: Profile::new(width, 1080),
                        ..Config::default()
                    };
                    barrier.wait();
                    for _ in 0..8 {
                        config.save(path)?;
                    }
                    Ok(())
                }));
            }
            barrier.wait();
            for _ in 0..32 {
                let loaded = Config::load_from_path(&path)?;
                loaded.validate()?;
            }
            for writer in writers {
                writer.join().expect("config writer panicked")?;
            }
            Ok(())
        })?;
        assert_eq!(fs::read_dir(directory.path())?.count(), 1);
        Ok(())
    }

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
        assert!(!c.restore_on_alt_tab);
        assert!(c.start_with_windows.is_none());
        assert_eq!(c.stretch, Profile::new(1440, 1080));
        c.validate().unwrap();
    }

    #[test]
    fn default_config_serializes_stretch_and_auto_preference() {
        let text = toml::to_string_pretty(&Config::default()).unwrap();
        assert!(text.contains("[stretch]"));
        assert!(!text.contains("[native]"));
        assert!(!text.contains("[profiles"));
        assert!(!text.contains("default_stretch_profile"));
        assert!(text.contains("auto_stretch = true"));
        assert!(text.contains("restore_on_alt_tab = false"));
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
        assert!(c.auto_stretch);
        assert!(!c.restore_on_alt_tab);
        assert!(c.start_with_windows.is_none());
        assert_eq!(c.stretch, Profile::new(1280, 960));
        let n = c.native.as_ref().expect("native override");
        assert_eq!((n.width, n.height, n.refresh), (1920, 1080, Some(165)));
        c.validate().unwrap();
    }

    #[test]
    fn disabled_auto_stretch_round_trips_with_display_profiles() {
        let c = Config {
            auto_stretch: false,
            restore_on_alt_tab: true,
            ..Config::default()
        };
        let text = toml::to_string_pretty(&c).unwrap();
        let (loaded, migrated) = parse_config(&text).unwrap();
        assert!(!migrated);
        assert!(!loaded.auto_stretch);
        assert!(loaded.restore_on_alt_tab);
        assert_eq!(loaded.stretch, c.stretch);
    }

    #[test]
    fn startup_opt_out_survives_saving_other_preferences() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let config = Config {
            start_with_windows: Some(false),
            ..Config::default()
        };
        config.save(&path)?;
        let mut loaded = Config::load_from_path(&path)?;
        assert_eq!(loaded.start_with_windows, Some(false));
        loaded.stretch = Profile::new(1280, 960);
        loaded.save(&path)?;
        let reloaded = Config::load_from_path(&path)?;
        assert_eq!(reloaded.start_with_windows, Some(false));
        assert_eq!(reloaded.stretch, loaded.stretch);
        Ok(())
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
            ..Config::default()
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
            ..Config::default()
        };
        assert!(c.validate().is_err());
    }
}
