//! Centralized filesystem path management.
//!
//! This module handles the discovery and construction of all filesystem paths
//! used by `ccprof`. It relies on the `directories` crate to find standard
//! system locations (e.g., home directory).
//!
//! The `Paths` struct acts as a "single source of truth" for locations,
//! ensuring consistency across the application.

use anyhow::{Context, Result};
use directories::BaseDirs;
use std::path::PathBuf;

/// All computed paths used by ccprof
#[derive(Debug, Clone)]
pub struct Paths {
    /// ~/.claude-profiles
    pub base_dir: PathBuf,
    /// ~/.claude-profiles/profiles
    pub profiles_dir: PathBuf,
    /// ~/.claude-profiles/backups
    pub backups_dir: PathBuf,
    /// ~/.claude-profiles/state.json
    pub state_file: PathBuf,

    /// ~/.claude
    pub claude_dir: PathBuf,
    /// ~/.claude/settings.json
    pub claude_settings: PathBuf,
}

impl Paths {
    pub fn new() -> Result<Self> {
        let base_dirs = BaseDirs::new().context("Could not determine user base directories")?;
        let home = base_dirs.home_dir();

        let base_dir = home.join(".claude-profiles");
        let claude_dir = home.join(".claude");

        Ok(Self {
            profiles_dir: base_dir.join("profiles"),
            backups_dir: base_dir.join("backups"),
            state_file: base_dir.join("state.json"),
            base_dir,

            claude_settings: claude_dir.join("settings.json"),
            claude_dir,
        })
    }

    /// Ensure that the base directories exist
    pub fn ensure_dirs(&self) -> Result<()> {
        if !self.base_dir.exists() {
            std::fs::create_dir_all(&self.base_dir)?;
        }
        if !self.profiles_dir.exists() {
            std::fs::create_dir_all(&self.profiles_dir)?;
        }
        if !self.backups_dir.exists() {
            std::fs::create_dir_all(&self.backups_dir)?;
        }
        Ok(())
    }

    /// Get path to a specific profile directory
    pub fn profile_dir(&self, name: &str) -> PathBuf {
        self.profiles_dir.join(name)
    }

    /// Get path to a specific profile's settings.json
    pub fn profile_settings(&self, name: &str) -> PathBuf {
        self.profile_dir(name).join("settings.json")
    }

    /// Check if a path is inside the profiles directory
    pub fn is_in_profiles_dir(&self, path: &std::path::Path) -> bool {
        // Canonicalize paths to resolve symlinks and absolute paths if possible
        // But for symlinks we might be checking the target.
        // Simple prefix check:
        path.starts_with(&self.profiles_dir)
    }
}