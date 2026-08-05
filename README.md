# vstretch

**Wanna play stretch, then go back to native in one click? We got you.**

vstretch is a small Windows TUI for FPS players who run **stretched resolutions** in games like **Valorant** and **CS2**. Hop into stretch before a session, then flip back to native when you’re done — no digging through Windows display settings every time.

Built for the classic loop:

1. **Stretch** — lower res, wider models, the competitive look
2. **Play**
3. **Native** — one click (or one hotkey) and your desktop is normal again

![vstretch TUI — native ↔ stretch](assets/screenshot.png)

## Install

Requires [Rust](https://rustup.rs/) on **Windows**.

```powershell
cargo install --git https://github.com/blocksdevpro/vstretch.git
# or from a local clone:
cargo install --path .
```

## Usage

### TUI (default)

```powershell
vstretch
```

| Key | Action |
|-----|--------|
| `↑` `↓` / `j` `k` | Move |
| `Enter` | Select |
| `1`–`4` | Quick actions |
| `r` | Refresh display info |
| `q` / `Esc` | Quit (or back) |

Everything lives in the TUI: toggle, native, stretch, and picking a popular default resolution.

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

Change the default stretch res in the TUI under **Change default stretch…**.

## Notes

- **Windows only** (Win32 display APIs)
- Changes the **primary** display
- Native res is detected from the panel (CCD)
- Still set the same res + stretch scaling in-game / GPU control panel as usual (Valorant, CS2, etc.)

## License

MIT
