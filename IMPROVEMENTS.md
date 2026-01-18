# ccprof Improvement Plan

This document outlines prioritized improvements, optimizations, and enhancements for the ccprof codebase. Items are organized into phases for incremental delivery.

---

## Phase 1: Critical Fixes (Priority: Immediate) ✅ COMPLETED

These issues affect correctness, stability, or prevent building on stable Rust.

### 1.2 Fix Unstable `let-chains` Syntax ✅

**File:** `src/profiles.rs:36-38`
**Issue:** Using `if path.is_dir() && let Some(name) = ...` requires nightly Rust
**Fix:** Refactor to nested `if` statements for stable Rust compatibility
**Status:** DONE - Refactored to nested if statements with explanatory comment

### 1.3 Fix Unsafe `set_var` in Tests ✅

**File:** `src/components.rs:195`
**Issue:** `unsafe { std::env::set_var("HOME", home) }` is unsound in multi-threaded test environment
**Fix:** Use test isolation pattern or `temp_env` crate
**Status:** DONE - Removed unsafe set_var, tests now use `Paths` struct with temp directories

### 1.4 Remove Duplicate `tempfile` Dependency ✅

**File:** `Cargo.toml`
**Issue:** `tempfile` listed in both `[dependencies]` and `[dev-dependencies]`
**Fix:** Keep only in `[dev-dependencies]` (not used at runtime)
**Status:** DONE - Now only in dev-dependencies

---

## Phase 2: Code Quality & Cleanup (Priority: High) ✅ COMPLETED

Improve maintainability and reduce technical debt.

### 2.1 Extract Duplicate Test Helpers ✅

**Files:** `commands.rs`, `profiles.rs`, `switch.rs`, `doctor.rs`
**Issue:** `setup_test_paths()` is copy-pasted across 4 modules
**Fix:** Create `#[cfg(test)] pub mod test_utils` in `lib.rs` or `src/test_utils.rs`
**Status:** DONE - Created `src/test_utils.rs` with shared `setup_test_paths()` function

### 2.2 Extract Nested Helper Functions ✅

**Files:** `commands.rs:238-267`, `switch.rs:215-244`
**Issue:** `dir_size()` and `copy_dir_recursive()` are nested functions that could be reused
**Fix:** Move to a shared `fs_utils` module or into `paths.rs`
**Status:** DONE - Created `src/fs_utils.rs` with `dir_size()` and `copy_dir_recursive()`

### 2.3 Remove Deprecated Standalone Functions ✅

**File:** `src/ui.rs:338-359`
**Issue:** `ok()`, `warn()`, `err()`, `dim()` are marked deprecated but still exported
**Fix:** Either remove entirely or add proper `#[deprecated(since, note)]` attributes
**Status:** N/A - No deprecated functions found in current codebase (already cleaned up)

### 2.4 Fix Error Message Formatting ✅

**File:** `src/commands.rs:159-163`
**Issue:** Uses `\\n\\` which produces literal characters instead of newlines
**Fix:** Use proper escape sequences `\n`
**Status:** DONE - All error messages now use proper `\n` escape sequences

### 2.5 Clean Up cfg Attribute Suppression ✅

**File:** `src/switch.rs:282-283`
**Issue:** `#[cfg_attr(unix, allow(unused_variables))]` suppresses warnings instead of proper handling
**Fix:** Use proper `#[cfg(unix)]` / `#[cfg(windows)]` conditional compilation
**Status:** DONE - Now uses proper `#[cfg(unix)]` and `#[cfg(windows)]` conditional compilation

### 2.6 Standardize Error Message Style ✅

**Files:** Multiple
**Issue:** Inconsistent error formats - some use hints, some don't
**Fix:** Establish pattern: primary message + optional `\nHint: ...` for actionable items
**Status:** DONE - All user-facing errors now follow consistent pattern with helpful hints

---

## Phase 3: Robustness & Safety (Priority: High)

Prevent data loss and handle edge cases.

### 3.1 Atomic Writes for State File

**File:** `src/state.rs`
**Issue:** Direct write to `state.json` - crash during write corrupts file
**Fix:** Write to temp file, then atomic rename

```rust
let temp = state_path.with_extension("json.tmp");
fs::write(&temp, contents)?;
fs::rename(&temp, state_path)?;
```

**Effort:** 30 min

### 3.2 Fix Symlink Detection on Directories

**File:** `src/switch.rs:285-286`
**Issue:** `source.is_dir()` returns false for symlinks to directories on some platforms
**Fix:** Check with `fs::symlink_metadata()` first
**Effort:** 20 min

### 3.3 Stricter Profile Name Validation

**File:** `src/profiles.rs`
**Issue:** No validation for special characters, unicode, emojis in profile names
**Fix:** Restrict to `[a-zA-Z0-9_-]` pattern
**Effort:** 30 min

### 3.4 Add Backup Rotation

**File:** `src/switch.rs` or new `src/backup.rs`
**Issue:** Backups accumulate forever in `~/.claude-profiles/backups/`
**Fix:** Add `--max-backups` config or auto-cleanup (keep last N)
**Effort:** 1 hour

---

## Phase 4: New Features (Priority: Medium)

User-requested functionality to complete the CLI experience.

### 4.1 Add `remove` Command

**Estimated Effort:** 2 hours

```bash
ccprof remove <name>        # Delete a profile
ccprof remove <name> --force  # Skip confirmation
```

- Validate profile exists
- Prevent removing active profile (or switch away first)
- Interactive confirmation unless `--force`
- Remove profile directory and update state if needed

### 4.2 Add `rename` Command

**Estimated Effort:** 2 hours

```bash
ccprof rename <old> <new>
```

- Validate old profile exists, new name doesn't
- Rename directory
- Update state.json if it was the active profile
- Update symlinks if currently active

### 4.3 Add `diff` Command

**Estimated Effort:** 3 hours

```bash
ccprof diff <profile1> <profile2>
ccprof diff <profile1> <profile2> --component settings
```

- Compare settings.json between profiles
- Show added/removed/changed keys
- Optional: Use `similar` crate for nice diff output

### 4.4 Add `backup` Subcommands

**Estimated Effort:** 2 hours

```bash
ccprof backup list              # List all backups with timestamps
ccprof backup restore <id>      # Restore a specific backup
ccprof backup clean --keep 5    # Remove old backups
```

### 4.5 Add `export`/`import` Commands

**Estimated Effort:** 3 hours

```bash
ccprof export <name> <path.tar.gz>  # Export profile to archive
ccprof import <path.tar.gz> [name]  # Import profile from archive
```

- Include metadata.json in archive
- Handle component selection on import

### 4.6 Add Shell Completions

**Estimated Effort:** 1 hour

```bash
ccprof completions bash > ~/.bash_completion.d/ccprof
ccprof completions zsh > ~/.zfunc/_ccprof
ccprof completions fish > ~/.config/fish/completions/ccprof.fish
```

- Use `clap_complete` crate
- Dynamic completion for profile names

### 4.7 Enhance `edit` Command for Components

**Estimated Effort:** 1 hour

```bash
ccprof edit <name> --component agents  # Open agents.json
ccprof edit <name> --all               # Open all managed components
```

---

## Phase 5: Performance Optimizations (Priority: Low)

Micro-optimizations - implement only if profiling shows need.

### 5.1 Cache Profile List

**Issue:** `list_profiles()` re-reads filesystem on every call
**Fix:** Consider caching for commands that call it multiple times
**Effort:** 1 hour
**Note:** Profile with benchmarks first - may not be noticeable

### 5.2 Reduce Redundant Metadata Reads

**Files:** `doctor.rs`, `commands.rs`
**Issue:** `ProfileMetadata::read()` called multiple times for same profile
**Fix:** Read once and pass reference
**Effort:** 30 min

### 5.3 Make `Component::all()` Const

**File:** `src/components.rs`
**Issue:** Allocates Vec on every call
**Fix:** `pub const ALL: [Component; 4] = [...]`
**Effort:** 15 min

### 5.4 Avoid format! in Display Impls

**Files:** Multiple
**Issue:** `format!()` allocations where `write!()` would suffice
**Fix:** Use `write!(f, ...)` directly
**Effort:** 30 min

---

## Phase 6: Testing Improvements (Priority: Medium)

Increase confidence in correctness.

### 6.1 Add Integration Tests

**Location:** `tests/` directory
**Scope:**

- End-to-end CLI tests using `assert_cmd` crate
- Test complete workflows: add → use → edit → list
- Test error cases and user-facing messages
**Effort:** 4 hours

### 6.2 Add Edge Case Unit Tests

**Scope:**

- Profile with 0 components (should error)
- Very long profile names (255+ chars)
- Profile names with unicode/special chars
- Concurrent profile switching
- Corrupted state.json recovery
**Effort:** 2 hours

### 6.3 Add Fuzzing Targets

**Location:** `fuzz/` directory
**Scope:**

- JSON parsing (settings.json, state.json, metadata.json)
- Profile name validation
**Effort:** 2 hours

---

## Phase 7: Documentation (Priority: Medium)

Improve discoverability and onboarding.

### 7.1 Add Module-Level Documentation

**Files:** `commands.rs`, `profiles.rs`, `switch.rs`, `state.rs`, `components.rs`
**Fix:** Add `//!` doc comments explaining module purpose
**Effort:** 1 hour

### 7.2 Add Public API Documentation

**Scope:** All `pub` functions without `///` docs
**Key functions:**

- `switch_to_profile`
- `create_profile_with_components`
- `list_profiles`
- `Paths::new`
- `State::read`, `State::write`
**Effort:** 2 hours

### 7.3 Create CHANGELOG.md

**Content:**

- Version history
- Breaking changes
- New features
- Bug fixes
**Effort:** 30 min (initial setup)

### 7.4 Update README.md

**Missing:**

- `inspect` command documentation
- Component selection for `add` command
- Environment variables section
- Troubleshooting section
**Effort:** 1 hour

---

## Phase 8: Modern Rust Patterns (Priority: Low)

Optional improvements for code elegance.

### 8.1 Add `#[must_use]` Attributes

**Scope:** Functions returning `bool` or `Result` that are queries

- `profile_exists()`
- `validate_profile_name()`
- `validate_json_file()`
**Effort:** 30 min

### 8.2 Consider `camino` Crate

**Issue:** Many `path.to_str().unwrap()` calls assume UTF-8
**Fix:** Use `camino::Utf8Path` for explicit UTF-8 paths
**Effort:** 2 hours (moderate refactor)

### 8.3 Consider `thiserror` for Library Errors

**Issue:** `anyhow` is great for apps but not ideal for library code
**Fix:** Define custom error types with `thiserror` for `lib.rs` exports
**Effort:** 3 hours

### 8.4 Use `Path::try_exists()`

**Issue:** `.exists()` silently swallows permission errors
**Fix:** Use `try_exists()` (stable since Rust 1.63) for explicit error handling
**Effort:** 1 hour

---

## Implementation Order (Recommended)

| Week | Phase | Focus |
|------|-------|-------|
| 1 | Phase 1 | Critical fixes - get to stable Rust |
| 1 | Phase 2.1-2.4 | Quick code quality wins |
| 2 | Phase 2.5-2.6 | Finish code quality |
| 2 | Phase 3 | Robustness improvements |
| 3 | Phase 4.1-4.3 | Core new features (remove, rename, diff) |
| 4 | Phase 4.4-4.7 | Remaining features |
| 5 | Phase 6 | Testing improvements |
| 5 | Phase 7 | Documentation |
| 6 | Phase 5, 8 | Optimizations and polish |

---

## Quick Wins (Can Be Done Anytime)

These are low-effort, high-value improvements:

- [x] Fix Cargo.toml edition (5 min) - Using edition 2024 with Rust 1.92+
- [x] Remove duplicate tempfile dep (5 min) - Now only in dev-dependencies
- [x] Fix error message escaping (10 min) - All messages use proper `\n`
- [ ] Add `#[must_use]` attributes (30 min)
- [ ] Make `Component::ALL` const (15 min)
- [ ] Add shell completions (1 hour)

---

## Metrics

| Category | Count | Estimated Total Effort |
|----------|-------|------------------------|
| Critical Fixes | 4 | ~1 hour |
| Code Quality | 6 | ~4 hours |
| Robustness | 4 | ~2.5 hours |
| New Features | 7 | ~14 hours |
| Performance | 4 | ~2.5 hours |
| Testing | 3 | ~8 hours |
| Documentation | 4 | ~4.5 hours |
| Modern Patterns | 4 | ~6.5 hours |
| **Total** | **36** | **~43 hours** |

---

## Version Roadmap

### v0.2.0 - Stability Release

- All Phase 1 (Critical Fixes) ✅ COMPLETED
- Phase 2 (Code Quality) ✅ COMPLETED
- Phase 3 (Robustness) - IN PROGRESS

### v0.3.0 - Feature Complete

- Phase 4.1-4.4 (remove, rename, diff, backup commands)
- Phase 6.1 (Integration tests)

### v0.4.0 - Polish Release

- Phase 4.5-4.7 (export/import, completions, edit enhancements)
- Phase 7 (Documentation)

### v1.0.0 - Production Ready

- All remaining phases
- Full test coverage
- Complete documentation
