//! Diagnostic tool for ccprof.
//!
//! This module implements the `ccprof doctor` command, which checks the system
//! for common issues:
//! - Existence of required directories.
//! - Validity of symbolic links.
//! - Correctness of profile metadata and JSON files.
//! - Permissions.
//!
//! It reports issues to the user with a pass/fail/warn status.

use anstyle::AnsiColor;
use std::env;

use crate::components::ProfileMetadata;
use crate::paths::Paths;
use crate::profiles::list_profiles;
use crate::state::State;
use crate::switch::SettingsStatus;
use crate::ui::Ui;

/// Run the doctor diagnostics
pub fn run_doctor(paths: &Paths, ui: &Ui) {
    ui.section("ccprof Doctor");
    ui.newline();

    // 1. Check directories
    check_step(ui, "Directories", || {
        let mut ok = true;
        if paths.base_dir.exists() {
            ui.println(format!(
                "  {} Base directory exists: {}",
                ui.icon_ok(),
                paths.base_dir.display()
            ));
        } else {
            ui.println(format!(
                "  {} Base directory missing: {}",
                ui.icon_err(),
                paths.base_dir.display()
            ));
            ok = false;
        }

        if paths.claude_dir.exists() {
            ui.println(format!(
                "  {} Claude directory exists: {}",
                ui.icon_ok(),
                paths.claude_dir.display()
            ));
        } else {
            ui.println(format!(
                "  {} Claude directory missing: {}",
                ui.icon_warn(),
                paths.claude_dir.display()
            ));
            // Not necessarily an error if they haven't installed Claude Code yet
        }
        ok
    });

    // 2. Check State
    check_step(ui, "State File", || {
        match State::read(&paths.state_file) {
            Ok(state) => {
                ui.println(format!(
                    "  {} State file readable",
                    ui.icon_ok()
                ));
                if let Some(profile) = &state.default_profile {
                    ui.println(format!(
                        "  {} Active profile in state: {}",
                        ui.icon_info(),
                        profile
                    ));
                    // Verify that this profile actually exists
                    if paths.profile_dir(profile).exists() {
                         ui.println(format!(
                            "  {} Active profile directory exists",
                            ui.icon_ok()
                        ));
                    } else {
                         ui.println(format!(
                            "  {} Active profile directory MISSING",
                            ui.icon_err()
                        ));
                        return false;
                    }
                } else {
                    ui.println(format!("  {} No active profile set", ui.icon_info()));
                }
                true
            }
            Err(e) => {
                if paths.state_file.exists() {
                    ui.println(format!("  {} State file corrupt: {}", ui.icon_err(), e));
                    false
                } else {
                    ui.println(format!("  {} State file missing (fresh install?)", ui.icon_warn()));
                    true
                }
            }
        }
    });

    // 3. Check Settings Link
    check_step(ui, "Settings Symlink", || {
        let status = SettingsStatus::detect(&paths.claude_settings);
        match status {
            SettingsStatus::Missing => {
                ui.println(format!("  {} ~/.claude/settings.json is missing", ui.icon_warn()));
                // Not fatal
                true
            }
            SettingsStatus::RegularFile => {
                ui.println(format!("  {} ~/.claude/settings.json is a regular file (not managed)", ui.icon_info()));
                true
            }
            SettingsStatus::Symlink { target } => {
                ui.println(format!("  {} Symlink points to: {}", ui.icon_ok(), target.display()));
                if paths.is_in_profiles_dir(&target) {
                    ui.println(format!("  {} Target is within ccprof profiles", ui.icon_ok()));
                } else {
                     ui.println(format!("  {} Target is EXTERNAL (not managed by ccprof?)", ui.icon_warn()));
                }
                true
            }
            SettingsStatus::BrokenSymlink { target } => {
                 ui.println(format!("  {} BROKEN symlink pointing to: {}", ui.icon_err(), target.display()));
                 false
            }
        }
    });

    // 4. Check Profiles
    check_step(ui, "Profiles", || {
        let profiles = match list_profiles(paths) {
            Ok(p) => p,
            Err(e) => {
                ui.println(format!("  {} Failed to list profiles: {}", ui.icon_err(), e));
                return false;
            }
        };

        if profiles.is_empty() {
             ui.println(format!("  {} No profiles found", ui.icon_warn()));
             return true;
        }

        ui.println(format!("  Found {} profiles:", profiles.len()));
        let mut all_valid = true;

        for name in profiles {
            let dir = paths.profile_dir(&name);
            let metadata_res = ProfileMetadata::read(&dir);

            match metadata_res {
                Ok(metadata) => {
                    // Check managed components
                    let mut missing_components = Vec::new();
                    for component in &metadata.managed_components {
                        let path = component.profile_path(paths, &name);
                        if !path.exists() {
                            missing_components.push(component.display_name());
                        }
                    }

                    if missing_components.is_empty() {
                         ui.println(format!("    {} {}", ui.icon_ok(), name));
                    } else {
                         ui.println(format!("    {} {} (missing components: {})", ui.icon_warn(), name, missing_components.join(", ")));
                         // Not strictly fatal, but warning
                    }
                },
                Err(_) => {
                    // Try to read settings.json directly to see if it's a legacy profile
                    let settings_path = dir.join("settings.json");
                    if settings_path.exists() {
                        match crate::profiles::validate_json_file(&settings_path) {
                            Ok(_) => ui.println(format!("    {} {} (legacy/no metadata)", ui.icon_warn(), name)),
                            Err(e) => {
                                ui.println(format!("    {} {} (invalid settings.json: {})", ui.icon_err(), name, e));
                                all_valid = false;
                            }
                        }
                    } else {
                         ui.println(format!("    {} {} (empty/corrupt)", ui.icon_err(), name));
                         all_valid = false;
                    }
                }
            }
        }
        all_valid
    });

    // 5. Environment
    check_step(ui, "Environment", || {
        match env::var("EDITOR") {
            Ok(e) => ui.println(format!("  {} EDITOR set to: {}", ui.icon_ok(), e)),
            Err(_) => ui.println(format!("  {} EDITOR not set (using system default)", ui.icon_info())),
        }
        true
    });
}

fn check_step<F>(ui: &Ui, name: &str, check_fn: F)
where
    F: FnOnce() -> bool,
{
    ui.println(ui.bold(format!("Checking {}...", name)));
    let success = check_fn();
    if success {
        // ui.println(format!("{} OK", ui.icon_ok()));
    } else {
        ui.println(ui.colored("  Issues detected!", AnsiColor::Red));
    }
    ui.newline();
}