use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

pub(crate) fn read_dir_sorted(dir: &Path) -> Result<Vec<fs::DirEntry>> {
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("collecting entries under {}", dir.display()))?;
    entries.sort_by_key(fs::DirEntry::file_name);
    Ok(entries)
}
