use anyhow::Result;
use clap::{Parser, Subcommand};

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

        /// Components to track (comma-separated: settings,agents,hooks,commands)
        /// Omit value for interactive mode
        #[arg(long, value_delimiter = ',', num_args = 0..)]
        components: Option<Vec<String>>,
    },

    /// Run diagnostics on the ccprof setup
    Doctor,
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
        Commands::Edit { name, components } => {
            if let Some(comps) = components {
                commands::edit_components(&paths, &name, &ui, Some(comps))
            } else {
                commands::edit(&paths, &name, &ui)
            }
        }
        Commands::Doctor => commands::doctor(&paths, &ui),
    }
}
