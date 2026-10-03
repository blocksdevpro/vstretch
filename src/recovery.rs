//! Journals temporary display changes before applying them, and recovers dead owners.

use std::{
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    config::{Config, Profile},
    display::{self, DesktopSnapshot, Mode},
    platform::{self, ProcessIdentity},
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    owner: ProcessIdentity,
    desktop: DesktopSnapshot,
    change: Change,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "state", deny_unknown_fields)]
enum Change {
    Prepared { before: Mode, requested: Mode },
    Applied { mode: Mode },
}

impl Change {
    fn matches(&self, current: Mode) -> bool {
        match *self {
            Self::Prepared { before, requested } => current == before || current == requested,
            Self::Applied { mode } => current == mode,
        }
    }
}

impl Record {
    fn owns(&self, current: &DesktopSnapshot) -> bool {
        self.desktop.monitor == current.monitor && self.change.matches(current.mode)
    }
}

struct Journal(PathBuf);

impl Journal {
    fn load(&self) -> Result<Option<Record>> {
        let text = match fs::read_to_string(&self.0) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("read display recovery record"),
        };
        let record: Record = toml::from_str(&text).context("parse display recovery record")?;
        ensure!(
            record.version == 1,
            "unsupported display recovery record version"
        );
        let changed_modes = match record.change {
            Change::Prepared { before, requested } => [before, requested],
            Change::Applied { mode } => [mode, mode],
        };
        for mode in [record.desktop.mode, changed_modes[0], changed_modes[1]] {
            ensure!(
                mode.width > 0 && mode.height > 0 && mode.refresh > 0,
                "invalid mode in display recovery record"
            );
        }
        ensure!(
            !record.desktop.monitor.is_empty(),
            "missing monitor in display recovery record"
        );
        ensure!(
            record.desktop.scaling.is_none_or(|value| value <= 2),
            "invalid scaling in display recovery record"
        );
        ensure!(
            record.owner.pid > 0 && record.owner.created > 0,
            "invalid owner in display recovery record"
        );
        Ok(Some(record))
    }

    fn save(&self, record: &Record) -> Result<()> {
        let parent = self
            .0
            .parent()
            .context("display recovery path has no parent")?;
        fs::create_dir_all(parent).context("create display recovery directory")?;
        let mut staged = tempfile::Builder::new()
            .prefix(".vstretch-recovery-")
            .tempfile_in(parent)
            .context("stage display recovery record")?;
        staged
            .write_all(toml::to_string(record)?.as_bytes())
            .context("write display recovery record")?;
        staged
            .as_file()
            .sync_all()
            .context("flush display recovery record")?;
        staged
            .into_temp_path()
            .persist(&self.0)
            .context("publish display recovery record")?;
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        match fs::remove_file(&self.0) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("clear display recovery record"),
        }
    }
}

trait DisplayBackend {
    fn snapshot(&self) -> Result<DesktopSnapshot>;
    fn saved_mode(&self) -> Result<Mode>;
    fn apply(&mut self, mode: Mode, scaling: Option<u32>, save_default: bool) -> Result<()>;
}

struct WindowsDisplay;

impl DisplayBackend for WindowsDisplay {
    fn snapshot(&self) -> Result<DesktopSnapshot> {
        display::desktop_snapshot()
    }
    fn saved_mode(&self) -> Result<Mode> {
        display::get_saved_resolution()
    }
    fn apply(&mut self, mode: Mode, scaling: Option<u32>, save_default: bool) -> Result<()> {
        display::apply_display_settings(mode, scaling, save_default)
    }
}

struct Session {
    journal: Journal,
    owner: ProcessIdentity,
}

impl Session {
    fn new(config_path: &Path, owner: ProcessIdentity) -> Self {
        Self {
            journal: Journal(config_path.with_extension("recovery.toml")),
            owner,
        }
    }

    fn recover(
        &self,
        backend: &mut impl DisplayBackend,
        alive: impl FnOnce(ProcessIdentity) -> Result<bool>,
    ) -> Result<Option<Mode>> {
        let Some(record) = self.journal.load()? else {
            return Ok(None);
        };
        // Another tray, TUI, or hotkey process may still own this change.
        if alive(record.owner)? {
            return Ok(None);
        }
        let current = backend.snapshot()?;
        if !record.owns(&current) {
            // Windows already reset after reboot, or another app/user/monitor took over.
            self.journal.clear()?;
            return Ok(None);
        }
        if !current.matches_settings(record.desktop.mode, record.desktop.scaling) {
            backend
                .apply(record.desktop.mode, record.desktop.scaling, false)
                .context("restore desktop after interrupted display session")?;
        }
        let restored = backend.snapshot()?;
        ensure!(
            restored.monitor == record.desktop.monitor && restored.mode == record.desktop.mode,
            "Windows did not restore the recorded desktop mode"
        );
        self.journal.clear()?;
        Ok(Some(restored.mode))
    }

    fn change(
        &self,
        backend: &mut impl DisplayBackend,
        mode: Mode,
        scaling: Option<u32>,
    ) -> Result<()> {
        let current = backend.snapshot()?;
        let previous = self.journal.load()?;
        let before = current.mode;
        let desktop = previous
            .filter(|record| record.owns(&current))
            .map(|record| record.desktop)
            .unwrap_or_else(|| current.clone());
        let restoring = mode == desktop.mode && scaling.is_none();
        let scaling = if restoring { desktop.scaling } else { scaling };
        // Reapplying an active mode can still notify/reinitialize desktop apps.
        // Keep any existing restore target, but don't journal a new no-op session.
        if current.matches_settings(mode, scaling) {
            if restoring {
                self.journal.clear()?;
            }
            return Ok(());
        }
        let mut record = Record {
            version: 1,
            owner: self.owner,
            desktop,
            change: Change::Prepared {
                before,
                requested: mode,
            },
        };
        // A crash immediately after the Windows call must still leave a restore target.
        // Failure to publish aborts the switch before it can affect the display.
        self.journal.save(&record)?;
        backend.apply(mode, scaling, false)?;
        let actual = backend.snapshot()?;
        ensure!(
            actual.monitor == record.desktop.monitor,
            "primary monitor changed during display switch"
        );
        if restoring && actual.mode == record.desktop.mode {
            self.journal.clear()?;
        } else {
            // Drivers can normalize refresh rates. Recover against the actual applied mode.
            record.change = Change::Applied { mode: actual.mode };
            self.journal.save(&record)?;
        }
        Ok(())
    }

    fn restore_native(
        &self,
        backend: &mut impl DisplayBackend,
        mode: Mode,
        stretch: &Profile,
    ) -> Result<()> {
        let current = backend.snapshot()?;
        let previous = self.journal.load()?;
        let scaling = previous
            .filter(|record| record.owns(&current) && record.desktop.mode == mode)
            .and_then(|record| record.desktop.scaling);
        let saved = backend.saved_mode()?;
        // The old implementation persisted stretch. An explicit Native choice is
        // the user's restore target; don't guess that target during ordinary startup.
        let repair_default = saved.matches_profile(stretch) && !mode.matches_profile(stretch);
        // Persisting a repaired legacy default is intentional even if already active.
        if repair_default || !current.matches_settings(mode, scaling) {
            backend.apply(mode, scaling, repair_default)?;
        }
        ensure!(
            backend.snapshot()?.mode == mode,
            "Windows did not apply the native mode"
        );
        self.journal.clear()
    }

    fn finish(&self) -> Result<()> {
        if self
            .journal
            .load()?
            .is_some_and(|record| record.owner == self.owner)
        {
            // A successful manual command/Exit deliberately leaves its mode active.
            // The temporary Windows setting still disappears after reboot.
            self.journal.clear()?;
        }
        Ok(())
    }
}

fn session() -> Result<Session> {
    Ok(Session::new(
        &Config::config_path()?,
        platform::current_process_identity()?,
    ))
}

pub fn recover() -> Result<Option<Mode>> {
    let _lock = platform::display_lock()?;
    session()?.recover(&mut WindowsDisplay, platform::process_is_alive)
}

pub fn change(mode: Mode, scaling: Option<u32>) -> Result<()> {
    let _lock = platform::display_lock()?;
    session()?.change(&mut WindowsDisplay, mode, scaling)
}

pub fn restore_native(mode: Mode, stretch: &Profile) -> Result<()> {
    let _lock = platform::display_lock()?;
    session()?.restore_native(&mut WindowsDisplay, mode, stretch)
}

pub fn finish() -> Result<()> {
    let _lock = platform::display_lock()?;
    session()?.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

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
    const OWNER: ProcessIdentity = ProcessIdentity {
        pid: 123,
        created: 456,
    };

    struct FakeDisplay {
        current: DesktopSnapshot,
        saved: Mode,
        fail: bool,
        calls: usize,
        journal_before_apply: Option<PathBuf>,
    }

    impl FakeDisplay {
        fn new() -> Self {
            Self {
                current: DesktopSnapshot {
                    monitor: "monitor-A".into(),
                    mode: DESKTOP,
                    scaling: Some(2),
                },
                saved: DESKTOP,
                fail: false,
                calls: 0,
                journal_before_apply: None,
            }
        }
    }

    impl DisplayBackend for FakeDisplay {
        fn snapshot(&self) -> Result<DesktopSnapshot> {
            Ok(self.current.clone())
        }
        fn saved_mode(&self) -> Result<Mode> {
            Ok(self.saved)
        }
        fn apply(&mut self, mode: Mode, scaling: Option<u32>, save_default: bool) -> Result<()> {
            if let Some(path) = &self.journal_before_apply {
                assert!(
                    Journal(path.clone()).load()?.is_some(),
                    "display changed before durable recovery record"
                );
            }
            self.calls += 1;
            ensure!(!self.fail, "simulated driver failure");
            self.current.mode = mode;
            if let Some(scaling) = scaling {
                self.current.scaling = Some(scaling);
            }
            if save_default {
                self.saved = mode;
            }
            Ok(())
        }
    }

    fn fixture() -> Result<(tempfile::TempDir, Session, FakeDisplay)> {
        let directory = tempfile::tempdir()?;
        let session = Session::new(&directory.path().join("config.toml"), OWNER);
        Ok((directory, session, FakeDisplay::new()))
    }

    #[test]
    fn crash_recovery_restores_original_mode_and_scaling_once() -> Result<()> {
        let (directory, session, mut display) = fixture()?;
        display.journal_before_apply = Some(session.journal.0.clone());
        session.change(&mut display, STRETCH, Some(1))?;
        assert_eq!(display.saved, DESKTOP);
        // New manager, same on-disk state: no clean finish occurred before the crash.
        let next = Session::new(
            &directory.path().join("config.toml"),
            ProcessIdentity {
                pid: 999,
                created: 888,
            },
        );
        assert_eq!(next.recover(&mut display, |_| Ok(false))?, Some(DESKTOP));
        assert_eq!(display.current.scaling, Some(2));
        let calls = display.calls;
        assert_eq!(next.recover(&mut display, |_| Ok(false))?, None);
        assert_eq!(display.calls, calls);
        Ok(())
    }

    #[test]
    fn live_owner_is_not_interrupted_and_other_process_cannot_finish_it() -> Result<()> {
        let (directory, session, mut display) = fixture()?;
        session.change(&mut display, STRETCH, Some(1))?;
        let other = Session::new(
            &directory.path().join("config.toml"),
            ProcessIdentity {
                pid: 999,
                created: 888,
            },
        );
        assert_eq!(
            other.recover(&mut display, |owner| {
                assert_eq!(owner, OWNER);
                Ok(true)
            })?,
            None
        );
        other.finish()?;
        assert!(session.journal.load()?.is_some());
        assert_eq!(display.current.mode, STRETCH);
        assert_eq!(display.calls, 1);
        Ok(())
    }

    #[test]
    fn reboot_reset_external_change_and_monitor_replacement_are_respected() -> Result<()> {
        for replacement in [
            DesktopSnapshot {
                monitor: "monitor-A".into(),
                mode: DESKTOP,
                scaling: Some(2),
            },
            DesktopSnapshot {
                monitor: "monitor-A".into(),
                mode: Mode {
                    width: 1920,
                    height: 1080,
                    refresh: 144,
                },
                scaling: None,
            },
            DesktopSnapshot {
                monitor: "monitor-B".into(),
                mode: STRETCH,
                scaling: Some(1),
            },
        ] {
            let (_directory, session, mut display) = fixture()?;
            session.change(&mut display, STRETCH, Some(1))?;
            display.current = replacement.clone();
            assert_eq!(session.recover(&mut display, |_| Ok(false))?, None);
            assert_eq!(display.current, replacement);
            assert_eq!(display.calls, 1);
            assert!(session.journal.load()?.is_none());
        }
        Ok(())
    }

    #[test]
    fn changing_presets_keeps_the_first_desktop_restore_target() -> Result<()> {
        let (_directory, session, mut display) = fixture()?;
        session.change(&mut display, STRETCH, Some(1))?;
        let preset = Mode {
            width: 1280,
            height: 960,
            refresh: 180,
        };
        session.change(&mut display, preset, Some(1))?;
        let record = session.journal.load()?.unwrap();
        assert_eq!(record.desktop.mode, DESKTOP);
        assert!(record.change.matches(preset));
        session.recover(&mut display, |_| Ok(false))?;
        assert_eq!(display.current.mode, DESKTOP);
        Ok(())
    }

    #[test]
    fn interrupted_preset_switch_keeps_recovery_for_the_previous_stretch() -> Result<()> {
        let (_directory, session, mut display) = fixture()?;
        session.change(&mut display, STRETCH, Some(1))?;
        display.fail = true;
        assert!(
            session
                .change(
                    &mut display,
                    Mode {
                        width: 1280,
                        height: 960,
                        refresh: 180
                    },
                    Some(1)
                )
                .is_err()
        );
        assert_eq!(display.current.mode, STRETCH);
        display.fail = false;
        assert_eq!(session.recover(&mut display, |_| Ok(false))?, Some(DESKTOP));
        assert_eq!(display.current.scaling, Some(2));
        Ok(())
    }

    #[test]
    fn killed_process_recovers_from_its_durable_record() -> Result<()> {
        use std::{
            os::windows::process::CommandExt,
            process::{Child, Command, Stdio},
            thread,
            time::{Duration, Instant},
        };
        use windows::Win32::System::Threading::CREATE_NO_WINDOW;

        const CHILD_DIRECTORY: &str = "VSTRETCH_RECOVERY_CHILD_DIRECTORY";
        if let Some(directory) = std::env::var_os(CHILD_DIRECTORY) {
            let directory = PathBuf::from(directory);
            let session = Session::new(
                &directory.join("config.toml"),
                platform::current_process_identity()?,
            );
            session.change(&mut FakeDisplay::new(), STRETCH, Some(1))?;
            fs::write(directory.join("ready"), "ready")?;
            thread::sleep(Duration::from_secs(30));
            return Ok(());
        }

        struct StopChild(Child);
        impl Drop for StopChild {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let directory = tempfile::tempdir()?;
        let mut child = StopChild(
            Command::new(std::env::current_exe()?)
                .args([
                    "--exact",
                    "recovery::tests::killed_process_recovers_from_its_durable_record",
                ])
                .env(CHILD_DIRECTORY, directory.path())
                .creation_flags(CREATE_NO_WINDOW.0)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while !directory.path().join("ready").exists() && Instant::now() < deadline {
            ensure!(
                child.0.try_wait()?.is_none(),
                "recovery child exited before writing its record"
            );
            thread::sleep(Duration::from_millis(25));
        }
        ensure!(
            directory.path().join("ready").exists(),
            "recovery child did not become ready"
        );
        let session = Session::new(
            &directory.path().join("config.toml"),
            platform::current_process_identity()?,
        );
        let owner = session.journal.load()?.unwrap().owner;
        let mut display = FakeDisplay::new();
        display.current.mode = STRETCH;
        display.current.scaling = Some(1);
        assert!(platform::process_is_alive(owner)?);
        assert_eq!(
            session.recover(&mut display, platform::process_is_alive)?,
            None
        );
        child.0.kill()?;
        child.0.wait()?;
        assert!(!platform::process_is_alive(owner)?);
        assert_eq!(
            session.recover(&mut display, platform::process_is_alive)?,
            Some(DESKTOP)
        );
        assert_eq!(display.current.scaling, Some(2));
        assert_eq!(display.saved, DESKTOP);
        assert!(session.journal.load()?.is_none());
        Ok(())
    }

    #[test]
    #[ignore = "temporarily switches the primary display; restores its original mode and scaling"]
    fn windows_temporary_switch_and_recovery_preserve_the_saved_default() -> Result<()> {
        let _lock = platform::display_lock()?;
        let original = display::desktop_snapshot()?;
        let saved = display::get_saved_resolution()?;
        struct RestoreDisplay(DesktopSnapshot);
        impl Drop for RestoreDisplay {
            fn drop(&mut self) {
                let _ = display::apply_display_settings(self.0.mode, self.0.scaling, false);
            }
        }
        let directory = tempfile::tempdir()?;
        let _restore = RestoreDisplay(original.clone());
        let modes = display::list_display_modes();
        let stretch = [
            Mode {
                width: 1280,
                height: 960,
                refresh: original.mode.refresh,
            },
            Mode {
                width: 1440,
                height: 1080,
                refresh: original.mode.refresh,
            },
        ]
        .into_iter()
        .find(|mode| !mode.size_eq(original.mode) && modes.contains(mode))
        .context("display must support 1280x960 or 1440x1080 at the current refresh rate")?;
        let config_path = directory.path().join("config.toml");
        let session = Session::new(&config_path, platform::current_process_identity()?);
        session.change(&mut WindowsDisplay, stretch, Some(1))?;
        assert_eq!(display::get_current_resolution()?, stretch);
        assert_eq!(
            display::get_saved_resolution()?,
            saved,
            "temporary stretch overwrote Windows' boot default"
        );
        let restarted = Session::new(&config_path, OWNER);
        assert_eq!(
            restarted.recover(&mut WindowsDisplay, |_| Ok(false))?,
            Some(original.mode)
        );
        assert_eq!(display::desktop_snapshot()?, original);
        assert_eq!(display::get_saved_resolution()?, saved);
        assert!(restarted.journal.load()?.is_none());
        Ok(())
    }

    #[test]
    fn failed_recovery_retains_the_record_for_retry() -> Result<()> {
        let (_directory, session, mut display) = fixture()?;
        session.change(&mut display, STRETCH, Some(1))?;
        display.fail = true;
        assert!(session.recover(&mut display, |_| Ok(false)).is_err());
        assert!(session.journal.load()?.is_some());
        display.fail = false;
        assert_eq!(session.recover(&mut display, |_| Ok(false))?, Some(DESKTOP));
        Ok(())
    }

    #[test]
    fn failed_apply_before_switch_leaves_desktop_unchanged_on_recovery() -> Result<()> {
        let (_directory, session, mut display) = fixture()?;
        display.fail = true;
        assert!(session.change(&mut display, STRETCH, Some(1)).is_err());
        display.fail = false;
        assert_eq!(session.recover(&mut display, |_| Ok(false))?, Some(DESKTOP));
        assert_eq!(display.current.mode, DESKTOP);
        Ok(())
    }

    #[test]
    fn journal_write_failure_prevents_the_display_switch() -> Result<()> {
        let (_directory, session, mut display) = fixture()?;
        fs::create_dir(&session.journal.0)?;
        assert!(session.change(&mut display, STRETCH, Some(1)).is_err());
        assert_eq!(display.calls, 0);
        assert_eq!(display.current.mode, DESKTOP);
        Ok(())
    }

    #[test]
    fn clean_manual_exit_preserves_current_mode_without_crash_recovery() -> Result<()> {
        let (_directory, session, mut display) = fixture()?;
        session.change(&mut display, STRETCH, Some(1))?;
        session.finish()?;
        assert_eq!(session.recover(&mut display, |_| Ok(false))?, None);
        assert_eq!(display.current.mode, STRETCH);
        assert_eq!(display.saved, DESKTOP);
        Ok(())
    }

    #[test]
    fn ordinary_restore_clears_the_record_and_preserves_saved_default() -> Result<()> {
        let (_directory, session, mut display) = fixture()?;
        session.change(&mut display, STRETCH, Some(1))?;
        session.change(&mut display, DESKTOP, None)?;
        assert_eq!(display.current.mode, DESKTOP);
        assert_eq!(display.current.scaling, Some(2));
        assert_eq!(display.saved, DESKTOP);
        assert!(session.journal.load()?.is_none());
        Ok(())
    }

    #[test]
    fn repeated_stretch_keeps_recovery_without_reapplying_the_display() -> Result<()> {
        let (_directory, session, mut display) = fixture()?;
        session.change(&mut display, STRETCH, Some(1))?;
        session.change(&mut display, STRETCH, Some(1))?;
        assert_eq!(display.calls, 1);
        assert_eq!(session.journal.load()?.unwrap().desktop.mode, DESKTOP);
        session.change(&mut display, DESKTOP, None)?;
        session.restore_native(&mut display, DESKTOP, &Profile::new(1440, 1080))?;
        assert_eq!(display.calls, 2);
        assert_eq!(display.current.scaling, Some(2));
        assert!(session.journal.load()?.is_none());
        Ok(())
    }

    #[test]
    fn already_active_settings_create_no_recovery_record() -> Result<()> {
        let (_directory, session, mut display) = fixture()?;
        session.change(&mut display, DESKTOP, None)?;
        session.change(&mut display, DESKTOP, Some(2))?;
        assert_eq!(display.calls, 0);
        assert!(session.journal.load()?.is_none());
        Ok(())
    }

    #[test]
    fn same_resolution_still_applies_changed_or_unknown_scaling_and_refresh() -> Result<()> {
        for scaling in [Some(2), None] {
            let (_directory, session, mut display) = fixture()?;
            display.current.mode = STRETCH;
            display.current.scaling = scaling;
            session.change(&mut display, STRETCH, Some(1))?;
            assert_eq!(display.calls, 1);
            assert_eq!(display.current.scaling, Some(1));
            let changed_refresh = Mode {
                refresh: 60,
                ..STRETCH
            };
            session.change(&mut display, changed_refresh, Some(1))?;
            assert_eq!(display.calls, 2);
            assert_eq!(display.current.mode, changed_refresh);
        }
        Ok(())
    }

    #[test]
    fn native_repairs_saved_stretch_even_when_desktop_is_already_active() -> Result<()> {
        let (_directory, session, mut display) = fixture()?;
        display.saved = STRETCH;
        session.restore_native(&mut display, DESKTOP, &Profile::new(1440, 1080))?;
        assert_eq!(display.calls, 1);
        assert_eq!(display.saved, DESKTOP);
        Ok(())
    }

    #[test]
    fn recovery_of_failed_apply_clears_record_without_reapplying_active_desktop() -> Result<()> {
        let (_directory, session, mut display) = fixture()?;
        display.fail = true;
        assert!(session.change(&mut display, STRETCH, Some(1)).is_err());
        display.fail = false;
        assert_eq!(session.recover(&mut display, |_| Ok(false))?, Some(DESKTOP));
        assert_eq!(display.calls, 1);
        assert!(session.journal.load()?.is_none());
        Ok(())
    }

    #[test]
    fn explicit_native_repairs_only_an_old_saved_stretch_default() -> Result<()> {
        for saved in [
            STRETCH,
            DESKTOP,
            Mode {
                width: 1920,
                height: 1080,
                refresh: 144,
            },
        ] {
            let (_directory, session, mut display) = fixture()?;
            display.saved = saved;
            session.change(&mut display, STRETCH, Some(1))?;
            session.restore_native(&mut display, DESKTOP, &Profile::new(1440, 1080))?;
            assert_eq!(
                display.saved,
                if saved == STRETCH { DESKTOP } else { saved }
            );
            assert!(session.journal.load()?.is_none());
        }
        Ok(())
    }

    #[test]
    fn corrupt_or_unknown_records_never_change_the_display() -> Result<()> {
        let (_directory, session, mut display) = fixture()?;
        for text in ["broken toml", "version = 999"] {
            fs::write(&session.journal.0, text)?;
            let queried = Cell::new(false);
            assert!(
                session
                    .recover(&mut display, |_| {
                        queried.set(true);
                        Ok(false)
                    })
                    .is_err()
            );
            assert!(!queried.get());
            assert_eq!(display.calls, 0);
        }
        Ok(())
    }
}
