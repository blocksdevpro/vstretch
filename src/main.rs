use clap::Parser;
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
    core::{Error, Result},
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
    Auto,
    Native,
    Stretch,
}

const STRETCH_WIDTH: u32 = 1440;
const STRETCH_HEIGHT: u32 = 1080;

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
        unsafe { Err(Error::from(windows::Win32::Foundation::GetLastError())) }
    }
}

fn get_native_resolution() -> Result<(u32, u32, u32)> {
    unsafe {
        let mut path_count = 0u32;
        let mut mode_count = 0u32;

        let status =
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count);

        if status != ERROR_SUCCESS {
            return Err(Error::from(status));
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
            return Err(Error::from(status));
        }

        // Find the primary active path
        for path in &paths {
            if path.flags & DISPLAYCONFIG_PATH_ACTIVE != 0 {
                let target_mode_idx = path.targetInfo.Anonymous.modeInfoIdx as usize;

                if target_mode_idx < modes.len() {
                    let mode = &modes[target_mode_idx];

                    if mode.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_TARGET {
                        let target_mode = mode.Anonymous.targetMode;

                        let width = target_mode.targetVideoSignalInfo.activeSize.cx as u32;
                        let height = target_mode.targetVideoSignalInfo.activeSize.cy as u32;

                        let refresh =
                            if target_mode.targetVideoSignalInfo.vSyncFreq.Denominator != 0 {
                                target_mode.targetVideoSignalInfo.vSyncFreq.Numerator
                                    / target_mode.targetVideoSignalInfo.vSyncFreq.Denominator
                            } else {
                                60
                            };

                        return Ok((width, height, refresh));
                    }
                }
            }
        }

        Err(Error::from(ERROR_NOT_FOUND))
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
        unsafe { Err(Error::from(windows::Win32::Foundation::GetLastError())) }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Auto => {
            let (n_width, n_height, n_refresh) = get_native_resolution()?;
            let (c_width, c_height, c_refresh) = get_current_resolution()?;
            println!("N: {}, {}, {}", n_width, n_height, n_refresh);
            println!("C: {}, {}, {}", c_width, c_height, c_refresh);

            if (c_width, c_height, c_refresh) == (n_width, n_height, n_refresh) {
                // Change resolution to stretch resolution;
                change_resolution(STRETCH_WIDTH, STRETCH_HEIGHT, n_refresh)?;
            } else {
                // Change resolution to native resolution;

                change_resolution(n_width, n_height, n_refresh)?;
            }
        }
        Commands::Native => {
            let (n_width, n_height, n_refresh) = get_native_resolution()?;

            change_resolution(n_width, n_height, n_refresh)?;
        }
        Commands::Stretch => {
            let (_, _, n_refresh) = get_native_resolution()?;
            change_resolution(STRETCH_WIDTH, STRETCH_HEIGHT, n_refresh)?;
        }
    }

    Ok(())
}
