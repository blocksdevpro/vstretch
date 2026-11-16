//! Windows process identity, console access, error dialogs, and named locks.

use std::{
    hash::{DefaultHasher, Hash, Hasher},
    path::Path,
};

use anyhow::{Context, Result, bail};
use windows::{
    Win32::{
        Foundation::{
            CloseHandle, ERROR_ALREADY_EXISTS, ERROR_INVALID_PARAMETER, FILETIME, GetLastError,
            HANDLE, WAIT_ABANDONED, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        System::{
            Console::{ATTACH_PARENT_PROCESS, AllocConsole, AttachConsole, GetConsoleCP},
            Threading::{
                CreateMutexW, GetCurrentProcess, GetProcessTimes, INFINITE, OpenProcess,
                PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, ReleaseMutex,
                WaitForSingleObject,
            },
        },
        UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
    },
    core::{Error as WinError, HRESULT, PCWSTR, w},
};

/// Keeps a valid Windows handle alive and closes it when its owner leaves scope.
pub struct OwnedHandle(pub(crate) HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: each handle is created or opened once and closed by its sole owner.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Holds the instance lock, or returns None when this config already has a tray.
pub fn tray_instance(config_path: &Path) -> Result<Option<OwnedHandle>> {
    let name = mutex_name(config_path, "Tray")?;
    // SAFETY: name is nul-terminated and stays alive until CreateMutexW returns.
    let handle = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }
        .context("could not create tray instance lock")?;
    let exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    let owned = OwnedHandle(handle);
    Ok(if exists { None } else { Some(owned) })
}

fn mutex_name(config_path: &Path, purpose: &str) -> Result<Vec<u16>> {
    let mut hash = DefaultHasher::new();
    std::path::absolute(config_path)?
        .to_string_lossy()
        .to_lowercase()
        .hash(&mut hash);
    Ok(format!("Local\\vstretch.{purpose}.{:x}", hash.finish())
        .encode_utf16()
        .chain(Some(0))
        .collect())
}

/// Releases the config file mutex before closing its handle.
pub struct ConfigFileLock(OwnedHandle);

impl Drop for ConfigFileLock {
    fn drop(&mut self) {
        // SAFETY: this guard is created only after this thread acquires the mutex.
        let _ = unsafe { ReleaseMutex(self.0.0) };
    }
}

/// Coordinates short reads and replacements across tray and terminal processes.
pub fn config_file_lock(config_path: &Path) -> Result<ConfigFileLock> {
    let name = mutex_name(config_path, "ConfigFile")?;
    named_lock(&name)
}

/// All tray, terminal, and hotkey processes change the same primary display.
pub fn display_lock() -> Result<ConfigFileLock> {
    let name: Vec<u16> = "Local\\vstretch.Display"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    named_lock(&name)
}

fn named_lock(name: &[u16]) -> Result<ConfigFileLock> {
    // SAFETY: name is nul-terminated; the returned handle is owned until it closes.
    let handle = OwnedHandle(
        unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }
            .context("could not create config file lock")?,
    );
    // SAFETY: handle owns a valid mutex for the entire wait.
    match unsafe { WaitForSingleObject(handle.0, INFINITE) } {
        // An abandoned mutex is also acquired. Atomic file replacement lets a
        // new process continue after the previous owner crashed.
        WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(ConfigFileLock(handle)),
        WAIT_FAILED => Err(WinError::from_thread()).context("could not acquire config file lock"),
        status => bail!("unexpected config file lock status: {}", status.0),
    }
}

/// Creation time distinguishes a live owner from a reused Windows process ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub created: u64,
}

fn process_created(handle: HANDLE) -> Result<u64> {
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: handle is valid; every output points to a writable FILETIME.
    unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) }
        .context("read display session owner's creation time")?;
    Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

pub fn current_process_identity() -> Result<ProcessIdentity> {
    Ok(ProcessIdentity {
        pid: std::process::id(),
        // SAFETY: Windows' current-process pseudo-handle needs no CloseHandle.
        created: process_created(unsafe { GetCurrentProcess() })?,
    })
}

pub fn process_is_alive(identity: ProcessIdentity) -> Result<bool> {
    // SAFETY: OpenProcess validates the ID and returns an owned handle.
    let handle = match unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            false,
            identity.pid,
        )
    } {
        Ok(handle) => OwnedHandle(handle),
        Err(error) if error.code() == HRESULT::from_win32(ERROR_INVALID_PARAMETER.0) => {
            return Ok(false);
        }
        Err(error) => return Err(error).context("check display session owner"),
    };
    // SAFETY: the process handle stays alive throughout this nonblocking query.
    match unsafe { WaitForSingleObject(handle.0, 0) } {
        WAIT_OBJECT_0 => Ok(false),
        WAIT_TIMEOUT => Ok(process_created(handle.0)? == identity.created),
        WAIT_FAILED => Err(WinError::from_thread()).context("check display session owner"),
        status => bail!("unexpected process wait status: {}", status.0),
    }
}

pub fn attach_console() {
    // Already having a console is fine; attachment is best effort for CLI output.
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

pub fn ensure_console() -> Result<()> {
    if unsafe { GetConsoleCP() } == 0 {
        unsafe { AllocConsole() }.context("could not open a terminal")?;
    }
    Ok(())
}

pub fn show_error(message: &str) {
    let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    // SAFETY: both strings are nul-terminated and stay alive while the dialog runs.
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            w!("Vstretch"),
            MB_OK | MB_ICONERROR,
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_owner_checks_process_creation_time() -> Result<()> {
        let current = current_process_identity()?;
        assert!(process_is_alive(current)?);
        assert!(!process_is_alive(ProcessIdentity {
            created: current.created + 1,
            ..current
        })?);
        Ok(())
    }

    #[test]
    fn only_one_tray_instance_holds_the_lock() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let first = tray_instance(&path).unwrap();
        assert!(first.is_some());
        assert!(tray_instance(&path).unwrap().is_none());
        drop(first);
        assert!(tray_instance(&path).unwrap().is_some());
    }
}
