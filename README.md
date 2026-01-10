# ccprof (Claude Code Profile Switcher)

`ccprof` is a command-line tool written in Rust to manage multiple user settings profiles for [Claude Code](https://docs.anthropic.com/claude/docs/claude-code). It allows you to easily switch between different configurations (e.g., personal vs. work, different API keys, or project-specific settings) by managing the `~/.claude/settings.json` file.

## Features

- **Multiple Profiles**: Create and manage distinct profiles (e.g., `work`, `personal`).
- **Easy Switching**: Switch profiles with a single command (`ccprof use <name>`).
- **Symlink-based**: Uses symlinks to point `~/.claude/settings.json` to the active profile, ensuring Claude Code always sees the correct file.
- **Safety**:
  - Backs up your existing configuration before overwriting or linking.
  - Validates JSON settings to prevent broken configurations.
- **Diagnostics**: Built-in `doctor` command to verify setup and health.
- **Editor Support**: Quickly open profile settings in your default editor.

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

See all available profiles and which one is active.

```bash
ccprof list
```

### 2. Check Current Status

View detailed information about the currently active profile and the state of `~/.claude/settings.json`.

```bash
ccprof current
```

### 3. Add a Profile

Create a new profile. Currently, you can only create a profile by copying your existing `~/.claude/settings.json`.

```bash
# Create a profile named "work" from your current settings
ccprof add work --from-current
```

### 4. Switch Profiles

Activate a different profile. This updates the symlink at `~/.claude/settings.json` to point to the selected profile's settings.

```bash
ccprof use work
```

### 5. Edit Profile Settings

Open a profile's `settings.json` in your default editor (`$EDITOR` or system default).

```bash
ccprof edit work
```

### 6. Troubleshooting

Run the diagnostics tool to check for common issues, such as broken symlinks or invalid JSON files.

```bash
ccprof doctor
```

## How It Works

`ccprof` operates by managing the `~/.claude/settings.json` file.

1. **Storage**: Profiles are stored in `~/.claude-profiles/profiles/`. Each profile is a directory containing a `settings.json` file.
2. **State**: It maintains a `state.json` in `~/.claude-profiles/` to track the last selected profile.
3. **Activation**: When you run `ccprof use <name>`, it:
    - Backs up the current `~/.claude/settings.json` if it's a regular file (not a symlink) to `~/.claude-profiles/backups/`.
    - Replaces `~/.claude/settings.json` with a symbolic link pointing to `~/.claude-profiles/profiles/<name>/settings.json`.

## Directory Structure

```text
~/.claude-profiles/
├── backups/           # Backups of original settings.json files
├── profiles/          # Profile storage
│   ├── default/       # Example profile
│   │   └── settings.json
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
