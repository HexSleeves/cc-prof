//! Test utilities shared across test modules
//!
//! This module provides common helper functions for testing, avoiding duplication
//! across multiple test suites.

use crate::paths::Paths;
use tempfile::TempDir;

/// Create a Paths struct for testing using a temporary directory
///
/// This creates a complete directory structure for ccprof within the temp directory,
/// mimicking the real ~/.claude-profiles/ and ~/.claude/ layout.
pub fn setup_test_paths(temp_dir: &TempDir) -> Paths {
    Paths {
        base_dir: temp_dir.path().join(".claude-profiles"),
        profiles_dir: temp_dir.path().join(".claude-profiles/profiles"),
        backups_dir: temp_dir.path().join(".claude-profiles/backups"),
        state_file: temp_dir.path().join(".claude-profiles/state.json"),
        claude_dir: temp_dir.path().join(".claude"),
        claude_settings: temp_dir.path().join(".claude/settings.json"),
    }
}
