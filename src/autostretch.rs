//! Tracks automatic display changes so only tray-owned modes are restored.

use crate::display::Mode;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum GameActivity {
    #[default]
    Stopped,
    Background,
    Focused,
}

impl GameActivity {
    fn session_ended(self, restore_on_alt_tab: bool) -> bool {
        self == Self::Stopped || (restore_on_alt_tab && self == Self::Background)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AutoPolicy {
    pub enabled: bool,
    pub restore_on_alt_tab: bool,
}

/// A display change requested by the state machine and carried out by the tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Stretch { desktop: Mode },
    Restore { desktop: Mode },
}

/// Records whether an automatic session owns a display change or must wait.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AutoStretch {
    #[default]
    Ready,
    /// Manual choices and failed applies persist until the session ends.
    /// By default that means game exit, even if the player Alt-Tabs meanwhile.
    PausedUntilSessionEnds,
    /// The applied mode must still be active before the tray restores the desktop.
    Owned { desktop: Mode, applied: Mode },
    /// Keep the restore target for another attempt on Exit, without error spam.
    RestoreFailed { desktop: Mode, applied: Mode },
}

impl AutoStretch {
    /// Plans the next action without performing display I/O.
    ///
    /// Pause before requesting a change so a failed apply is not retried every
    /// poll. The caller reports success through `applied` or `restored`.
    pub fn observe(
        &mut self,
        policy: AutoPolicy,
        activity: GameActivity,
        current: Mode,
    ) -> Option<Action> {
        let ended = activity.session_ended(policy.restore_on_alt_tab);
        match *self {
            Self::Ready if policy.enabled && activity == GameActivity::Focused => {
                *self = Self::PausedUntilSessionEnds;
                Some(Action::Stretch { desktop: current })
            }
            Self::PausedUntilSessionEnds if ended => {
                *self = Self::Ready;
                None
            }
            Self::Owned { applied, .. } | Self::RestoreFailed { applied, .. }
                if current != applied =>
            {
                // Another app, --auto, or the player changed the display. Relinquish
                // ownership so we never undo their new settings later.
                *self = if ended {
                    Self::Ready
                } else {
                    Self::PausedUntilSessionEnds
                };
                None
            }
            Self::Owned { desktop, applied } if !policy.enabled || ended => {
                *self = Self::RestoreFailed { desktop, applied };
                Some(Action::Restore { desktop })
            }
            _ => None,
        }
    }

    pub fn applied(&mut self, desktop: Mode, applied: Mode) {
        // If stretch was already active, the tray owns no change to restore.
        *self = if desktop == applied {
            Self::PausedUntilSessionEnds
        } else {
            Self::Owned { desktop, applied }
        };
    }

    pub fn restored(&mut self, policy: AutoPolicy, activity: GameActivity) {
        *self = if activity.session_ended(policy.restore_on_alt_tab) {
            Self::Ready
        } else {
            Self::PausedUntilSessionEnds
        };
    }

    pub fn manual_choice(&mut self) {
        *self = Self::PausedUntilSessionEnds;
    }

    pub fn restore_on_exit(self, current: Mode) -> Option<Mode> {
        match self {
            Self::Owned { desktop, applied } | Self::RestoreFailed { desktop, applied }
                if current == applied =>
            {
                Some(desktop)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use GameActivity::{Background, Focused, Stopped};

    const DEFAULT: AutoPolicy = AutoPolicy {
        enabled: true,
        restore_on_alt_tab: false,
    };
    const ON_ALT_TAB: AutoPolicy = AutoPolicy {
        enabled: true,
        restore_on_alt_tab: true,
    };
    const DISABLED: AutoPolicy = AutoPolicy {
        enabled: false,
        restore_on_alt_tab: false,
    };
    const DESKTOP: Mode = Mode {
        width: 2560,
        height: 1440,
        refresh: 180,
    };
    const STRETCH: Mode = Mode {
        width: 1440,
        height: 1080,
        refresh: 180,
    };

    #[test]
    fn default_keeps_stretch_across_alt_tab_and_restores_on_game_exit() {
        let mut auto = AutoStretch::default();
        assert_eq!(auto.observe(DEFAULT, Stopped, DESKTOP), None);
        assert_eq!(auto.observe(DEFAULT, Background, DESKTOP), None);
        assert_eq!(
            auto.observe(DEFAULT, Focused, DESKTOP),
            Some(Action::Stretch { desktop: DESKTOP })
        );
        auto.applied(DESKTOP, STRETCH);
        for activity in [Focused, Background, Background, Focused] {
            assert_eq!(auto.observe(DEFAULT, activity, STRETCH), None);
        }
        assert_eq!(
            auto.observe(DEFAULT, Stopped, STRETCH),
            Some(Action::Restore { desktop: DESKTOP })
        );
        auto.restored(DEFAULT, Stopped);
        assert_eq!(
            auto.observe(DEFAULT, Focused, DESKTOP),
            Some(Action::Stretch { desktop: DESKTOP })
        );
    }

    #[test]
    fn alt_tab_restoration_requires_opt_in() {
        let mut auto = AutoStretch::default();
        auto.applied(DESKTOP, STRETCH);
        assert_eq!(
            auto.observe(ON_ALT_TAB, Background, STRETCH),
            Some(Action::Restore { desktop: DESKTOP })
        );
        auto.restored(ON_ALT_TAB, Background);
        assert_eq!(
            auto.observe(ON_ALT_TAB, Focused, DESKTOP),
            Some(Action::Stretch { desktop: DESKTOP })
        );
    }

    #[test]
    fn turning_off_alt_tab_restoration_keeps_an_active_change() {
        let mut auto = AutoStretch::default();
        auto.applied(DESKTOP, STRETCH);
        assert_eq!(auto.observe(DEFAULT, Background, STRETCH), None);
        assert_eq!(auto.restore_on_exit(STRETCH), Some(DESKTOP));
    }

    #[test]
    fn disabled_auto_never_applies_and_restores_its_active_change() {
        let mut auto = AutoStretch::default();
        assert_eq!(auto.observe(DISABLED, Focused, DESKTOP), None);
        auto.applied(DESKTOP, STRETCH);
        assert_eq!(
            auto.observe(DISABLED, Background, STRETCH),
            Some(Action::Restore { desktop: DESKTOP })
        );
    }

    #[test]
    fn default_manual_choice_and_external_changes_survive_alt_tab() {
        let mut auto = AutoStretch::default();
        auto.manual_choice();
        for activity in [Focused, Background, Focused] {
            assert_eq!(auto.observe(DEFAULT, activity, DESKTOP), None);
        }
        auto.observe(DEFAULT, Stopped, DESKTOP);
        auto.applied(DESKTOP, STRETCH);
        let other = Mode {
            width: 1920,
            height: 1080,
            refresh: 144,
        };
        for activity in [Focused, Background, Focused] {
            assert_eq!(auto.observe(DEFAULT, activity, other), None);
        }
        assert_eq!(auto.restore_on_exit(other), None);
    }

    #[test]
    fn failed_default_apply_waits_for_game_exit_before_retrying() {
        let mut auto = AutoStretch::default();
        assert!(auto.observe(DEFAULT, Focused, DESKTOP).is_some());
        for activity in [Focused, Background, Focused] {
            assert_eq!(auto.observe(DEFAULT, activity, DESKTOP), None);
        }
        auto.observe(DEFAULT, Stopped, DESKTOP);
        assert!(auto.observe(DEFAULT, Focused, DESKTOP).is_some());
    }

    #[test]
    fn failed_restore_keeps_exit_recovery_without_repeated_attempts() {
        let mut auto = AutoStretch::default();
        auto.applied(DESKTOP, STRETCH);
        assert!(auto.observe(DEFAULT, Stopped, STRETCH).is_some());
        assert_eq!(auto.observe(DEFAULT, Stopped, STRETCH), None);
        assert_eq!(auto.restore_on_exit(STRETCH), Some(DESKTOP));
    }

    #[test]
    fn existing_stretch_and_manual_modes_are_not_restored_on_exit() {
        let mut auto = AutoStretch::default();
        auto.applied(STRETCH, STRETCH);
        assert_eq!(auto.restore_on_exit(STRETCH), None);
        auto.applied(DESKTOP, STRETCH);
        assert_eq!(auto.restore_on_exit(STRETCH), Some(DESKTOP));
        auto.manual_choice();
        assert_eq!(auto.restore_on_exit(STRETCH), None);
    }
}
