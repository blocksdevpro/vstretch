//! Windows process detection, console access, and the tray instance lock.

use std::{
    ffi::OsString,
    hash::{DefaultHasher, Hash, Hasher},
    os::windows::ffi::OsStringExt,
    path::Path,
};

use anyhow::{Context, Result, bail};
use windows::{
    Win32::{
        Foundation::{
            CloseHandle, ERROR_ALREADY_EXISTS, ERROR_INVALID_PARAMETER, ERROR_NO_MORE_FILES,
            FILETIME, GetLastError, HANDLE, WAIT_ABANDONED, WAIT_FAILED, WAIT_OBJECT_0,
            WAIT_TIMEOUT,
        },
        System::{
            Console::{ATTACH_PARENT_PROCESS, AllocConsole, AttachConsole, GetConsoleCP},
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                CreateMutexW, GetCurrentProcess, GetProcessTimes, INFINITE, OpenProcess,
                PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
                QueryFullProcessImageNameW, ReleaseMutex, WaitForSingleObject,
            },
        },
        UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId, MB_ICONERROR, MB_OK, MessageBoxW,
        },
    },
    core::{Error as WinError, HRESULT, PCWSTR, PWSTR, w},
};

use crate::autostretch::GameActivity;

/// Keeps a valid Windows handle alive and closes it when its owner leaves scope.
pub struct OwnedHandle(HANDLE);

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

/// None means the foreground process could not be queried. Leave the session
/// alone in that case, rather than interpreting an access error as Alt-Tab.
fn foreground_is_game() -> Option<bool> {
    let window = unsafe { GetForegroundWindow() };
    if window.is_invalid() {
        return None;
    }
    let mut process_id = 0;
    unsafe { GetWindowThreadProcessId(window, Some(&mut process_id)) };
    let process = OwnedHandle(
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }.ok()?,
    );
    let mut buffer = vec![0u16; 32768];
    let mut length = buffer.len() as u32;
    // SAFETY: process owns a valid handle, and length reports the writable buffer
    // capacity in UTF-16 code units, as required by the API.
    unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    }
    .ok()?;
    let path = OsString::from_wide(&buffer[..length as usize]);
    let name = Path::new(&path).file_name()?.to_str()?;
    Some(is_game_executable(name))
}

/// Alt-Tab and game exit are separate observations. Failed queries leave the
/// session alone instead of incorrectly restoring its display settings.
pub fn game_activity() -> Option<GameActivity> {
    if !has_running_game()? {
        return Some(GameActivity::Stopped);
    }
    foreground_is_game().map(|focused| {
        if focused {
            GameActivity::Focused
        } else {
            GameActivity::Background
        }
    })
}

fn has_running_game() -> Option<bool> {
    let snapshot = OwnedHandle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.ok()?);
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: snapshot stays alive throughout enumeration. dwSize describes the
    // writable entry passed to Process32FirstW and Process32NextW.
    let mut next = unsafe { Process32FirstW(snapshot.0, &mut entry) };
    loop {
        match next {
            Ok(()) => {
                let end = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                if is_game_executable(&String::from_utf16_lossy(&entry.szExeFile[..end])) {
                    return Some(true);
                }
                next = unsafe { Process32NextW(snapshot.0, &mut entry) };
            }
            Err(error) if error.code() == HRESULT::from_win32(ERROR_NO_MORE_FILES.0) => {
                return Some(false);
            }
            Err(_) => return None,
        }
    }
}

fn is_game_executable(name: &str) -> bool {
    name.eq_ignore_ascii_case("VALORANT-Win64-Shipping.exe") || name.eq_ignore_ascii_case("cs2.exe")
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
    fn process_snapshot_detects_game_launch_and_exit() -> Result<()> {
        use std::{
            fs,
            os::windows::process::CommandExt,
            process::{Child, Command, Stdio},
            thread,
            time::Duration,
        };
        use windows::Win32::System::Threading::CREATE_NO_WINDOW;

        const CHILD: &str = "VSTRETCH_GAME_PROCESS_TEST";
        if std::env::var_os(CHILD).is_some() {
            thread::sleep(Duration::from_secs(30));
            return Ok(());
        }
        // Another real game would keep the aggregate running state true after
        // this child exits. Avoid interfering with an ongoing gaming session.
        if has_running_game() == Some(true) {
            println!("Process test skipped while an actual game is running.");
            return Ok(());
        }
        anyhow::ensure!(
            has_running_game() == Some(false),
            "could not query running games"
        );
        struct StopChild(Child);
        impl Drop for StopChild {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let directory = tempfile::tempdir()?;
        let executable = directory.path().join("cs2.exe");
        fs::copy(std::env::current_exe()?, &executable)?;
        let mut child = StopChild(
            Command::new(&executable)
                .args([
                    "--exact",
                    "platform::tests::process_snapshot_detects_game_launch_and_exit",
                ])
                .env(CHILD, "1")
                .creation_flags(CREATE_NO_WINDOW.0)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        );
        for _ in 0..20 {
            if has_running_game() == Some(true) {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        anyhow::ensure!(
            has_running_game() == Some(true),
            "running background game was not detected"
        );
        child.0.kill()?;
        child.0.wait()?;
        anyhow::ensure!(
            has_running_game() == Some(false),
            "exited game was still detected"
        );
        anyhow::ensure!(
            game_activity() == Some(GameActivity::Stopped),
            "game exit was not reported"
        );
        Ok(())
    }

    #[test]
    fn detects_only_game_executables() {
        assert!(is_game_executable("VALORANT-Win64-Shipping.exe"));
        assert!(is_game_executable("CS2.EXE"));
        for name in [
            "VALORANT.exe",
            "RiotClientServices.exe",
            "steam.exe",
            "cs2.exe.bak",
        ] {
            assert!(!is_game_executable(name));
        }
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
