# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`ccprof` is a CLI tool for managing multiple Claude Code settings profiles through symlink-based switching. The tool manipulates `~/.claude/settings.json` (and other components like `agents/`, `hooks/`) by creating symbolic links to profile-specific settings stored in `~/.claude-profiles/profiles/`.

## Common Development Commands

### Building and Running

```bash
# Build the project
cargo build

# Build release version
cargo build --release

# Install locally to ~/.cargo/bin
cargo install --path .

# Run without installing
cargo run -- <subcommand>
```

### Testing and Quality

```bash
# Run all tests (uses tempfile for isolated filesystem tests)
cargo test

# Run specific test
cargo test <test_name>

# Format code
cargo fmt

# Lint with clippy
cargo clippy
```

### Testing the CLI

```bash
# Basic workflows
cargo run -- list
cargo run -- add test-profile --from-current
cargo run -- use test-profile
cargo run -- current
cargo run -- doctor

# Inspect and Manage
cargo run -- inspect test-profile
cargo run -- diff test-profile default
cargo run -- edit test-profile
cargo run -- rename test-profile my-profile
cargo run -- remove my-profile

# Backup Management
cargo run -- backup list
cargo run -- backup clean --keep 5
```

## Architecture

The codebase follows a modular architecture with clear separation of concerns:

### Module Responsibilities

- **`main.rs`**: CLI entry point using `clap` for argument parsing. Initializes `Paths` and `Ui` structs, then dispatches to command handlers.

- **`commands.rs`**: High-level orchestration for each CLI command. Coordinates between paths, profiles, state, switch logic, and UI.

- **`paths.rs`**: Single source of truth for filesystem paths. Provides the `Paths` struct with all relevant directories (`~/.claude-profiles/`, `~/.claude/`, etc.) computed once at startup.

- **`profiles.rs`**: Core profile management logic - listing, validation, existence checks. Does NOT handle switching; that's in `switch.rs`.

- **`components.rs`**: Component definitions (Settings, Agents, etc.) and profile metadata management.

- **`switch.rs`**: Profile switching logic with backup handling. Creates/updates symlinks at `~/.claude/settings.json` pointing to active profile. Implements safety mechanisms (backups before overwriting, broken symlink detection).

- **`state.rs`**: Manages persistent state file (`~/.claude-profiles/state.json`) tracking the currently active profile name and last switch timestamp.

- **`doctor.rs`**: Diagnostics for common issues (broken symlinks, invalid JSON, permission problems).

- **`ui.rs`**: UI abstraction layer using `comfy-table` for tables, `indicatif` for progress indicators, and `anstyle` for colors. Respects `--no-color` and `--color` flags.

### Key Data Flow

1. **Profile Creation** (`add` command):
   - Validates profile name (profiles.rs)
   - Determines components to include (interactive or CLI arg)
   - Copies current files from `~/.claude/` to `~/.claude-profiles/profiles/<name>/`
   - Creates `metadata.json`

2. **Profile Switching** (`use` command):
   - Checks profile exists (profiles.rs)
   - For each managed component:
     - Backs up existing file/dir if it's not a symlink (switch.rs)
     - Removes old symlink if present
     - Creates new symlink pointing to selected profile
   - Updates state.json with active profile name (state.rs)

3. **Symlink Safety**:
   - Regular file → backs up to `~/.claude-profiles/backups/` with timestamp
   - Existing symlink → updates target
   - Broken symlink → removes and recreates
   - Missing file → creates symlink

### Critical Filesystem Paths

All paths are centralized in the `Paths` struct:

- `~/.claude-profiles/` - Base directory for ccprof data
- `~/.claude-profiles/profiles/<name>/` - Individual profile data
- `~/.claude-profiles/backups/` - Timestamped backups
- `~/.claude-profiles/state.json` - Tracks active profile
- `~/.claude/` - Target directory for symlinks

## Important Patterns

### Error Handling

Uses `anyhow` throughout for error context. Most functions return `Result<T>` and chain `.context()` calls for helpful error messages.

### Symlink Detection

The `switch.rs` module detects symlink status using `fs::symlink_metadata()` to distinguish between regular files and symlinks without following them.

### JSON Validation

Profile settings.json files are validated using `serde_json` before creating/switching profiles to prevent broken configurations.

### Testing Strategy

Tests use `tempfile::TempDir` to create isolated test environments with temporary `~/.claude-profiles/` directories, ensuring tests don't affect real user data.
