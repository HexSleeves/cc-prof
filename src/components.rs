//! Component definitions and metadata.
//!
//! This module defines the `Component` enum which represents the different parts of
//! the Claude configuration that `ccprof` can manage (Settings, Agents, Hooks, Commands).
//!
//! It also handles `ProfileMetadata` serialization/deserialization, which tracks
//! the components managed by each profile.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::paths::Paths;

/// Types of components that can be managed by a profile
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Component {
    /// ~/.claude/settings.json
    Settings,
    /// ~/.claude/agents/
    Agents,
    /// ~/.claude/hooks/
    Hooks,
    /// ~/.claude/commands/
    Commands,
}

impl Component {
    /// Get all available components
    pub fn all() -> Vec<Self> {
        vec![Self::Settings, Self::Agents, Self::Hooks, Self::Commands]
    }

    /// Get user-friendly display name
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Settings => "Settings",
            Self::Agents => "Agents",
            Self::Hooks => "Hooks",
            Self::Commands => "Commands",
        }
    }

    /// Get short name for CLI args and filenames
    pub fn short_name(&self) -> &'static str {
        match self {
            Self::Settings => "settings",
            Self::Agents => "agents",
            Self::Hooks => "hooks",
            Self::Commands => "commands",
        }
    }

    /// Get source path in ~/.claude/
    pub fn source_path(&self, paths: &Paths) -> PathBuf {
        match self {
            Self::Settings => paths.claude_settings.clone(),
            Self::Agents => paths.claude_dir.join("agents"),
            Self::Hooks => paths.claude_dir.join("hooks"),
            Self::Commands => paths.claude_dir.join("commands"),
        }
    }

    /// Get storage path in ~/.claude-profiles/profiles/<name>/
    pub fn profile_path(&self, paths: &Paths, profile_name: &str) -> PathBuf {
        let profile_dir = paths.profile_dir(profile_name);
        match self {
            Self::Settings => profile_dir.join("settings.json"),
            Self::Agents => profile_dir.join("agents"),
            Self::Hooks => profile_dir.join("hooks"),
            Self::Commands => profile_dir.join("commands"),
        }
    }

    /// Is this component a single file? (vs a directory)
    pub fn is_file(&self) -> bool {
        matches!(self, Self::Settings)
    }
}

impl FromStr for Component {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "settings" | "settings.json" => Ok(Self::Settings),
            "agents" => Ok(Self::Agents),
            "hooks" => Ok(Self::Hooks),
            "commands" => Ok(Self::Commands),
            _ => Err(()),
        }
    }
}

/// Metadata stored in profile's metadata.json
#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileMetadata {
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub managed_components: HashSet<Component>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration: Option<MigrationInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MigrationInfo {
    pub original_version: String,
    pub migration_date: DateTime<Utc>,
}

impl ProfileMetadata {
    pub fn read(profile_dir: &Path) -> Result<Self> {
        let path = profile_dir.join("metadata.json");
        if !path.exists() {
            // Fallback for legacy profiles: assume only settings.json is managed
            return Ok(Self {
                version: "0.1.0".to_string(),
                created_at: Utc::now(), // We don't know real creation time
                updated_at: Utc::now(),
                managed_components: HashSet::from([Component::Settings]),
                migration: None,
            });
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read metadata from {}", path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse metadata from {}", path.display()))
    }

    pub fn write(&self, profile_dir: &Path) -> Result<()> {
        let path = profile_dir.join("metadata.json");
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)
            .with_context(|| format!("Failed to write metadata to {}", path.display()))?;
        Ok(())
    }
}