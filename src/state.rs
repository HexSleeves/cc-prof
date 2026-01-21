//! Persistent state management.
//!
//! This module handles the `state.json` file which tracks the currently active profile
//! and other persistent metadata.
//!
//! It uses file locking (`fs2`) to ensure safe concurrent access, and atomic writes
//! (write to temp + rename) to prevent data corruption.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// State stored in ~/.claude-profiles/state.json
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct State {
    /// Name of the currently active profile
    pub default_profile: Option<String>,
    /// Timestamp of the last profile switch
    pub updated_at: Option<DateTime<Utc>>,
}

impl State {
    /// Read state from file
    pub fn read(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read state file: {}", path.display()))?;

        if content.trim().is_empty() {
            return Ok(Self::default());
        }

        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse state file: {}", path.display()))
    }

    /// Write state to file atomically
    pub fn write(&self, path: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;

        // Write to temp file first
        let temp_path = path.with_extension("json.tmp");
        let mut file = File::create(&temp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;

        // Atomic rename
        std::fs::rename(&temp_path, path)?;

        Ok(())
    }
}

/// A wrapper for State that holds a file lock
pub struct LockedState {
    file: File,
    _path: std::path::PathBuf,
    pub state: State,
}

impl LockedState {
    /// Acquire an exclusive lock on the state file and read it
    pub fn lock(path: &Path) -> Result<Self> {
        // Ensure parent exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;

        file.lock_exclusive()?;

        // Read current content
        let mut content = String::new();
        file.read_to_string(&mut content)?;

        let state = if content.trim().is_empty() {
            State::default()
        } else {
            serde_json::from_str(&content).unwrap_or_default()
        };

        Ok(Self {
            file,
            _path: path.to_path_buf(),
            state,
        })
    }

    /// Update the state and write it back
    pub fn update<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&mut State),
    {
        f(&mut self.state);

        let content = serde_json::to_string_pretty(&self.state)?;

        // We can't use atomic rename here because we hold the lock on the file!
        // So we truncate and write.
        self.file.seek(SeekFrom::Start(0))?;
        self.file.set_len(0)?;
        self.file.write_all(content.as_bytes())?;
        self.file.sync_all()?;

        Ok(())
    }
}

impl Drop for LockedState {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_state_read_write() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut state = State::default();
        state.default_profile = Some("test".to_string());
        state.write(path).unwrap();

        let read_state = State::read(path).unwrap();
        assert_eq!(read_state.default_profile, Some("test".to_string()));
    }

    #[test]
    fn test_locked_state_update() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        {
            let mut locked = LockedState::lock(path).unwrap();
            locked
                .update(|s| {
                    s.default_profile = Some("locked".to_string());
                })
                .unwrap();
        }

        let read_state = State::read(path).unwrap();
        assert_eq!(read_state.default_profile, Some("locked".to_string()));
    }
}