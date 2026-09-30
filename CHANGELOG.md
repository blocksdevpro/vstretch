# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.2.0] - Unreleased

### Added
- Background update checks at TUI startup, with an update banner and confirmation prompt
- Download and install newer stable releases from the TUI with `u`, verifying SHA-256 before replacing the executable
- `--version`, `--check-update`, and `--update` commands
- PowerShell installer with a user PATH entry, plus a `curl ... | sh` entry point for Git Bash on Windows
- Offline installer verification script covering repeat installs, failed downloads, and executable replacement

## [1.1.1] - 2026-09-27

### Fixed
- Detect the primary monitor's panel mode and refresh rate instead of using the first active display
- Report the requested mode and actual display change error instead of the misleading "operation completed successfully" message
- Request full-screen scaling when applying stretch so 4:3 resolutions fill a widescreen display

## [1.1.0] - 2026-08-29

### Added
- Optional `[native]` override in config when panel auto-detect is wrong or you want a fixed restore target
- TUI action **Change default native…** with a picker over real Windows display modes (plus Auto-detect)
- Clearer mode badge: `NATIVE` / `STRETCH` / `PANEL` / `OTHER`
- Automatic migration from the old multi-profile config format on load

### Changed
- Config is now a single `[stretch]` target plus optional `[native]`, instead of a named profile map
- Stretch refresh prefers the panel Hz (not the native override Hz) unless `refresh` is set on `[stretch]`
- Config writes are atomic and include a short header comment
- README documents the new config shape and native picker

### Fixed
- More reliable native restore when a custom native size/refresh is configured
- Safer validation for zero width/height/refresh values

## [1.0.0] - 2026-08-05

### Added
- Initial public release: Windows TUI to toggle native ↔ stretch for FPS games
- `vstretch --auto` quiet toggle for hotkeys
- Popular stretch presets and first-run config under `%APPDATA%\vstretch\config.toml`

[1.2.0]: https://github.com/blocksdevpro/vstretch/compare/v1.1.1...HEAD
[1.1.1]: https://github.com/blocksdevpro/vstretch/compare/v1.1.0...v1.1.1
[1.1.0]: https://github.com/blocksdevpro/vstretch/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/blocksdevpro/vstretch/releases/tag/v1.0.0
