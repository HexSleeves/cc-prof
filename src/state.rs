use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// State stored in ~/.claude-profiles/state.json
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct State {
    /// The currently selected/default profile name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,

    /// When the state was last updated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

impl State {
    /// Read state from file, returning default if file doesn't exist
    pub fn read(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read state file: {:?}", path))?;

        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse state file: {:?}", path))
    }

    /// Write state to file atomically (without locking - use write_locked for concurrent safety)
    ///
    /// Uses atomic write pattern: write to temp file, then rename.
    /// This ensures the state file is never corrupted even if the process crashes.
    pub fn write(&self, path: &Path) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create state directory: {:?}", parent))?;
        }

        let content = serde_json::to_string_pretty(self).context("Failed to serialize state")?;

        // Atomic write: write to temp file, then rename
        let temp_path = path.with_extension("json.tmp");
        std::fs::write(&temp_path, &content)
            .with_context(|| format!("Failed to write temp state file: {:?}", temp_path))?;

        std::fs::rename(&temp_path, path)
            .with_context(|| format!("Failed to rename state file: {:?} -> {:?}", temp_path, path))
    }
}

/// A locked state file handle for safe concurrent access
pub struct LockedState {
    file: File,
    state: State,
    path: std::path::PathBuf,
}

impl LockedState {
    /// Open and lock the state file for exclusive access
    pub fn lock(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create state directory: {:?}", parent))?;
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .with_context(|| format!("Failed to open state file: {:?}", path))?;

        // Acquire exclusive lock (blocks until available)
        file.lock_exclusive()
            .with_context(|| format!("Failed to lock state file: {:?}", path))?;

        // Read current state
        let state = Self::read_from_file(&file, path)?;

        Ok(Self {
            file,
            state,
            path: path.to_path_buf(),
        })
    }

    fn read_from_file(mut file: &File, path: &Path) -> Result<State> {
        let mut content = String::new();
        file.read_to_string(&mut content)
            .with_context(|| format!("Failed to read state file: {:?}", path))?;

        if content.trim().is_empty() {
            return Ok(State::default());
        }

        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse state file: {:?}", path))
    }

    /// Get the current state
    pub fn state(&self) -> &State {
        &self.state
    }

    /// Update and save the state
    pub fn update<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&mut State),
    {
        f(&mut self.state);
        self.state.updated_at = Some(Utc::now());
        self.save()
    }

    fn save(&mut self) -> Result<()> {
        let content =
            serde_json::to_string_pretty(&self.state).context("Failed to serialize state")?;

        // Truncate and write from beginning
        self.file
            .set_len(0)
            .with_context(|| format!("Failed to truncate state file: {:?}", self.path))?;
        self.file
            .seek(SeekFrom::Start(0))
            .with_context(|| format!("Failed to seek state file: {:?}", self.path))?;
        self.file
            .write_all(content.as_bytes())
            .with_context(|| format!("Failed to write state file: {:?}", self.path))?;
        self.file
            .sync_all()
            .with_context(|| format!("Failed to sync state file: {:?}", self.path))?;

        Ok(())
    }
}

impl Drop for LockedState {
    fn drop(&mut self) {
        // Release the lock (ignore errors during drop)
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_state_default() {
        let state = State::default();
        assert!(state.default_profile.is_none());
        assert!(state.updated_at.is_none());
    }

    #[test]
    fn test_state_read_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("nonexistent.json");
        let state = State::read(&path).unwrap();
        assert!(state.default_profile.is_none());
    }

    #[test]
    fn test_state_write_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("state.json");

        let state = State {
            default_profile: Some("work".to_string()),
            updated_at: Some(Utc::now()),
        };
        state.write(&path).unwrap();

        let read_state = State::read(&path).unwrap();
        assert_eq!(read_state.default_profile, Some("work".to_string()));
        assert!(read_state.updated_at.is_some());
    }

    #[test]
    fn test_locked_state() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("state.json");

        {
            let mut locked = LockedState::lock(&path).unwrap();
            locked
                .update(|s| {
                    s.default_profile = Some("personal".to_string());
                })
                .unwrap();
        }

        let state = State::read(&path).unwrap();
        assert_eq!(state.default_profile, Some("personal".to_string()));
    }

    #[test]
    fn test_state_serialization() {
        let state = State {
            default_profile: Some("test".to_string()),
            updated_at: Some(Utc::now()),
        };

        let json = serde_json::to_string(&state).unwrap();
        let parsed: State = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.default_profile, state.default_profile);
    }
}
