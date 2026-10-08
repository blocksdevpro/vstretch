# vstretch

**Wanna play stretch, then go back to native in one click? We got you.**

vstretch is a Windows tray app for FPS players who use stretched resolutions in Valorant and CS2. Double-click the executable, then use its tray menu to switch between stretch and your native desktop resolution.

This README describes the current source checkout. See the [changelog](CHANGELOG.md) for released changes and updates under **Unreleased**.

Built for the classic loop:

1. **Stretch** — lower res, wider models, the competitive look
2. **Play**
3. **Native** — one click (or one hotkey) and your desktop is normal again

## Install

No Rust or terminal is required for the downloaded app:

1. Download **`vstretch.exe`** from [vstretch.blocksdev.pro/download](https://vstretch.blocksdev.pro/download).
2. Keep it in a permanent folder, such as `C:\Tools\vstretch`.
3. Double-click it. Vstretch appears in the system tray without opening a console window.
4. Click its tray icon to open the menu. If Windows hides the icon, open the tray overflow with the arrow beside the clock.

To install with **Windows PowerShell** and add Vstretch to your user PATH, run:

```powershell
irm https://vstretch.blocksdev.pro/install.ps1 | iex
```

The installer puts `vstretch.exe` in `%LOCALAPPDATA%\vstretch\bin`, verifies the download's SHA-256 checksum, creates a per-user Start Menu shortcut, and needs no admin rights. Search for **vstretch** in Start to open or reopen the tray app, or run `vstretch` from a terminal. Running the installer again also repairs the shortcut.

For **Git Bash on Windows**, the equivalent command is:

```sh
curl -fsSL https://vstretch.blocksdev.pro/install.sh | sh
```

The shell installer calls Windows PowerShell. Vstretch requires Windows and does not run on Linux, macOS, or inside WSL.

Optional: add that folder to your PATH if you want `vstretch` / `vstretch -a` from anywhere (handy for hotkeys).

<details>
<summary>Build from source (Rust)</summary>

Build on Windows with the MSVC toolchain and Visual Studio C++ build tools. Local builds and CI use Rust 1.99, pinned in `rust-toolchain.toml`.

```powershell
cargo install --locked --git https://github.com/blocksdevpro/vstretch.git
# or from a local clone:
cargo install --locked --path .
```

</details>

## Usage

### System tray (default)

```powershell
vstretch
```

Left-click or right-click the tray icon to open the menu:

The header shows the installed app version and current display. Manual mode controls and stretch presets come first, followed by **Automatic switching**, **Settings**, and **Exit**. Windows aligns resolutions and shortcuts in the menu's right-hand column.

| Menu item | Action |
| --- | --- |
| **Toggle mode** | Toggle Native ↔ Stretch immediately; the global hotkey appears beside the action |
| **Native** | Apply the detected native resolution or your saved native override |
| **Stretch** | Apply your saved stretch resolution |
| **Stretch presets** | Save a stretch preset; if stretch is active, apply the new preset immediately |
| **Automatic switching → Auto-stretch Valorant & CS2** | Apply stretch when a supported game gains focus; keep it active until the game exits |
| **Automatic switching → Restore desktop on Alt+Tab** | Opt in to restoring the desktop when the game loses focus and reapplying stretch when you return |
| **Settings → Enable hotkey** | Enable or disable the global toggle hotkey |
| **Settings → Start with Windows** | Enable or disable launch at sign-in for your Windows account |
| **Exit** | Close the tray app; restore an automatic display change that it still owns |

The Native and Stretch checkmarks reflect the current display. Preset checkmarks reflect your saved choice. Vstretch stays running as a background process while its tray icon is active; no main window needs to stay open. Opening it again with the same configuration keeps a single tray instance. Windows may place its icon in the tray overflow beside the clock.

**Stretch presets** lists built-in resolutions with their aspect ratios. A saved resolution outside that list appears as **Custom**. If Vstretch cannot resolve a native target, **Native** shows **Unavailable**, and both **Native** and **Stretch** are disabled. Set a native override through the TUI to supply a restore target.

Auto-stretch is enabled by default. It detects `VALORANT-Win64-Shipping.exe` and `cs2.exe`, applies your preset when the game gains focus, and restores the previous desktop when you close the game. Alt+Tab leaves the resolution unchanged by default. Enable **Restore desktop on Alt+Tab** if you want focus-based switching. The option is off for new and existing configurations unless you enable it.

Disable auto-stretch in the menu if you prefer manual switching. Manual mode choices and display changes from other apps pause the automatic session until the game exits. If you opt in to Alt+Tab restoration, that pause lasts until the game loses focus.

**Start with Windows** is enabled automatically on the first tray launch. It adds a per-user startup entry and requires no admin rights. You can turn it off in the tray menu; that choice is remembered across launches. Keep the executable in the same folder. If you move it, enable the option again from its new location.

Errors appear in the tray menu and tooltip. Manual failures also open an error dialog. Failed automatic switches wait for the current session to end before trying again. Manually applied modes remain active after Exit.

**Exit** ends the background process and automatic detection. To bring it back, search for **vstretch** in Start if you used the installer, or double-click your saved executable. With **Start with Windows** enabled, it also opens automatically at your next sign-in.

Stretch switches are temporary and do not replace the saved Windows desktop mode used after reboot. If Vstretch crashes while it owns a display change, its next launch restores the previous desktop resolution, refresh rate, and reported scaling setting. Recovery runs only after the owning process has ended and the same monitor still has the recorded mode. Changes from another app or the user are left in place.

An unfinished session is recorded in `%APPDATA%\vstretch\config.recovery.toml`, next to the configuration file. Clean Exit and successful hotkey commands clear their recovery record, preserving manual modes until you change them or reboot. If an older Vstretch version saved your current stretch preset as the Windows default, choose **Native** once with this version to repair that saved default.

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
Invoke-WebRequest https://vstretch.blocksdev.pro/install.ps1 -OutFile install.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 -InstallDir C:\Tools\vstretch -NoPath
```

`-NoPath` still creates the Start Menu shortcut. Add `-NoShortcut` to skip it.

Close Vstretch before running the installer again. The in-app updater can replace its own running executable.

### Hotkey toggle

The built-in global hotkey **Ctrl+Alt+S** toggles Native ↔ Stretch while the tray app runs, including in-game. The tray menu shows the current combo beside **Toggle mode** and **Settings → Enable hotkey**. Disabling it preserves your saved combo.

To change it, edit `hotkey` in `config.toml`, for example to `Ctrl+Shift+F9`. The tray re-registers it automatically. Set `hotkey = ""` to clear it. If Windows cannot register the combo because another app uses it, the tray shows an error and keeps running. Invalid config edits show a reload error and leave the last valid configuration active.

For a PowerToys or AutoHotkey binding, use the quiet command below. Run `vstretch` or `vstretch --tui` once first to create the configuration. The tray does not need to stay open for this command.

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

The default stretch resolution is 1440×1080. Native is auto-detected from the panel unless `[native]` is set. Stretch uses panel refresh unless `refresh` is set on `[stretch]`. If a native override omits `refresh`, it uses panel Hz, falling back to 60 Hz when panel detection fails.

Set `VSTRETCH_CONFIG` to an absolute file path to use a separate configuration, such as for portable use or development checks.

Default configuration after the first tray launch:

```toml
# vstretch — omit [native] to auto-detect the panel

auto_stretch = true
restore_on_alt_tab = false
start_with_windows = true
hotkey = "Ctrl+Alt+S"
hotkey_enabled = true

[stretch]
width = 1440
height = 1080
```

To override native detection, add this section with your desktop's dimensions and refresh rate:

```toml
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
- Animated wallpaper apps must react to the desktop resolution change. Wallpaper Engine has been reported to show sizing artifacts, black lines, temporary pixelation, or frozen animation during Stretch; returning to Native fixes the reported problem. Repeated selections skip unchanged display settings, but a complete wallpaper fix is still under investigation.

## Development checks

```powershell
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
.\target\release\vstretch.exe --version
```

CI runs these checks on Windows for pushes and pull requests targeting `main` and `develop`. For the landing page's separate build and checks, see the [website README](website/README.md).

Display-switching, Windows shell, and startup integration tests are ignored by default. Run them individually with `cargo test <test-name> -- --ignored --exact --nocapture`; tray integration tests require `VSTRETCH_CONFIG` set to an isolated absolute config path. Real display and startup tests temporarily change those Windows settings and restore them afterward.

## License

MIT
