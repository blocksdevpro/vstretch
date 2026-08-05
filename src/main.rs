#![cfg_attr(not(windows), allow(dead_code, unused_imports))]

mod config;
mod display;
mod tui;

#[cfg(not(windows))]
compile_error!("vstretch is Windows-only (uses Win32 display APIs)");

use anyhow::{Context, Result};
use clap::Parser;
use config::{Config, POPULAR_STRETCH, PopularPreset};

#[derive(Parser)]
#[command(name = "vstretch")]
#[command(about = "Valorant resolution switcher — run with no args for the TUI")]
#[command(after_help = "Tip: run `vstretch` with no command to open the interactive TUI.\nHotkeys: bind `vstretch auto` for one-shot toggle.")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Open the interactive TUI (same as no args)
    Ui,
    /// Toggle between native and stretch (quiet, for hotkeys)
    Auto {
        #[arg(short, long)]
        profile: Option<String>,
    },
    /// Switch to native panel resolution (quiet)
    Native,
    /// Switch to a stretch profile (quiet)
    Stretch {
        #[arg(short, long)]
        profile: Option<String>,
    },
    /// Create a default config file if missing
    Init {
        #[arg(long)]
        force: bool,
    },
    /// Print config path and profiles
    Config,
    /// Print built-in popular stretch resolutions
    Presets,
    /// Set the default stretch resolution
    SetDefault {
        /// Resolution like `1440x1080`. Omit to open the TUI picker.
        resolution: Option<String>,
        #[arg(short, long, conflicts_with = "resolution")]
        profile: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None | Some(Commands::Ui) => tui::run(),
        Some(Commands::Init { force }) => cmd_init(force),
        Some(Commands::Config) => cmd_config(),
        Some(Commands::Presets) => cmd_presets(),
        Some(Commands::SetDefault {
            resolution: None,
            profile: None,
        }) => tui::run(),
        Some(Commands::SetDefault {
            resolution,
            profile,
        }) => cmd_set_default(resolution, profile),
        Some(Commands::Native) => {
            let native = display::get_native_resolution()?;
            display::change_resolution(native)?;
            println!("{}", native.label());
            Ok(())
        }
        Some(Commands::Stretch { profile }) => {
            let config = Config::load().context("stretch needs a config — run `vstretch` once")?;
            let (name, p) = config.get_profile(profile.as_deref())?;
            let native = display::get_native_resolution()?;
            let mode = display::apply_profile(p, native.refresh)?;
            println!("{} [{}]", mode.label(), name);
            Ok(())
        }
        Some(Commands::Auto { profile }) => {
            let config = Config::load().context("auto needs a config — run `vstretch` once")?;
            let (name, p) = config.get_profile(profile.as_deref())?;
            let mode = display::toggle_stretch(p)?;
            println!("{} [{}]", mode.label(), name);
            Ok(())
        }
    }
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
    println!("default: {}", config.default_profile);
    println!("run `vstretch` to open the TUI");
    Ok(())
}

fn cmd_config() -> Result<()> {
    let path = Config::config_path()?;
    println!("{}", path.display());
    if !path.exists() {
        println!("(missing — run `vstretch` or `vstretch init`)");
        return Ok(());
    }
    let config = Config::load()?;
    println!("default: {}", config.default_profile);
    for (name, p) in &config.profiles {
        let mark = if name == &config.default_profile {
            " *"
        } else {
            ""
        };
        println!("  {name}: {}x{}{mark}", p.width, p.height);
    }
    Ok(())
}

fn cmd_presets() -> Result<()> {
    let current = Config::load().ok().and_then(|c| {
        c.get_profile(None)
            .ok()
            .map(|(_, p)| (p.width, p.height))
    });

    for p in POPULAR_STRETCH {
        let mark = if current == Some((p.width, p.height)) {
            " *"
        } else {
            ""
        };
        println!("{:12}  {:5}  {}{}", p.name, p.aspect, p.note, mark);
    }
    Ok(())
}

fn cmd_set_default(resolution: Option<String>, profile: Option<String>) -> Result<()> {
    let mut config = Config::load_or_init()?;

    if let Some(name) = profile {
        config.set_default_profile(&name)?;
        let (_, p) = config.get_profile(Some(&name))?;
        println!("{}x{} [{}]", p.width, p.height, name);
        return Ok(());
    }

    let res = resolution.context("internal: resolution required")?;
    let (width, height) = if let Some(preset) = PopularPreset::find(&res) {
        (preset.width, preset.height)
    } else {
        PopularPreset::parse_res(&res)?
    };

    let name = config.set_default_resolution(width, height)?;
    println!("{width}x{height} [{name}]");
    Ok(())
}
