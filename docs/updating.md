# Update and installer design

`update::check()` returns an optional newer stable `Release`. `update::install(&release)` downloads, verifies, and replaces the current executable. The CLI calls these directly. The TUI runs them on worker threads and reads their results through channels.

`src/update.rs` owns GitHub API parsing, version comparisons, download limits, checksum verification, and executable replacement. `src/tui.rs` owns the check, available, confirmation, installing, installed, and failure states. Startup starts one check, and `u` retries on request. `--auto` performs no network requests.

The checker accepts a stable release only when its version is greater than the running package version. A release must have a `vstretch.exe` asset at the expected repository URL and a valid GitHub SHA-256 digest. The download is limited to the asset size and checked before replacement. Check requests time out after 10 seconds, and download requests after 120 seconds. No GitHub credentials are needed.

`self-replace` handles Windows executable locks and removes the old executable after the running process exits. The running session keeps its original code until the user quits and reopens Vstretch. Config lives elsewhere and is never modified by an update. Quitting is blocked while replacement is in progress.

The alternative of invoking PowerShell from the app would require a separate helper script and coordinating process exit with replacement. Keeping replacement in Rust lets the app report completion directly and leaves the installer independent. Blocking the TUI during startup would delay display actions when GitHub is unreachable, so it uses channels instead.

`install.ps1` installs to `%LOCALAPPDATA%\vstretch\bin` by default. It verifies release metadata and downloads before replacing a file. It uses an exclusive file lock for overlapping runs and atomic replacement for an existing executable. Repeating an install with the same checksum skips the download. The installer adds one user PATH entry, and `-NoPath` disables PATH changes. `install.sh` supports Git Bash on Windows and delegates to PowerShell.

GitHub digests verify that downloads match the release asset. They do not protect against a compromised repository publishing malicious assets. App replacement needs write permission in the executable's folder. Network errors, rate limits, missing assets, and verification failures remain visible in the update banner.

## Verify

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
.\target\release\vstretch.exe --version
.\target\release\vstretch.exe --check-update
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-installer.ps1
# In Git Bash on Windows:
sh scripts/test-shell-installer.sh
```

The two display hardware tests remain opt-in. Tests for updates use release fixtures, so they do not need an actual newer public release. Installer tests mock GitHub and downloads, use a folder under `target`, and leave user PATH untouched.

## Publish

Merge the scripts before sharing the installer commands. Both one-command installers use the `main` branch on `raw.githubusercontent.com` and fetch the latest stable GitHub release. Publish future releases with a `vstretch.exe` asset. GitHub supplies its SHA-256 digest automatically. Vstretch 1.1.1 has no update checker, so existing users need to download or install 1.2.0 once to receive future update notices.
