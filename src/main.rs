#![cfg_attr(not(windows), allow(dead_code, unused_imports))]

mod config;

#[cfg(not(windows))]
compile_error!("vstretch is Windows-only (uses Win32 display APIs)");

use anyhow::{Context, Result};
use clap::Parser;
use config::{Config, POPULAR_STRETCH, PopularPreset, Profile};
use dialoguer::{Select, theme::ColorfulTheme};
use windows::{
    Win32::{
        Devices::Display::{
            DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_MODE_INFO_TYPE_TARGET, DISPLAYCONFIG_PATH_INFO,
            GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
        },
        Foundation::{ERROR_NOT_FOUND, ERROR_SUCCESS},
        Graphics::Gdi::{
            CDS_UPDATEREGISTRY, ChangeDisplaySettingsExW, DEVMODEW, DISP_CHANGE_SUCCESSFUL,
            DISPLAYCONFIG_PATH_ACTIVE, DM_DISPLAYFREQUENCY, DM_PELSHEIGHT, DM_PELSWIDTH,
            ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW,
        },
    },
    core::Error as WinError,
};

#[derive(Parser)]
#[command(name = "vstretch")]
#[command(about = "Valorant resolution switcher for stretch res")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Toggle between native and stretch (default profile, or --profile)
    Auto {
        /// Stretch profile name from config
        #[arg(short, long)]
        profile: Option<String>,
    },
    /// Switch to native panel resolution
    Native,
    /// Switch to a stretch profile
    Stretch {
        /// Stretch profile name from config (default: config.default_profile)
        #[arg(short, long)]
        profile: Option<String>,
    },
    /// Create a default config file if missing
    Init {
        /// Overwrite an existing config
        #[arg(long)]
        force: bool,
    },
    /// Show config path, default profile, and all profiles
    Config,
    /// List built-in popular stretch resolutions
    Presets,
    /// Set the default stretch resolution (interactive picker, or pass a res)
    SetDefault {
        /// Resolution like `1440x1080`, or a popular preset name.
        /// Omit to pick interactively from the popular list.
        resolution: Option<String>,

        /// Set default to an existing profile name in your config
        #[arg(short, long, conflicts_with = "resolution")]
        profile: Option<String>,
    },
}

fn get_current_resolution() -> Result<(u32, u32, u32)> {
    let mut devmode = DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    let result = unsafe { EnumDisplaySettingsW(None, ENUM_CURRENT_SETTINGS, &mut devmode) };

    if result.as_bool() {
        Ok((
            devmode.dmPelsWidth,
            devmode.dmPelsHeight,
            devmode.dmDisplayFrequency,
        ))
    } else {
        Err(WinError::from(unsafe { windows::Win32::Foundation::GetLastError() }).into())
    }
}

fn get_native_resolution() -> Result<(u32, u32, u32)> {
    unsafe {
        let mut path_count = 0u32;
        let mut mode_count = 0u32;

        let status =
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count);

        if status != ERROR_SUCCESS {
            return Err(WinError::from(status).into());
        }

        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];

        let status = QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        );

        if status != ERROR_SUCCESS {
            return Err(WinError::from(status).into());
        }

        // Prefer the first active path (typically primary)
        for path in &paths {
            if path.flags & DISPLAYCONFIG_PATH_ACTIVE != 0 {
                let target_mode_idx = path.targetInfo.Anonymous.modeInfoIdx as usize;

                if target_mode_idx < modes.len() {
                    let mode = &modes[target_mode_idx];

                    if mode.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_TARGET {
                        let target_mode = mode.Anonymous.targetMode;

                        let width = target_mode.targetVideoSignalInfo.activeSize.cx as u32;
                        let height = target_mode.targetVideoSignalInfo.activeSize.cy as u32;

                        let refresh = target_mode
                            .targetVideoSignalInfo
                            .vSyncFreq
                            .Numerator
                            .checked_div(
                                target_mode.targetVideoSignalInfo.vSyncFreq.Denominator,
                            )
                            .unwrap_or(60);

                        return Ok((width, height, refresh));
                    }
                }
            }
        }

        Err(WinError::from(ERROR_NOT_FOUND).into())
    }
}

fn change_resolution(width: u32, height: u32, refresh: u32) -> Result<()> {
    let devmode = DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        dmFields: DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY,
        dmPelsWidth: width,
        dmPelsHeight: height,
        dmDisplayFrequency: refresh,
        ..Default::default()
    };
    let result =
        unsafe { ChangeDisplaySettingsExW(None, Some(&devmode), None, CDS_UPDATEREGISTRY, None) };
    if result == DISP_CHANGE_SUCCESSFUL {
        println!("→ Switched to {}x{} @ {}Hz", width, height, refresh);
        Ok(())
    } else {
        eprintln!("Failed to change resolution");
        Err(WinError::from(unsafe { windows::Win32::Foundation::GetLastError() }).into())
    }
}

fn apply_profile(profile_name: &str, profile: &Profile, native_refresh: u32) -> Result<()> {
    let refresh = profile.refresh.unwrap_or(native_refresh);
    println!(
        "profile `{}` → {}x{} @ {}Hz",
        profile_name, profile.width, profile.height, refresh
    );
    change_resolution(profile.width, profile.height, refresh)
}

fn cmd_init(force: bool) -> Result<()> {
    let path = Config::config_path()?;
    if path.exists() && !force {
        anyhow::bail!(
            "config already exists at {}\nuse `vstretch init --force` to overwrite",
            path.display()
        );
    }

    let config = Config::default();
    config.save(&path)?;
    println!("created {}", path.display());
    println!("default profile: `{}`", config.default_profile);
    for (name, p) in &config.profiles {
        let marker = if name == &config.default_profile {
            " (default)"
        } else {
            ""
        };
        println!("  {name}: {}x{}{marker}", p.width, p.height);
    }
    println!("\nchange default:  vstretch set-default");
    println!("list presets:    vstretch presets");
    Ok(())
}

fn cmd_config() -> Result<()> {
    let path = Config::config_path()?;
    println!("config: {}", path.display());

    if !path.exists() {
        println!("(missing — run `vstretch init`)");
        return Ok(());
    }

    let config = Config::load()?;
    println!("default_profile: {}", config.default_profile);
    println!("profiles:");
    for (name, p) in &config.profiles {
        let refresh = p
            .refresh
            .map(|r| format!(" @ {r}Hz"))
            .unwrap_or_else(|| " @ native Hz".into());
        let marker = if name == &config.default_profile {
            "  ← default"
        } else {
            ""
        };
        println!("  {name}: {}x{}{refresh}{marker}", p.width, p.height);
    }
    Ok(())
}

fn cmd_presets() -> Result<()> {
    println!("Popular stretch resolutions:\n");
    let mut current: Option<(u32, u32)> = None;
    if let Ok(cfg) = Config::load()
        && let Ok((_, p)) = cfg.get_profile(None)
    {
        current = Some((p.width, p.height));
    }

    for p in POPULAR_STRETCH {
        let marker = if current == Some((p.width, p.height)) {
            "  ← your default"
        } else {
            ""
        };
        println!("  {:12}  ({})  {}{}", p.name, p.aspect, p.note, marker);
    }

    println!("\nset default:  vstretch set-default");
    println!("             vstretch set-default 1440x1080");
    Ok(())
}

fn pick_popular_interactive(current: Option<(u32, u32)>) -> Result<(u32, u32)> {
    let items: Vec<String> = POPULAR_STRETCH
        .iter()
        .map(|p| {
            let mut label = p.label();
            if current == Some((p.width, p.height)) {
                label.push_str("  ← current default");
            }
            label
        })
        .collect();

    let default_idx = current
        .and_then(|(w, h)| {
            POPULAR_STRETCH
                .iter()
                .position(|p| p.width == w && p.height == h)
        })
        .unwrap_or_else(|| {
            POPULAR_STRETCH
                .iter()
                .position(|p| p.name == "1440x1080")
                .unwrap_or(0)
        });

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Pick default stretch resolution")
        .items(&items)
        .default(default_idx)
        .interact()
        .context("selection cancelled")?;

    let chosen = &POPULAR_STRETCH[selection];
    Ok((chosen.width, chosen.height))
}

fn cmd_set_default(resolution: Option<String>, profile: Option<String>) -> Result<()> {
    let mut config = Config::load_or_init()?;

    if let Some(name) = profile {
        config.set_default_profile(&name)?;
        let (_, p) = config.get_profile(Some(&name))?;
        println!(
            "✓ default profile → `{}` ({}x{})",
            name, p.width, p.height
        );
        return Ok(());
    }

    let (width, height) = if let Some(res) = resolution {
        if let Some(preset) = PopularPreset::find(&res) {
            (preset.width, preset.height)
        } else {
            PopularPreset::parse_res(&res)?
        }
    } else {
        let current = config
            .get_profile(None)
            .ok()
            .map(|(_, p)| (p.width, p.height));
        pick_popular_interactive(current)?
    };

    let name = config.set_default_resolution(width, height)?;
    println!("✓ default stretch → `{name}` ({width}x{height})");
    println!("  used by: vstretch auto / vstretch stretch");
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { force } => cmd_init(force)?,
        Commands::Config => cmd_config()?,
        Commands::Presets => cmd_presets()?,
        Commands::SetDefault {
            resolution,
            profile,
        } => cmd_set_default(resolution, profile)?,
        Commands::Native => {
            let (n_width, n_height, n_refresh) = get_native_resolution()?;
            change_resolution(n_width, n_height, n_refresh)?;
        }
        Commands::Stretch { profile } => {
            let config = Config::load().context("stretch needs a config")?;
            let (name, p) = config.get_profile(profile.as_deref())?;
            let (_, _, n_refresh) = get_native_resolution()?;
            apply_profile(name, p, n_refresh)?;
        }
        Commands::Auto { profile } => {
            let config = Config::load().context("auto needs a config")?;
            let (name, p) = config.get_profile(profile.as_deref())?;
            let (n_width, n_height, n_refresh) = get_native_resolution()?;
            let (c_width, c_height, _) = get_current_resolution()?;

            // Toggle: native → stretch profile, anything else → native
            if c_width == n_width && c_height == n_height {
                apply_profile(name, p, n_refresh)?;
            } else {
                change_resolution(n_width, n_height, n_refresh)?;
            }
        }
    }

    Ok(())
}
