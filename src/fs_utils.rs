//! Filesystem utility functions
//!
//! This module provides common filesystem operations used across the codebase.

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;

/// Recursively calculate the total size of a directory in bytes
///
/// This function walks through all files in a directory tree and sums their sizes.
/// Symbolic links are not followed.
///
/// # Arguments
/// * `path` - The directory path to calculate size for
///
/// # Returns
/// Total size in bytes, or an IO error if directory traversal fails
pub fn dir_size(path: &Path) -> std::io::Result<u64> {
    let mut total = 0;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_file() {
            total += metadata.len();
        } else if metadata.is_dir() {
            total += dir_size(&entry.path())?;
        }
    }
    Ok(total)
}

/// Recursively copy a directory and all its contents to a new location
///
/// This function creates the destination directory if it doesn't exist and copies
/// all files and subdirectories from source to destination.
///
/// # Arguments
/// * `src` - Source directory path
/// * `dst` - Destination directory path
///
/// # Errors
/// Returns an error if:
/// - Source doesn't exist or is not a directory
/// - Destination cannot be created
/// - Any file or directory cannot be copied
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    if !src.exists() {
        bail!("Source directory does not exist: {:?}", src);
    }

    if !src.is_dir() {
        bail!("Source is not a directory: {:?}", src);
    }

    fs::create_dir_all(dst)
        .with_context(|| format!("Failed to create destination directory: {:?}", dst))?;

    for entry in
        fs::read_dir(src).with_context(|| format!("Failed to read source directory: {:?}", src))?
    {
        let entry = entry.context("Failed to read directory entry")?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).with_context(|| {
                format!("Failed to copy file: {:?} -> {:?}", src_path, dst_path)
            })?;
        }
    }

    Ok(())
}
