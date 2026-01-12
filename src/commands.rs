use anstyle::AnsiColor;
use anyhow::{Context, Result, bail};
use inquire::MultiSelect;
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use crate::components::Component;
use crate::doctor::run_doctor;
use crate::paths::Paths;
use crate::profiles::{create_profile_with_components, list_profiles, profile_exists};
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
        ui.header_cell("Components"),
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

        // Read profile metadata to show components
        let profile_dir = paths.profile_dir(name);
        let components_display = match crate::components::ProfileMetadata::read(&profile_dir) {
            Ok(metadata) => {
                let mut comp_codes: Vec<&str> = metadata
                    .managed_components
                    .iter()
                    .map(|c| c.short_name())
                    .collect();
                comp_codes.sort();
                let display = comp_codes.join(",");

                // Show migration indicator if migrated
                if metadata.migration.is_some() {
                    format!("{} (migrated)", display)
                } else {
                    display
                }
            }
            Err(_) => String::from("?"),
        };

        table.add_row(vec![
            ui.cell(icon),
            ui.cell(name),
            ui.cell(components_display),
            status_cell,
        ]);
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

/// Show detailed information about a profile
pub fn inspect(paths: &Paths, name: &str, ui: &Ui) -> Result<()> {
    if !profile_exists(paths, name) {
        bail!(
            "Profile '{}' does not exist.\\n\\\n             Use 'ccprof list' to see available profiles.",
            name
        );
    }

    let profile_dir = paths.profile_dir(name);
    let metadata = crate::components::ProfileMetadata::read(&profile_dir)?;

    ui.section(format!("Profile: {}", name));
    ui.newline();

    // Build metadata table
    let mut table = ui.simple_table();

    table.add_row(vec![
        ui.cell("Created:"),
        ui.cell(metadata.created_at.format("%Y-%m-%d %H:%M:%S").to_string()),
    ]);

    table.add_row(vec![
        ui.cell("Updated:"),
        ui.cell(metadata.updated_at.format("%Y-%m-%d %H:%M:%S").to_string()),
    ]);

    table.add_row(vec![ui.cell("Version:"), ui.cell(&metadata.version)]);

    if let Some(migration) = &metadata.migration {
        table.add_row(vec![
            ui.cell("Migration:"),
            ui.colored_cell(
                format!(
                    "Migrated from legacy ({})",
                    migration.migration_date.format("%Y-%m-%d")
                ),
                AnsiColor::Yellow,
            ),
        ]);
    }

    ui.println(table.to_string());
    ui.newline();

    // Show managed components with sizes
    ui.section("Managed Components");
    ui.newline();

    let mut comp_table = ui.simple_table();
    comp_table.set_header(vec![
        ui.header_cell("Component"),
        ui.header_cell("Path"),
        ui.header_cell("Size"),
    ]);

    for component in &metadata.managed_components {
        let path = component.profile_path(paths, name);

        if path.exists() {
            let size_str = calculate_size(&path)?;
            comp_table.add_row(vec![
                ui.cell(component.display_name()),
                ui.cell(format!("{}", path.display())),
                ui.cell(size_str),
            ]);
        } else {
            comp_table.add_row(vec![
                ui.cell(component.display_name()),
                ui.cell(format!("{}", path.display())),
                ui.colored_cell("missing", AnsiColor::Red),
            ]);
        }
    }

    ui.println(comp_table.to_string());

    Ok(())
}

/// Calculate human-readable size of a file or directory
fn calculate_size(path: &Path) -> Result<String> {
    use std::fs;

    let size = if path.is_file() {
        fs::metadata(path)
            .with_context(|| format!("Failed to read metadata for {}", path.display()))?
            .len()
    } else if path.is_dir() {
        // Recursively calculate directory size
        fn dir_size(path: &Path) -> std::io::Result<u64> {
            let mut total = 0;
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let metadata = entry.metadata()?;
                if metadata.is_file() {
                    total += metadata.len();
                } else if metadata.is_dir() {
                    total += dir_size(&entry.path())?;
                }
            }
            Ok(total)
        }
        dir_size(path)
            .with_context(|| format!("Failed to calculate size for {}", path.display()))?
    } else {
        0
    };

    Ok(format_bytes(size))
}

/// Format bytes as human-readable string
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Interactive component selection for profile creation
pub fn select_components(paths: &Paths) -> Result<HashSet<Component>> {
    let all_components = Component::all();

    // Build display options with availability indicators
    let options: Vec<String> = all_components
        .iter()
        .map(|c| {
            let path = c.source_path(paths);
            let exists = path.exists();
            let indicator = if exists { "✓" } else { "✗" };
            let availability = if exists { "" } else { " (not found)" };
            format!("{} {}{}", indicator, c.display_name(), availability)
        })
        .collect();

    // Default: select all components that exist
    let defaults: Vec<usize> = all_components
        .iter()
        .enumerate()
        .filter(|(_, c)| c.source_path(paths).exists())
        .map(|(i, _)| i)
        .collect();

    let selected_indices = MultiSelect::new(
        "Which components should this profile manage?",
        options.clone(),
    )
    .with_default(&defaults)
    .with_help_message("Space to select, Enter to confirm")
    .prompt()
    .context("Component selection cancelled")?;

    let selected: HashSet<Component> = selected_indices
        .into_iter()
        .filter_map(|selected_str| {
            options
                .iter()
                .position(|opt| *opt == selected_str)
                .map(|idx| all_components[idx])
        })
        .collect();

    if selected.is_empty() {
        bail!("At least one component must be selected");
    }

    Ok(selected)
}

/// Add a new profile from current settings
pub fn add(paths: &Paths, name: &str, ui: &Ui, components_arg: Option<Vec<String>>) -> Result<()> {
    paths.ensure_dirs()?;

    if profile_exists(paths, name) {
        bail!(
            "Profile '{}' already exists.\n\
             Use 'ccprof edit {}' to modify it, or choose a different name.",
            name,
            name
        );
    }

    // Determine which components to include
    let components = if let Some(comp_names) = components_arg {
        // Non-interactive mode: parse component names
        let mut selected = HashSet::new();
        for comp_name in comp_names {
            match comp_name.parse::<Component>() {
                Ok(c) => {
                    selected.insert(c);
                }
                Err(_) => {
                    bail!(
                        "Invalid component name: '{}'\n\
                         Valid components: settings, agents, hooks, commands",
                        comp_name
                    );
                }
            }
        }
        selected
    } else {
        // Interactive mode: use multi-select UI
        select_components(paths)?
    };

    // Create profile with selected components
    create_profile_with_components(paths, name, components.clone())?;

    ui.ok(format!("Created profile '{}'", name));
    ui.newline();
    ui.println("Included components:");
    for component in &components {
        ui.println(format!("  {} {}", ui.icon_ok(), component.display_name()));
    }
    ui.newline();
    ui.println("To activate it:");
    ui.println(format!("  ccprof use {}", name));

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
            claude_agents: temp_dir.path().join(".claude/agents"),
            claude_hooks: temp_dir.path().join(".claude/hooks"),
            claude_commands: temp_dir.path().join(".claude/commands"),
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

        // Add profile with explicit components (non-interactive)
        add(&paths, "work", &ui, Some(vec!["settings".to_string()])).unwrap();

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

        // Add profile with explicit components (non-interactive)
        add(&paths, "work", &ui, Some(vec!["settings".to_string()])).unwrap();
        assert!(add(&paths, "work", &ui, Some(vec!["settings".to_string()])).is_err());
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
