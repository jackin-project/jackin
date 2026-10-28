use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

pub(super) fn read_dir_sorted_controlled(
    dir: &Path,
    entries_seen: &mut usize,
    entries_limit: usize,
    mut check_start: impl FnMut() -> Result<()>,
) -> Result<Vec<fs::DirEntry>> {
    check_start()?;
    let directory = fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    let mut entries = Vec::new();
    let mut directory = directory;
    loop {
        check_start()?;
        let Some(entry) = directory.next() else {
            break;
        };
        let entry = entry.with_context(|| format!("collecting entries under {}", dir.display()))?;
        let next_count = (*entries_seen)
            .checked_add(1)
            .context("counting reviewed workspace entries")?;
        anyhow::ensure!(
            next_count <= entries_limit,
            "reviewed workspace exceeds its entry limit"
        );
        *entries_seen = next_count;
        entries.push(entry);
    }
    entries.sort_by_key(fs::DirEntry::file_name);
    check_start()?;
    Ok(entries)
}
