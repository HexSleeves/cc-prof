use anyhow::{Context, Result, bail};
use chrono::Utc;
use std::fs;
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::Path;
use tempfile::NamedTempFile;

use crate::paths::Paths;
use crate::profiles::validate_json_file;
use crate::state::LockedState;

/// Information about the current settings file status
#[derive(Debug, Clone)]
pub enum SettingsStatus {
    /// File is missing
    Missing,
    /// Regular file (not a symlink)
    RegularFile,
    /// Symlink pointing to the given target
    Symlink { target: std::path::PathBuf },
    /// Broken symlink (target doesn't exist)
    BrokenSymlink { target: std::path::PathBuf },
}

impl SettingsStatus {
    pub fn detect(path: &Path) -> Self {
        // Check if it's a symlink first (before checking exists)
        match fs::read_link(path) {
            Ok(target) => {
                // It's a symlink - check if target exists
                let resolved = if target.is_absolute() {
                    target.clone()
                } else {
                    path.parent().unwrap_or(Path::new(".")).join(&target)
                };

                if resolved.exists() {
                    SettingsStatus::Symlink { target }
                } else {
                    SettingsStatus::BrokenSymlink { target }
                }
            }
            Err(_) => {
                // Not a symlink - check if file exists
                if path.exists() {
                    SettingsStatus::RegularFile
                } else {
                    SettingsStatus::Missing
                }
            }
        }
    }

    /// Check if this is a symlink pointing into the profiles directory
    pub fn is_profile_symlink(&self, paths: &Paths) -> bool {
        match self {
            SettingsStatus::Symlink { target } | SettingsStatus::BrokenSymlink { target } => {
                // Resolve to absolute path if relative
                let resolved = if target.is_absolute() {
                    target.clone()
                } else {
                    paths.claude_dir.join(target)
                };
                paths.is_in_profiles_dir(&resolved)
            }
            _ => false,
        }
    }
}

impl std::fmt::Display for SettingsStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettingsStatus::Missing => write!(f, "missing"),
            SettingsStatus::RegularFile => write!(f, "regular file"),
            SettingsStatus::Symlink { target } => write!(f, "symlink -> {:?}", target),
            SettingsStatus::BrokenSymlink { target } => write!(f, "broken symlink -> {:?}", target),
        }
    }
}

/// Switch to a profile by creating a symlink or copying
pub fn switch_to_profile(paths: &Paths, profile_name: &str) -> Result<()> {
    let profile_settings = paths.profile_settings(profile_name);

    // Validate profile exists and has valid JSON
    if !profile_settings.exists() {
        bail!(
            "Profile '{}' does not exist.\n\
             Expected file at: {:?}\n\
             Hint: Use 'ccprof list' to see available profiles,\n\
             or 'ccprof add {} --from-current' to create it.",
            profile_name,
            profile_settings,
            profile_name
        );
    }

    validate_json_file(&profile_settings)
        .with_context(|| format!("Profile '{}' has invalid JSON", profile_name))?;

    // Ensure ~/.claude/ directory exists
    fs::create_dir_all(&paths.claude_dir)
        .with_context(|| format!("Failed to create Claude directory: {:?}", paths.claude_dir))?;

    // Check current status and backup if needed
    let status = SettingsStatus::detect(&paths.claude_settings);
    backup_if_needed(paths, &status)?;

    // Try symlink first, fallback to atomic copy
    if let Err(_symlink_err) = create_symlink(paths, &profile_settings) {
        // Symlink failed, use atomic copy fallback
        atomic_copy(&profile_settings, &paths.claude_settings)?;
    }

    // Update state with lock
    let mut locked = LockedState::lock(&paths.state_file)?;
    locked.update(|state| {
        state.default_profile = Some(profile_name.to_string());
    })?;

    Ok(())
}

/// Backup the current settings file if it's not already a profile symlink
fn backup_if_needed(paths: &Paths, status: &SettingsStatus) -> Result<()> {
    // Only backup if it exists and is NOT a symlink into profiles dir
    let needs_backup = match status {
        SettingsStatus::Missing | SettingsStatus::BrokenSymlink { .. } => false,
        SettingsStatus::RegularFile => true,
        SettingsStatus::Symlink { .. } => !status.is_profile_symlink(paths),
    };

    if !needs_backup {
        return Ok(());
    }

    // Ensure backups directory exists
    fs::create_dir_all(&paths.backups_dir).with_context(|| {
        format!(
            "Failed to create backups directory: {:?}",
            paths.backups_dir
        )
    })?;

    // Generate backup filename with timestamp
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let backup_name = format!("settings.json.{}.bak", timestamp);
    let backup_path = paths.backups_dir.join(backup_name);

    // Copy current file to backup
    fs::copy(&paths.claude_settings, &backup_path)
        .with_context(|| format!("Failed to create backup at: {:?}", backup_path))?;

    Ok(())
}

/// Create a symlink from claude settings to profile settings
fn create_symlink(paths: &Paths, profile_settings: &Path) -> Result<()> {
    // Remove existing file/symlink if present
    if paths.claude_settings.exists() || fs::read_link(&paths.claude_settings).is_ok() {
        fs::remove_file(&paths.claude_settings).with_context(|| {
            format!(
                "Failed to remove existing settings file: {:?}",
                paths.claude_settings
            )
        })?;
    }

    // Create symlink: claude_settings -> profile_settings
    symlink(profile_settings, &paths.claude_settings).with_context(|| {
        format!(
            "Failed to create symlink: {:?} -> {:?}",
            paths.claude_settings, profile_settings
        )
    })?;

    Ok(())
}

/// Atomically copy a file to destination
fn atomic_copy(source: &Path, dest: &Path) -> Result<()> {
    let dest_dir = dest
        .parent()
        .context("Destination has no parent directory")?;

    // Read source content
    let content =
        fs::read(source).with_context(|| format!("Failed to read source file: {:?}", source))?;

    // Create temp file in the same directory as destination (for atomic rename)
    let mut temp_file = NamedTempFile::new_in(dest_dir)
        .with_context(|| format!("Failed to create temp file in: {:?}", dest_dir))?;

    // Write content to temp file
    temp_file
        .write_all(&content)
        .context("Failed to write to temp file")?;

    // fsync for durability
    temp_file
        .as_file()
        .sync_all()
        .context("Failed to sync temp file")?;

    // Remove existing destination if present
    if dest.exists() || fs::read_link(dest).is_ok() {
        fs::remove_file(dest)
            .with_context(|| format!("Failed to remove existing file: {:?}", dest))?;
    }

    // Atomic rename
    temp_file
        .persist(dest)
        .with_context(|| format!("Failed to rename temp file to: {:?}", dest))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_paths(temp_dir: &TempDir) -> Paths {
        Paths {
            base_dir: temp_dir.path().join(".claude-profiles"),
            profiles_dir: temp_dir.path().join(".claude-profiles/profiles"),
            backups_dir: temp_dir.path().join(".claude-profiles/backups"),
            state_file: temp_dir.path().join(".claude-profiles/state.json"),
            claude_dir: temp_dir.path().join(".claude"),
            claude_settings: temp_dir.path().join(".claude/settings.json"),
        }
    }

    #[test]
    fn test_settings_status_missing() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("nonexistent.json");
        let status = SettingsStatus::detect(&path);
        assert!(matches!(status, SettingsStatus::Missing));
    }

    #[test]
    fn test_settings_status_regular_file() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("settings.json");
        fs::write(&path, "{}").unwrap();
        let status = SettingsStatus::detect(&path);
        assert!(matches!(status, SettingsStatus::RegularFile));
    }

    #[test]
    fn test_settings_status_symlink() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("target.json");
        let link = temp_dir.path().join("link.json");

        fs::write(&target, "{}").unwrap();
        symlink(&target, &link).unwrap();

        let status = SettingsStatus::detect(&link);
        assert!(matches!(status, SettingsStatus::Symlink { .. }));
    }

    #[test]
    fn test_settings_status_broken_symlink() {
        let temp_dir = TempDir::new().unwrap();
        let target = temp_dir.path().join("nonexistent.json");
        let link = temp_dir.path().join("link.json");

        symlink(&target, &link).unwrap();

        let status = SettingsStatus::detect(&link);
        assert!(matches!(status, SettingsStatus::BrokenSymlink { .. }));
    }

    #[test]
    fn test_is_profile_symlink() {
        let temp_dir = TempDir::new().unwrap();
        let paths = setup_test_paths(&temp_dir);
        paths.ensure_dirs().unwrap();

        // Create a profile
        let profile_settings = paths.profile_settings("work");
        fs::create_dir_all(profile_settings.parent().unwrap()).unwrap();
        fs::write(&profile_settings, "{}").unwrap();

        // Create a symlink to it
        fs::create_dir_all(&paths.claude_dir).unwrap();
        symlink(&profile_settings, &paths.claude_settings).unwrap();

        let status = SettingsStatus::detect(&paths.claude_settings);
        assert!(status.is_profile_symlink(&paths));

        // Create a symlink to somewhere else
        fs::remove_file(&paths.claude_settings).unwrap();
        let other_file = temp_dir.path().join("other.json");
        fs::write(&other_file, "{}").unwrap();
        symlink(&other_file, &paths.claude_settings).unwrap();

        let status = SettingsStatus::detect(&paths.claude_settings);
        assert!(!status.is_profile_symlink(&paths));
    }

    #[test]
    fn test_atomic_copy() {
        let temp_dir = TempDir::new().unwrap();
        let source = temp_dir.path().join("source.json");
        let dest = temp_dir.path().join("dest.json");

        fs::write(&source, r#"{"test": true}"#).unwrap();
        atomic_copy(&source, &dest).unwrap();

        assert!(dest.exists());
        let content = fs::read_to_string(&dest).unwrap();
        assert_eq!(content, r#"{"test": true}"#);
    }

    #[test]
    fn test_switch_to_profile() {
        let temp_dir = TempDir::new().unwrap();
        let paths = setup_test_paths(&temp_dir);
        paths.ensure_dirs().unwrap();

        // Create a profile
        let profile_dir = paths.profile_dir("test");
        fs::create_dir_all(&profile_dir).unwrap();
        fs::write(paths.profile_settings("test"), r#"{"profile": "test"}"#).unwrap();

        // Switch to it
        switch_to_profile(&paths, "test").unwrap();

        // Verify symlink was created
        let status = SettingsStatus::detect(&paths.claude_settings);
        assert!(matches!(status, SettingsStatus::Symlink { .. }));
        assert!(status.is_profile_symlink(&paths));
    }
}
