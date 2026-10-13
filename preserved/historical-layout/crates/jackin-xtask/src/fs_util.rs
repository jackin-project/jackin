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
    entries.sort_by_key(fs::DirEntry::file_name);
    Ok(entries)
}

/// Reject bare directory iteration in xtask gate code.
pub(crate) fn enforce_sorted_iteration(root: &Path) -> Result<()> {
    let source_root = root.join("crates/jackin-xtask/src");
    let mut offenders = Vec::new();
    find_unsorted_reads(&source_root, &source_root, &mut offenders)?;
    if offenders.is_empty() {
        return Ok(());
    }
    anyhow::bail!(
        "unsorted directory iteration in gate code:\n  {}\n\nfix: use crate::fs_util::read_dir_sorted\nre-run: cargo xtask lint --strict",
        offenders.join("\n  ")
    )
}

fn find_unsorted_reads(dir: &Path, source_root: &Path, offenders: &mut Vec<String>) -> Result<()> {
    for entry in read_dir_sorted(dir)? {
        let path = entry.path();
        if path.is_dir() {
            find_unsorted_reads(&path, source_root, offenders)?;
            continue;
        }
        if path.extension().is_none_or(|extension| extension != "rs")
            || path.file_name().is_some_and(|name| name == "fs_util.rs")
        {
            continue;
        }
        let source =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        for (index, line) in source.lines().enumerate() {
            if line.contains("fs::read_dir(") || line.contains("std::fs::read_dir(") {
                let relative = path.strip_prefix(source_root).unwrap_or(&path);
                offenders.push(format!("{}:{}", relative.display(), index + 1));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
