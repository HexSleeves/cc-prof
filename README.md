# ccprof (Claude Code Profile Switcher)

`ccprof` is a command-line tool written in Rust to manage multiple user settings profiles for [Claude Code](https://docs.anthropic.com/claude/docs/claude-code). It allows you to easily switch between different configurations (e.g., personal vs. work, different API keys, project-specific settings, or different sets of MCP servers) by managing `~/.claude/settings.json` and other configuration files.

## Features

- **Multiple Profiles**: Create and manage distinct profiles (e.g., `work`, `personal`).
- **Component Management**: Manage not just `settings.json`, but also `agents`, `hooks`, and `commands` directories.
- **Easy Switching**: Switch profiles with a single command (`ccprof use <name>`).
- **Symlink-based**: Uses symlinks to point `~/.claude/settings.json` (and other components) to the active profile.
- **Safety**:
  - Automatic backups before overwriting or linking.
  - Validates JSON settings to prevent broken configurations.
  - Backup management (list, restore, clean).
- **Diagnostics**: Built-in `doctor` command to verify setup and health.
- **Comparison**: Diff profiles to see what changed between environments.
- **Editor Support**: Quickly open profile settings or specific components in your default editor.

## Installation

### Prerequisites

- [Rust and Cargo](https://rustup.rs/) (latest stable version recommended)

### Build from Source

1. Clone the repository:

   ```bash
   git clone https://github.com/HexSleeves/ccprof.git
   cd ccprof
   ```

2. Build and install:

   ```bash
   cargo install --path .
   ```

   This will install the `ccprof` binary to your Cargo bin directory (usually `~/.cargo/bin`), which should be in your `PATH`.

## Usage

### 1. List Profiles

See all available profiles, which one is active, and which components they manage.

```bash
ccprof list
```

### 2. Check Current Status

View detailed information about the currently active profile and the state of your configuration files.

```bash
ccprof current
```

### 3. Add a Profile

Create a new profile. You can interactively select which components to include (settings, agents, hooks, commands).

```bash
# Create a profile named "work" from your current settings (interactive)
ccprof add work --from-current

# Create a profile with specific components
ccprof add work --from-current --components settings,agents
```

### 4. Switch Profiles

Activate a different profile. This updates the symlinks in `~/.claude/` to point to the selected profile's files.

```bash
ccprof use work
```

### 5. Inspect a Profile

View detailed metadata about a profile, including creation date, version, and managed components.

```bash
ccprof inspect work
```

### 6. Edit Profile Settings

Open a profile's configuration in your default editor (`$EDITOR` or system default).

```bash
# Open settings.json
ccprof edit work

# Open a specific component
ccprof edit work --component agents

# Open all managed components
ccprof edit work --all

# Change which components are tracked by this profile
ccprof edit work --track
```

### 7. Manage Backups

View and restore backups created automatically during profile switching.

```bash
# List all backups
ccprof backup list

# Restore a specific backup
ccprof backup restore settings.json.20240115_120000.bak

# Clean old backups (keep last 5 per component)
ccprof backup clean --keep 5
```

### 8. Compare Profiles

See differences between two profiles.

```bash
# Diff settings.json
ccprof diff work personal

# Diff agents directory
ccprof diff work personal --component agents
```

### 9. Manage Profiles

Rename or remove profiles.

```bash
# Rename a profile
ccprof rename work job

# Remove a profile
ccprof remove job
```

### 10. Shell Completions

Generate shell completions for your shell.

```bash
# For Bash
ccprof completions bash > ~/.bash_completion.d/ccprof

# For Zsh
ccprof completions zsh > ~/.zfunc/_ccprof

# For Fish
ccprof completions fish > ~/.config/fish/completions/ccprof.fish
```

### 11. Troubleshooting

Run the diagnostics tool to check for common issues, such as broken symlinks or invalid JSON files.

```bash
ccprof doctor
```

## How It Works

`ccprof` operates by managing the content of `~/.claude/`.

1. **Storage**: Profiles are stored in `~/.claude-profiles/profiles/`. Each profile is a directory containing the files it manages (e.g., `settings.json`, `agents/`, etc.).
2. **State**: It maintains a `state.json` in `~/.claude-profiles/` to track the last selected profile.
3. **Activation**: When you run `ccprof use <name>`, it:
    - Backs up the current files in `~/.claude/` if they are regular files/directories (not symlinks) to `~/.claude-profiles/backups/`.
    - Replaces them with symbolic links pointing to `~/.claude-profiles/profiles/<name>/...`.

## Directory Structure

```text
~/.claude-profiles/
├── backups/           # Backups of original files
├── profiles/          # Profile storage
│   ├── default/       # Example profile
│   │   ├── settings.json
│   │   └── agents/
│   └── work/          # Example profile
│       └── settings.json
└── state.json         # Internal state tracking
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the `Cargo.toml` file for details.
