//! Manages Vstretch's startup entry for the current Windows user.

use std::{io::ErrorKind, path::Path};

use anyhow::{Context, Result, ensure};
use winreg::{
    RegKey,
    enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE},
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "Vstretch";

fn command(executable: &Path) -> Result<String> {
    let path = executable
        .to_str()
        .context("startup executable path is not valid Unicode")?;
    ensure!(
        executable.is_absolute() && !path.contains('"'),
        "invalid startup executable path"
    );
    let command = format!("\"{path}\"");
    ensure!(
        command.encode_utf16().count() <= 260,
        "startup executable path exceeds Windows' 260-character Run limit"
    );
    Ok(command)
}

pub fn enabled() -> Result<bool> {
    let executable = std::env::current_exe().context("locate vstretch.exe")?;
    let expected = command(&executable)?;
    let key = match RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_READ) {
        Ok(key) => key,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error).context("read Windows startup settings"),
    };
    match key.get_value::<String, _>(VALUE) {
        Ok(value) => Ok(value == expected),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context("read Vstretch startup entry"),
    }
}

pub fn set_enabled(enabled: bool) -> Result<()> {
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)
        .context("open Windows startup settings")?;
    if enabled {
        let executable = std::env::current_exe().context("locate vstretch.exe")?;
        key.set_value(VALUE, &command(&executable)?)
            .context("enable Start with Windows")?;
    } else {
        match key.delete_value(VALUE) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("disable Start with Windows"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "temporarily changes the Vstretch startup entry, then restores its original value"]
    fn windows_startup_entry_round_trips() -> Result<()> {
        use winreg::{RegValue, enums::KEY_QUERY_VALUE};

        struct RestoreEntry {
            key: RegKey,
            previous: Option<RegValue>,
        }
        impl RestoreEntry {
            fn restore(&self) -> Result<()> {
                match &self.previous {
                    Some(value) => self.key.set_raw_value(VALUE, value)?,
                    None => match self.key.delete_value(VALUE) {
                        Ok(()) => {}
                        Err(error) if error.kind() == ErrorKind::NotFound => {}
                        Err(error) => return Err(error.into()),
                    },
                }
                Ok(())
            }
        }
        impl Drop for RestoreEntry {
            fn drop(&mut self) {
                let _ = self.restore();
            }
        }

        let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey_with_flags(RUN_KEY, KEY_QUERY_VALUE | KEY_SET_VALUE)?;
        let previous = match key.get_raw_value(VALUE) {
            Ok(value) => Some(value),
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let restore = RestoreEntry { key, previous };
        set_enabled(true)?;
        ensure!(enabled()?, "Windows startup entry did not enable");
        ensure!(
            restore.key.get_value::<String, _>(VALUE)? == command(&std::env::current_exe()?)?,
            "Windows startup entry contains the wrong command"
        );
        set_enabled(true)?;
        set_enabled(false)?;
        ensure!(!enabled()?, "Windows startup entry did not disable");
        set_enabled(false)?;
        restore.restore()?;
        println!("Start with Windows enabled and disabled successfully; original entry restored.");
        Ok(())
    }

    #[test]
    fn quotes_startup_paths_with_spaces_and_keeps_unicode() {
        assert_eq!(
            command(Path::new(r"C:\Users\Player Name\游戏\vstretch.exe")).unwrap(),
            "\"C:\\Users\\Player Name\\游戏\\vstretch.exe\""
        );
    }

    #[test]
    fn rejects_relative_quoted_and_overlong_paths() {
        assert!(command(Path::new("vstretch.exe")).is_err());
        assert!(command(Path::new("C:\\bad\"path.exe")).is_err());
        assert!(command(Path::new(&format!("C:\\{}\\vstretch.exe", "a".repeat(260)))).is_err());
    }
}
