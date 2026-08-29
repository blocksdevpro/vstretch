#![cfg_attr(not(windows), allow(dead_code, unused_imports))]

mod config;
mod display;
mod tui;

#[cfg(not(windows))]
compile_error!("vstretch is Windows-only (uses Win32 display APIs)");

use anyhow::{Context, Result};
use clap::Parser;
use config::Config;

/// Native ↔ stretch resolution switcher for FPS games (Valorant, CS2, …).
///
/// Run with no flags to open the TUI. Use `--auto` for a quiet hotkey toggle.
#[derive(Parser)]
#[command(name = "vstretch")]
#[command(about = "Native ↔ stretch resolution switcher for FPS (TUI)")]
#[command(
    after_help = "Examples:\n  vstretch          Open the TUI\n  vstretch --auto   Toggle resolution (for hotkeys)"
)]
struct Cli {
    /// Toggle native ↔ stretch without opening the TUI (for hotkeys)
    #[arg(short = 'a', long)]
    auto: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.auto {
        let config = Config::load().context("no config yet — run `vstretch` once to set up")?;
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

    tui::run()
}
