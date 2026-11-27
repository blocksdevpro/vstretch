//! Detects whether a supported game is running and whether it has focus.

use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::Path};

use windows::{
    Win32::{
        Foundation::ERROR_NO_MORE_FILES,
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    },
    core::{HRESULT, PWSTR},
};

use crate::{autostretch::GameActivity, platform::OwnedHandle};

/// Executable names of the supported games, matched without regard to case.
const GAME_EXECUTABLES: &[&str] = &["VALORANT-Win64-Shipping.exe", "cs2.exe"];

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
    GAME_EXECUTABLES
        .iter()
        .any(|game| name.eq_ignore_ascii_case(game))
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

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
                    "game::tests::process_snapshot_detects_game_launch_and_exit",
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
}
