# vstretch

**Wanna play stretch, then go back to native in one click? We got you.**

vstretch is a Windows tray app for FPS players who use stretched resolutions in Valorant and CS2. Double-click the executable, then use its tray menu to switch between stretch and your native desktop resolution.

Built for the classic loop:

1. **Stretch** — lower res, wider models, the competitive look
2. **Play**
3. **Native** — one click (or one hotkey) and your desktop is normal again

## Install

No Rust or terminal is required for the downloaded app:

1. Download **`vstretch.exe`** from the [latest release](https://github.com/blocksdevpro/vstretch/releases/latest).
2. Keep it in a permanent folder, such as `C:\Tools\vstretch`.
3. Double-click it. Vstretch appears in the system tray without opening a console window.
4. Click its tray icon to open the menu. If Windows hides the icon, open the tray overflow with the arrow beside the clock.

To install with **Windows PowerShell** and add Vstretch to your user PATH, run:

```powershell
irm https://raw.githubusercontent.com/blocksdevpro/vstretch/main/install.ps1 | iex
```

The installer puts `vstretch.exe` in `%LOCALAPPDATA%\vstretch\bin`, verifies the download's SHA-256 checksum, and needs no admin rights. Double-click that executable or run `vstretch` to start the tray app.

For **Git Bash on Windows**, the equivalent command is:

```sh
curl -fsSL https://raw.githubusercontent.com/blocksdevpro/vstretch/main/install.sh | sh
```

The shell installer calls Windows PowerShell. Vstretch requires Windows and does not run on Linux, macOS, or inside WSL.

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

### System tray (default)

```powershell
vstretch
```

Left-click or right-click the tray icon to open the menu:

| Menu item | Action |
| --- | --- |
| **Mode → Native** | Apply the detected native resolution or your saved native override |
| **Mode → Stretch** | Apply your saved stretch resolution |
| **Presets** | Save a stretch preset; if stretch is active, apply the new preset immediately |
| **Auto-stretch Valorant & CS2** | Apply stretch when a supported game gains focus; keep it active until the game exits |
| **Restore desktop on Alt+Tab** | Opt in to restoring the desktop when the game loses focus and reapplying stretch when you return |
| **Start with Windows** | Enable or disable launch at sign-in for your Windows account |
| **Exit** | Close the tray app; restore an automatic display change that it still owns |

The Mode checkmarks reflect the current display. Preset checkmarks reflect your saved choice. Opening the app again keeps a single tray instance.

Auto-stretch is enabled by default. It detects `VALORANT-Win64-Shipping.exe` and `cs2.exe`, applies your preset when the game gains focus, and restores the previous desktop when you close the game. Alt+Tab leaves the resolution unchanged by default. Enable **Restore desktop on Alt+Tab** if you want focus-based switching. The option is off for new and existing configurations unless you enable it.

Disable auto-stretch in the menu if you prefer manual switching. Manual mode choices and display changes from other apps pause the automatic session until the game exits. If you opt in to Alt+Tab restoration, that pause lasts until the game loses focus.

**Start with Windows** is off by default. It adds a per-user startup entry and requires no admin rights. Keep the executable in the same folder after enabling it. If you move it, enable the option again from its new location.

Errors appear in the tray menu and tooltip. Manual failures also open an error dialog. Failed automatic switches wait for the current session to end before trying again. Manually applied modes remain active after Exit.

### Terminal interface

Open the keyboard interface for custom native settings and in-app updates:

```powershell
vstretch --tui
```

![vstretch terminal interface](assets/screenshot.png)

| Key | Action |
|-----|--------|
| `↑` `↓` / `j` `k` | Move |
| `Enter` | Select |
| `1`–`5` | Quick actions |
| `r` | Refresh display info |
| `u` | Install an available update, or check again |
| `q` / `Esc` | Quit (or back) |

The terminal interface includes mode switching and pickers for default stretch and native resolutions. The tray reloads saved configuration changes while it runs.

### Updates

The terminal interface checks GitHub for a newer stable release in the background when you open it with `--tui`. An update banner appears when a new version is available. Press `u`, then `Enter` to download and install it, or `Esc` to keep using your current version. Quit and reopen Vstretch after installation.

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

Choose a stretch preset in the tray menu. Use `vstretch --tui` to set a custom native resolution under **Change default native…**.

Native is auto-detected from the panel unless `[native]` is set. Stretch uses panel refresh unless `refresh` is set on `[stretch]`.

Set `VSTRETCH_CONFIG` to an absolute file path to use a separate configuration, such as for portable use or development checks.

```toml
# vstretch — omit [native] to auto-detect the panel

auto_stretch = true
restore_on_alt_tab = false

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

## Development checks

Run `cargo test` and `cargo clippy --all-targets -- -D warnings`. Run `powershell -NoProfile -File scripts/test-tray.ps1` to check the built executable's CLI output, console-free launch, isolated first-run configuration, native menu events, and duplicate launch handling. The tray check uses temporary config and does not switch resolutions or edit Windows startup entries.

Add `-Release` to check the optimized executable instead of the debug build.

For local integration checks, add `-SwitchDisplay` to temporarily apply 1280x960 and 1440x1080 and verify automatic and manual restoration. Add `-CheckStartup` to verify the actual Windows startup entry. These checks restore the original display mode and startup value when finished.

## License

MIT
