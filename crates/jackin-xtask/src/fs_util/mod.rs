//! Deterministic filesystem helpers for gate code (plan 027).
//!
//! Gate output must not depend on platform `readdir` order. Prefer
//! [`read_dir_sorted`] over bare `std::fs::read_dir` in jackin-xtask gates.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

/// Read `dir` and return directory entries sorted by file name (Unicode order).
pub(crate) fn read_dir_sorted(dir: &Path) -> Result<Vec<fs::DirEntry>> {
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("collecting entries under {}", dir.display()))?;
    entries.sort_by_key(|e| e.file_name());
    Ok(entries)
}

#[cfg(test)]
mod tests;
