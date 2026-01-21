# Changelog

All notable changes to this project will be documented in this file.

## [0.3.0] - 2026-01-18

### Added

- **Component Management**: Support for managing `agents`, `hooks`, and `commands` directories in addition to `settings.json`.
- **Inspect Command**: `ccprof inspect <name>` to view profile details and managed components.
- **Remove Command**: `ccprof remove <name>` to safely delete profiles.
- **Rename Command**: `ccprof rename <old> <new>` to rename profiles and update active symlinks.
- **Diff Command**: `ccprof diff` to compare settings or other components between two profiles.
- **Backup Management**: `ccprof backup` suite (`list`, `restore`, `clean`) to manage automatic backups.
- **Shell Completions**: `ccprof completions` to generate scripts for Bash, Zsh, Fish, PowerShell, and Elvish.
- **Enhanced Edit**: `ccprof edit` now supports opening specific components (`--component`) or all components (`--all`), and changing tracked components (`--track`).
- **Enhanced Add**: `ccprof add` now supports specifying components via CLI (`--components`) or interactive selection.
- **Backup Rotation**: Automatically cleans up old backups (keeps last 10 by default) to save space.

### Changed

- **Architecture**: Refactored to support generic "Components" rather than hardcoded `settings.json` handling.
- **State Management**: Using `state.json` with atomic writes and file locking for safety.
- **Documentation**: Comprehensive updates to README, architecture docs, and internal module documentation.

### Fixed

- **Symlink Detection**: Improved detection logic to handle broken symlinks and directory symlinks correctly across platforms.
- **Error Messages**: Standardized error reporting with actionable hints.
- **Safety**: Added checks to prevent removing the currently active profile.

## [0.2.0] - 2025-12-28

### Added

- Initial release of `ccprof`.
- Basic profile management (`list`, `add`, `use`).
- `doctor` command for diagnostics.
- `current` command for status.
- Symlink-based switching mechanism.
