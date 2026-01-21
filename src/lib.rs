//! Claude Code Profile Switcher (ccprof).
//!
//! `ccprof` is a library and CLI tool for managing multiple configurations for Claude Code.
//! It works by managing symbolic links in `~/.claude/` that point to profile-specific
//! configuration files stored in `~/.claude-profiles/`.
//!
//! # Architecture
//!
//! - **Profiles**: Named configurations stored in `~/.claude-profiles/profiles/`.
//! - **Components**: Parts of the configuration (Settings, Agents, Hooks, Commands).
//! - **Switching**: Atomically updating symlinks to change the active profile.
//! - **State**: Tracking the active profile in `state.json`.

pub mod commands;
pub mod components;
pub mod doctor;
pub mod fs_utils;
pub mod paths;
pub mod profiles;
pub mod state;
pub mod switch;
#[cfg(test)]
pub mod test_utils;
pub mod ui;