//! Raw Windows calls that read and change the primary display.
//!
//! Everything here talks to GDI or CCD directly. Policy (which mode to apply,
//! crash recovery) lives in the parent module and in `recovery`.

use std::mem;

use anyhow::{Result, bail};
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
            CDS_TYPE, CDS_UPDATEREGISTRY, ChangeDisplaySettingsExW, DEVMODE_DISPLAY_FIXED_OUTPUT,
            DEVMODEW, DISP_CHANGE, DISP_CHANGE_BADDUALVIEW, DISP_CHANGE_BADFLAGS,
            DISP_CHANGE_BADMODE, DISP_CHANGE_BADPARAM, DISP_CHANGE_FAILED, DISP_CHANGE_NOTUPDATED,
            DISP_CHANGE_RESTART, DISP_CHANGE_SUCCESSFUL, DISPLAY_DEVICE_ACTIVE,
            DISPLAY_DEVICE_PRIMARY_DEVICE, DISPLAY_DEVICEW, DISPLAYCONFIG_PATH_ACTIVE,
            DM_DISPLAYFIXEDOUTPUT, DM_DISPLAYFREQUENCY, DM_PELSHEIGHT, DM_PELSWIDTH, DMDFO_STRETCH,
            ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE, ENUM_REGISTRY_SETTINGS,
            EnumDisplayDevicesW, EnumDisplaySettingsW,
        },
        UI::WindowsAndMessaging::EDD_GET_DEVICE_INTERFACE_NAME,
    },
    core::{Error as WinError, PCWSTR},
};

use super::{DesktopSnapshot, FALLBACK_REFRESH_HZ, Mode};

/// Full-screen scaling, so 4:3 modes fill a 16:9 panel instead of leaving bars.
pub(super) const STRETCH_SCALING: u32 = DMDFO_STRETCH.0;

fn mode_from_devmode(devmode: &DEVMODEW) -> Mode {
    Mode {
        width: devmode.dmPelsWidth,
        height: devmode.dmPelsHeight,
        refresh: devmode.dmDisplayFrequency,
    }
}

/// Identifies the primary monitor and its exact settings for crash recovery.
pub fn desktop_snapshot() -> Result<DesktopSnapshot> {
    let name = primary_display_name()?;
    let mut monitors = Vec::new();
    let mut index = 0;
    loop {
        let mut device = DISPLAY_DEVICEW {
            cb: mem::size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        // SAFETY: name is nul-terminated; cb describes the writable device buffer.
        if !unsafe {
            EnumDisplayDevicesW(
                PCWSTR(name.as_ptr()),
                index,
                &mut device,
                EDD_GET_DEVICE_INTERFACE_NAME,
            )
        }
        .as_bool()
        {
            break;
        }
        if device.StateFlags & DISPLAY_DEVICE_ACTIVE != Default::default() {
            let end = device
                .DeviceID
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(device.DeviceID.len());
            let id = String::from_utf16_lossy(&device.DeviceID[..end]);
            if !id.is_empty() {
                monitors.push(id);
            }
        }
        index += 1;
    }
    anyhow::ensure!(
        !monitors.is_empty(),
        "could not identify the primary monitor for crash recovery"
    );
    // Clone displays can share a source; changing that set invalidates recovery.
    monitors.sort();
    let mut devmode = empty_devmode();
    // SAFETY: name and the correctly sized output buffer remain valid for the call.
    anyhow::ensure!(
        unsafe { EnumDisplaySettingsW(PCWSTR(name.as_ptr()), ENUM_CURRENT_SETTINGS, &mut devmode) }
            .as_bool(),
        "could not capture the desktop for crash recovery"
    );
    let scaling = if devmode.dmFields.contains(DM_DISPLAYFIXEDOUTPUT) {
        // SAFETY: the display setting's field flag identifies this union member.
        Some(unsafe { devmode.Anonymous1.Anonymous2.dmDisplayFixedOutput }.0)
    } else {
        None
    };
    Ok(DesktopSnapshot {
        monitor: monitors.join("\n"),
        mode: mode_from_devmode(&devmode),
        scaling,
    })
}

/// The mode Windows restores at sign-in, from the registry.
pub fn saved_mode() -> Result<Mode> {
    let mut devmode = empty_devmode();
    // SAFETY: dmSize describes the writable output buffer.
    anyhow::ensure!(
        unsafe { EnumDisplaySettingsW(None, ENUM_REGISTRY_SETTINGS, &mut devmode) }.as_bool(),
        "could not read the saved Windows display mode"
    );
    Ok(mode_from_devmode(&devmode))
}

fn empty_devmode() -> DEVMODEW {
    DEVMODEW {
        dmSize: mem::size_of::<DEVMODEW>() as u16,
        ..Default::default()
    }
}

/// The primary display's active desktop mode.
pub fn current_mode() -> Result<Mode> {
    let mut devmode = empty_devmode();
    // SAFETY: dmSize describes the writable DEVMODEW passed to Windows.
    if !unsafe { EnumDisplaySettingsW(None, ENUM_CURRENT_SETTINGS, &mut devmode) }.as_bool() {
        bail!("Windows could not read the primary display's current resolution");
    }
    Ok(mode_from_devmode(&devmode))
}

/// Unique width×height@Hz modes Windows reports for the primary display.
pub fn available_modes() -> Vec<Mode> {
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
///
/// This is the panel's real resolution even while a stretch mode is active.
pub fn panel_mode() -> Result<Mode> {
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

/// Applies a mode to the primary display. Callers go through `recovery` so the
/// change is journaled first.
pub fn apply_mode(mode: Mode, scaling: Option<u32>, save_default: bool) -> Result<()> {
    let scaling = scaling.map(DEVMODE_DISPLAY_FIXED_OUTPUT);
    let devmode = resolution_devmode(mode, scaling);
    let flags = if save_default {
        CDS_UPDATEREGISTRY
    } else {
        CDS_TYPE(0)
    };
    // SAFETY: dmSize and dmFields describe the initialized settings, and Windows
    // borrows the structure only for this call.
    let result = unsafe { ChangeDisplaySettingsExW(None, Some(&devmode), None, flags, None) };
    check_display_change(result, mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Profile, display};
    use anyhow::Context;

    #[test]
    fn recovery_snapshot_identifies_the_real_primary_monitor() -> Result<()> {
        let snapshot = desktop_snapshot()?;
        assert!(!snapshot.monitor.is_empty());
        assert_eq!(snapshot.mode, current_mode()?);
        assert!(snapshot.scaling.is_none_or(|value| value <= 2));
        let saved = saved_mode()?;
        assert!(saved.width > 0 && saved.height > 0 && saved.refresh > 0);
        Ok(())
    }

    #[test]
    #[ignore = "requires a local display supporting 1280x960; does not change display settings"]
    fn primary_stretch_mode_passes_driver_validation() -> Result<()> {
        use windows::Win32::Graphics::Gdi::CDS_TEST;

        let current = current_mode()?;
        let panel = panel_mode()?;
        let stretch = mode(
            1280,
            960,
            display::stretch_refresh(Some(panel), Some(current)),
        );
        println!(
            "Current: {}; primary panel: {}",
            current.label(),
            panel.label()
        );
        assert!(available_modes().contains(&stretch));
        let devmode = resolution_devmode(stretch, Some(DMDFO_STRETCH));
        let result =
            unsafe { ChangeDisplaySettingsExW(None, Some(&devmode), None, CDS_TEST, None) };
        check_display_change(result, stretch)?;
        assert_eq!(current_mode()?, current);
        println!("Driver accepts {}", stretch.label());
        Ok(())
    }

    #[test]
    #[ignore = "tests 1280x960 and 1440x1080 full-screen scaling, restoring the original mode"]
    fn primary_stretch_toggle_round_trip() -> Result<()> {
        let original = current_mode()?;
        let panel = panel_mode()?;
        let refresh = display::stretch_refresh(Some(panel), Some(original));
        let result = (|| -> Result<()> {
            for profile in [Profile::new(1280, 960), Profile::new(1440, 1080)] {
                let applied = display::toggle_stretch(&profile, original, refresh)?;
                anyhow::ensure!(
                    applied.matches_profile(&profile),
                    "toggle did not select stretch"
                );
                anyhow::ensure!(current_mode()? == applied, "stretch was not applied");
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
                let restored = display::toggle_stretch(&profile, original, refresh)?;
                anyhow::ensure!(restored == original, "toggle did not select native");
                anyhow::ensure!(current_mode()? == original, "native was not restored");
                println!("Restored {}", original.label());
            }
            Ok(())
        })();
        // Restore even if applying or verifying either half of the toggle fails.
        display::change_mode(original).context("restore original display after test")?;
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
}
