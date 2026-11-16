//! Display modes, the rules that pick native and stretch targets, and the
//! journaled actions that change the primary display.
//!
//! The raw Windows calls live in [`win32`]. Changes go through `recovery`, which
//! journals them first and then calls back into [`win32::apply_mode`].

mod win32;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub(crate) use win32::apply_mode;
pub use win32::{available_modes, current_mode, desktop_snapshot, panel_mode, saved_mode};

use crate::config::Profile;

/// Last-resort Hz when CCD and the profile both omit refresh.
pub const FALLBACK_REFRESH_HZ: u32 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

    pub fn matches_profile(self, p: &Profile) -> bool {
        self.width == p.width
            && self.height == p.height
            && p.refresh.is_none_or(|r| r == self.refresh)
    }
}

/// The monitor identity and settings needed to recover an interrupted session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopSnapshot {
    pub monitor: String,
    pub mode: Mode,
    pub scaling: Option<u32>,
}

impl DesktopSnapshot {
    /// Unknown scaling cannot prove that an explicit scaling request is active.
    pub(crate) fn matches_settings(&self, mode: Mode, scaling: Option<u32>) -> bool {
        self.mode == mode && scaling.is_none_or(|value| self.scaling == Some(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeKind {
    Native,
    Stretch,
    Panel,
    Other,
    Unknown,
}

/// The primary display as one poll sees it. Each field is `None` when Windows
/// could not report it, so callers can keep running and show the gap.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DisplayState {
    /// The active desktop mode.
    pub current: Option<Mode>,
    /// The panel's real output signal, even while stretched.
    pub panel: Option<Mode>,
    /// The Native restore target: the saved override, or else the panel.
    pub native: Option<Mode>,
}

impl DisplayState {
    pub fn read(native_override: Option<&Profile>) -> Self {
        let panel = panel_mode().ok();
        Self {
            current: current_mode().ok(),
            panel,
            native: resolve_native(native_override, panel).ok(),
        }
    }

    pub fn kind(&self, stretch: &Profile) -> ModeKind {
        classify_mode(self.current, self.native, self.panel, Some(stretch))
    }

    pub fn native(&self) -> Result<Mode> {
        self.native.context("could not detect native resolution")
    }

    pub fn stretch_refresh(&self) -> u32 {
        stretch_refresh(self.panel, self.native)
    }

    pub fn apply_stretch(&self, profile: &Profile) -> Result<Mode> {
        apply_stretch(profile, self.stretch_refresh())
    }

    pub fn restore_native(&self, stretch: &Profile) -> Result<Mode> {
        let native = self.native()?;
        restore_native(native, stretch)?;
        Ok(native)
    }

    pub fn toggle(&self, stretch: &Profile) -> Result<Mode> {
        toggle_stretch(stretch, self.native()?, self.stretch_refresh())
    }
}

/// Changes the primary display, journaling the change for crash recovery.
pub fn change_mode(mode: Mode) -> Result<()> {
    crate::recovery::change(mode, None)
}

/// Only an explicit Native choice repairs a stretch default saved by old versions.
pub fn restore_native(mode: Mode, stretch: &Profile) -> Result<()> {
    crate::recovery::restore_native(mode, stretch)
}

/// Applies a stretch profile with full-screen scaling and returns the requested mode.
pub fn apply_stretch(profile: &Profile, panel_refresh: u32) -> Result<Mode> {
    let mode = Mode {
        width: profile.width,
        height: profile.height,
        refresh: profile.refresh.unwrap_or(panel_refresh),
    };
    // A 4:3 resolution alone can preserve its aspect ratio and leave side bars.
    crate::recovery::change(mode, Some(win32::STRETCH_SCALING))?;
    Ok(mode)
}

/// `native` is the desktop restore target. Stretch uses `panel_refresh`, not the override Hz.
pub fn toggle_stretch(profile: &Profile, native: Mode, panel_refresh: u32) -> Result<Mode> {
    if current_mode()?.size_eq(native) {
        apply_stretch(profile, panel_refresh)
    } else {
        restore_native(native, profile)?;
        Ok(native)
    }
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

    fn mode(width: u32, height: u32, refresh: u32) -> Mode {
        Mode {
            width,
            height,
            refresh,
        }
    }

    #[test]
    fn display_state_falls_back_without_panel_or_native() {
        let state = DisplayState {
            current: Some(mode(1440, 1080, 144)),
            panel: None,
            native: None,
        };
        assert_eq!(state.stretch_refresh(), FALLBACK_REFRESH_HZ);
        assert!(state.native().is_err());
        assert!(state.toggle(&Profile::new(1440, 1080)).is_err());
        assert!(state.restore_native(&Profile::new(1440, 1080)).is_err());
        assert_eq!(state.kind(&Profile::new(1440, 1080)), ModeKind::Stretch);
    }

    #[test]
    fn display_state_prefers_panel_refresh_for_stretch() {
        let state = DisplayState {
            current: None,
            panel: Some(mode(2560, 1440, 180)),
            native: Some(mode(1920, 1080, 60)),
        };
        assert_eq!(state.stretch_refresh(), 180);
        assert_eq!(state.kind(&Profile::new(1440, 1080)), ModeKind::Unknown);
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
