# vstretch

Windows **TUI + CLI** to switch between **native** and **stretch** display resolutions — built for Valorant.

```text
┌─ vstretch ──────────────────────────────────┐
│  Mode      NATIVE                           │
│  Current   1920x1080 @ 144Hz                │
│  Native    1920x1080 @ 144Hz                │
│  Default   1440x1080  (1440x1080)           │
├─────────────────────────────────────────────┤
│  › Toggle  native ↔ stretch                 │
│    Apply stretch (default profile)          │
│    Apply native                             │
│    Change default stretch…                  │
│    Quit                                     │
└─────────────────────────────────────────────┘
```

## Install

Requires [Rust](https://rustup.rs/) on **Windows**.

```powershell
cargo install --git https://github.com/blocksdevpro/vstretch.git
# or from a local clone:
cargo install --path .
```

## Usage

### Interactive TUI (default)

```powershell
vstretch
# or
vstretch ui
```

| Key | Action |
|-----|--------|
| `↑` `↓` / `j` `k` | Move |
| `Enter` | Select |
| `1`–`4` | Quick actions |
| `r` | Refresh display info |
| `q` / `Esc` | Quit (or back on preset list) |

### Quiet CLI (for hotkeys)

One-line output — bind these in PowerToys / AutoHotkey / etc.

```powershell
vstretch auto              # toggle native ↔ stretch
vstretch stretch           # stretch only
vstretch native            # native only
vstretch set-default 1440x1080
```

### Other

```powershell
vstretch init              # create config
vstretch config            # show profiles
vstretch presets           # list popular stretch res
```

## Config

`%APPDATA%\vstretch\config.toml`

```toml
default_profile = "1440x1080"

[profiles.1440x1080]
width = 1440
height = 1080
```

Change the default in the TUI (**Change default stretch…**) or:

```powershell
vstretch set-default 1280x960
```

## Notes

- **Windows only** (Win32 display APIs)
- Changes the **primary** display
- Native res is detected from the panel (CCD)
- Still set the same res + stretch scaling in Valorant / GPU control panel as usual

## License

MIT
