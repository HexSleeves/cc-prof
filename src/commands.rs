//! High-level command orchestration for the CLI.
//! 
//! This module contains the handler functions for each CLI command (`list`, `add`, `use`, etc.).
//! It serves as the coordination layer, interacting with:
//! - `crate::ui` for user interaction (output, prompts).
//! - `crate::paths` for filesystem locations.
//! - `crate::profiles` for profile management logic.
//! - `crate::switch` for profile activation logic.
//! - `crate::state` for persistent state.
//! 
//! Each function here generally corresponds to a subcommand in `main.rs`.

use anstyle::AnsiColor;
use anyhow::{Context, Result, bail};
use inquire::MultiSelect;
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use crate::components::Component;
use crate::doctor::run_doctor;
use crate::paths::Paths;
use crate::profiles::{
    create_profile_with_components,
    list_profiles,
    profile_exists,
    update_profile_components,
};
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
        return Ok(())
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
            table.add_row(vec![ui.cell("Selected profile:"), ui.header_cell(profile)]); // bold
            if let Some(updated) = &state.updated_at {
                table.add_row(vec![ui.cell("Last switched:"), ui.cell(updated.to_string())]);
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
                .and_then(|c| c.as_os_str().to_str()) {
                table.add_row(vec![
                    ui.cell("Linked profile:"),
                    ui.colored_cell(profile_name, AnsiColor::Green),
                ]);
            }
        } else {
            table.add_row(vec![ui.cell(""), ui.colored_cell("(symlink outside profiles dir)", AnsiColor::Yellow)]);
        }
    }

    ui.println(table.to_string());
    Ok(())
}

/// Show detailed information about a profile
pub fn inspect(paths: &Paths, name: &str, ui: &Ui) -> Result<()> {
    if !profile_exists(paths, name) {
        bail!(
            "Profile '{}' does not exist.\nHint: Use 'ccprof list' to see available profiles.",
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

    table.add_row(vec![ui.cell("Updated:"), ui.cell(metadata.updated_at.format("%Y-%m-%d %H:%M:%S").to_string())]);

    table.add_row(vec![ui.cell("Version:"), ui.cell(&metadata.version)]);

    if let Some(migration) = &metadata.migration {
        table.add_row(vec![
            ui.cell("Migration:"),
            ui.colored_cell(
                format!("Migrated from legacy ({})", migration.migration_date.format("%Y-%m-%d")),
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
        crate::fs_utils::dir_size(path)
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
        bail!(
            "At least one component must be selected.\nHint: Use Space to toggle components, then press Enter to confirm."
        );
    }

    Ok(selected)
}

/// Add a new profile from current settings
pub fn add(paths: &Paths, name: &str, ui: &Ui, components_arg: Option<Vec<String>>) -> Result<()> {
    paths.ensure_dirs()?;

    if profile_exists(paths, name) {
        bail!(
            "Profile '{}' already exists.\nHint: Use 'ccprof edit {}' to modify it, or choose a different name.",
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
                        "Invalid component name: '{}'\nHint: Valid components are settings, agents, hooks, commands",
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
            "Profile '{}' does not exist.\nHint: Use 'ccprof list' to see available profiles.",
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

/// Edit a specific component of a profile
pub fn edit_component(paths: &Paths, name: &str, component: &str, ui: &Ui) -> Result<()> {
    if !profile_exists(paths, name) {
        bail!(
            "Profile '{}' does not exist.\nHint: Use 'ccprof list' to see available profiles.",
            name
        );
    }

    // Parse component
    let comp: Component = component.parse().map_err(|_| {
        anyhow::anyhow!(
            "Invalid component: '{}'\nHint: Valid components are settings, agents, hooks, commands",
            component
        )
    })?;

    let component_path = comp.profile_path(paths, name);

    if !component_path.exists() {
        bail!(
            "Component '{}' not found in profile '{}'.\nHint: This profile may not include this component.",
            component,
            name
        );
    }

    // Open in editor
    open_in_editor(&component_path)?;
    ui.ok(format!("Opened {} in editor", component_path.display()));
    Ok(())
}

/// Edit all managed components of a profile
pub fn edit_all_components(paths: &Paths, name: &str, ui: &Ui) -> Result<()> {
    if !profile_exists(paths, name) {
        bail!(
            "Profile '{}' does not exist.\nHint: Use 'ccprof list' to see available profiles.",
            name
        );
    }

    let profile_dir = paths.profile_dir(name);
    let metadata = crate::components::ProfileMetadata::read(&profile_dir)?;

    if metadata.managed_components.is_empty() {
        bail!(
            "Profile '{}' has no managed components.\nHint: Use 'ccprof edit {} --track' to add components.",
            name,
            name
        );
    }

    // Collect paths to open
    let mut paths_to_open: Vec<std::path::PathBuf> = Vec::new();
    for comp in &metadata.managed_components {
        let path = comp.profile_path(paths, name);
        if path.exists() {
            paths_to_open.push(path);
        }
    }

    if paths_to_open.is_empty() {
        bail!("No component files found for profile '{}'", name);
    }

    // Open all in editor
    open_multiple_in_editor(&paths_to_open)?;
    ui.ok(format!("Opened {} component(s) in editor", paths_to_open.len()));
    Ok(())
}

/// Open a file in the user's editor
fn open_in_editor(path: &std::path::Path) -> Result<()> {
    if let Ok(editor) = std::env::var("EDITOR") {
        let status = Command::new(&editor)
            .arg(path)
            .status()
            .with_context(|| format!("Failed to run editor: {}", editor))?;

        if !status.success() {
            bail!("Editor exited with non-zero status");
        }
    } else {
        // Fallback to macOS 'open -t'
        let status = Command::new("open")
            .arg("-t")
            .arg(path)
            .status()
            .context("Failed to run 'open -t'")?;

        if !status.success() {
            bail!("'open -t' exited with non-zero status");
        }
    }
    Ok(())
}

/// Open multiple files in the user's editor
fn open_multiple_in_editor(paths: &[std::path::PathBuf]) -> Result<()> {
    if let Ok(editor) = std::env::var("EDITOR") {
        let status = Command::new(&editor)
            .args(paths)
            .status()
            .with_context(|| format!("Failed to run editor: {}", editor))?;

        if !status.success() {
            bail!("Editor exited with non-zero status");
        }
    } else {
        // Fallback to macOS 'open -t' with multiple files
        let status = Command::new("open")
            .arg("-t")
            .args(paths)
            .status()
            .context("Failed to run 'open -t'")?;

        if !status.success() {
            bail!("'open -t' exited with non-zero status");
        }
    }
    Ok(())
}

/// Edit a profile's tracked components
pub fn edit_components(
    paths: &Paths,
    name: &str,
    ui: &Ui,
    components_arg: Option<Vec<String>>,
) -> Result<()> {
    if !profile_exists(paths, name) {
        bail!(
            "Profile '{}' does not exist.\nHint: Use 'ccprof list' to see available profiles.",
            name
        );
    }

    let profile_dir = paths.profile_dir(name);
    let metadata = crate::components::ProfileMetadata::read(&profile_dir)?;

    // Determine which components to use
    let new_components = if let Some(comp_names) = components_arg {
        // Interactive mode: use multi-select UI with current selection as default
        if comp_names.is_empty() {
            edit_select_components(paths, &metadata.managed_components)?
        } else {
            // Non-interactive mode: parse component names
            let mut selected = HashSet::new();
            for comp_name in comp_names {
                match comp_name.parse::<Component>() {
                    Ok(c) => {
                        selected.insert(c);
                    }
                    Err(_) => {
                        bail!(
                            "Invalid component name: '{}'\nValid components: settings, agents, hooks, commands",
                            comp_name
                        );
                    }
                }
            }
            selected
        }
    } else {
        bail!(
            "No components specified.\nHint: Use --components settings,agents,hooks,commands or run interactively."
        );
    };

    // Update the profile components
    update_profile_components(paths, name, new_components.clone())?;

    ui.ok(format!("Updated components for profile '{}'", name));
    ui.newline();
    ui.println("Now tracking:");
    for component in &new_components {
        ui.println(format!("  {} {}", ui.icon_ok(), component.display_name()));
    }

    Ok(())
}

/// Interactive component selection for editing profile components
fn edit_select_components(
    paths: &Paths,
    current_components: &HashSet<Component>,
) -> Result<HashSet<Component>> {
    let all_components = Component::all();

    // Build display options with availability indicators
    let options: Vec<String> = all_components
        .iter()
        .map(|c| {
            let path = c.source_path(paths);
            let exists = path.exists();
            let indicator = if exists { "✓" } else { "✗" };
            let availability = if exists { "" } else { " (not found)" };
            let tracked = if current_components.contains(c) {
                " [tracked]"
            } else {
                ""
            };
            format!(
                "{}{} {}{}",
                indicator,
                c.display_name(),
                availability,
                tracked
            )
        })
        .collect();

    // Default: select all currently tracked components
    let defaults: Vec<usize> = all_components
        .iter()
        .enumerate()
        .filter(|(_, c)| current_components.contains(c))
        .map(|(i, _)| i)
        .collect();

    let selected_indices = MultiSelect::new(
        "Which components should this profile manage?",
        options.clone(),
    )
    .with_default(&defaults)
    .with_help_message(
        "Space to select, Enter to confirm. Currently tracked components are pre-selected.",
    )
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
        bail!(
            "At least one component must be selected.\nHint: Use Space to toggle components, then press Enter to confirm."
        );
    }

    Ok(selected)
}

/// Run diagnostics
pub fn doctor(paths: &Paths, ui: &Ui) -> Result<()> {
    run_doctor(paths, ui);
    Ok(())
}

/// List all backups
pub fn backup_list(paths: &Paths, ui: &Ui) -> Result<()> {
    if !paths.backups_dir.exists() {
        ui.warn("No backups found.");
        ui.newline();
        ui.println("Backups are created automatically when switching profiles.");
        return Ok(())
    }

    let entries: Vec<_> = std::fs::read_dir(&paths.backups_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_str().is_some_and(|n| n.ends_with(".bak")))
        .collect();

    if entries.is_empty() {
        ui.warn("No backups found.");
        return Ok(())
    }

    // Parse and sort backups by timestamp
    let mut backups: Vec<_> = entries
        .iter()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let metadata = e.metadata().ok()?;
            let modified = metadata.modified().ok()?;
            let size = if metadata.is_file() {
                metadata.len()
            } else {
                crate::fs_utils::dir_size(&e.path()).unwrap_or(0)
            };
            Some((name, modified, size, e.path()))
        })
        .collect();

    backups.sort_by(|a, b| b.1.cmp(&a.1)); // Most recent first

    ui.section("Backups");
    ui.newline();

    let mut table = ui.table();
    table.set_header(vec![
        ui.header_cell("ID"),
        ui.header_cell("Component"),
        ui.header_cell("Date"),
        ui.header_cell("Size"),
    ]);

    for (name, modified, size, _path) in &backups {
        // Parse component from name (e.g., "settings.json.20240115_103045.bak")
        let component = if name.starts_with("settings.json.") {
            "Settings"
        } else if name.starts_with("agents.") {
            "Agents"
        } else if name.starts_with("hooks.") {
            "Hooks"
        } else if name.starts_with("commands.") {
            "Commands"
        } else {
            "Unknown"
        };

        // Format date
        let datetime: chrono::DateTime<chrono::Utc> = (*modified).into();
        let date_str = datetime.format("%Y-%m-%d %H:%M:%S").to_string();

        table.add_row(vec![
            ui.cell(name),
            ui.cell(component),
            ui.cell(date_str),
            ui.cell(format_bytes(*size)),
        ]);
    }

    ui.println(table.to_string());
    ui.newline();
    ui.info(format!("{} backup(s) found", backups.len()));

    Ok(())
}

/// Restore a backup
pub fn backup_restore(paths: &Paths, id: &str, ui: &Ui) -> Result<()> {
    let backup_path = paths.backups_dir.join(id);

    if !backup_path.exists() {
        bail!(
            "Backup '{}' not found.\nHint: Use 'ccprof backup list' to see available backups.",
            id
        );
    }

    // Determine component from backup name
    let component = if id.starts_with("settings.json.") {
        Component::Settings
    } else if id.starts_with("agents.") {
        Component::Agents
    } else if id.starts_with("hooks.") {
        Component::Hooks
    } else if id.starts_with("commands.") {
        Component::Commands
    } else {
        bail!(
            "Cannot determine component type from backup name: {}\nHint: Backup names should start with 'settings.json.', 'agents.', etc.",
            id
        );
    };

    // Confirm restore
    let target = component.source_path(paths);
    let confirm = inquire::Confirm::new(&format!("Restore '{}' to {}?", id, target.display()))
        .with_default(false)
        .with_help_message("This will overwrite the current file/directory")
        .prompt()
        .context("Confirmation cancelled")?;

    if !confirm {
        ui.warn("Restore cancelled.");
        return Ok(())
    }

    // Remove current target if it exists
    if target.exists() || std::fs::read_link(&target).is_ok() {
        if target.is_dir() && !target.is_symlink() {
            std::fs::remove_dir_all(&target)
                .with_context(|| format!("Failed to remove {}", target.display()))?;
        } else {
            std::fs::remove_file(&target)
                .with_context(|| format!("Failed to remove {}", target.display()))?;
        }
    }

    // Copy backup to target
    if backup_path.is_dir() {
        crate::fs_utils::copy_dir_recursive(&backup_path, &target)?;
    } else {
        std::fs::copy(&backup_path, &target)
            .with_context(|| format!("Failed to copy backup to {}", target.display()))?;
    }

    ui.ok(format!("Restored '{}' to {}", id, target.display()));
    Ok(())
}

/// Clean old backups
pub fn backup_clean(paths: &Paths, keep: usize, ui: &Ui) -> Result<()> {
    if !paths.backups_dir.exists() {
        ui.warn("No backups directory found.");
        return Ok(())
    }

    let mut removed = 0;

    // Clean each component type separately
    for prefix in ["settings.json.", "agents.", "hooks.", "commands."] {
        let mut backups: Vec<_> = std::fs::read_dir(&paths.backups_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with(prefix) && n.ends_with(".bak"))
            })
            .filter_map(|e| {
                let modified = e.metadata().ok()?.modified().ok()?;
                Some((e.path(), modified))
            })
            .collect();

        if backups.len() <= keep {
            continue;
        }

        // Sort by date (oldest first)
        backups.sort_by_key(|(_, time)| *time);

        // Remove oldest backups
        let to_remove = backups.len() - keep;
        for (path, _) in backups.iter().take(to_remove) {
            if path.is_dir() {
                std::fs::remove_dir_all(path)?;
            } else {
                std::fs::remove_file(path)?;
            }
            removed += 1;
        }
    }

    if removed > 0 {
        ui.ok(format!(
            "Removed {} old backup(s), keeping {} per component",
            removed,
            keep
        ));
    } else {
        ui.ok(format!("No backups to clean (keeping {} per component)", keep));
    }

    Ok(())
}

/// Remove a profile
pub fn remove(paths: &Paths, name: &str, ui: &Ui, force: bool) -> Result<()> {
    if !profile_exists(paths, name) {
        bail!(
            "Profile '{}' does not exist.\nHint: Use 'ccprof list' to see available profiles.",
            name
        );
    }

    // Check if this is the active profile
    let state = State::read(&paths.state_file).unwrap_or_default();
    let is_active = state.default_profile.as_deref() == Some(name);

    if is_active {
        bail!(
            "Cannot remove '{}' because it is the currently active profile.\nHint: Switch to another profile first with 'ccprof use <other-profile>'.",
            name
        );
    }

    // Confirm unless --force
    if !force {
        let confirm = inquire::Confirm::new(&format!("Are you sure you want to remove profile '{}'?", name))
            .with_default(false)
            .with_help_message("This will permanently delete the profile and all its settings")
            .prompt()
            .context("Confirmation cancelled")?;

        if !confirm {
            ui.warn("Removal cancelled.");
            return Ok(())
        }
    }

    // Remove the profile
    crate::profiles::remove_profile(paths, name)?;

    ui.ok(format!("Removed profile '{}'", name));
    Ok(())
}

/// Compare two profiles
pub fn diff(paths: &Paths, profile1: &str, profile2: &str, component: &str, ui: &Ui) -> Result<()> {
    // Validate both profiles exist
    if !profile_exists(paths, profile1) {
        bail!(
            "Profile '{}' does not exist.\nHint: Use 'ccprof list' to see available profiles.",
            profile1
        );
    }
    if !profile_exists(paths, profile2) {
        bail!(
            "Profile '{}' does not exist.\nHint: Use 'ccprof list' to see available profiles.",
            profile2
        );
    }

    // Parse component
    let comp: Component = component.parse().map_err(|_| {
        anyhow::anyhow!(
            "Invalid component: '{}'\nHint: Valid components are settings, agents, hooks, commands",
            component
        )
    })?;

    // Get paths to the component in each profile
    let path1 = comp.profile_path(paths, profile1);
    let path2 = comp.profile_path(paths, profile2);

    // Check if component exists in both profiles
    if !path1.exists() {
        bail!(
            "Component '{}' not found in profile '{}'.\nHint: This profile may not include this component.",
            component,
            profile1
        );
    }
    if !path2.exists() {
        bail!(
            "Component '{}' not found in profile '{}'.\nHint: This profile may not include this component.",
            component,
            profile2
        );
    }

    ui.section(format!("Comparing {} between '{}' and '{}'", comp.display_name(), profile1, profile2));
    ui.newline();

    if comp.is_file() {
        // Compare JSON files
        diff_json_files(&path1, &path2, profile1, profile2, ui)?;
    } else {
        // Compare directories
        diff_directories(&path1, &path2, profile1, profile2, ui)?;
    }

    Ok(())
}

/// Compare two JSON files and display differences
fn diff_json_files(
    path1: &std::path::Path,
    path2: &std::path::Path,
    name1: &str,
    name2: &str,
    ui: &Ui,
) -> Result<()> {
    let content1 = std::fs::read_to_string(path1)
        .with_context(|| format!("Failed to read {}", path1.display()))?;
    let content2 = std::fs::read_to_string(path2)
        .with_context(|| format!("Failed to read {}", path2.display()))?;

    let json1: serde_json::Value = serde_json::from_str(&content1)
        .with_context(|| format!("Failed to parse JSON from {}", path1.display()))?;
    let json2: serde_json::Value = serde_json::from_str(&content2)
        .with_context(|| format!("Failed to parse JSON from {}", path2.display()))?;

    if json1 == json2 {
        ui.ok("Files are identical");
        return Ok(())
    }

    // Find differences
    let mut differences = Vec::new();
    compare_json_values(&json1, &json2, "", &mut differences);

    if differences.is_empty() {
        ui.ok("Files are identical");
        return Ok(())
    }

    // Display differences
    let mut table = ui.table();
    table.set_header(vec![ui.header_cell("Key"), ui.header_cell(name1), ui.header_cell(name2)]);

    for (key, val1, val2) in &differences {
        table.add_row(vec![ui.cell(key), ui.cell(format_json_value(val1)), ui.cell(format_json_value(val2))]);
    }

    ui.println(table.to_string());
    ui.newline();
    ui.info(format!("{} difference(s) found", differences.len()));

    Ok(())
}

/// Recursively compare JSON values and collect differences
fn compare_json_values(
    v1: &serde_json::Value,
    v2: &serde_json::Value,
    path: &str,
    differences: &mut Vec<(String, Option<serde_json::Value>, Option<serde_json::Value>)>, 
) {
    use serde_json::Value;

    match (v1, v2) {
        (Value::Object(o1), Value::Object(o2)) => {
            // Check keys in o1
                        for (key, val1) in o1 {
                            let new_path = if path.is_empty() {
                                key.clone()
                            } else {
                                format!("{}.{}", path, key)
                            };
            
                            match o2.get(key) {
                                Some(val2) => {
                                    compare_json_values(val1, val2, &new_path, differences);
                                }
                                None => {
                                    differences.push((new_path, Some(val1.clone()), None));
                                }
                            }
                        }
                        // Check keys only in o2
                        for (key, val2) in o2 {
                            if !o1.contains_key(key) {
                                let new_path = if path.is_empty() {
                                    key.clone()
                                } else {
                                    format!("{}.{}", path, key)
                                };
                                differences.push((new_path, None, Some(val2.clone())));
                            }
                        }
                    }
                    (Value::Array(a1), Value::Array(a2)) => {
                        if a1 != a2 {
                            differences.push((path.to_string(), Some(v1.clone()), Some(v2.clone())));
                        }
                    }
                    _ => {
                        if v1 != v2 {
                            differences.push((path.to_string(), Some(v1.clone()), Some(v2.clone())));
                        }
                    }
                }
            }
            
            /// Format a JSON value for display (truncate if too long)
fn format_json_value(val: &Option<serde_json::Value>) -> String {
    match val {
        None => "(missing)".to_string(),
        Some(v) => {
            let s = match v {
                serde_json::Value::String(s) => format!("\"{}\"", s),
                serde_json::Value::Array(arr) => format!("[{} items]", arr.len()),
                serde_json::Value::Object(obj) => format!("{{...}} ({} keys)", obj.len()),
                other => other.to_string(),
            };
            if s.len() > 50 {
                format!("{}...", &s[..47])
            } else {
                s
            }
        }
    }
}

/// Compare two directories and list differences
fn diff_directories(
    path1: &std::path::Path,
    path2: &std::path::Path,
    name1: &str,
    name2: &str,
    ui: &Ui,
) -> Result<()> {
    use std::collections::HashSet;

    let files1: HashSet<String> = std::fs::read_dir(path1)?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .collect();

    let files2: HashSet<String> = std::fs::read_dir(path2)?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .collect();

    let only_in_1: Vec<_> = files1.difference(&files2).collect();
    let only_in_2: Vec<_> = files2.difference(&files1).collect();
    let in_both: Vec<_> = files1.intersection(&files2).collect();

    let mut has_diff = false;

    if !only_in_1.is_empty() {
        has_diff = true;
        ui.println(format!("Only in '{}':", name1));
        for f in &only_in_1 {
            ui.println(format!("  - {}", f));
        }
        ui.newline();
    }

    if !only_in_2.is_empty() {
        has_diff = true;
        ui.println(format!("Only in '{}':", name2));
        for f in &only_in_2 {
            ui.println(format!("  + {}", f));
        }
        ui.newline();
    }

    // Check content differences for files in both
    let mut content_diffs = Vec::new();
    for file in &in_both {
        let p1 = path1.join(file);
        let p2 = path2.join(file);

        if p1.is_file() && p2.is_file() {
            let c1 = std::fs::read(&p1).unwrap_or_default();
            let c2 = std::fs::read(&p2).unwrap_or_default();
            if c1 != c2 {
                content_diffs.push(file.as_str());
            }
        }
    }

    if !content_diffs.is_empty() {
        has_diff = true;
        ui.println("Files with different content:");
        for f in &content_diffs {
            ui.println(format!("  ~ {}", f));
        }
        ui.newline();
    }

    if !has_diff {
        ui.ok("Directories are identical");
    } else {
        ui.info(format!("{} only in {}, {} only in {}, {} different", only_in_1.len(), name1, only_in_2.len(), name2, content_diffs.len()));
    }

    Ok(())
}

/// Rename a profile
pub fn rename(paths: &Paths, old_name: &str, new_name: &str, ui: &Ui) -> Result<()> {
    if !profile_exists(paths, old_name) {
        bail!(
            "Profile '{}' does not exist.\nHint: Use 'ccprof list' to see available profiles.",
            old_name
        );
    }

    if profile_exists(paths, new_name) {
        bail!(
            "Profile '{}' already exists.\nHint: Choose a different name or remove the existing profile first.",
            new_name
        );
    }

    // Validate new name
    crate::profiles::validate_profile_name(new_name)?;

    // Check if this is the active profile
    let state = State::read(&paths.state_file).unwrap_or_default();
    let is_active = state.default_profile.as_deref() == Some(old_name);

    // Rename the profile directory
    crate::profiles::rename_profile(paths, old_name, new_name)?;

    // Update state if it was the active profile
    if is_active {
        use crate::state::LockedState;
        let mut locked = LockedState::lock(&paths.state_file)?;
        locked.update(|s| {
            s.default_profile = Some(new_name.to_string());
        })?;

        // Update symlinks to point to new location
        let profile_dir = paths.profile_dir(new_name);
        let metadata = crate::components::ProfileMetadata::read(&profile_dir)?;

        for component in &metadata.managed_components {
            let source = component.source_path(paths);
            let target = component.profile_path(paths, new_name);

            // Only update if it's already a symlink pointing to our profiles
            if let Ok(current_target) = std::fs::read_link(&source)
                && (paths.is_in_profiles_dir(&current_target)
                    || paths.is_in_profiles_dir(
                        &source.parent().unwrap_or(&source).join(&current_target),
                    )) {
                crate::switch::create_component_symlink(&source, &target, component, &paths.backups_dir)?;
            }
        }

        ui.ok(format!("Renamed profile '{}' to '{}' (symlinks updated)", old_name, new_name));
    } else {
        ui.ok(format!("Renamed profile '{}' to '{}'", old_name, new_name));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_test_paths;
    use crate::ui::ColorMode;
    use std::fs;
    use tempfile::TempDir;

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
