//! Brings a stretched display back after Windows sleeps or turns monitors off.
//!
//! Windows can reset the primary display to its native mode while resuming. That
//! looks like an outside change to the auto-stretch state machine, which then
//! deliberately leaves the display alone. This module reports power events and
//! plans a bounded re-apply so the player does not have to stretch again.

use std::{
    ffi::c_void,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};

use windows::Win32::{
    Foundation::HANDLE,
    System::{
        Power::{
            DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, HPOWERNOTIFY, POWERBROADCAST_SETTING,
            RegisterPowerSettingNotification, RegisterSuspendResumeNotification,
            UnregisterPowerSettingNotification, UnregisterSuspendResumeNotification,
        },
        SystemServices::GUID_CONSOLE_DISPLAY_STATE,
    },
    UI::WindowsAndMessaging::{
        DEVICE_NOTIFY_CALLBACK, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND, PBT_APMSUSPEND,
        PBT_POWERSETTINGCHANGE,
    },
};

/// Counters let the UI thread poll events that Windows reports on another thread.
static SUSPENDS: AtomicU32 = AtomicU32::new(0);
static WAKES: AtomicU32 = AtomicU32::new(0);
static MONITORS_OFF: AtomicBool = AtomicBool::new(false);

/// Console display state values from `GUID_CONSOLE_DISPLAY_STATE`.
const DISPLAY_OFF: u32 = 0;
const DISPLAY_ON: u32 = 1;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PowerEvents {
    /// The system is suspending or its monitors turned off since the last poll.
    pub suspended: bool,
    /// The system resumed or its monitors turned back on since the last poll.
    pub woke: bool,
}

/// Subscribes to suspend/resume and monitor power notifications.
///
/// Registration is best effort: without it the tray still works, it just cannot
/// bring stretch back after sleep.
pub struct PowerWatcher {
    handles: Vec<HPOWERNOTIFY>,
    resume: Option<HPOWERNOTIFY>,
    // Windows keeps reading this registration record until the handle is closed.
    _recipient: Box<DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS>,
    seen_suspends: u32,
    seen_wakes: u32,
}

impl PowerWatcher {
    pub fn new() -> Self {
        let recipient = Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
            Callback: Some(power_callback),
            Context: std::ptr::null_mut(),
        });
        let target =
            HANDLE(&*recipient as *const DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS as *mut c_void);
        // SAFETY: target points at a live registration record that outlives both
        // subscriptions, and DEVICE_NOTIFY_CALLBACK tells Windows to read it as one.
        let resume =
            unsafe { RegisterSuspendResumeNotification(target, DEVICE_NOTIFY_CALLBACK) }.ok();
        // SAFETY: as above; the GUID is a valid constant borrowed only for the call.
        let monitors = unsafe {
            RegisterPowerSettingNotification(
                target,
                &GUID_CONSOLE_DISPLAY_STATE,
                DEVICE_NOTIFY_CALLBACK,
            )
        }
        .ok();
        Self {
            handles: monitors.into_iter().collect(),
            resume,
            _recipient: recipient,
            seen_suspends: SUSPENDS.load(Ordering::Acquire),
            seen_wakes: WAKES.load(Ordering::Acquire),
        }
    }

    pub fn poll(&mut self) -> PowerEvents {
        let suspends = SUSPENDS.load(Ordering::Acquire);
        let wakes = WAKES.load(Ordering::Acquire);
        let events = PowerEvents {
            suspended: suspends != self.seen_suspends,
            woke: wakes != self.seen_wakes,
        };
        self.seen_suspends = suspends;
        self.seen_wakes = wakes;
        events
    }
}

impl Drop for PowerWatcher {
    fn drop(&mut self) {
        // SAFETY: each handle came from a successful registration and is closed once.
        unsafe {
            if let Some(handle) = self.resume.take() {
                let _ = UnregisterSuspendResumeNotification(handle);
            }
            for handle in self.handles.drain(..) {
                let _ = UnregisterPowerSettingNotification(handle);
            }
        }
    }
}

/// Runs on a Windows thread-pool thread, so it only records what happened.
unsafe extern "system" fn power_callback(
    _context: *const c_void,
    kind: u32,
    setting: *const c_void,
) -> u32 {
    match kind {
        PBT_APMSUSPEND => {
            SUSPENDS.fetch_add(1, Ordering::Release);
        }
        PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => {
            WAKES.fetch_add(1, Ordering::Release);
        }
        PBT_POWERSETTINGCHANGE if !setting.is_null() => {
            // SAFETY: for this notification Windows passes a POWERBROADCAST_SETTING
            // whose DataLength bytes follow the header. Read the DWORD unaligned.
            let state = unsafe {
                let setting = setting.cast::<POWERBROADCAST_SETTING>();
                if (*setting).DataLength < size_of::<u32>() as u32
                    || (*setting).PowerSetting != GUID_CONSOLE_DISPLAY_STATE
                {
                    return 0;
                }
                std::ptr::addr_of!((*setting).Data)
                    .cast::<u32>()
                    .read_unaligned()
            };
            match state {
                DISPLAY_OFF => {
                    MONITORS_OFF.store(true, Ordering::Release);
                    SUSPENDS.fetch_add(1, Ordering::Release);
                }
                // The first notification reports the current state; only an
                // off-to-on change is a wake.
                DISPLAY_ON if MONITORS_OFF.swap(false, Ordering::AcqRel) => {
                    WAKES.fetch_add(1, Ordering::Release);
                }
                _ => {}
            }
        }
        _ => {}
    }
    0
}

/// Wait this many ticks after waking so monitors can finish re-enumerating.
const SETTLE_TICKS: u32 = 2;
/// Stop watching for a late reset after this many ticks.
const WINDOW_TICKS: u32 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakePlan {
    /// Nothing to do this tick.
    Idle,
    /// Waking is in progress; leave the display alone and keep observing.
    Waiting,
    /// Stretch was lost across the wake; apply it again.
    Reapply,
}

/// Remembers whether the display was stretched before power went away.
#[derive(Debug, Default)]
pub struct WakeRestore {
    was_stretch: bool,
    frozen: bool,
    ticks: Option<u32>,
}

impl WakeRestore {
    /// A wake is waiting for monitors to settle or for stretch to return.
    #[cfg(test)]
    fn is_pending(&self) -> bool {
        self.ticks.is_some()
    }

    /// Feeds one poll of the real display plus the power events since the last one.
    pub fn observe(
        &mut self,
        events: PowerEvents,
        stretched: bool,
        display_known: bool,
    ) -> WakePlan {
        if events.suspended {
            // Windows may reset the mode before the first tick after waking, so
            // stop learning from the display once power is going away.
            self.frozen = true;
            self.ticks = None;
        }
        if events.woke && self.frozen {
            self.frozen = false;
            if self.was_stretch {
                self.ticks.get_or_insert(0);
            }
        }
        let Some(ticks) = self.ticks.as_mut() else {
            if !self.frozen && display_known {
                self.was_stretch = stretched;
            }
            return WakePlan::Idle;
        };
        *ticks += 1;
        if *ticks > WINDOW_TICKS {
            self.ticks = None;
            return WakePlan::Idle;
        }
        if *ticks < SETTLE_TICKS || !display_known || stretched {
            return WakePlan::Waiting;
        }
        WakePlan::Reapply
    }

    /// Stop restoring, for example after a manual display choice or a finished game.
    pub fn cancel(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: PowerEvents = PowerEvents {
        suspended: false,
        woke: false,
    };
    const SUSPEND: PowerEvents = PowerEvents {
        suspended: true,
        woke: false,
    };
    const WAKE: PowerEvents = PowerEvents {
        suspended: false,
        woke: true,
    };

    #[test]
    fn windows_accepts_both_power_subscriptions() {
        let mut watcher = PowerWatcher::new();
        assert!(watcher.resume.is_some());
        assert_eq!(watcher.handles.len(), 1);
        assert!(!watcher.poll().suspended);
    }

    #[test]
    fn stretch_lost_across_sleep_is_reapplied_after_the_display_settles() {
        let mut restore = WakeRestore::default();
        assert_eq!(restore.observe(NONE, true, true), WakePlan::Idle);
        assert_eq!(restore.observe(SUSPEND, true, true), WakePlan::Idle);
        // Windows resets to native before the first tick after waking.
        assert_eq!(restore.observe(WAKE, false, true), WakePlan::Waiting);
        assert!(restore.is_pending());
        assert_eq!(restore.observe(NONE, false, true), WakePlan::Reapply);
    }

    #[test]
    fn sleeping_while_native_never_stretches() {
        let mut restore = WakeRestore::default();
        restore.observe(NONE, false, true);
        restore.observe(SUSPEND, false, true);
        for _ in 0..4 {
            assert_eq!(restore.observe(WAKE, false, true), WakePlan::Idle);
        }
    }

    #[test]
    fn a_stretch_that_survives_waking_is_left_alone_but_watched() {
        let mut restore = WakeRestore::default();
        restore.observe(NONE, true, true);
        restore.observe(SUSPEND, true, true);
        restore.observe(WAKE, true, true);
        assert_eq!(restore.observe(NONE, true, true), WakePlan::Waiting);
        // A late driver reset is still caught inside the window.
        assert_eq!(restore.observe(NONE, false, true), WakePlan::Reapply);
    }

    #[test]
    fn failed_reapply_retries_until_the_window_closes() {
        let mut restore = WakeRestore::default();
        restore.observe(NONE, true, true);
        restore.observe(SUSPEND, true, true);
        restore.observe(WAKE, false, true);
        let mut attempts = 0;
        for _ in 0..WINDOW_TICKS * 2 {
            if restore.observe(NONE, false, true) == WakePlan::Reapply {
                attempts += 1;
            }
        }
        assert_eq!(attempts, (WINDOW_TICKS - SETTLE_TICKS + 1) as usize);
        assert!(!restore.is_pending());
        assert_eq!(restore.observe(NONE, false, true), WakePlan::Idle);
    }

    #[test]
    fn unreadable_display_waits_instead_of_guessing() {
        let mut restore = WakeRestore::default();
        restore.observe(NONE, true, true);
        restore.observe(SUSPEND, true, true);
        restore.observe(WAKE, false, false);
        assert_eq!(restore.observe(NONE, false, false), WakePlan::Waiting);
    }

    #[test]
    fn cancel_forgets_the_stretch_and_any_pending_restore() {
        let mut restore = WakeRestore::default();
        restore.observe(NONE, true, true);
        restore.observe(SUSPEND, true, true);
        restore.observe(WAKE, false, true);
        restore.cancel();
        assert!(!restore.is_pending());
        restore.observe(SUSPEND, false, true);
        assert_eq!(restore.observe(WAKE, false, true), WakePlan::Idle);
    }

    #[test]
    fn suspend_and_wake_reported_in_one_poll_still_restore() {
        let mut restore = WakeRestore::default();
        restore.observe(NONE, true, true);
        let both = PowerEvents {
            suspended: true,
            woke: true,
        };
        assert_eq!(restore.observe(both, false, true), WakePlan::Waiting);
        assert_eq!(restore.observe(NONE, false, true), WakePlan::Reapply);
    }

    #[test]
    fn a_wake_without_a_suspend_is_ignored() {
        let mut restore = WakeRestore::default();
        restore.observe(NONE, true, true);
        assert_eq!(restore.observe(WAKE, false, true), WakePlan::Idle);
    }
}
