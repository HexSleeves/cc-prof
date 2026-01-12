use anstyle::AnsiColor;
use std::env;
use std::path::Path;

use crate::components::ProfileMetadata;
use crate::paths::Paths;
use crate::profiles::list_profiles;
use crate::state::State;
use crate::switch::{ComponentStatus, SettingsStatus};
use crate::ui::Ui;

/// Run diagnostics on the ccprof setup
pub fn run_doctor(paths: &Paths, ui: &Ui) {
    ui.section("ccprof doctor - Diagnostics Report");
    ui.newline();

    // --- Computed Paths ---
    ui.section("Computed Paths");
    let mut paths_table = ui.simple_table();
    paths_table.add_row(vec!["Base directory", &format!("{:?}", paths.base_dir)]);
    paths_table.add_row(vec![
        "Profiles directory",
        &format!("{:?}", paths.profiles_dir),
    ]);
    paths_table.add_row(vec![
        "Backups directory",
        &format!("{:?}", paths.backups_dir),
    ]);
    paths_table.add_row(vec!["State file", &format!("{:?}", paths.state_file)]);
    paths_table.add_row(vec!["Claude directory", &format!("{:?}", paths.claude_dir)]);
    paths_table.add_row(vec![
        "Claude settings",
        &format!("{:?}", paths.claude_settings),
    ]);
    paths_table.add_row(vec!["Claude agents", &format!("{:?}", paths.claude_agents)]);
    paths_table.add_row(vec!["Claude hooks", &format!("{:?}", paths.claude_hooks)]);
    paths_table.add_row(vec![
        "Claude commands",
        &format!("{:?}", paths.claude_commands),
    ]);
    ui.println(paths_table.to_string());
    ui.newline();

    // --- Directory Status ---
    ui.section("Directory Status");
    let mut dir_table = ui.table();
    dir_table.set_header(vec![ui.header_cell("Directory"), ui.header_cell("Status")]);
    add_exists_row(ui, &mut dir_table, "Base directory", &paths.base_dir);
    add_exists_row(
        ui,
        &mut dir_table,
        "Profiles directory",
        &paths.profiles_dir,
    );
    add_exists_row(ui, &mut dir_table, "Backups directory", &paths.backups_dir);
    add_exists_row(ui, &mut dir_table, "Claude directory", &paths.claude_dir);
    ui.println(dir_table.to_string());
    ui.newline();

    // --- Settings File Status ---
    let status = SettingsStatus::detect(&paths.claude_settings);
    ui.section("Settings File Status");
    let mut settings_table = ui.simple_table();

    let status_cell = match &status {
        SettingsStatus::Missing => ui.colored_cell("missing", AnsiColor::Yellow),
        SettingsStatus::RegularFile => ui.cell("regular file"),
        SettingsStatus::Symlink { target } => ui.colored_cell(
            format!("{} symlink → {}", ui.icon_ok(), target.display()),
            AnsiColor::Green,
        ),
        SettingsStatus::BrokenSymlink { target } => ui.colored_cell(
            format!("{} broken symlink → {}", ui.icon_err(), target.display()),
            AnsiColor::Red,
        ),
    };
    settings_table.add_row(vec![ui.cell("~/.claude/settings.json"), status_cell]);

    if let SettingsStatus::Symlink { ref target } | SettingsStatus::BrokenSymlink { ref target } =
        status
    {
        settings_table.add_row(vec![ui.cell("Target"), ui.cell(format!("{:?}", target))]);
        let is_profile_cell = if status.is_profile_symlink(paths) {
            ui.colored_cell("yes", AnsiColor::Green)
        } else {
            ui.colored_cell("no", AnsiColor::Yellow)
        };
        settings_table.add_row(vec![ui.cell("Is profile symlink"), is_profile_cell]);
    }
    ui.println(settings_table.to_string());
    ui.newline();

    // --- State File ---
    ui.section("State File");
    let mut state_table = ui.simple_table();
    match State::read(&paths.state_file) {
        Ok(state) => {
            let profile_str = state.default_profile.as_deref().unwrap_or("(not set)");
            state_table.add_row(vec!["Default profile", profile_str]);
            if let Some(ref updated) = state.updated_at {
                state_table.add_row(vec!["Last updated", &updated.to_string()]);
            }
        }
        Err(e) => {
            state_table.add_row(vec![
                &format!("{} Error reading state", ui.icon_err()),
                &e.to_string(),
            ]);
        }
    }
    ui.println(state_table.to_string());
    ui.newline();

    // --- Profiles ---
    ui.section("Profiles");
    match list_profiles(paths) {
        Ok(profiles) if profiles.is_empty() => {
            ui.println(ui.dim("  (no profiles found)"));
        }
        Ok(profiles) => {
            let mut profiles_table = ui.table();
            profiles_table.set_header(vec![
                ui.header_cell(""),
                ui.header_cell("Profile"),
                ui.header_cell("Components"),
                ui.header_cell("Metadata"),
                ui.header_cell("Status"),
            ]);

            for name in &profiles {
                let profile_dir = paths.profile_dir(name);
                let metadata_path = paths.profile_metadata(name);

                // Check metadata file
                let (meta_icon, meta_status) = if metadata_path.exists() {
                    match ProfileMetadata::read(&profile_dir) {
                        Ok(_) => (ui.icon_ok(), "valid"),
                        Err(_) => (ui.icon_err(), "invalid"),
                    }
                } else {
                    (ui.icon_warn(), "missing")
                };

                // Get component info
                let (components_str, overall_icon, overall_status) =
                    match ProfileMetadata::read(&profile_dir) {
                        Ok(metadata) => {
                            let mut comp_codes: Vec<&str> = metadata
                                .managed_components
                                .iter()
                                .map(|c| c.short_name())
                                .collect();
                            comp_codes.sort();
                            let comp_str = comp_codes.join(",");

                            // Check if all components exist
                            let mut all_exist = true;
                            for component in &metadata.managed_components {
                                let path = component.profile_path(paths, name);
                                if !path.exists() {
                                    all_exist = false;
                                    break;
                                }
                            }

                            let (icon, status) = if all_exist {
                                (ui.icon_ok(), "ok")
                            } else {
                                (ui.icon_warn(), "missing components")
                            };

                            (comp_str, icon, status)
                        }
                        Err(_) => (String::from("?"), ui.icon_err(), "metadata error"),
                    };

                profiles_table.add_row(vec![
                    ui.cell(overall_icon),
                    ui.cell(name),
                    ui.cell(components_str),
                    ui.cell(format!("{} {}", meta_icon, meta_status)),
                    ui.cell(overall_status),
                ]);
            }
            ui.println(profiles_table.to_string());
        }
        Err(e) => {
            ui.err(format!("Error listing profiles: {}", e));
        }
    }
    ui.newline();

    // --- Active Profile Validation ---
    let state = State::read(&paths.state_file).unwrap_or_default();
    if let Some(ref profile_name) = state.default_profile {
        ui.section("Active Profile Validation");

        let profile_dir = paths.profile_dir(profile_name);
        match ProfileMetadata::read(&profile_dir) {
            Ok(metadata) => {
                ui.ok(format!(
                    "Profile '{}' has {} managed component(s)",
                    profile_name,
                    metadata.managed_components.len()
                ));

                // Check each managed component
                let mut comp_table = ui.simple_table();
                comp_table.set_header(vec![
                    ui.header_cell(""),
                    ui.header_cell("Component"),
                    ui.header_cell("Profile File"),
                    ui.header_cell("Symlink Status"),
                ]);

                for component in &metadata.managed_components {
                    let profile_path = component.profile_path(paths, profile_name);
                    let source_path = component.source_path(paths);

                    // Check if profile component exists
                    let (profile_icon, profile_status) = if profile_path.exists() {
                        (ui.icon_ok(), "exists")
                    } else {
                        (ui.icon_err(), "missing")
                    };

                    // Check symlink status
                    let symlink_status = ComponentStatus::detect(&source_path);
                    let symlink_cell = match symlink_status {
                        ComponentStatus::Missing => ui.colored_cell("missing", AnsiColor::Yellow),
                        ComponentStatus::RegularFile | ComponentStatus::RegularDirectory => {
                            ui.colored_cell("not a symlink", AnsiColor::Yellow)
                        }
                        ComponentStatus::Symlink { ref target } => {
                            if target == &profile_path {
                                ui.colored_cell(
                                    format!("{} correct", ui.icon_ok()),
                                    AnsiColor::Green,
                                )
                            } else {
                                ui.colored_cell(
                                    format!("{} wrong target", ui.icon_warn()),
                                    AnsiColor::Yellow,
                                )
                            }
                        }
                        ComponentStatus::BrokenSymlink { .. } => {
                            ui.colored_cell(format!("{} broken", ui.icon_err()), AnsiColor::Red)
                        }
                    };

                    comp_table.add_row(vec![
                        ui.cell(profile_icon),
                        ui.cell(component.display_name()),
                        ui.cell(profile_status),
                        symlink_cell,
                    ]);
                }

                ui.println(comp_table.to_string());
            }
            Err(e) => {
                ui.err(format!("Profile '{}' metadata error: {}", profile_name, e));
            }
        }
        ui.newline();
    }

    // --- Project-Level Claude Files ---
    check_project_claude_files(ui);
}

fn add_exists_row(ui: &Ui, table: &mut comfy_table::Table, label: &str, path: &Path) {
    let (icon, status, color) = if path.exists() {
        (ui.icon_ok(), "exists", AnsiColor::Green)
    } else {
        (ui.icon_err(), "missing", AnsiColor::Red)
    };
    table.add_row(vec![
        ui.cell(label),
        ui.colored_cell(format!("{} {}", icon, status), color),
    ]);
}

fn check_project_claude_files(ui: &Ui) {
    ui.section("Project-Level Claude Files");

    let cwd = match env::current_dir() {
        Ok(dir) => dir,
        Err(e) => {
            ui.warn(format!("Could not determine current directory: {}", e));
            return;
        }
    };

    let project_claude_dir = cwd.join(".claude");
    let project_settings = project_claude_dir.join("settings.json");
    let project_local = project_claude_dir.join("settings.local.json");

    let mut found_any = false;
    let mut table = ui.simple_table();

    if project_settings.exists() {
        table.add_row(vec![
            ui.icon_warn(),
            ".claude/settings.json",
            "project-level (not managed by ccprof)",
        ]);
        found_any = true;
    }

    if project_local.exists() {
        table.add_row(vec![
            ui.icon_warn(),
            ".claude/settings.local.json",
            "project-level (not managed by ccprof)",
        ]);
        found_any = true;
    }

    if found_any {
        ui.println(table.to_string());
    } else {
        ui.println(format!(
            "  {} None found in current directory",
            ui.icon_ok()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::ColorMode;

    fn test_ui() -> Ui {
        Ui::new(ColorMode::Never, false)
    }

    #[test]
    fn test_run_doctor_does_not_panic() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let paths = Paths {
            base_dir: temp_dir.path().join(".claude-profiles"),
            profiles_dir: temp_dir.path().join(".claude-profiles/profiles"),
            backups_dir: temp_dir.path().join(".claude-profiles/backups"),
            state_file: temp_dir.path().join(".claude-profiles/state.json"),
            claude_dir: temp_dir.path().join(".claude"),
            claude_settings: temp_dir.path().join(".claude/settings.json"),
            claude_agents: temp_dir.path().join(".claude/agents"),
            claude_hooks: temp_dir.path().join(".claude/hooks"),
            claude_commands: temp_dir.path().join(".claude/commands"),
        };
        let ui = test_ui();

        // Just ensure it doesn't panic
        run_doctor(&paths, &ui);
    }
}
