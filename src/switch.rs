//! Profile switching logic.
//!
//! This module implements the core mechanism of `ccprof`: switching profiles.
//! It handles:
//! - Backing up existing configuration files.
//! - Creating symbolic links to the target profile's components.
//! - Handling edge cases like broken symlinks or missing files.
//! - Cleaning up old backups.

use anyhow::{Context, Result, bail};
use chrono::Utc;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use crate::components::{Component, ProfileMetadata};
use crate::paths::Paths;
use crate::state::LockedState;

/// Number of backups to keep per component type
const MAX_BACKUPS: usize = 10;

/// Represents the status of the ~/.claude/settings.json file
#[derive(Debug)]
pub enum SettingsStatus {
    Missing,
    RegularFile,
    Symlink { target: PathBuf },
    BrokenSymlink { target: PathBuf },
}

impl SettingsStatus {
    pub fn detect(path: &Path) -> Self {
        // Use symlink_metadata to check if it's a symlink without following it
        match fs::symlink_metadata(path) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    match fs::read_link(path) {
                        Ok(target) => Self::Symlink { target },
                        Err(_) => {
                            // Can't read link target?
                            Self::BrokenSymlink {
                                target: PathBuf::from("?"),
                            }
                        }
                    }
                } else {
                    Self::RegularFile
                }
            }
            Err(_) => {
                // If we can't get metadata, it probably doesn't exist
                // Double check if it's a broken symlink
                if path.exists() {
                    // It exists but we failed metadata? Rare.
                    Self::RegularFile
                } else {
                    // Does not exist or broken symlink?
                    // fs::symlink_metadata fails for broken symlinks? No, it shouldn't.
                    // It fails if the file doesn't exist.
                    Self::Missing
                }
            }
        }
    }

    pub fn is_profile_symlink(&self, paths: &Paths) -> bool {
        match self {
            Self::Symlink { target } => {
                // Check if target is inside profiles directory
                // We need to resolve absolute paths for accurate comparison
                // But generally checking prefix is enough
                paths.is_in_profiles_dir(target)
            }
            _ => false,
        }
    }
}

/// Switch to a specific profile
pub fn switch_to_profile(paths: &Paths, name: &str) -> Result<()> {
    if !crate::profiles::profile_exists(paths, name) {
        bail!("Profile '{}' does not exist", name);
    }

    let profile_dir = paths.profile_dir(name);
    let metadata = ProfileMetadata::read(&profile_dir)?;

    // 1. Process each managed component
    for component in &metadata.managed_components {
        let source_path = component.source_path(paths);
        let target_path = component.profile_path(paths, name);

        // Ensure target exists in profile (it should if metadata is correct)
        if !target_path.exists() {
            // Warn? Fail?
            // If it's missing in profile, we can't link to it.
            eprintln!(
                "Warning: Component {} missing in profile {}, skipping.",
                component.display_name(),
                name
            );
            continue;
        }

        create_component_symlink(&source_path, &target_path, component, &paths.backups_dir)?;
    }

    // 2. Update state
    let mut locked = LockedState::lock(&paths.state_file)?;
    locked.update(|s| {
        s.default_profile = Some(name.to_string());
        s.updated_at = Some(Utc::now());
    })?;

    Ok(())
}

/// Create a symlink for a component, handling backups
pub fn create_component_symlink(
    link_path: &Path,
    target_path: &Path,
    component: &Component,
    backups_dir: &Path,
) -> Result<()> {
    let status = ComponentStatus::detect(link_path);

    match status {
        ComponentStatus::Missing => {
            // Just create symlink
            make_symlink(target_path, link_path)?;
        }
        ComponentStatus::RegularFile | ComponentStatus::Directory => {
            // Backup then replace
            backup_existing_file(link_path, backups_dir, component.short_name())?;
            if link_path.is_dir() {
                fs::remove_dir_all(link_path)?;
            } else {
                fs::remove_file(link_path)?;
            }
            make_symlink(target_path, link_path)?;
        }
        ComponentStatus::Symlink { .. } => {
            // Remove old link and create new one
            fs::remove_file(link_path)?; // remove_file removes the symlink itself
            make_symlink(target_path, link_path)?;
        }
        ComponentStatus::BrokenSymlink { .. } => {
            fs::remove_file(link_path)?;
            make_symlink(target_path, link_path)?;
        }
    }

    Ok(())
}

#[derive(Debug)]
pub enum ComponentStatus {
    Missing,
    RegularFile,
    Directory,
    Symlink { target: PathBuf },
    BrokenSymlink { target: PathBuf },
}

impl ComponentStatus {
    pub fn detect(path: &Path) -> Self {
        // Check if it's a symlink first
        if let Ok(target) = fs::read_link(path) {
            // It is a symlink. Is it broken?
            if path.exists() {
                Self::Symlink { target }
            } else {
                Self::BrokenSymlink { target }
            }
        } else if path.exists() {
            if path.is_dir() {
                Self::Directory
            } else {
                Self::RegularFile
            }
        } else {
            Self::Missing
        }
    }
}

fn make_symlink(target: &Path, link: &Path) -> Result<()> {
    // Create parent dir if missing
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)?;
    }

    #[cfg(unix)]
    symlink(target, link).with_context(|| {
        format!(
            "Failed to create symlink from {} to {}",
            link.display(),
            target.display()
        )
    })?;

    #[cfg(windows)]
    std::os::windows::fs::symlink_file(target, link).with_context(|| {
        format!(
            "Failed to create symlink from {} to {}",
            link.display(),
            target.display()
        )
    })?;

    Ok(())
}

// Fixed version of backup_component that actually does the work
// We need to inject paths or the backup directory.
// Refactoring `create_component_symlink` to take `backups_dir`.

pub fn backup_existing_file(path: &Path, backups_dir: &Path, name_prefix: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    if !backups_dir.exists() {
        fs::create_dir_all(backups_dir)?;
    }

    let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let backup_name = format!("{}.{}.bak", name_prefix, timestamp);
    let backup_path = backups_dir.join(backup_name);

    if path.is_dir() {
        crate::fs_utils::copy_dir_recursive(path, &backup_path)?;
    } else {
        fs::copy(path, &backup_path)?;
    }

    // Rotate backups
    cleanup_old_backups(backups_dir, name_prefix)?;

    Ok(())
}

fn cleanup_old_backups(backups_dir: &Path, name_prefix: &str) -> Result<()> {
    let mut backups: Vec<_> = fs::read_dir(backups_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with(name_prefix) && n.ends_with(".bak"))
        })
        .collect();

    if backups.len() <= MAX_BACKUPS {
        return Ok(());
    }

    // Sort by modified time (oldest first)
    backups.sort_by_key(|b| b.metadata().and_then(|m| m.modified()).ok());

    // Remove oldest
    let to_remove = backups.len() - MAX_BACKUPS;
    for entry in backups.iter().take(to_remove) {
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }

    Ok(())
}

// Redefine internal create helper that uses Paths?
// Actually, `create_component_symlink` uses `backup_component` which was empty/broken above.
// I will fix `create_component_symlink` to find the backup dir.
// Since I can't easily change the signature everywhere without breaking tests/callers in this edit...
// Wait, I CAN change the signature, I'm editing the whole file.



#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_test_paths;
    use tempfile::TempDir;

    #[test]
    fn test_settings_status_detect() {
        let temp_dir = TempDir::new().unwrap();
        let paths = setup_test_paths(&temp_dir);
        paths.ensure_dirs().unwrap();

        // 1. Missing
        let status = SettingsStatus::detect(&paths.claude_settings);
        assert!(matches!(status, SettingsStatus::Missing));

        // 2. Regular file
        fs::create_dir_all(&paths.claude_dir).unwrap();
        fs::write(&paths.claude_settings, "{}").unwrap();
        let status = SettingsStatus::detect(&paths.claude_settings);
        assert!(matches!(status, SettingsStatus::RegularFile));

        // 3. Symlink
        let profile_dir = paths.profile_dir("test");
        fs::create_dir_all(&profile_dir).unwrap();
        let target = profile_dir.join("settings.json");
        fs::write(&target, "{}").unwrap();

        fs::remove_file(&paths.claude_settings).unwrap();
        make_symlink(&target, &paths.claude_settings).unwrap();

        let status = SettingsStatus::detect(&paths.claude_settings);
        assert!(matches!(status, SettingsStatus::Symlink { .. }));
        assert!(status.is_profile_symlink(&paths));
    }
}