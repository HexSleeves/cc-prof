use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::generate;
use std::io;

use ccprof::{
    commands,
    paths::Paths,
    ui::{ColorMode, Ui},
};

#[derive(Parser)]
#[command(name = "ccprof")]
#[command(about = "Claude Code Profile Switcher - manage multiple user settings profiles")]
#[command(version)]
struct Cli {
    /// Disable colored output
    #[arg(long, global = true)]
    no_color: bool,

    /// When to use colors: always, auto, never
    #[arg(long, global = true, value_name = "WHEN", default_value = "auto")]
    color: ColorMode,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all available profiles
    List,

    /// Show the current/active profile and settings file status
    Current,

    /// Show detailed information about a profile
    Inspect {
        /// Name of the profile to inspect
        name: String,
    },

    /// Add a new profile
    Add {
        /// Name of the profile to create
        name: String,

        /// Copy settings from current ~/.claude/settings.json
        #[arg(long)]
        from_current: bool,

        /// Components to include (skip interactive selection)
        /// Comma-separated list: settings,agents,hooks,commands
        #[arg(long, value_delimiter = ',')]
        components: Option<Vec<String>>,
    },

    /// Switch to a profile (activate it)
    Use {
        /// Name of the profile to activate
        name: String,
    },

    /// Open a profile's settings.json in your editor
    Edit {
        /// Name of the profile to edit
        name: String,

        /// Modify which components are tracked (comma-separated: settings,agents,hooks,commands)
        /// Omit value for interactive mode
        #[arg(long = "track", value_delimiter = ',', num_args = 0..)]
        track_components: Option<Vec<String>>,

        /// Open a specific component (settings, agents, hooks, commands)
        #[arg(long, short)]
        component: Option<String>,

        /// Open all managed components in editor
        #[arg(long)]
        all: bool,
    },

    /// Run diagnostics on the ccprof setup
    Doctor,

    /// Remove a profile
    Remove {
        /// Name of the profile to remove
        name: String,

        /// Skip confirmation prompt
        #[arg(long, short)]
        force: bool,
    },

    /// Rename a profile
    Rename {
        /// Current name of the profile
        old_name: String,

        /// New name for the profile
        new_name: String,
    },

    /// Compare two profiles
    Diff {
        /// First profile to compare
        profile1: String,

        /// Second profile to compare
        profile2: String,

        /// Component to compare (default: settings)
        #[arg(long, short, default_value = "settings")]
        component: String,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Manage backups
    Backup {
        #[command(subcommand)]
        action: BackupCommands,
    },
}

#[derive(Subcommand)]
enum BackupCommands {
    /// List all backups
    List,

    /// Restore a backup
    Restore {
        /// Backup identifier (use 'ccprof backup list' to see available backups)
        id: String,
    },

    /// Clean old backups
    Clean {
        /// Number of backups to keep per component
        #[arg(long, default_value = "5")]
        keep: usize,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let paths = Paths::new()?;
    let ui = Ui::new(cli.color, cli.no_color);

    match cli.command {
        Commands::List => commands::list(&paths, &ui),
        Commands::Current => commands::current(&paths, &ui),
        Commands::Inspect { name } => commands::inspect(&paths, &name, &ui),
        Commands::Add {
            name,
            from_current,
            components,
        } => {
            if !from_current {
                anyhow::bail!("Currently only --from-current is supported for adding profiles");
            }
            commands::add(&paths, &name, &ui, components)
        }
        Commands::Use { name } => commands::use_profile(&paths, &name, &ui),
        Commands::Edit {
            name,
            track_components,
            component,
            all,
        } => {
            if let Some(comps) = track_components {
                // Modify tracked components
                commands::edit_components(&paths, &name, &ui, Some(comps))
            } else if all {
                // Open all managed components
                commands::edit_all_components(&paths, &name, &ui)
            } else if let Some(comp) = component {
                // Open specific component
                commands::edit_component(&paths, &name, &comp, &ui)
            } else {
                // Default: open settings.json
                commands::edit(&paths, &name, &ui)
            }
        }
        Commands::Doctor => commands::doctor(&paths, &ui),
        Commands::Remove { name, force } => commands::remove(&paths, &name, &ui, force),
        Commands::Rename { old_name, new_name } => {
            commands::rename(&paths, &old_name, &new_name, &ui)
        }
        Commands::Diff {
            profile1,
            profile2,
            component,
        } => commands::diff(&paths, &profile1, &profile2, &component, &ui),
        Commands::Completions { shell } => {
            generate(shell, &mut Cli::command(), "ccprof", &mut io::stdout());
            Ok(())
        }
        Commands::Backup { action } => match action {
            BackupCommands::List => commands::backup_list(&paths, &ui),
            BackupCommands::Restore { id } => commands::backup_restore(&paths, &id, &ui),
            BackupCommands::Clean { keep } => commands::backup_clean(&paths, keep, &ui),
        },
    }
}
