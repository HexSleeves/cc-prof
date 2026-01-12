use anyhow::{Context, Result, bail};
use chrono::Utc;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use crate::components::{Component, ProfileMetadata};
use crate::paths::Paths;
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

/// Switch to a profile by creating symlinks for all managed components
pub fn switch_to_profile(paths: &Paths, profile_name: &str) -> Result<()> {
    let profile_dir = paths.profile_dir(profile_name);

    // Check if profile directory exists
    if !profile_dir.exists() {
        bail!(
            "Profile '{}' does not exist.\n\
             Hint: Use 'ccprof list' to see available profiles,\n\
             or 'ccprof add {} --from-current' to create it.",
            profile_name,
            profile_name
        );
    }

    // Read profile metadata
    let metadata = ProfileMetadata::read(&profile_dir)?;

    // Validate all managed components exist in profile
    for component in &metadata.managed_components {
        let component_path = component.profile_path(paths, profile_name);
        if !component_path.exists() {
            bail!(
                "Profile '{}' is missing component: {}\n\
                 Expected at: {:?}\n\
                 \n\
                 This profile may be corrupted. Try:\n\
                   ccprof doctor",
                profile_name,
                component.display_name(),
                component_path
            );
        }
    }

    // Ensure ~/.claude/ directory exists
    fs::create_dir_all(&paths.claude_dir)
        .with_context(|| format!("Failed to create Claude directory: {:?}", paths.claude_dir))?;

    // Switch each managed component
    for component in &metadata.managed_components {
        let source = component.source_path(paths);
        let target = component.profile_path(paths, profile_name);

        // Detect current status
        let status = ComponentStatus::detect(&source);

        // Backup if needed
        if status.needs_backup(paths, &source) {
            backup_component(paths, component, &source)?;
        }

        // Create symlink
        create_component_symlink(&source, &target, component)?;
    }

    // Update state with lock
    let mut locked = LockedState::lock(&paths.state_file)?;
    locked.update(|state| {
        state.default_profile = Some(profile_name.to_string());
    })?;

    Ok(())
}

/// Status of a component (file or directory)
#[derive(Debug, Clone)]
pub enum ComponentStatus {
    /// Component is missing
    Missing,
    /// Regular file (not a symlink)
    RegularFile,
    /// Regular directory (not a symlink)
    RegularDirectory,
    /// Symlink pointing to the given target
    Symlink { target: PathBuf },
    /// Broken symlink (target doesn't exist)
    BrokenSymlink { target: PathBuf },
}

impl ComponentStatus {
    /// Detect the status of a component (file or directory)
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
                    ComponentStatus::Symlink { target }
                } else {
                    ComponentStatus::BrokenSymlink { target }
                }
            }
            Err(_) => {
                // Not a symlink - check if file/directory exists
                if !path.exists() {
                    ComponentStatus::Missing
                } else if path.is_dir() {
                    ComponentStatus::RegularDirectory
                } else {
                    ComponentStatus::RegularFile
                }
            }
        }
    }

    /// Check if this component needs backup before switching
    pub fn needs_backup(&self, paths: &Paths, component_source: &Path) -> bool {
        match self {
            ComponentStatus::Missing | ComponentStatus::BrokenSymlink { .. } => false,
            ComponentStatus::RegularFile | ComponentStatus::RegularDirectory => true,
            ComponentStatus::Symlink { target } => {
                // Only backup if symlink points outside profiles dir
                let resolved = if target.is_absolute() {
                    target.clone()
                } else {
                    component_source
                        .parent()
                        .unwrap_or(Path::new("."))
                        .join(target)
                };
                !paths.is_in_profiles_dir(&resolved)
            }
        }
    }
}

/// Recursively copy a directory
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    if !src.exists() {
        bail!("Source directory does not exist: {:?}", src);
    }

    if !src.is_dir() {
        bail!("Source is not a directory: {:?}", src);
    }

    fs::create_dir_all(dst)
        .with_context(|| format!("Failed to create destination directory: {:?}", dst))?;

    for entry in
        fs::read_dir(src).with_context(|| format!("Failed to read source directory: {:?}", src))?
    {
        let entry = entry.context("Failed to read directory entry")?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).with_context(|| {
                format!("Failed to copy file: {:?} -> {:?}", src_path, dst_path)
            })?;
        }
    }

    Ok(())
}

/// Backup a component (file or directory) before switching
pub fn backup_component(paths: &Paths, component: &Component, source: &Path) -> Result<()> {
    fs::create_dir_all(&paths.backups_dir).with_context(|| {
        format!(
            "Failed to create backups directory: {:?}",
            paths.backups_dir
        )
    })?;

    let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let backup_name = match component {
        Component::Settings => format!("settings.json.{}.bak", timestamp),
        Component::Agents => format!("agents.{}.bak", timestamp),
        Component::Hooks => format!("hooks.{}.bak", timestamp),
        Component::Commands => format!("commands.{}.bak", timestamp),
    };
    let backup_path = paths.backups_dir.join(backup_name);

    if component.is_file() {
        // File backup
        fs::copy(source, &backup_path)
            .with_context(|| format!("Failed to backup file: {:?} -> {:?}", source, backup_path))?;
    } else {
        // Directory backup (recursive copy)
        copy_dir_recursive(source, &backup_path).with_context(|| {
            format!(
                "Failed to backup directory: {:?} -> {:?}",
                source, backup_path
            )
        })?;
    }

    Ok(())
}

/// Create a symlink for a component (file or directory)
#[cfg_attr(unix, allow(unused_variables))]
pub fn create_component_symlink(source: &Path, target: &Path, component: &Component) -> Result<()> {
    // Remove existing file/directory/symlink
    if source.exists() || fs::read_link(source).is_ok() {
        if source.is_dir() {
            fs::remove_dir_all(source)
                .with_context(|| format!("Failed to remove existing directory: {:?}", source))?;
        } else {
            fs::remove_file(source)
                .with_context(|| format!("Failed to remove existing file: {:?}", source))?;
        }
    }

    // Create symlink (Unix for now, Windows support can be added later)
    #[cfg(unix)]
    {
        symlink(target, source)
            .with_context(|| format!("Failed to create symlink: {:?} -> {:?}", source, target))?;
    }

    #[cfg(windows)]
    {
        if component.is_file() {
            std::os::windows::fs::symlink_file(target, source).with_context(|| {
                format!(
                    "Failed to create file symlink: {:?} -> {:?}",
                    source, target
                )
            })?;
        } else {
            std::os::windows::fs::symlink_dir(target, source).with_context(|| {
                format!(
                    "Failed to create directory symlink: {:?} -> {:?}",
                    source, target
                )
            })?;
        }
    }

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
            claude_agents: temp_dir.path().join(".claude/agents"),
            claude_hooks: temp_dir.path().join(".claude/hooks"),
            claude_commands: temp_dir.path().join(".claude/commands"),
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
