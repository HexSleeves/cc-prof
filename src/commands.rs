use anstyle::AnsiColor;
use anyhow::{Context, Result, bail};
use std::process::Command;

use crate::doctor::run_doctor;
use crate::paths::Paths;
use crate::profiles::{create_profile_from, list_profiles, profile_exists};
use crate::state::State;
use crate::switch::{SettingsStatus, switch_to_profile};
use crate::ui::Ui;

/// List all available profiles
pub fn list(paths: &Paths, ui: &Ui) -> Result<()> {
    let profiles = list_profiles(paths)?;

    if profiles.is_empty() {
        ui.warn("No profiles found.");
        ui.newline();
        ui.println("Create one with:");
        ui.println(format!("  {} add <name> --from-current", ui.bold("ccprof")));
        return Ok(());
    }

    // Get current default profile for marking
    let state = State::read(&paths.state_file).unwrap_or_default();
    let current = state.default_profile.as_deref();

    // Build table
    let mut table = ui.simple_table();
    table.set_header(vec![
        ui.header_cell(""),
        ui.header_cell("Profile"),
        ui.header_cell("Status"),
    ]);

    for name in &profiles {
        let is_active = Some(name.as_str()) == current;
        let icon = if is_active { ui.icon_ok() } else { " " };
        let status_cell = if is_active {
            ui.colored_cell("active", AnsiColor::Green)
        } else {
            ui.cell("-")
        };

        table.add_row(vec![ui.cell(icon), ui.cell(name), status_cell]);
    }

    ui.section("Profiles");
    ui.println(table.to_string());

    Ok(())
}

/// Show the current/active profile and settings status
pub fn current(paths: &Paths, ui: &Ui) -> Result<()> {
    // Read state
    let state = State::read(&paths.state_file).unwrap_or_default();

    ui.section("Current Profile");
    ui.newline();

    // Build info table
    let mut table = ui.simple_table();

    // Show default profile from state
    match &state.default_profile {
        Some(profile) => {
            table.add_row(vec![
                ui.cell("Selected profile:"),
                ui.header_cell(profile), // bold
            ]);
            if let Some(updated) = &state.updated_at {
                table.add_row(vec![
                    ui.cell("Last switched:"),
                    ui.cell(updated.to_string()),
                ]);
            }
        }
        None => {
            table.add_row(vec![ui.cell("Selected profile:"), ui.cell("(none)")]);
        }
    }

    // Inspect the actual settings file
    let status = SettingsStatus::detect(&paths.claude_settings);
    let status_cell = match &status {
        SettingsStatus::Missing => ui.colored_cell("missing", AnsiColor::Yellow),
        SettingsStatus::RegularFile => ui.cell("regular file"),
        SettingsStatus::Symlink { target } => ui.cell(format!("symlink → {}", target.display())),
        SettingsStatus::BrokenSymlink { target } => ui.colored_cell(
            format!("broken symlink → {}", target.display()),
            AnsiColor::Red,
        ),
    };
    table.add_row(vec![ui.cell("Settings file:"), status_cell]);

    if let SettingsStatus::Symlink { ref target } = status {
        if status.is_profile_symlink(paths) {
            if let Some(profile_name) = target
                .strip_prefix(&paths.profiles_dir)
                .ok()
                .and_then(|p| p.components().next())
                .and_then(|c| c.as_os_str().to_str())
            {
                table.add_row(vec![
                    ui.cell("Linked profile:"),
                    ui.colored_cell(profile_name, AnsiColor::Green),
                ]);
            }
        } else {
            table.add_row(vec![
                ui.cell(""),
                ui.colored_cell("(symlink outside profiles dir)", AnsiColor::Yellow),
            ]);
        }
    }

    ui.println(table.to_string());
    Ok(())
}

/// Add a new profile from current settings
pub fn add(paths: &Paths, name: &str, ui: &Ui) -> Result<()> {
    paths.ensure_dirs()?;

    if profile_exists(paths, name) {
        bail!(
            "Profile '{}' already exists.\n\
             Use 'ccprof edit {}' to modify it, or choose a different name.",
            name,
            name
        );
    }

    create_profile_from(paths, name, &paths.claude_settings)?;

    ui.ok(format!("Created profile '{}' from current settings", name));
    ui.newline();
    ui.println("To activate it:");
    ui.println(format!("  ccprof use {}", name));
    ui.newline();
    ui.println("To edit it:");
    ui.println(format!("  ccprof edit {}", name));

    Ok(())
}

/// Switch to a profile
pub fn use_profile(paths: &Paths, name: &str, ui: &Ui) -> Result<()> {
    paths.ensure_dirs()?;

    // Start spinner for the switch operation
    let spinner = ui.spinner(format!("Switching to profile '{}'...", name));

    match switch_to_profile(paths, name) {
        Ok(()) => {
            ui.spinner_finish_ok(&spinner, format!("Active profile: {}", name));
            Ok(())
        }
        Err(e) => {
            ui.spinner_finish_err(&spinner, format!("Failed to switch: {}", e));
            Err(e)
        }
    }
}

/// Edit a profile's settings.json
pub fn edit(paths: &Paths, name: &str, ui: &Ui) -> Result<()> {
    if !profile_exists(paths, name) {
        bail!(
            "Profile '{}' does not exist.\n\
             Use 'ccprof list' to see available profiles.",
            name
        );
    }

    let settings_path = paths.profile_settings(name);

    // Try $EDITOR first
    if let Ok(editor) = std::env::var("EDITOR") {
        let status = Command::new(&editor)
            .arg(&settings_path)
            .status()
            .with_context(|| format!("Failed to run editor: {}", editor))?;

        if !status.success() {
            bail!("Editor exited with non-zero status");
        }
    } else {
        // Fallback to macOS 'open -t' (opens in default text editor)
        let status = Command::new("open")
            .arg("-t")
            .arg(&settings_path)
            .status()
            .context("Failed to run 'open -t'")?;

        if !status.success() {
            bail!("'open -t' exited with non-zero status");
        }
    }

    ui.ok(format!("Opened {} in editor", settings_path.display()));
    Ok(())
}

/// Run diagnostics
pub fn doctor(paths: &Paths, ui: &Ui) -> Result<()> {
    run_doctor(paths, ui);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::ColorMode;
    use std::fs;
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

    fn test_ui() -> Ui {
        Ui::new(ColorMode::Never, false)
    }

    #[test]
    fn test_list_empty() {
        let temp_dir = TempDir::new().unwrap();
        let paths = setup_test_paths(&temp_dir);
        let ui = test_ui();
        // Should not error, just show "no profiles"
        assert!(list(&paths, &ui).is_ok());
    }

    #[test]
    fn test_add_and_list() {
        let temp_dir = TempDir::new().unwrap();
        let paths = setup_test_paths(&temp_dir);
        let ui = test_ui();
        paths.ensure_dirs().unwrap();

        // Create source settings
        fs::create_dir_all(&paths.claude_dir).unwrap();
        fs::write(&paths.claude_settings, r#"{"test": true}"#).unwrap();

        // Add profile
        add(&paths, "work", &ui).unwrap();

        // Verify it exists
        assert!(profile_exists(&paths, "work"));
    }

    #[test]
    fn test_add_duplicate() {
        let temp_dir = TempDir::new().unwrap();
        let paths = setup_test_paths(&temp_dir);
        let ui = test_ui();
        paths.ensure_dirs().unwrap();

        fs::create_dir_all(&paths.claude_dir).unwrap();
        fs::write(&paths.claude_settings, "{}").unwrap();

        add(&paths, "work", &ui).unwrap();
        assert!(add(&paths, "work", &ui).is_err());
    }

    #[test]
    fn test_use_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let paths = setup_test_paths(&temp_dir);
        let ui = test_ui();
        paths.ensure_dirs().unwrap();

        assert!(use_profile(&paths, "nonexistent", &ui).is_err());
    }

    #[test]
    fn test_current_no_state() {
        let temp_dir = TempDir::new().unwrap();
        let paths = setup_test_paths(&temp_dir);
        let ui = test_ui();
        // Should not error
        assert!(current(&paths, &ui).is_ok());
    }
}
