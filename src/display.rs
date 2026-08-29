use std::mem;

use anyhow::{Context, Result};
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
            ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE, EnumDisplaySettingsW,
        },
    },
    core::Error as WinError,
};

use crate::config::Profile;

/// Last-resort Hz when CCD and the profile both omit refresh.
pub const FALLBACK_REFRESH_HZ: u32 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    pub refresh: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeKind {
    Native,
    Stretch,
    Panel,
    Other,
    Unknown,
}

impl Mode {
    pub fn label(self) -> String {
        format!("{}x{} @ {}Hz", self.width, self.height, self.refresh)
    }

    pub fn size_eq(self, other: Mode) -> bool {
        self.width == other.width && self.height == other.height
    }

    pub fn matches_profile(self, p: &Profile) -> bool {
        self.width == p.width
            && self.height == p.height
            && p.refresh.is_none_or(|r| r == self.refresh)
    }
}

fn empty_devmode() -> DEVMODEW {
    DEVMODEW {
        dmSize: mem::size_of::<DEVMODEW>() as u16,
        ..Default::default()
    }
}

pub fn get_current_resolution() -> Result<Mode> {
    let mut devmode = empty_devmode();
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

/// Unique width×height@Hz modes Windows reports for the primary display.
pub fn list_display_modes() -> Vec<Mode> {
    use std::collections::BTreeSet;

    let mut seen = BTreeSet::new();
    let mut i = 0u32;
    loop {
        let mut devmode = empty_devmode();
        let ok = unsafe { EnumDisplaySettingsW(None, ENUM_DISPLAY_SETTINGS_MODE(i), &mut devmode) };
        if !ok.as_bool() {
            break;
        }
        i += 1;
        if devmode.dmPelsWidth == 0 || devmode.dmPelsHeight == 0 || devmode.dmDisplayFrequency == 0
        {
            continue;
        }
        seen.insert((
            devmode.dmPelsWidth,
            devmode.dmPelsHeight,
            devmode.dmDisplayFrequency,
        ));
    }

    let mut modes: Vec<Mode> = seen
        .into_iter()
        .map(|(width, height, refresh)| Mode {
            width,
            height,
            refresh,
        })
        .collect();
    modes.sort_by(|a, b| {
        b.height
            .cmp(&a.height)
            .then(b.width.cmp(&a.width))
            .then(b.refresh.cmp(&a.refresh))
    });
    modes
}

pub fn get_native_resolution() -> Result<Mode> {
    let mut path_count = 0u32;
    let mut mode_count = 0u32;

    let status = unsafe {
        GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
    };
    if status != ERROR_SUCCESS {
        return Err(WinError::from(status).into());
    }

    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
    let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];

    let status = unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(WinError::from(status).into());
    }

    paths.truncate(path_count as usize);
    modes.truncate(mode_count as usize);

    for path in &paths {
        if path.flags & DISPLAYCONFIG_PATH_ACTIVE == 0 {
            continue;
        }
        let target_mode_idx = unsafe { path.targetInfo.Anonymous.modeInfoIdx } as usize;
        let Some(mode) = modes.get(target_mode_idx) else {
            continue;
        };
        if mode.infoType != DISPLAYCONFIG_MODE_INFO_TYPE_TARGET {
            continue;
        }

        let target_mode = unsafe { mode.Anonymous.targetMode };
        let width = target_mode.targetVideoSignalInfo.activeSize.cx;
        let height = target_mode.targetVideoSignalInfo.activeSize.cy;
        let freq = target_mode.targetVideoSignalInfo.vSyncFreq;
        let refresh = freq
            .Numerator
            .checked_div(freq.Denominator)
            .unwrap_or(FALLBACK_REFRESH_HZ);

        return Ok(Mode {
            width,
            height,
            refresh,
        });
    }

    Err(WinError::from(ERROR_NOT_FOUND).into())
}

pub fn change_resolution(mode: Mode) -> Result<()> {
    let devmode = DEVMODEW {
        dmSize: mem::size_of::<DEVMODEW>() as u16,
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

pub fn apply_profile(profile: &Profile, panel_refresh: u32) -> Result<Mode> {
    let mode = Mode {
        width: profile.width,
        height: profile.height,
        refresh: profile.refresh.unwrap_or(panel_refresh),
    };
    change_resolution(mode)?;
    Ok(mode)
}

/// Restore target: saved override if set, otherwise the already-detected panel.
pub fn resolve_native(over: Option<&Profile>, panel: Option<Mode>) -> Result<Mode> {
    match over {
        Some(n) => Ok(Mode {
            width: n.width,
            height: n.height,
            refresh: n
                .refresh
                .or(panel.map(|p| p.refresh))
                .unwrap_or(FALLBACK_REFRESH_HZ),
        }),
        None => panel.context("could not detect panel native resolution"),
    }
}

pub fn stretch_refresh(panel: Option<Mode>, native: Option<Mode>) -> u32 {
    panel
        .map(|p| p.refresh)
        .or(native.map(|n| n.refresh))
        .unwrap_or(FALLBACK_REFRESH_HZ)
}

/// `native` is the desktop restore target. Stretch uses `panel_refresh`, not the override Hz.
pub fn toggle_stretch(profile: &Profile, native: Mode, panel_refresh: u32) -> Result<Mode> {
    let current = get_current_resolution()?;

    if current.size_eq(native) {
        apply_profile(profile, panel_refresh)
    } else {
        change_resolution(native)?;
        Ok(native)
    }
}

/// The single list row that represents a saved native override.
pub fn preferred_override_mode(
    modes: &[Mode],
    over: &Profile,
    panel: Option<Mode>,
) -> Option<Mode> {
    if over.refresh.is_some() {
        return modes.iter().copied().find(|m| m.matches_profile(over));
    }
    if let Some(p) = panel
        && let Some(m) = modes
            .iter()
            .copied()
            .find(|m| m.width == over.width && m.height == over.height && m.refresh == p.refresh)
    {
        return Some(m);
    }
    modes
        .iter()
        .copied()
        .find(|m| m.width == over.width && m.height == over.height)
}

pub fn synthetic_override_mode(over: &Profile, panel: Option<Mode>) -> Mode {
    Mode {
        width: over.width,
        height: over.height,
        refresh: over
            .refresh
            .or(panel.map(|p| p.refresh))
            .unwrap_or(FALLBACK_REFRESH_HZ),
    }
}

pub fn classify_mode(
    current: Option<Mode>,
    native: Option<Mode>,
    panel: Option<Mode>,
    stretch: Option<&Profile>,
) -> ModeKind {
    let Some(current) = current else {
        return ModeKind::Unknown;
    };
    if native.is_some_and(|n| current.size_eq(n)) {
        return ModeKind::Native;
    }
    if stretch.is_some_and(|p| current.width == p.width && current.height == p.height) {
        return ModeKind::Stretch;
    }
    if panel.is_some_and(|p| current.size_eq(p)) {
        return ModeKind::Panel;
    }
    ModeKind::Other
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Profile;

    fn mode(width: u32, height: u32, refresh: u32) -> Mode {
        Mode {
            width,
            height,
            refresh,
        }
    }

    #[test]
    fn matches_profile_ignores_refresh_when_unset() {
        let p = Profile::new(1920, 1080);
        assert!(mode(1920, 1080, 60).matches_profile(&p));
        assert!(mode(1920, 1080, 180).matches_profile(&p));
        assert!(!mode(2560, 1440, 180).matches_profile(&p));
    }

    #[test]
    fn matches_profile_requires_refresh_when_set() {
        let p = Profile {
            width: 1920,
            height: 1080,
            refresh: Some(180),
        };
        assert!(mode(1920, 1080, 180).matches_profile(&p));
        assert!(!mode(1920, 1080, 60).matches_profile(&p));
    }

    #[test]
    fn preferred_override_stars_panel_hz_when_refresh_omitted() {
        let modes = [
            mode(1920, 1080, 180),
            mode(1920, 1080, 60),
            mode(2560, 1440, 180),
        ];
        let over = Profile::new(1920, 1080);
        let panel = mode(2560, 1440, 180);
        assert_eq!(
            preferred_override_mode(&modes, &over, Some(panel)),
            Some(mode(1920, 1080, 180))
        );
    }

    #[test]
    fn preferred_override_uses_saved_refresh() {
        let modes = [mode(1920, 1080, 180), mode(1920, 1080, 60)];
        let over = Profile {
            width: 1920,
            height: 1080,
            refresh: Some(60),
        };
        assert_eq!(
            preferred_override_mode(&modes, &over, None),
            Some(mode(1920, 1080, 60))
        );
    }

    #[test]
    fn preferred_override_falls_back_to_highest_hz() {
        let modes = [mode(1920, 1080, 144), mode(1920, 1080, 60)];
        let over = Profile::new(1920, 1080);
        assert_eq!(
            preferred_override_mode(&modes, &over, Some(mode(2560, 1440, 240))),
            Some(mode(1920, 1080, 144))
        );
    }

    #[test]
    fn resolve_native_override_keeps_panel_refresh_when_omitted() {
        let over = Profile::new(1920, 1080);
        let panel = mode(2560, 1440, 180);
        assert_eq!(
            resolve_native(Some(&over), Some(panel)).unwrap(),
            mode(1920, 1080, 180)
        );
    }

    #[test]
    fn resolve_native_without_override_is_panel() {
        let panel = mode(2560, 1440, 180);
        assert_eq!(resolve_native(None, Some(panel)).unwrap(), panel);
        assert!(resolve_native(None, None).is_err());
    }

    #[test]
    fn classify_native_stretch_panel_other() {
        let panel = mode(2560, 1440, 180);
        let native = mode(1920, 1080, 180);
        let stretch = Profile::new(1440, 1080);
        assert_eq!(
            classify_mode(Some(native), Some(native), Some(panel), Some(&stretch)),
            ModeKind::Native
        );
        assert_eq!(
            classify_mode(
                Some(mode(1440, 1080, 180)),
                Some(native),
                Some(panel),
                Some(&stretch)
            ),
            ModeKind::Stretch
        );
        assert_eq!(
            classify_mode(Some(panel), Some(native), Some(panel), Some(&stretch)),
            ModeKind::Panel
        );
        assert_eq!(
            classify_mode(
                Some(mode(1280, 960, 60)),
                Some(native),
                Some(panel),
                Some(&stretch)
            ),
            ModeKind::Other
        );
        assert_eq!(
            classify_mode(None, Some(native), Some(panel), Some(&stretch)),
            ModeKind::Unknown
        );
    }

    #[test]
    fn classify_panel_as_native_when_no_override() {
        let panel = mode(2560, 1440, 180);
        let stretch = Profile::new(1440, 1080);
        assert_eq!(
            classify_mode(Some(panel), Some(panel), Some(panel), Some(&stretch)),
            ModeKind::Native
        );
    }
}
