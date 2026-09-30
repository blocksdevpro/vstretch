#![cfg_attr(not(windows), allow(dead_code, unused_imports))]
#![cfg_attr(windows, windows_subsystem = "windows")]

//! Windows tray app with terminal and command-line controls for stretched displays.

mod autostretch;
mod config;
mod display;
mod platform;
mod startup;
mod tray;
mod tui;
mod update;

#[cfg(not(windows))]
compile_error!("vstretch is Windows-only (uses Win32 display APIs)");

use anyhow::{Context, Result};
use clap::Parser;
use config::Config;

/// Native ↔ stretch resolution switcher for FPS games (Valorant, CS2, …).
///
/// Run with no flags for the tray menu. Use `--tui` for the terminal interface.
#[derive(Parser)]
#[command(name = "vstretch", version)]
#[command(about = "Native ↔ stretch resolution switcher for FPS games")]
#[command(
    after_help = "Examples:\n  vstretch          Run in the system tray\n  vstretch --tui    Open the terminal interface\n  vstretch --auto   Toggle resolution (for hotkeys)"
)]
struct Cli {
    /// Open the terminal interface instead of the system tray
    #[arg(long, conflicts_with_all = ["auto", "check_update", "update"])]
    tui: bool,

    /// Toggle native ↔ stretch without opening the TUI (for hotkeys)
    #[arg(short = 'a', long, conflicts_with_all = ["check_update", "update"])]
    auto: bool,

    /// Check GitHub for a newer stable release
    #[arg(long, conflicts_with = "update")]
    check_update: bool,

    /// Download, verify, and install the latest stable release
    #[arg(long)]
    update: bool,
}

fn main() {
    // A GUI-subsystem executable never flashes a console on double-click.
    // Attach before parsing so --help, --version, and errors still reach a terminal.
    let has_arguments = std::env::args_os().len() > 1;
    if has_arguments {
        platform::attach_console();
    }
    let cli = Cli::parse();
    if cli.tui
        && let Err(error) = platform::ensure_console()
    {
        platform::show_error(&format!("{error:#}"));
        std::process::exit(1);
    }
    if let Err(error) = run(cli) {
        if has_arguments {
            eprintln!("vstretch: {error:#}");
        } else {
            platform::show_error(&format!("{error:#}"));
        }
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    if cli.check_update || cli.update {
        match update::check()? {
            Some(release) if cli.update => {
                println!("Installing vstretch {}...", release.version);
                update::install(&release)?;
                println!("Updated to {}. Reopen vstretch to use it.", release.version);
            }
            Some(release) => println!(
                "vstretch {} is available. Run vstretch --update to install it.",
                release.version
            ),
            None => println!("vstretch {} is up to date.", env!("CARGO_PKG_VERSION")),
        }
        return Ok(());
    }

    if cli.auto {
        let config = Config::load().context("load configuration for the display toggle")?;
        let panel = display::get_native_resolution().ok();
        let native = display::resolve_native(config.native.as_ref(), panel)?;
        let mode = display::toggle_stretch(
            &config.stretch,
            native,
            display::stretch_refresh(panel, Some(native)),
        )?;
        println!("{} [{}]", mode.label(), config.stretch.name());
        return Ok(());
    }

    if cli.tui { tui::run() } else { tray::run() }
}
