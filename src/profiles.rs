//! Core profile management logic.
//! 
//! This module handles the "data model" of profiles:
//! - Creating and removing profiles
//! - Listing available profiles
//! - Validating profile names
//! - Renaming profiles
//! - Managing profile components
//! 
//! It interacts directly with the filesystem to manage the `~/.claude-profiles/profiles/` directory.

use anyhow::{Context, Result, bail};
use chrono::Utc;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::components::{Component, ProfileMetadata};
use crate::paths::Paths;
use crate::fs_utils::copy_dir_recursive;

/// List available profiles
pub fn list_profiles(paths: &Paths) -> Result<Vec<String>> {
    paths.ensure_dirs()?;

    let mut profiles = Vec::new();
    if paths.profiles_dir.exists() {
        for entry in fs::read_dir(&paths.profiles_dir)? {
            let entry = entry?;
            let path = entry.path();
            #[allow(clippy::collapsible_if)]
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    profiles.push(name.to_string());
                }
            }
        }
    }
    profiles.sort();
    Ok(profiles)
}

/// Check if a profile exists
pub fn profile_exists(paths: &Paths, name: &str) -> bool {
    paths.profile_dir(name).exists()
}

/// Validate profile name
///
/// Only allows alphanumeric characters, underscores, and hyphens.
pub fn validate_profile_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("Profile name cannot be empty");
    }

    if name.chars().count() > 64 {
        bail!("Profile name cannot be longer than 64 characters");
    }

    // Allow a-z, A-Z, 0-9, -, _
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!(
            "Invalid profile name '{}'.\n\n Only alphanumeric characters, hyphens (-), and underscores (_) are allowed.",
            name
        );
    }

    Ok(())
}

/// Create a new profile with specific components
pub fn create_profile_with_components(
    paths: &Paths,
    name: &str,
    components: HashSet<Component>,
) -> Result<()> {
    validate_profile_name(name)?;
    let profile_dir = paths.profile_dir(name);

    if profile_dir.exists() {
        bail!("Profile directory already exists: {}", profile_dir.display());
    }

    fs::create_dir_all(&profile_dir)
        .with_context(|| format!("Failed to create profile directory: {}", profile_dir.display()))?;

    // Copy selected components from source
    for component in &components {
        let source = component.source_path(paths);
        let target = component.profile_path(paths, name);

        if !source.exists() {
            // Warn but continue? Or just skip?
            // For now, if user explicitly selected it, we might want to create an empty version or skip
            // But usually we're creating FROM current, so if current doesn't exist, we can't copy.
            // Let's skip copying but still track it in metadata?
            // Or better: ensure we only copy if it exists.
            continue;
        }

        if source.is_dir() {
            copy_dir_recursive(&source, &target)?;
        } else {
            fs::copy(&source, &target).with_context(|| {
                format!(
                    "Failed to copy {} to {}",
                    source.display(),
                    target.display()
                )
            })?;
        }

        // Validate if it's settings.json
        if matches!(component, Component::Settings) {
            validate_json_file(&target)?;
        }
    }

    // Create metadata
    let metadata = ProfileMetadata {
        version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        managed_components: components,
        migration: None,
    };
    metadata.write(&profile_dir)?;

    Ok(())
}

/// Update which components a profile manages
pub fn update_profile_components(
    paths: &Paths,
    name: &str,
    new_components: HashSet<Component>,
) -> Result<()> {
    let profile_dir = paths.profile_dir(name);
    let mut metadata = ProfileMetadata::read(&profile_dir)?;

    // Identify added components
    let added: HashSet<_> = new_components
        .difference(&metadata.managed_components)
        .collect();

    // Copy added components from source
    for component in added {
        let source = component.source_path(paths);
        let target = component.profile_path(paths, name);

        if source.exists() && !target.exists() {
            if source.is_dir() {
                copy_dir_recursive(&source, &target)?;
            } else {
                fs::copy(&source, &target)?;
            }
        }
    }

    // Update metadata
    metadata.managed_components = new_components;
    metadata.updated_at = Utc::now();
    metadata.write(&profile_dir)?;

    Ok(())
}

/// Remove a profile
pub fn remove_profile(paths: &Paths, name: &str) -> Result<()> {
    let profile_dir = paths.profile_dir(name);

    if !profile_dir.exists() {
        bail!("Profile '{}' does not exist", name);
    }

    fs::remove_dir_all(&profile_dir).with_context(|| {
        format!(
            "Failed to remove profile directory: {}",
            profile_dir.display()
        )
    })?;

    Ok(())
}

/// Rename a profile
pub fn rename_profile(paths: &Paths, old_name: &str, new_name: &str) -> Result<()> {
    let old_dir = paths.profile_dir(old_name);
    let new_dir = paths.profile_dir(new_name);

    if !old_dir.exists() {
        bail!("Profile '{}' does not exist", old_name);
    }
    if new_dir.exists() {
        bail!("Profile '{}' already exists", new_name);
    }

    fs::rename(&old_dir, &new_dir).with_context(|| {
        format!(
            "Failed to rename '{}' to '{}'",
            old_dir.display(),
            new_dir.display()
        )
    })?;

    Ok(())
}

/// Validate that a file contains valid JSON
pub fn validate_json_file(path: &Path) -> Result<()> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    serde_json::from_str::<serde_json::Value>(&content)
        .with_context(|| format!("Invalid JSON in file: {}", path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_test_paths;
    use tempfile::TempDir;

    #[test]
    fn test_profile_name_validation() {
        assert!(validate_profile_name("work").is_ok());
        assert!(validate_profile_name("my-profile").is_ok());
        assert!(validate_profile_name("test_123").is_ok());

        assert!(validate_profile_name("").is_err());
        assert!(validate_profile_name("invalid name").is_err());
        assert!(validate_profile_name("test/profile").is_err());
        assert!(validate_profile_name("emoji😊").is_err());
    }

    #[test]
    fn test_create_profile_with_components() {
        let temp_dir = TempDir::new().unwrap();
        let paths = setup_test_paths(&temp_dir);
        paths.ensure_dirs().unwrap();

        // Create source files
        fs::create_dir_all(&paths.claude_dir).unwrap();
        fs::write(&paths.claude_settings, "{}").unwrap();
        let agents_dir = paths.claude_dir.join("agents");
        fs::create_dir(&agents_dir).unwrap();
        fs::write(agents_dir.join("test.txt"), "agent").unwrap();

        let mut components = HashSet::new();
        components.insert(Component::Settings);
        components.insert(Component::Agents);

        create_profile_with_components(&paths, "full-profile", components).unwrap();

        let profile_dir = paths.profile_dir("full-profile");
        assert!(profile_dir.join("settings.json").exists());
        assert!(profile_dir.join("agents").join("test.txt").exists());

        // Check metadata
        let metadata = ProfileMetadata::read(&profile_dir).unwrap();
        assert!(metadata.managed_components.contains(&Component::Settings));
        assert!(metadata.managed_components.contains(&Component::Agents));
    }

    #[test]
    fn test_rename_profile() {
        let temp_dir = TempDir::new().unwrap();
        let paths = setup_test_paths(&temp_dir);
        paths.ensure_dirs().unwrap();

        fs::create_dir_all(&paths.claude_dir).unwrap();
        fs::write(&paths.claude_settings, "{}").unwrap();

        let mut components = HashSet::new();
        components.insert(Component::Settings);
        create_profile_with_components(&paths, "old-name", components).unwrap();

        rename_profile(&paths, "old-name", "new-name").unwrap();

        assert!(!paths.profile_dir("old-name").exists());
        assert!(paths.profile_dir("new-name").exists());
    }
}