//! Queries and changes the primary display through the Windows display APIs.

use std::mem;

use anyhow::{Context, Result, bail};
use windows::{
    Win32::{
        Devices::Display::{
            DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_HEADER,
            DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_MODE_INFO_TYPE_TARGET, DISPLAYCONFIG_PATH_INFO,
            DISPLAYCONFIG_SOURCE_DEVICE_NAME, DisplayConfigGetDeviceInfo,
            GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
        },
        Foundation::{ERROR_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR},
        Graphics::Gdi::{
            CDS_UPDATEREGISTRY, ChangeDisplaySettingsExW, DEVMODE_DISPLAY_FIXED_OUTPUT, DEVMODEW,
            DISP_CHANGE, DISP_CHANGE_BADDUALVIEW, DISP_CHANGE_BADFLAGS, DISP_CHANGE_BADMODE,
            DISP_CHANGE_BADPARAM, DISP_CHANGE_FAILED, DISP_CHANGE_NOTUPDATED, DISP_CHANGE_RESTART,
            DISP_CHANGE_SUCCESSFUL, DISPLAY_DEVICE_PRIMARY_DEVICE, DISPLAY_DEVICEW,
            DISPLAYCONFIG_PATH_ACTIVE, DM_DISPLAYFIXEDOUTPUT, DM_DISPLAYFREQUENCY, DM_PELSHEIGHT,
            DM_PELSWIDTH, DMDFO_STRETCH, ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE,
            EnumDisplayDevicesW, EnumDisplaySettingsW,
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
    // SAFETY: dmSize describes the writable DEVMODEW passed to Windows.
    let result = unsafe { EnumDisplaySettingsW(None, ENUM_CURRENT_SETTINGS, &mut devmode) };

    if result.as_bool() {
        Ok(Mode {
            width: devmode.dmPelsWidth,
            height: devmode.dmPelsHeight,
            refresh: devmode.dmDisplayFrequency,
        })
    } else {
        bail!("Windows could not read the primary display's current resolution")
    }
}

/// Unique width×height@Hz modes Windows reports for the primary display.
pub fn list_display_modes() -> Vec<Mode> {
    use std::collections::BTreeSet;

    let mut seen = BTreeSet::new();
    let mut i = 0u32;
    loop {
        let mut devmode = empty_devmode();
        // SAFETY: the buffer is initialized with its size before each query.
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

fn primary_display_name() -> Result<[u16; 32]> {
    let mut index = 0;
    loop {
        let mut device = DISPLAY_DEVICEW {
            cb: mem::size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        // SAFETY: cb describes the writable DISPLAY_DEVICEW buffer.
        if !unsafe { EnumDisplayDevicesW(None, index, &mut device, 0) }.as_bool() {
            bail!("Windows did not report a primary display");
        }
        if device.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE != Default::default() {
            return Ok(device.DeviceName);
        }
        index += 1;
    }
}

/// Reads the primary panel's output signal rather than the scaled desktop size.
pub fn get_native_resolution() -> Result<Mode> {
    let primary_name = primary_display_name()?;
    let mut path_count = 0u32;
    let mut mode_count = 0u32;

    // SAFETY: both count pointers refer to initialized, writable values.
    let status = unsafe {
        GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
    };
    if status != ERROR_SUCCESS {
        return Err(WinError::from(status).into());
    }

    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
    let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];

    // SAFETY: the buffers have the capacities reported by Windows. A topology
    // change can make the query fail, but Windows stays within those capacities.
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
        // CCD path order does not identify the primary display. Match the GDI
        // source used by EnumDisplaySettingsW/ChangeDisplaySettingsExW(None).
        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                size: mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                adapterId: path.sourceInfo.adapterId,
                id: path.sourceInfo.id,
            },
            ..Default::default()
        };
        // SAFETY: header is the first field of this repr(C) structure, and its
        // size and type tell Windows to fill a DISPLAYCONFIG_SOURCE_DEVICE_NAME.
        let status = WIN32_ERROR(unsafe { DisplayConfigGetDeviceInfo(&mut source.header) } as u32);
        if status != ERROR_SUCCESS {
            return Err(WinError::from(status).into());
        }
        if source.viewGdiDeviceName != primary_name {
            continue;
        }
        // SAFETY: without QDC_VIRTUAL_MODE_AWARE, Windows uses modeInfoIdx.
        let target_mode_idx = unsafe { path.targetInfo.Anonymous.modeInfoIdx } as usize;
        let Some(mode) = modes.get(target_mode_idx) else {
            continue;
        };
        if mode.infoType != DISPLAYCONFIG_MODE_INFO_TYPE_TARGET {
            continue;
        }

        // SAFETY: infoType was checked before reading the target-mode union.
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

fn resolution_devmode(mode: Mode, scaling: Option<DEVMODE_DISPLAY_FIXED_OUTPUT>) -> DEVMODEW {
    let mut devmode = DEVMODEW {
        dmSize: mem::size_of::<DEVMODEW>() as u16,
        dmFields: DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY,
        dmPelsWidth: mode.width,
        dmPelsHeight: mode.height,
        dmDisplayFrequency: mode.refresh,
        ..Default::default()
    };
    if let Some(scaling) = scaling {
        devmode.dmFields |= DM_DISPLAYFIXEDOUTPUT;
        devmode.Anonymous1.Anonymous2.dmDisplayFixedOutput = scaling;
    }
    devmode
}

fn check_display_change(result: DISP_CHANGE, mode: Mode) -> Result<()> {
    let reason = match result {
        DISP_CHANGE_SUCCESSFUL => return Ok(()),
        DISP_CHANGE_BADMODE => "resolution or refresh rate is not supported",
        DISP_CHANGE_FAILED => "display driver rejected the mode",
        DISP_CHANGE_RESTART => "restart required to apply the mode",
        DISP_CHANGE_NOTUPDATED => "Windows could not save the display settings",
        DISP_CHANGE_BADFLAGS => "invalid display change flags",
        DISP_CHANGE_BADPARAM => "invalid display change parameters",
        DISP_CHANGE_BADDUALVIEW => "mode is incompatible with DualView",
        _ => "unknown display change error",
    };
    bail!("{}: {reason} (DISP_CHANGE {})", mode.label(), result.0)
}

pub fn change_resolution(mode: Mode) -> Result<()> {
    change_resolution_with_scaling(mode, None)
}

fn change_resolution_with_scaling(
    mode: Mode,
    scaling: Option<DEVMODE_DISPLAY_FIXED_OUTPUT>,
) -> Result<()> {
    let devmode = resolution_devmode(mode, scaling);
    // SAFETY: dmSize and dmFields describe the initialized settings, and Windows
    // borrows the structure only for this call.
    let result =
        unsafe { ChangeDisplaySettingsExW(None, Some(&devmode), None, CDS_UPDATEREGISTRY, None) };
    check_display_change(result, mode)
}

pub fn apply_profile(profile: &Profile, panel_refresh: u32) -> Result<Mode> {
    let mode = Mode {
        width: profile.width,
        height: profile.height,
        refresh: profile.refresh.unwrap_or(panel_refresh),
    };
    // A 4:3 resolution alone can preserve its aspect ratio and leave side bars.
    change_resolution_with_scaling(mode, Some(DMDFO_STRETCH))?;
    Ok(mode)
}

/// Restore target: saved override if set, otherwise the already-detected panel.
pub fn resolve_native(override_profile: Option<&Profile>, panel: Option<Mode>) -> Result<Mode> {
    match override_profile {
        Some(profile) => Ok(native_override_mode(profile, panel)),
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
    override_profile: &Profile,
    panel: Option<Mode>,
) -> Option<Mode> {
    if override_profile.refresh.is_some() {
        return modes
            .iter()
            .copied()
            .find(|mode| mode.matches_profile(override_profile));
    }
    if let Some(panel) = panel
        && let Some(mode) = modes
            .iter()
            .copied()
            .find(|mode| mode.matches_profile(override_profile) && mode.refresh == panel.refresh)
    {
        return Some(mode);
    }
    modes
        .iter()
        .copied()
        .filter(|mode| mode.matches_profile(override_profile))
        .max_by_key(|mode| mode.refresh)
}

/// Uses the panel refresh rate when the saved override leaves it unspecified.
pub fn native_override_mode(override_profile: &Profile, panel: Option<Mode>) -> Mode {
    Mode {
        width: override_profile.width,
        height: override_profile.height,
        refresh: override_profile
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

    #[test]
    #[ignore = "requires a local display supporting 1280x960; does not change display settings"]
    fn primary_stretch_mode_passes_driver_validation() -> Result<()> {
        use windows::Win32::Graphics::Gdi::CDS_TEST;

        let current = get_current_resolution()?;
        let panel = get_native_resolution()?;
        let stretch = mode(1280, 960, stretch_refresh(Some(panel), Some(current)));
        println!(
            "Current: {}; primary panel: {}",
            current.label(),
            panel.label()
        );
        assert!(list_display_modes().contains(&stretch));
        let devmode = resolution_devmode(stretch, Some(DMDFO_STRETCH));
        let result =
            unsafe { ChangeDisplaySettingsExW(None, Some(&devmode), None, CDS_TEST, None) };
        check_display_change(result, stretch)?;
        assert_eq!(get_current_resolution()?, current);
        println!("Driver accepts {}", stretch.label());
        Ok(())
    }

    #[test]
    #[ignore = "tests 1280x960 and 1440x1080 full-screen scaling, restoring the original mode"]
    fn primary_stretch_toggle_round_trip() -> Result<()> {
        let original = get_current_resolution()?;
        let panel = get_native_resolution()?;
        let refresh = stretch_refresh(Some(panel), Some(original));
        let result = (|| -> Result<()> {
            for profile in [Profile::new(1280, 960), Profile::new(1440, 1080)] {
                let applied = toggle_stretch(&profile, original, refresh)?;
                anyhow::ensure!(
                    applied.matches_profile(&profile),
                    "toggle did not select stretch"
                );
                anyhow::ensure!(
                    get_current_resolution()? == applied,
                    "stretch was not applied"
                );
                let mut active = empty_devmode();
                anyhow::ensure!(
                    unsafe { EnumDisplaySettingsW(None, ENUM_CURRENT_SETTINGS, &mut active) }
                        .as_bool(),
                    "could not read active display scaling"
                );
                anyhow::ensure!(
                    unsafe { active.Anonymous1.Anonymous2.dmDisplayFixedOutput } == DMDFO_STRETCH,
                    "driver did not apply full-screen stretch scaling"
                );
                println!("Applied {} with full-screen scaling", applied.label());
                let restored = toggle_stretch(&profile, original, refresh)?;
                anyhow::ensure!(restored == original, "toggle did not select native");
                anyhow::ensure!(
                    get_current_resolution()? == original,
                    "native was not restored"
                );
                println!("Restored {}", original.label());
            }
            Ok(())
        })();
        // Restore even if applying or verifying either half of the toggle fails.
        change_resolution(original).context("restore original display after test")?;
        result
    }

    fn mode(width: u32, height: u32, refresh: u32) -> Mode {
        Mode {
            width,
            height,
            refresh,
        }
    }

    #[test]
    fn display_change_success_is_not_an_error() {
        assert!(check_display_change(DISP_CHANGE_SUCCESSFUL, mode(1280, 960, 180)).is_ok());
    }

    #[test]
    fn display_change_errors_use_return_code_and_requested_mode() {
        let requested = mode(1280, 960, 164);
        let cases = [
            (DISP_CHANGE_BADMODE, "not supported"),
            (DISP_CHANGE_FAILED, "driver rejected"),
            (DISP_CHANGE_RESTART, "restart required"),
            (DISP_CHANGE_NOTUPDATED, "could not save"),
            (DISP_CHANGE_BADFLAGS, "invalid display change flags"),
            (DISP_CHANGE_BADPARAM, "invalid display change parameters"),
            (DISP_CHANGE_BADDUALVIEW, "DualView"),
            (DISP_CHANGE(-99), "unknown display change error"),
        ];
        for (result, reason) in cases {
            let message = check_display_change(result, requested)
                .unwrap_err()
                .to_string();
            assert!(message.contains(&requested.label()), "{message}");
            assert!(message.contains(reason), "{message}");
            assert!(
                message.contains(&format!("DISP_CHANGE {}", result.0)),
                "{message}"
            );
            assert!(!message.contains("completed successfully"), "{message}");
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
        // Driver or saved-mode ordering should not determine the chosen refresh rate.
        let modes = [
            mode(1920, 1080, 60),
            mode(1920, 1080, 144),
            mode(1920, 1080, 120),
        ];
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
