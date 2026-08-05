use clap;
use clap::Parser;
use windows::{
    Win32::Graphics::Gdi::{
        CDS_UPDATEREGISTRY, ChangeDisplaySettingsExW, DEVMODEW, DISP_CHANGE_SUCCESSFUL,
        DM_DISPLAYFREQUENCY, DM_PELSHEIGHT, DM_PELSWIDTH,
    },
    core::Error,
    core::Result,
};

#[derive(clap::Parser)]
#[command(name = "vstretch")]
#[command(about = "Valorant resolution switcher for stretch res,")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    Auto {
        #[arg(long, default_value_t = 1440)]
        width: u32,
        #[arg(long, default_value_t = 1080)]
        height: u32,
        #[arg(long, default_value_t = 180)]
        refresh: u32,
    },
    Stretch {
        #[arg(long, default_value_t = 1440)]
        width: u32,
        #[arg(long, default_value_t = 1080)]
        height: u32,
        #[arg(long, default_value_t = 180)]
        refresh: u32,
    },
    Native {
        #[arg(long, default_value_t = 2560)]
        width: u32,
        #[arg(long, default_value_t = 1440)]
        height: u32,
        #[arg(long, default_value_t = 180)]
        refresh: u32,
    },
}

fn change_resolution(width: u32, height: u32, refresh: u32) -> Result<()> {
    let mut devmode = DEVMODEW {
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
        unsafe { Err(Error::from(windows::Win32::Foundation::GetLastError())) }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Auto {
            width,
            height,
            refresh,
        } => {
            change_resolution(width, height, refresh)?;
            println!("Done. Restored native resolution.");
        }
        Commands::Stretch {
            width,
            height,
            refresh,
        } => {
            change_resolution(width, height, refresh)?;
        }
        Commands::Native {
            width,
            height,
            refresh,
        } => {
            change_resolution(width, height, refresh)?;
        }
    }

    Ok(())
}
