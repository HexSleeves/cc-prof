# ccprof Technical Documentation

This document provides a deeper dive into the architecture, internals, and advanced usage of `ccprof`.

## Architecture

`ccprof` is a Rust CLI application structured as a library (`lib.rs`) with a binary entry point (`main.rs`).

### Module Structure

- **`main.rs`**: Entry point. Parses command-line arguments using `clap` and dispatches to command handlers.
- **`lib.rs`**: Library root, re-exporting modules.
- **`commands.rs`**: High-level handlers for each CLI command (`list`, `add`, `use`, etc.). Orchestrates interactions between the UI, paths, and logic.
- **`paths.rs`**: Centralized management of filesystem paths (`~/.claude-profiles`, etc.). specific to the user's OS (via `directories` crate).
- **`profiles.rs`**: Core logic for profile management (listing, creating, validating).
- **`switch.rs`**: Logic for switching profiles, handling backups, and managing symlinks.
- **`state.rs`**: Manages the persistent state file (`state.json`) which tracks the active profile.
- **`ui.rs`**: Abstraction for console output, colors, tables (using `comfy-table`), and progress indicators (using `indicatif`).
- **`doctor.rs`**: Diagnostics logic.

### Data Storage

`ccprof` creates a hidden directory in your home folder:

- **Mac/Linux**: `~/.claude-profiles`
- **Windows**: `C:\Users\<User>\.claude-profiles`

Inside this directory:

- `profiles/`: Subdirectories for each profile (e.g., `work/settings.json`).
- `backups/`: timestamped backups of `settings.json` created when `ccprof` replaces a regular file with a symlink.
- `state.json`: A JSON file recording the name of the currently active profile and the timestamp of the last switch.

## Symlink Mechanism

The core feature of `ccprof` is manipulating `~/.claude/settings.json`.

1. **Detection**: `ccprof` checks if `~/.claude/settings.json` is a regular file, a symlink, or missing.
2. **Safety**:
   - If it's a **regular file**, `ccprof` moves it to the `backups` directory before creating a symlink. This ensures no data loss.
   - If it's a **symlink** managed by `ccprof`, it updates the link to point to the new profile.
   - If it's a **broken symlink**, it forces an update.

## Environment Variables

- `EDITOR`: Used by the `ccprof edit` command to determine which text editor to open.
  - If not set, `ccprof` attempts to use the system default (e.g., `open -t` on macOS).
- `RUST_LOG`: If built with `env_logger` (dev dependency), this controls logging verbosity.

## Error Handling

`ccprof` uses `anyhow` for error handling. Common errors you might encounter:

- **Permission Denied**: If `ccprof` cannot write to `~/.claude` or `~/.claude-profiles`.
- **Invalid JSON**: If a profile's `settings.json` is corrupted, `ccprof` will warn you but may still allow switching (use `ccprof doctor` to verify).
- **Profile Already Exists**: When trying to `add` a profile with a name that is already taken.

## Development

### Building

```bash
cargo build --release
```

### Testing

Run the test suite, which uses `tempfile` to create isolated environments for filesystem tests.

```bash
cargo test
```

### formatting

Ensure code adheres to standard Rust formatting:

```bash
cargo fmt
```

### Linting

Run Clippy to catch common mistakes:

```bash
cargo clippy
```
