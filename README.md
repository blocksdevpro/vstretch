# vstretch

Windows CLI to switch between **native** and **stretch** display resolutions — built for Valorant players who are tired of flipping settings by hand.

## Install

Requires [Rust](https://rustup.rs/) on **Windows**.

```powershell
# from crates.io (after publish)
cargo install vstretch

# from GitHub
cargo install --git https://github.com/blocksdevpro/vstretch.git

# from a local clone
cargo install --path .
```

Binary lands in `%USERPROFILE%\.cargo\bin\vstretch.exe` (must be on your `PATH`).

## Quick start

```powershell
# create config (%APPDATA%\vstretch\config.toml)
vstretch init

# pick a popular stretch res as default (interactive)
vstretch set-default

# or set one directly
vstretch set-default 1440x1080

# toggle native ↔ stretch (bind this to a hotkey)
vstretch auto

# explicit switches
vstretch stretch
vstretch native
```

## Commands

| Command | Description |
|---------|-------------|
| `vstretch auto` | Toggle native ↔ default stretch profile |
| `vstretch stretch` | Apply default (or `-p <profile>`) stretch res |
| `vstretch native` | Restore panel native resolution |
| `vstretch init` | Create default config |
| `vstretch config` | Show config path and profiles |
| `vstretch presets` | List built-in popular stretch resolutions |
| `vstretch set-default` | Interactive picker / set default stretch |

## Config

Path: `%APPDATA%\vstretch\config.toml`

```toml
default_profile = "1440x1080"

[profiles.1440x1080]
width = 1440
height = 1080

[profiles.1280x960]
width = 1280
height = 960
```

Optional per-profile refresh (omit to keep native Hz):

```toml
[profiles.1440x1080]
width = 1440
height = 1080
refresh = 240
```

## Popular presets

Built-in list includes common 4:3 / 5:4 / 16:10 stretch sizes (1024×768, 1280×960, 1440×1080, 1728×1080, …). See `vstretch presets`.

## Notes

- **Windows only** (Win32 display APIs).
- Changes the **primary** display.
- Native resolution is detected from the panel (CCD), not hard-coded.
- This only changes Windows display mode. Set the same res + stretch scaling in Valorant / GPU control panel as you normally would.

## License

MIT
