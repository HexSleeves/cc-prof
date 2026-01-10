use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;

use crate::paths::Paths;

/// List all profile names (directories under profiles/)
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
            if settings_path.exists()
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
            {
                profiles.push(name.to_string());
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
pub fn validate_profile_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("Profile name cannot be empty");
    }

    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        bail!("Profile name cannot contain path separators or null bytes");
    }

    if name.starts_with('.') {
        bail!("Profile name cannot start with a dot");
    }

    if name == "." || name == ".." {
        bail!("Invalid profile name");
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

/// Validate that a file contains valid JSON
pub fn validate_json_file(path: &Path) -> Result<()> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read file: {:?}", path))?;

    serde_json::from_str::<serde_json::Value>(&content)
        .with_context(|| format!("Invalid JSON in file: {:?}", path))?;

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
    fn test_validate_profile_name() {
        assert!(validate_profile_name("work").is_ok());
        assert!(validate_profile_name("my-profile").is_ok());
        assert!(validate_profile_name("profile_1").is_ok());

        assert!(validate_profile_name("").is_err());
        assert!(validate_profile_name("path/with/slash").is_err());
        assert!(validate_profile_name(".hidden").is_err());
        assert!(validate_profile_name(".").is_err());
        assert!(validate_profile_name("..").is_err());
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
}
