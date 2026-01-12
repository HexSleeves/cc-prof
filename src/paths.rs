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
    /// ~/.claude/agents
    pub claude_agents: PathBuf,
    /// ~/.claude/hooks
    pub claude_hooks: PathBuf,
    /// ~/.claude/commands
    pub claude_commands: PathBuf,
}

impl Paths {
    pub fn new() -> Result<Self> {
        let base_dirs = BaseDirs::new().context("Failed to determine home directory")?;
        let home = base_dirs.home_dir();

        let base_dir = home.join(".claude-profiles");
        let profiles_dir = base_dir.join("profiles");
        let backups_dir = base_dir.join("backups");
        let state_file = base_dir.join("state.json");
        let claude_dir = home.join(".claude");
        let claude_settings = claude_dir.join("settings.json");
        let claude_agents = claude_dir.join("agents");
        let claude_hooks = claude_dir.join("hooks");
        let claude_commands = claude_dir.join("commands");

        Ok(Self {
            base_dir,
            profiles_dir,
            backups_dir,
            state_file,
            claude_dir,
            claude_settings,
            claude_agents,
            claude_hooks,
            claude_commands,
        })
    }

    /// Get the path to a specific profile's settings.json
    pub fn profile_settings(&self, name: &str) -> PathBuf {
        self.profiles_dir.join(name).join("settings.json")
    }

    /// Get the path to a specific profile directory
    pub fn profile_dir(&self, name: &str) -> PathBuf {
        self.profiles_dir.join(name)
    }

    /// Get the path to a specific profile's metadata file
    pub fn profile_metadata(&self, name: &str) -> PathBuf {
        self.profile_dir(name).join("profile.json")
    }

    /// Check if a path is within the profiles directory
    pub fn is_in_profiles_dir(&self, path: &std::path::Path) -> bool {
        path.starts_with(&self.profiles_dir)
    }

    /// Ensure all required directories exist
    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.profiles_dir).with_context(|| {
            format!(
                "Failed to create profiles directory: {:?}",
                self.profiles_dir
            )
        })?;
        std::fs::create_dir_all(&self.backups_dir).with_context(|| {
            format!("Failed to create backups directory: {:?}", self.backups_dir)
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_settings_path() {
        let paths = Paths::new().unwrap();
        let profile_path = paths.profile_settings("work");
        assert!(profile_path.ends_with("profiles/work/settings.json"));
    }

    #[test]
    fn test_is_in_profiles_dir() {
        let paths = Paths::new().unwrap();
        let profile_path = paths.profile_settings("test");
        assert!(paths.is_in_profiles_dir(&profile_path));
        assert!(!paths.is_in_profiles_dir(&paths.claude_settings));
    }
}
