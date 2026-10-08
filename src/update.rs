//! Checks stable GitHub releases and verifies downloads before replacing the app.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::windows::{fs::OpenOptionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use reqwest::blocking::Client;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const RELEASE_API: &str = "https://api.github.com/repos/blocksdevpro/vstretch/releases/latest";
const DOWNLOAD_PREFIX: &str = "https://github.com/blocksdevpro/vstretch/releases/download/";
const MAX_BINARY_SIZE: u64 = 64 * 1024 * 1024;

/// A newer stable release whose download URL, size, and digest passed validation.
#[derive(Clone, Debug)]
pub struct Release {
    pub version: Version,
    download_url: String,
    size: u64,
    sha256: String,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

fn client(timeout: Duration) -> Result<Client> {
    Client::builder()
        .user_agent(concat!("vstretch/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .connect_timeout(Duration::from_secs(5))
        .timeout(timeout)
        .build()
        .context("create update client")
}

pub fn check() -> Result<Option<Release>> {
    let response = client(Duration::from_secs(10))?
        .get(RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .context("could not reach GitHub")?
        .error_for_status()
        .context("GitHub update check failed")?;
    let release = response.json().context("invalid GitHub release response")?;
    select_release(release, &Version::parse(env!("CARGO_PKG_VERSION"))?)
}

fn select_release(release: GithubRelease, current: &Version) -> Result<Option<Release>> {
    let version = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )
    .context("invalid release version")?;
    if release.draft || release.prerelease || !version.pre.is_empty() || version <= *current {
        return Ok(None);
    }
    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name == "vstretch.exe")
        .context("release has no Windows executable")?;
    let expected_url = format!("{DOWNLOAD_PREFIX}{}/vstretch.exe", release.tag_name);
    ensure!(
        asset.browser_download_url == expected_url,
        "unexpected release download URL"
    );
    ensure!(
        asset.size > 0 && asset.size <= MAX_BINARY_SIZE,
        "invalid release binary size"
    );
    let sha256 = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .context("release has no SHA-256 digest")?;
    ensure!(
        sha256.len() == 64 && sha256.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid release SHA-256 digest"
    );
    Ok(Some(Release {
        version,
        download_url: asset.browser_download_url,
        size: asset.size,
        sha256: sha256.to_ascii_lowercase(),
    }))
}

pub fn install(release: &Release) -> Result<()> {
    let response = client(Duration::from_secs(120))?
        .get(&release.download_url)
        .send()
        .context("download update")?
        .error_for_status()
        .context("update download failed")?;
    let mut bytes = Vec::new();
    // Read one extra byte to detect a response larger than the declared asset.
    response
        .take(release.size + 1)
        .read_to_end(&mut bytes)
        .context("read update download")?;
    install_download(&bytes, release)
}

fn install_download(bytes: &[u8], release: &Release) -> Result<()> {
    verify_binary(bytes, release)?;
    let executable = std::env::current_exe().context("locate vstretch.exe")?;
    let directory = executable.parent().context("executable has no directory")?;
    // The installer uses the same exclusive file handle, including during repairs.
    let _lock = installation_lock(directory)?;
    let mut staged = tempfile::Builder::new()
        .prefix("vstretch-update-")
        .suffix(".exe")
        .tempfile()
        .context("stage update")?;
    staged.write_all(bytes).context("write update")?;
    staged.flush()?;
    replace_executable(staged.path())?;
    refresh_integration(&executable)
        .context("executable updated, but Windows integration repair failed; rerun the installer")
}

fn installation_lock(directory: &Path) -> Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(directory.join(".install.lock"))
        .context("installation is busy or not writable; close other installers and retry")
}

fn refresh_integration(executable: &Path) -> Result<()> {
    let mut script = tempfile::Builder::new().suffix(".ps1").tempfile()?;
    script.write_all(include_bytes!("../install.ps1"))?;
    script.flush()?;
    // PowerShell opens scripts with sharing that excludes a live writer.
    let script = script.into_temp_path();
    let powershell = PathBuf::from(std::env::var_os("SystemRoot").context("locate Windows")?)
        .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let output = Command::new(powershell)
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script)
        .arg("-RepairExecutable")
        .arg(executable)
        .arg("-RefreshExistingOnly")
        .output()
        .context("run Windows integration repair")?;
    ensure!(
        output.status.success(),
        "Windows integration repair: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

/// Watches the original launch path, even when Windows renames the running image.
/// Missing/locked files during replacement are retried on the next tray tick.
pub struct ExecutableWatch {
    pub path: PathBuf,
    modified: std::time::SystemTime,
    length: u64,
    digest: Vec<u8>,
}

impl ExecutableWatch {
    pub fn new(path: PathBuf) -> Result<Self> {
        let metadata = fs::metadata(&path)?;
        Ok(Self {
            digest: Sha256::digest(fs::read(&path)?).to_vec(),
            modified: metadata.modified()?,
            length: metadata.len(),
            path,
        })
    }

    pub fn changed(&self) -> bool {
        let Some(directory) = self.path.parent() else {
            return false;
        };
        // Don't restart until replacement and shell repairs have both finished.
        let Ok(_lock) = installation_lock(directory) else {
            return false;
        };
        let Ok(metadata) = fs::metadata(&self.path) else {
            return false;
        };
        if metadata.len() == self.length && metadata.modified().ok() == Some(self.modified) {
            return false;
        }
        fs::read(&self.path)
            .map(|bytes| Sha256::digest(bytes).as_slice() != self.digest)
            .unwrap_or(false)
    }
}

fn replace_executable(replacement: &Path) -> Result<()> {
    let executable = std::env::current_exe()
        .context("locate vstretch.exe")?
        .canonicalize()
        .context("resolve vstretch.exe path")?;
    let directory = executable.parent().context("executable has no directory")?;
    let backup = tempfile::Builder::new()
        .prefix("vstretch-backup-")
        .suffix(".exe")
        .tempfile_in(directory)
        .context("back up vstretch.exe; move it to a writable folder if access is denied")?;
    fs::copy(&executable, backup.path()).context("back up vstretch.exe")?;

    // self-replace can move the original away before copying the replacement fails.
    if let Err(error) = self_replace::self_replace(replacement) {
        if executable.exists() {
            return Err(error).context("replace vstretch.exe; existing executable was kept");
        }
        if let Err(restore) = backup.persist_noclobber(&executable) {
            let mut backup = restore.file;
            backup.disable_cleanup(true);
            anyhow::bail!(
                "update failed: {error}; restore failed: {}; original executable is saved at {}",
                restore.error,
                backup.path().display()
            );
        }
        return Err(error).context("update failed; original executable was restored");
    }
    Ok(())
}

fn verify_binary(bytes: &[u8], release: &Release) -> Result<()> {
    ensure!(
        bytes.len() as u64 == release.size,
        "update download size does not match the release"
    );
    ensure!(
        format!("{:x}", Sha256::digest(bytes)) == release.sha256,
        "update checksum mismatch; existing executable was kept"
    );
    ensure!(
        bytes.starts_with(b"MZ"),
        "update is not a Windows executable"
    );
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn release(tag: &str) -> GithubRelease {
        GithubRelease {
            tag_name: tag.into(),
            draft: false,
            prerelease: false,
            assets: vec![GithubAsset {
                name: "vstretch.exe".into(),
                browser_download_url: format!("{DOWNLOAD_PREFIX}{tag}/vstretch.exe"),
                size: 3,
                digest: Some(format!("sha256:{:x}", Sha256::digest(b"MZ!"))),
            }],
        }
    }

    pub fn available_release() -> Release {
        select_release(release("v9.0.0"), &Version::parse("1.2.0").unwrap())
            .unwrap()
            .unwrap()
    }

    #[test]
    fn replaces_a_running_executable_on_windows() {
        use std::{fs, process::Command};
        const CHILD_REPLACEMENT: &str = "VSTRETCH_SELF_REPLACE_TEST";
        if let Some(replacement) = std::env::var_os(CHILD_REPLACEMENT) {
            let bytes = fs::read(replacement).unwrap();
            let mut release = available_release();
            release.size = bytes.len() as u64;
            release.sha256 = format!("{:x}", Sha256::digest(&bytes));
            // Exercise verified staging, replacement, the shared install lock,
            // and the real embedded PowerShell repair in the running child.
            install_download(&bytes, &release).unwrap();
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("vstretch-test.exe");
        let replacement = directory.path().join("replacement.exe");
        let mut updated = fs::read(std::env::current_exe().unwrap()).unwrap();
        fs::write(&original, &updated).unwrap();
        // A PE overlay keeps the test executable runnable and gives it a different hash.
        updated.extend_from_slice(b"vstretch replacement test");
        fs::write(&replacement, &updated).unwrap();
        let child = Command::new(&original)
            .args([
                "--exact",
                "update::tests::replaces_a_running_executable_on_windows",
            ])
            .env(CHILD_REPLACEMENT, &replacement)
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
        assert_eq!(fs::read(&original).unwrap(), updated);
        assert!(
            Command::new(&original)
                .arg("--list")
                .output()
                .unwrap()
                .status
                .success()
        );
    }

    #[test]
    fn failed_replacement_keeps_a_runnable_executable() {
        use std::{fs, process::Command};
        const CHILD_FAILURE: &str = "VSTRETCH_FAILED_REPLACE_TEST";
        if std::env::var_os(CHILD_FAILURE).is_some() {
            let executable = std::env::current_exe().unwrap();
            let missing = executable.with_file_name("missing-replacement.exe");
            assert!(replace_executable(&missing).is_err());
            assert!(executable.exists(), "failed update removed the executable");
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("vstretch-test.exe");
        let original = fs::read(std::env::current_exe().unwrap()).unwrap();
        fs::write(&executable, &original).unwrap();
        let child = Command::new(&executable)
            .args([
                "--exact",
                "update::tests::failed_replacement_keeps_a_runnable_executable",
            ])
            .env(CHILD_FAILURE, "1")
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stdout)
        );
        assert_eq!(fs::read(&executable).unwrap(), original);
        assert!(
            Command::new(&executable)
                .arg("--list")
                .output()
                .unwrap()
                .status
                .success()
        );
    }

    #[test]
    fn compares_versions_numerically_and_never_downgrades() {
        let current = Version::parse("1.2.0").unwrap();
        assert!(
            select_release(release("v1.10.0"), &current)
                .unwrap()
                .is_some()
        );
        assert!(
            select_release(release("v1.2.0"), &current)
                .unwrap()
                .is_none()
        );
        assert!(
            select_release(release("v1.1.1"), &current)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn ignores_drafts_and_prereleases() {
        let current = Version::parse("1.2.0").unwrap();
        let mut draft = release("v2.0.0");
        draft.draft = true;
        assert!(select_release(draft, &current).unwrap().is_none());
        let mut preview = release("v2.0.0");
        preview.prerelease = true;
        assert!(select_release(preview, &current).unwrap().is_none());
        assert!(
            select_release(release("v2.0.0-beta.1"), &current)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn rejects_missing_asset_digest_and_external_url() {
        let current = Version::parse("1.2.0").unwrap();
        let mut missing = release("v2.0.0");
        missing.assets.clear();
        assert!(select_release(missing, &current).is_err());
        let mut unsigned = release("v2.0.0");
        unsigned.assets[0].digest = None;
        assert!(select_release(unsigned, &current).is_err());
        let mut external = release("v2.0.0");
        external.assets[0].browser_download_url = "https://example.com/vstretch.exe".into();
        assert!(select_release(external, &current).is_err());
    }

    #[test]
    fn verifies_download_before_replacement() {
        let selected = select_release(release("v2.0.0"), &Version::parse("1.2.0").unwrap())
            .unwrap()
            .unwrap();
        assert!(verify_binary(b"MZ!", &selected).is_ok());
        assert!(verify_binary(b"MZ", &selected).is_err());
        assert!(verify_binary(b"MZ?", &selected).is_err());
        let mut invalid = selected;
        invalid.sha256 = format!("{:x}", Sha256::digest(b"bad"));
        assert!(verify_binary(b"bad", &invalid).is_err());
    }

    #[test]
    fn tray_waits_for_installation_then_detects_the_replaced_image() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("vstretch.exe");
        fs::write(&executable, b"old image").unwrap();
        let watch = ExecutableWatch::new(executable.clone()).unwrap();
        assert!(!watch.changed());
        let lock = installation_lock(directory.path()).unwrap();
        assert!(installation_lock(directory.path()).is_err());
        fs::remove_file(&executable).unwrap();
        assert!(!watch.changed());
        fs::write(&executable, b"updated image").unwrap();
        assert!(
            !watch.changed(),
            "tray restarted before shell repair completed"
        );
        drop(lock);
        assert!(watch.changed());
    }

    #[test]
    fn unchanged_or_temporarily_missing_image_does_not_restart_the_tray() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("vstretch.exe");
        fs::write(&executable, b"old image").unwrap();
        let watch = ExecutableWatch::new(executable.clone()).unwrap();
        let backup = directory.path().join("backup.exe");
        fs::rename(&executable, &backup).unwrap();
        assert!(!watch.changed());
        fs::copy(&backup, &executable).unwrap();
        assert!(!watch.changed(), "identical content should not restart");
    }
}
