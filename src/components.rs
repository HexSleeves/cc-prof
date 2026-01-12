use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::paths::Paths;

/// Component types that can be managed by ccprof profiles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Component {
    Settings,
    Agents,
    Hooks,
    Commands,
}

impl Component {
    /// Get all available component types
    pub fn all() -> Vec<Component> {
        vec![
            Component::Settings,
            Component::Agents,
            Component::Hooks,
            Component::Commands,
        ]
    }

    /// Get the source path in ~/.claude/
    pub fn source_path(&self, paths: &Paths) -> PathBuf {
        match self {
            Component::Settings => paths.claude_settings.clone(),
            Component::Agents => paths.claude_agents.clone(),
            Component::Hooks => paths.claude_hooks.clone(),
            Component::Commands => paths.claude_commands.clone(),
        }
    }

    /// Get the profile-specific path
    pub fn profile_path(&self, paths: &Paths, profile: &str) -> PathBuf {
        let base = paths.profile_dir(profile);
        match self {
            Component::Settings => base.join("settings.json"),
            Component::Agents => base.join("agents"),
            Component::Hooks => base.join("hooks"),
            Component::Commands => base.join("commands"),
        }
    }

    /// Check if this is a file component (vs directory)
    pub fn is_file(&self) -> bool {
        matches!(self, Component::Settings)
    }

    /// Get human-readable display name
    pub fn display_name(&self) -> &'static str {
        match self {
            Component::Settings => "Settings (settings.json)",
            Component::Agents => "Agents (agents/*.md)",
            Component::Hooks => "Hooks (hooks/*.sh)",
            Component::Commands => "Commands (commands/*.md)",
        }
    }

    /// Get short identifier for display
    pub fn short_name(&self) -> &'static str {
        match self {
            Component::Settings => "S",
            Component::Agents => "A",
            Component::Hooks => "H",
            Component::Commands => "C",
        }
    }
}

impl FromStr for Component {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "settings" => Ok(Component::Settings),
            "agents" => Ok(Component::Agents),
            "hooks" => Ok(Component::Hooks),
            "commands" => Ok(Component::Commands),
            _ => Err(format!("Unknown component: {}", s)),
        }
    }
}

/// Information about profile migration from legacy format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationInfo {
    pub migrated_from_legacy: bool,
    pub migration_date: DateTime<Utc>,
}

/// Metadata for a profile, tracking which components it manages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMetadata {
    pub version: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub managed_components: HashSet<Component>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration: Option<MigrationInfo>,
}

impl ProfileMetadata {
    /// Create new metadata for a profile
    pub fn new(name: String, components: HashSet<Component>) -> Self {
        let now = Utc::now();
        Self {
            version: "1.0".to_string(),
            name,
            created_at: now,
            updated_at: now,
            managed_components: components,
            migration: None,
        }
    }

    /// Create metadata for a legacy profile (settings-only)
    pub fn from_legacy(name: String) -> Self {
        let mut components = HashSet::new();
        components.insert(Component::Settings);

        let now = Utc::now();
        Self {
            version: "1.0".to_string(),
            name,
            created_at: now,
            updated_at: now,
            managed_components: components,
            migration: Some(MigrationInfo {
                migrated_from_legacy: true,
                migration_date: now,
            }),
        }
    }

    /// Read metadata from profile directory
    /// Auto-detects legacy profiles and creates appropriate metadata
    pub fn read(profile_dir: &Path) -> Result<Self> {
        let metadata_path = profile_dir.join("profile.json");

        // If profile.json doesn't exist, this is a legacy profile
        if !metadata_path.exists() {
            let name = profile_dir
                .file_name()
                .and_then(|n| n.to_str())
                .context("Invalid profile directory name")?
                .to_string();
            return Ok(Self::from_legacy(name));
        }

        // Read and parse profile.json
        let content = fs::read_to_string(&metadata_path).context("Failed to read profile.json")?;
        serde_json::from_str(&content).context("Failed to parse profile.json")
    }

    /// Write metadata to profile directory
    pub fn write(&self, profile_dir: &Path) -> Result<()> {
        let metadata_path = profile_dir.join("profile.json");
        let content = serde_json::to_string_pretty(self).context("Failed to serialize metadata")?;
        fs::write(metadata_path, content).context("Failed to write profile.json")?;
        Ok(())
    }

    /// Silently create profile.json for legacy profile (migration)
    pub fn migrate_legacy(profile_dir: &Path) -> Result<()> {
        let name = profile_dir
            .file_name()
            .and_then(|n| n.to_str())
            .context("Invalid profile directory name")?
            .to_string();

        let metadata = Self::from_legacy(name);
        metadata.write(profile_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_component_paths() {
        let temp = TempDir::new().unwrap();
        let home = temp.path();
        unsafe { std::env::set_var("HOME", home) };

        let paths = Paths::new().unwrap();

        // Test source paths
        assert!(paths.claude_settings.ends_with("settings.json"));
        assert!(paths.claude_agents.ends_with("agents"));
        assert!(paths.claude_hooks.ends_with("hooks"));
        assert!(paths.claude_commands.ends_with("commands"));

        // Test profile paths
        let profile_settings = Component::Settings.profile_path(&paths, "test");
        assert!(profile_settings.ends_with("profiles/test/settings.json"));

        let profile_agents = Component::Agents.profile_path(&paths, "test");
        assert!(profile_agents.ends_with("profiles/test/agents"));
    }

    #[test]
    fn test_component_properties() {
        assert!(Component::Settings.is_file());
        assert!(!Component::Agents.is_file());
        assert!(!Component::Hooks.is_file());
        assert!(!Component::Commands.is_file());

        assert_eq!(Component::Settings.short_name(), "S");
        assert_eq!(Component::Agents.short_name(), "A");
        assert_eq!(Component::Hooks.short_name(), "H");
        assert_eq!(Component::Commands.short_name(), "C");
    }

    #[test]
    fn test_component_from_str() {
        assert_eq!("settings".parse::<Component>(), Ok(Component::Settings));
        assert_eq!("AGENTS".parse::<Component>(), Ok(Component::Agents));
        assert_eq!("Hooks".parse::<Component>(), Ok(Component::Hooks));
        assert_eq!("commands".parse::<Component>(), Ok(Component::Commands));
        assert!("invalid".parse::<Component>().is_err());
    }

    #[test]
    fn test_metadata_serialization() {
        let mut components = HashSet::new();
        components.insert(Component::Settings);
        components.insert(Component::Agents);

        let metadata = ProfileMetadata::new("test".to_string(), components);
        let json = serde_json::to_string(&metadata).unwrap();
        let parsed: ProfileMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "test");
        assert_eq!(parsed.version, "1.0");
        assert_eq!(parsed.managed_components.len(), 2);
        assert!(parsed.managed_components.contains(&Component::Settings));
        assert!(parsed.managed_components.contains(&Component::Agents));
        assert!(parsed.migration.is_none());
    }

    #[test]
    fn test_legacy_metadata() {
        let metadata = ProfileMetadata::from_legacy("legacy".to_string());

        assert_eq!(metadata.name, "legacy");
        assert_eq!(metadata.managed_components.len(), 1);
        assert!(metadata.managed_components.contains(&Component::Settings));
        assert!(metadata.migration.is_some());
        assert!(metadata.migration.unwrap().migrated_from_legacy);
    }

    #[test]
    fn test_metadata_read_write() {
        let temp = TempDir::new().unwrap();
        let profile_dir = temp.path().join("test-profile");
        fs::create_dir_all(&profile_dir).unwrap();

        let mut components = HashSet::new();
        components.insert(Component::Settings);
        components.insert(Component::Hooks);

        let metadata = ProfileMetadata::new("test-profile".to_string(), components);
        metadata.write(&profile_dir).unwrap();

        let read_metadata = ProfileMetadata::read(&profile_dir).unwrap();
        assert_eq!(read_metadata.name, "test-profile");
        assert_eq!(read_metadata.managed_components.len(), 2);
    }

    #[test]
    fn test_legacy_profile_detection() {
        let temp = TempDir::new().unwrap();
        let profile_dir = temp.path().join("legacy");
        fs::create_dir_all(&profile_dir).unwrap();

        // Create settings.json but no profile.json (legacy format)
        fs::write(profile_dir.join("settings.json"), "{}").unwrap();

        let metadata = ProfileMetadata::read(&profile_dir).unwrap();
        assert_eq!(metadata.name, "legacy");
        assert_eq!(metadata.managed_components.len(), 1);
        assert!(metadata.managed_components.contains(&Component::Settings));
        assert!(metadata.migration.is_some());
    }
}
