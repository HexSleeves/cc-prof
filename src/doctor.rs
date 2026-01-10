use anstyle::AnsiColor;
use std::env;
use std::path::Path;

use crate::paths::Paths;
use crate::profiles::{ValidationResult, list_profiles, validate_json_file_result};
use crate::state::State;
use crate::switch::SettingsStatus;
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
                ui.header_cell("JSON Status"),
            ]);

            for name in &profiles {
                let settings_path = paths.profile_settings(name);
                let validation = validate_json_file_result(&settings_path);
                let (icon, status_cell) = match validation {
                    ValidationResult::Valid => {
                        (ui.icon_ok(), ui.colored_cell("valid", AnsiColor::Green))
                    }
                    ValidationResult::Missing => (
                        ui.icon_warn(),
                        ui.colored_cell("missing", AnsiColor::Yellow),
                    ),
                    ValidationResult::Invalid(ref e) => (
                        ui.icon_err(),
                        ui.colored_cell(format!("invalid: {}", e), AnsiColor::Red),
                    ),
                };
                profiles_table.add_row(vec![ui.cell(icon), ui.cell(name), status_cell]);
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
        let profile_settings = paths.profile_settings(profile_name);
        let validation = validate_json_file_result(&profile_settings);
        match validation {
            ValidationResult::Valid => {
                ui.ok(format!("Profile '{}' JSON is valid", profile_name));
            }
            ValidationResult::Missing => {
                ui.err(format!(
                    "Profile '{}' is set as default but settings file is missing!",
                    profile_name
                ));
                ui.println(format!("  Expected: {:?}", profile_settings));
            }
            ValidationResult::Invalid(ref err) => {
                ui.err(format!(
                    "Profile '{}' has invalid JSON: {}",
                    profile_name, err
                ));
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
        };
        let ui = test_ui();

        // Just ensure it doesn't panic
        run_doctor(&paths, &ui);
    }
}
