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
    /// Profile / config key, e.g. `1440x1080`.
    pub fn name(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }

    /// Display line, e.g. `1440 × 1080`.
    pub fn resolution_label(&self) -> String {
        format!("{:>4} × {:<4}", self.width, self.height)
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
    pub fn find(query: &str) -> Option<&'static PopularPreset> {
        let q = query.trim().to_lowercase().replace('×', "x");
        POPULAR_STRETCH.iter().find(|p| p.name().eq_ignore_ascii_case(&q))
    }
}

/// User config for stretch resolution profiles.
///
/// Path (Windows): `%APPDATA%\vstretch\config.toml`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Profile used by `auto` / `stretch` when none is specified.
    pub default_profile: String,
    /// Named stretch profiles (width × height).
    pub profiles: BTreeMap<String, Profile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub width: u32,
    pub height: u32,
    /// Optional refresh rate. If omitted, native refresh is used.
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
}

impl Default for Config {
    fn default() -> Self {
        let mut profiles = BTreeMap::new();
        // Seed with a few of the most common presets
        for name in ["1440x1080", "1280x960", "1728x1080"] {
            if let Some(p) = PopularPreset::find(name) {
                profiles.insert(p.name(), Profile::new(p.width, p.height));
            }
        }
        Self {
            default_profile: "1440x1080".to_string(),
            profiles,
        }
    }
}

impl Config {
    pub fn config_path() -> Result<PathBuf> {
        // Windows: %APPDATA%\vstretch\config.toml
        let dirs = BaseDirs::new().context("could not resolve home/config directories")?;
        Ok(dirs.config_dir().join("vstretch").join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            bail!(
                "no config found at {}\nrun `vstretch init` to create one",
                path.display()
            );
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let config: Config = toml::from_str(&text)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        config.validate()?;
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
        let text = toml::to_string_pretty(self).context("failed to serialize config")?;
        fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    pub fn save_default_path(&self) -> Result<()> {
        let path = Self::config_path()?;
        self.save(&path)
    }

    pub fn validate(&self) -> Result<()> {
        if self.profiles.is_empty() {
            bail!("config has no profiles");
        }
        if !self.profiles.contains_key(&self.default_profile) {
            bail!(
                "default_profile `{}` not found in [profiles]\navailable: {}",
                self.default_profile,
                self.profile_names().join(", ")
            );
        }
        for (name, p) in &self.profiles {
            if p.width == 0 || p.height == 0 {
                bail!(
                    "profile `{}` has invalid size {}x{}",
                    name,
                    p.width,
                    p.height
                );
            }
        }
        Ok(())
    }

    pub fn profile_names(&self) -> Vec<&str> {
        self.profiles.keys().map(String::as_str).collect()
    }

    pub fn get_profile(&self, name: Option<&str>) -> Result<(&str, &Profile)> {
        let key = name.unwrap_or(self.default_profile.as_str());
        let (stored_name, profile) = self.profiles.get_key_value(key).with_context(|| {
            format!(
                "unknown profile `{key}`\navailable: {}",
                self.profile_names().join(", ")
            )
        })?;
        Ok((stored_name.as_str(), profile))
    }

    /// Ensure a profile exists for this resolution, set it as default, save.
    pub fn set_default_resolution(&mut self, width: u32, height: u32) -> Result<String> {
        let name = format!("{width}x{height}");
        self.profiles
            .entry(name.clone())
            .or_insert_with(|| Profile::new(width, height));
        self.default_profile = name.clone();
        self.validate()?;
        self.save_default_path()?;
        Ok(name)
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
        Config::default().validate().unwrap();
    }
}

