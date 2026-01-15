# Repository Guidelines

## Project Structure & Module Organization

- `src/main.rs`: CLI entry point (clap parsing) and command dispatch.
- `src/lib.rs`: library root; re-exports modules.
- `src/commands.rs`: high-level command orchestration (`list`, `add`, `use`, `doctor`, etc.).
- `src/paths.rs`: centralized filesystem paths (home/config discovery via `directories`).
- `src/profiles.rs`, `src/switch.rs`, `src/state.rs`: core profile/state/symlink logic.
- `src/ui.rs`: console output (tables/progress/color handling).
- Docs: `README.md` (usage), `DOCS.md` (architecture), `CLAUDE.md` (developer notes).

## Build, Test, and Development Commands

- `cargo build`: compile debug build.
- `cargo build --release`: optimized build.
- `cargo run -- <subcommand>`: run locally (example: `cargo run -- list`).
- `cargo install --path .`: install `ccprof` into `~/.cargo/bin`.
- `cargo test`: run unit tests (uses `tempfile` for isolated filesystem tests).
- `cargo fmt`: format with rustfmt.
- `cargo clippy`: lint and catch common Rust issues.

## Coding Style & Naming Conventions

- Use standard Rust formatting (`cargo fmt`) and keep code clippy-clean (`cargo clippy`).
- Rust conventions: 4-space indentation, `snake_case` for functions/modules, `PascalCase` for types.
- Prefer `anyhow::Result` with contextual errors (`.context(...)`) for fallible operations.
- Keep responsibilities separated: path discovery in `paths`, switching in `switch`, orchestration in `commands`.

## Testing Guidelines

- Prefer unit tests colocated with the module (existing examples in `src/components.rs` / `src/state.rs`).
- Tests must not touch real user data; use `tempfile::TempDir` and route all filesystem operations through test-only paths.
- Name tests for behavior (e.g., `switch_creates_symlink_when_missing_settings`).

## Commit & Pull Request Guidelines

- Follow the existing Conventional Commits style: `feat: ...`, `chore: ...` (short, imperative subject).
- PRs should include: what/why, how to test (exact commands), and any user-facing behavior changes (CLI output/flags).
- If changing filesystem behavior, call out safety/backup implications and add/adjust tests accordingly.
