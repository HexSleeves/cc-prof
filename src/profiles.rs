use anyhow::{Context, Result, bail};
use chrono::Utc;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::components::{Component, ProfileMetadata};
use crate::paths::Paths;
use crate::switch::copy_dir_recursive;

/// List all profile names (directories under profiles/)
/// Automatically migrates legacy profiles (creates profile.json)
pub fn list_profiles(paths: &Paths) -> Result<Vec<String>> {
    if !paths.profiles_dir.exists() {
        return Ok(Vec::new());
    }

    let mut profiles = Vec::new();

    let entries = fs::read_dir(&paths.profiles_dir).with_context(|| {
        format!(
            "Failed to read profiles directory: {:?}",
            paths.profiles_dir
        )
    })?;

    for entry in entries {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if path.is_dir() {
            // Check if it has a settings.json
            let settings_path = path.join("settings.json");
            let metadata_path = path.join("profile.json");

            // Note: Intentionally not using let-chains (if cond && let Some(x) = ...)
            // to maintain stable Rust compatibility (let-chains requires nightly)
            #[allow(clippy::collapsible_if)]
            if settings_path.exists() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    profiles.push(name.to_string());

                    // Auto-migrate legacy profiles (silent)
                    if !metadata_path.exists() {
                        // This is a legacy profile - create profile.json
                        let _ = ProfileMetadata::migrate_legacy(&path);
                    }
                }
            }
        }
    }

    profiles.sort();
    Ok(profiles)
}

/// Check if a profile exists
pub fn profile_exists(paths: &Paths, name: &str) -> bool {
    let settings_path = paths.profile_settings(name);
    settings_path.exists()
}

/// Validate that a profile name is acceptable
///
/// Profile names must:
/// - Not be empty
/// - Only contain alphanumeric characters, underscores, and hyphens: [a-zA-Z0-9_-]
/// - Not start with a dot or hyphen
///
/// This strict validation prevents issues with:
/// - Unicode characters and emojis
/// - Special characters that may cause filesystem issues
/// - Spaces and other whitespace
/// - Path separators and other problematic characters
pub fn validate_profile_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("Profile name cannot be empty");
    }

    // Check that all characters are alphanumeric, underscore, or hyphen
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        bail!(
            "Profile name can only contain letters, numbers, underscores, and hyphens.\nHint: '{}'  contains invalid characters",
            name
        );
    }

    // Don't allow names starting with dot or hyphen
    if name.starts_with('.') || name.starts_with('-') {
        bail!("Profile name cannot start with a dot or hyphen");
    }

    Ok(())
}

/// Create a new profile by copying from a source file
pub fn create_profile_from(paths: &Paths, name: &str, source: &Path) -> Result<()> {
    validate_profile_name(name)?;

    if profile_exists(paths, name) {
        bail!("Profile '{}' already exists", name);
    }

    if !source.exists() {
        bail!(
            "Source file does not exist: {:?}\n\
             Hint: You need an existing ~/.claude/settings.json to copy from.\n\
             Create one by running Claude Code first, then try again.",
            source
        );
    }

    // Validate source is valid JSON
    validate_json_file(source)?;

    // Create profile directory
    let profile_dir = paths.profile_dir(name);
    fs::create_dir_all(&profile_dir)
        .with_context(|| format!("Failed to create profile directory: {:?}", profile_dir))?;

    // Copy settings file
    let dest = paths.profile_settings(name);
    fs::copy(source, &dest).with_context(|| format!("Failed to copy settings to: {:?}", dest))?;

    Ok(())
}

/// Create a new profile with selected components
pub fn create_profile_with_components(
    paths: &Paths,
    name: &str,
    components: HashSet<Component>,
) -> Result<()> {
    validate_profile_name(name)?;

    if profile_exists(paths, name) {
        bail!("Profile '{}' already exists", name);
    }

    if components.is_empty() {
        bail!("At least one component must be selected");
    }

    // Create profile directory
    let profile_dir = paths.profile_dir(name);
    fs::create_dir_all(&profile_dir)
        .with_context(|| format!("Failed to create profile directory: {:?}", profile_dir))?;

    // Copy each selected component
    for component in &components {
        let source = component.source_path(paths);
        let dest = component.profile_path(paths, name);

        if !source.exists() {
            bail!(
                "Component {} does not exist at {:?}\n\
                 Hint: This component is not present in your ~/.claude/ directory.\n\
                 You can either create it first, or deselect this component.",
                component.display_name(),
                source
            );
        }

        if component.is_file() {
            // Validate JSON for settings component
            if matches!(component, Component::Settings) {
                validate_json_file(&source)?;
            }
            fs::copy(&source, &dest)
                .with_context(|| format!("Failed to copy file: {:?} -> {:?}", source, dest))?;
        } else {
            // Copy directory recursively
            copy_dir_recursive(&source, &dest)
                .with_context(|| format!("Failed to copy directory: {:?} -> {:?}", source, dest))?;
        }
    }

    // Create metadata
    let metadata = ProfileMetadata::new(name.to_string(), components);
    metadata.write(&profile_dir)?;

    Ok(())
}

/// Validate that a file contains valid JSON
pub fn validate_json_file(path: &Path) -> Result<()> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read file: {:?}", path))?;

    serde_json::from_str::<serde_json::Value>(&content)
        .with_context(|| format!("Invalid JSON in file: {:?}", path))?;

    Ok(())
}

/// Update the managed components of an existing profile
/// Adds new components from source, removes components from profile
pub fn update_profile_components(
    paths: &Paths,
    name: &str,
    new_components: HashSet<Component>,
) -> Result<()> {
    validate_profile_name(name)?;

    if !profile_exists(paths, name) {
        bail!("Profile '{}' does not exist", name);
    }

    if new_components.is_empty() {
        bail!("At least one component must be selected");
    }

    let profile_dir = paths.profile_dir(name);

    // Read existing metadata
    let mut metadata = ProfileMetadata::read(&profile_dir)?;

    let old_components = metadata.managed_components.clone();

    // Add new components that weren't in the old set
    for component in &new_components {
        if !old_components.contains(component) {
            let source = component.source_path(paths);
            let dest = component.profile_path(paths, name);

            if !source.exists() {
                bail!(
                    "Component {} does not exist at {:?}\n\
                     Hint: This component is not present in your ~/.claude/ directory.\n\
                     You can either create it first, or deselect this component.",
                    component.display_name(),
                    source
                );
            }

            if component.is_file() {
                // Validate JSON for settings component
                if matches!(component, Component::Settings) {
                    validate_json_file(&source)?;
                }
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("Failed to create directory: {:?}", parent))?;
                }
                fs::copy(&source, &dest)
                    .with_context(|| format!("Failed to copy file: {:?} -> {:?}", source, dest))?;
            } else {
                // Copy directory recursively
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("Failed to create directory: {:?}", parent))?;
                }
                copy_dir_recursive(&source, &dest).with_context(|| {
                    format!("Failed to copy directory: {:?} -> {:?}", source, dest)
                })?;
            }
        }
    }

    // Remove components that are in old set but not in new set
    for component in &old_components {
        if !new_components.contains(component) {
            let dest = component.profile_path(paths, name);
            if dest.exists() {
                if dest.is_file() {
                    fs::remove_file(&dest)
                        .with_context(|| format!("Failed to remove file: {:?}", dest))?;
                } else if dest.is_dir() {
                    fs::remove_dir_all(&dest)
                        .with_context(|| format!("Failed to remove directory: {:?}", dest))?;
                }
            }
        }
    }

    // Update metadata
    metadata.managed_components = new_components;
    metadata.updated_at = Utc::now();
    metadata.write(&profile_dir)?;

    Ok(())
}

/// Get validation result without failing (for doctor command)
pub fn validate_json_file_result(path: &Path) -> ValidationResult {
    if !path.exists() {
        return ValidationResult::Missing;
    }

    match fs::read_to_string(path) {
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(_) => ValidationResult::Valid,
            Err(e) => ValidationResult::Invalid(format!("JSON parse error: {}", e)),
        },
        Err(e) => ValidationResult::Invalid(format!("Read error: {}", e)),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationResult {
    Valid,
    Missing,
    Invalid(String),
}

impl std::fmt::Display for ValidationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationResult::Valid => write!(f, "valid"),
            ValidationResult::Missing => write!(f, "missing"),
            ValidationResult::Invalid(e) => write!(f, "invalid: {}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_test_paths;
    use tempfile::TempDir;

    #[test]
    fn test_validate_profile_name() {
        // Valid names
        assert!(validate_profile_name("work").is_ok());
        assert!(validate_profile_name("my-profile").is_ok());
        assert!(validate_profile_name("profile_1").is_ok());
        assert!(validate_profile_name("Profile_Name_123").is_ok());

        // Invalid: empty
        assert!(validate_profile_name("").is_err());

        // Invalid: path separators
        assert!(validate_profile_name("path/with/slash").is_err());
        assert!(validate_profile_name("path\\with\\backslash").is_err());

        // Invalid: starts with dot or hyphen
        assert!(validate_profile_name(".hidden").is_err());
        assert!(validate_profile_name(".").is_err());
        assert!(validate_profile_name("..").is_err());
        assert!(validate_profile_name("-invalid").is_err());

        // Invalid: special characters
        assert!(validate_profile_name("profile name").is_err()); // space
        assert!(validate_profile_name("profile@name").is_err());
        assert!(validate_profile_name("profile!name").is_err());
        assert!(validate_profile_name("profile$name").is_err());

        // Invalid: unicode and emojis
        assert!(validate_profile_name("profile🚀").is_err());
        assert!(validate_profile_name("プロフィール").is_err());
    }

    #[test]
    fn test_list_profiles_empty() {
        let temp_dir = TempDir::new().unwrap();
        let paths = setup_test_paths(&temp_dir);

        let profiles = list_profiles(&paths).unwrap();
        assert!(profiles.is_empty());
    }

    #[test]
    fn test_list_profiles() {
        let temp_dir = TempDir::new().unwrap();
        let paths = setup_test_paths(&temp_dir);

        // Create some profiles
        fs::create_dir_all(paths.profiles_dir.join("work")).unwrap();
        fs::write(paths.profiles_dir.join("work/settings.json"), "{}").unwrap();

        fs::create_dir_all(paths.profiles_dir.join("personal")).unwrap();
        fs::write(paths.profiles_dir.join("personal/settings.json"), "{}").unwrap();

        let profiles = list_profiles(&paths).unwrap();
        assert_eq!(profiles, vec!["personal", "work"]);
    }

    #[test]
    fn test_create_profile_from() {
        let temp_dir = TempDir::new().unwrap();
        let paths = setup_test_paths(&temp_dir);
        paths.ensure_dirs().unwrap();

        // Create source file
        fs::create_dir_all(&paths.claude_dir).unwrap();
        fs::write(&paths.claude_settings, r#"{"key": "value"}"#).unwrap();

        create_profile_from(&paths, "test", &paths.claude_settings).unwrap();

        assert!(paths.profile_settings("test").exists());
    }

    #[test]
    fn test_validate_json_file() {
        let temp_dir = TempDir::new().unwrap();
        let valid_path = temp_dir.path().join("valid.json");
        let invalid_path = temp_dir.path().join("invalid.json");

        fs::write(&valid_path, r#"{"key": "value"}"#).unwrap();
        fs::write(&invalid_path, "not json").unwrap();

        assert!(validate_json_file(&valid_path).is_ok());
        assert!(validate_json_file(&invalid_path).is_err());
    }

    #[test]
    fn test_validation_result() {
        let temp_dir = TempDir::new().unwrap();
        let valid_path = temp_dir.path().join("valid.json");
        let invalid_path = temp_dir.path().join("invalid.json");
        let missing_path = temp_dir.path().join("missing.json");

        fs::write(&valid_path, r#"{"key": "value"}"#).unwrap();
        fs::write(&invalid_path, "not json").unwrap();

        assert_eq!(
            validate_json_file_result(&valid_path),
            ValidationResult::Valid
        );
        assert_eq!(
            validate_json_file_result(&missing_path),
            ValidationResult::Missing
        );
        assert!(matches!(
            validate_json_file_result(&invalid_path),
            ValidationResult::Invalid(_)
        ));
    }

    #[test]
    fn test_update_profile_components() {
        use crate::components::Component;

        let temp_dir = TempDir::new().unwrap();
        let paths = setup_test_paths(&temp_dir);
        paths.ensure_dirs().unwrap();

        // Create source files for all components
        fs::create_dir_all(&paths.claude_dir).unwrap();
        fs::write(&paths.claude_settings, r#"{"test": true}"#).unwrap();

        // Create agents dir with a file
        fs::create_dir_all(&paths.claude_agents).unwrap();
        fs::write(paths.claude_agents.join("agent.md"), "# Agent").unwrap();

        // Create hooks dir
        fs::create_dir_all(&paths.claude_hooks).unwrap();

        // Create commands dir
        fs::create_dir_all(&paths.claude_commands).unwrap();

        // Create initial profile with only settings
        let mut initial_components = HashSet::new();
        initial_components.insert(Component::Settings);
        create_profile_with_components(&paths, "test", initial_components).unwrap();

        // Verify profile exists with only settings
        let profile_dir = paths.profile_dir("test");
        let metadata = crate::components::ProfileMetadata::read(&profile_dir).unwrap();
        assert_eq!(metadata.managed_components.len(), 1);
        assert!(metadata.managed_components.contains(&Component::Settings));
        assert!(!metadata.managed_components.contains(&Component::Agents));

        // Now update profile to include agents
        let mut new_components = HashSet::new();
        new_components.insert(Component::Settings);
        new_components.insert(Component::Agents);
        update_profile_components(&paths, "test", new_components).unwrap();

        // Verify profile now has both settings and agents
        let metadata = crate::components::ProfileMetadata::read(&profile_dir).unwrap();
        assert_eq!(metadata.managed_components.len(), 2);
        assert!(metadata.managed_components.contains(&Component::Settings));
        assert!(metadata.managed_components.contains(&Component::Agents));

        // Verify agents file was copied
        assert!(paths.profile_dir("test").join("agents/agent.md").exists());
    }

    #[test]
    fn test_update_profile_components_remove() {
        use crate::components::Component;

        let temp_dir = TempDir::new().unwrap();
        let paths = setup_test_paths(&temp_dir);
        paths.ensure_dirs().unwrap();

        // Create source files
        fs::create_dir_all(&paths.claude_dir).unwrap();
        fs::write(&paths.claude_settings, r#"{"test": true}"#).unwrap();

        fs::create_dir_all(&paths.claude_agents).unwrap();
        fs::write(paths.claude_agents.join("agent.md"), "# Agent").unwrap();

        // Create profile with settings and agents
        let mut initial_components = HashSet::new();
        initial_components.insert(Component::Settings);
        initial_components.insert(Component::Agents);
        create_profile_with_components(&paths, "test", initial_components).unwrap();

        // Verify profile exists with both
        let profile_dir = paths.profile_dir("test");
        let metadata = crate::components::ProfileMetadata::read(&profile_dir).unwrap();
        assert_eq!(metadata.managed_components.len(), 2);

        // Update profile to only have settings (remove agents)
        let mut new_components = HashSet::new();
        new_components.insert(Component::Settings);
        update_profile_components(&paths, "test", new_components).unwrap();

        // Verify profile now has only settings
        let metadata = crate::components::ProfileMetadata::read(&profile_dir).unwrap();
        assert_eq!(metadata.managed_components.len(), 1);
        assert!(metadata.managed_components.contains(&Component::Settings));
        assert!(!metadata.managed_components.contains(&Component::Agents));

        // Verify agents directory was removed
        assert!(!paths.profile_dir("test").join("agents").exists());
    }
}
