# vstretch

**Wanna play stretch, then go back to native in one click? We got you.**

vstretch is a small Windows TUI for FPS players who run **stretched resolutions** in games like **Valorant** and **CS2**. Hop into stretch before a session, then flip back to native when you’re done — no digging through Windows display settings every time.

Built for the classic loop:

1. **Stretch** — lower res, wider models, the competitive look
2. **Play**
3. **Native** — one click (or one hotkey) and your desktop is normal again

![vstretch TUI — native ↔ stretch](assets/screenshot.png)

## Install

Run this in **Windows PowerShell** to install the latest release and add it to your user PATH:

```powershell
irm https://raw.githubusercontent.com/blocksdevpro/vstretch/main/install.ps1 | iex
```

The installer puts `vstretch.exe` in `%LOCALAPPDATA%\vstretch\bin`, verifies the download's SHA-256 checksum, and needs no admin rights. Run `vstretch` in PowerShell after installing. Open a new terminal if needed.

For **Git Bash on Windows**, the equivalent command is:

```sh
curl -fsSL https://raw.githubusercontent.com/blocksdevpro/vstretch/main/install.sh | sh
```

The shell installer calls Windows PowerShell. Vstretch requires Windows and does not run on Linux, macOS, or inside WSL.

You can also download the executable directly:

**Windows only.** No Rust, no cargo — just the exe.

1. Grab **`vstretch.exe`** from the [latest release](https://github.com/blocksdevpro/vstretch/releases/latest)
2. Put it somewhere handy (Desktop, `C:\Tools`, etc.)
3. Double-click to open the TUI, or run it from a terminal

Optional: add that folder to your PATH if you want `vstretch` / `vstretch -a` from anywhere (handy for hotkeys).

<details>
<summary>Build from source (Rust)</summary>

```powershell
cargo install --git https://github.com/blocksdevpro/vstretch.git
# or from a local clone:
cargo install --path .
```

</details>

## Usage

### TUI (default)

```powershell
vstretch
```

| Key | Action |
|-----|--------|
| `↑` `↓` / `j` `k` | Move |
| `Enter` | Select |
| `1`–`5` | Quick actions |
| `r` | Refresh display info |
| `u` | Install an available update, or check again |
| `q` / `Esc` | Quit (or back) |

Everything lives in the TUI: toggle, native, stretch, and picking default stretch / native resolutions.

### Updates

The TUI checks GitHub for a newer stable release in the background when you open it. An update banner appears when a new version is available. Press `u`, then `Enter` to download and install it, or `Esc` to keep using your current version. Quit and reopen Vstretch after installation.

Updates replace the executable in its current folder and preserve your display config. The download must match the release's SHA-256 digest before replacement. Offline checks show a retry message and keep the TUI usable. The hotkey command `--auto` skips update checks.

You can also check or update from a terminal:

```powershell
vstretch --version
vstretch --check-update
vstretch --update
```

To install in a different folder without changing PATH, download and run the installer with options:

```powershell
Invoke-WebRequest https://raw.githubusercontent.com/blocksdevpro/vstretch/main/install.ps1 -OutFile install.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 -InstallDir C:\Tools\vstretch -NoPath
```

Close Vstretch before running the installer again. The in-app updater can replace its own running executable.

### Hotkey toggle

One quiet command for PowerToys / AutoHotkey / etc.:

```powershell
vstretch --auto
# short:
vstretch -a
```

Prints one line, e.g. `1440x1080 @ 180Hz [1440x1080]`.

## Config

Created automatically on first launch at:

`%APPDATA%\vstretch\config.toml`

Change the defaults in the TUI under **Change default stretch…** / **Change default native…**.

Native is auto-detected from the panel unless `[native]` is set. Stretch uses panel refresh unless `refresh` is set on `[stretch]`.

```toml
# vstretch — omit [native] to auto-detect the panel

[stretch]
width = 1440
height = 1080

[native]
width = 2560
height = 1440
refresh = 180
```

## Notes

- **Windows only** (Win32 display APIs)
- Changes the **primary** display
- Native res is detected from the panel (CCD) unless you set an override
- Stretch requests full-screen scaling to fill the display
- Set the same resolution in-game. If a game overrides desktop scaling, select full-screen scaling in the game or GPU control panel

## License

MIT
