use std::path::Path;
use std::{cmp::Ordering, fs};

use anyhow::{Context, Result};

pub(crate) fn read_dir_sorted(dir: &Path) -> Result<Vec<fs::DirEntry>> {
    read_dir_sorted_with(
        dir,
        |entry| Ok(Some(entry)),
        |left, right| left.file_name().cmp(&right.file_name()),
    )
}

pub(crate) fn read_dir_sorted_with<T>(
    dir: &Path,
    mut admit: impl FnMut(fs::DirEntry) -> Result<Option<T>>,
    mut compare: impl FnMut(&T, &T) -> Ordering,
) -> Result<Vec<T>> {
    let entries = fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    let mut admitted = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("collecting entries under {}", dir.display()))?;
        if let Some(value) = admit(entry)? {
            admitted.push(value);
        }
    }
    admitted.sort_by(|left, right| compare(left, right));
    Ok(admitted)
}
