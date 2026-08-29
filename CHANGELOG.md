# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[1.1.0]: https://github.com/blocksdevpro/vstretch/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/blocksdevpro/vstretch/releases/tag/v1.0.0
