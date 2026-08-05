use anyhow::Result;
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

use crate::config::Profile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    pub refresh: u32,
}

impl Mode {
    pub fn label(self) -> String {
        format!("{}x{} @ {}Hz", self.width, self.height, self.refresh)
    }

    pub fn size_eq(self, other: Mode) -> bool {
        self.width == other.width && self.height == other.height
    }
}

pub fn get_current_resolution() -> Result<Mode> {
    let mut devmode = DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    let result = unsafe { EnumDisplaySettingsW(None, ENUM_CURRENT_SETTINGS, &mut devmode) };

    if result.as_bool() {
        Ok(Mode {
            width: devmode.dmPelsWidth,
            height: devmode.dmPelsHeight,
            refresh: devmode.dmDisplayFrequency,
        })
    } else {
        Err(WinError::from(unsafe { windows::Win32::Foundation::GetLastError() }).into())
    }
}

pub fn get_native_resolution() -> Result<Mode> {
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
                            .checked_div(target_mode.targetVideoSignalInfo.vSyncFreq.Denominator)
                            .unwrap_or(60);

                        return Ok(Mode {
                            width,
                            height,
                            refresh,
                        });
                    }
                }
            }
        }

        Err(WinError::from(ERROR_NOT_FOUND).into())
    }
}

pub fn change_resolution(mode: Mode) -> Result<()> {
    let devmode = DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        dmFields: DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY,
        dmPelsWidth: mode.width,
        dmPelsHeight: mode.height,
        dmDisplayFrequency: mode.refresh,
        ..Default::default()
    };
    let result =
        unsafe { ChangeDisplaySettingsExW(None, Some(&devmode), None, CDS_UPDATEREGISTRY, None) };
    if result == DISP_CHANGE_SUCCESSFUL {
        Ok(())
    } else {
        Err(WinError::from(unsafe { windows::Win32::Foundation::GetLastError() }).into())
    }
}

pub fn apply_profile(profile: &Profile, native_refresh: u32) -> Result<Mode> {
    let mode = Mode {
        width: profile.width,
        height: profile.height,
        refresh: profile.refresh.unwrap_or(native_refresh),
    };
    change_resolution(mode)?;
    Ok(mode)
}

pub fn toggle_stretch(profile: &Profile) -> Result<Mode> {
    let native = get_native_resolution()?;
    let current = get_current_resolution()?;

    if current.size_eq(native) {
        apply_profile(profile, native.refresh)
    } else {
        change_resolution(native)?;
        Ok(native)
    }
}
