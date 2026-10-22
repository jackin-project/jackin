use anyhow::{Context, Result, bail, ensure};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

#[cfg(not(unix))]
use std::fs;

#[cfg(unix)]
use std::{fs::File, os::unix::ffi::OsStringExt};

#[cfg(unix)]
use nix::dir::Dir;

use super::{EntryKind, RepositoryDirectory, validate_relative};

impl RepositoryDirectory {
    pub(in crate::consumer_ci_gate) fn list_children(
        &self,
        relative: &Path,
        max_entries: usize,
    ) -> Result<Vec<(OsString, EntryKind)>> {
        validate_relative(relative, "source directory")?;
        #[cfg(unix)]
        {
            let directory = super::open_relative_directory(&self.directory, relative)?;
            list_children_at(&directory, max_entries)
        }
        #[cfg(not(unix))]
        {
            let path = self.root.join(relative);
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(metadata.is_dir() && !metadata.file_type().is_symlink());
            let mut entries = Vec::new();
            for item in crate::fs_util::read_dir_sorted(&path)? {
                let metadata = fs::symlink_metadata(item.path())?;
                let kind = if metadata.file_type().is_symlink() {
                    EntryKind::Symlink
                } else if metadata.is_file() {
                    EntryKind::File
                } else if metadata.is_dir() {
                    EntryKind::Directory
                } else {
                    EntryKind::Other
                };
                entries.push((item.file_name(), kind));
                ensure!(
                    entries.len() <= max_entries,
                    "directory entry bound exceeded"
                );
            }
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(entries)
        }
    }

    pub(in crate::consumer_ci_gate) fn list_files(
        &self,
        relative: &Path,
        extension: &str,
        max_entries: usize,
    ) -> Result<Vec<PathBuf>> {
        validate_relative(relative, "source directory")?;
        let mut found = Vec::new();
        let mut remaining_entries = max_entries;
        self.collect_files(relative, extension, &mut remaining_entries, 0, &mut found)?;
        found.sort();
        Ok(found)
    }

    fn collect_files(
        &self,
        relative: &Path,
        extension: &str,
        remaining_entries: &mut usize,
        depth: usize,
        found: &mut Vec<PathBuf>,
    ) -> Result<()> {
        ensure!(depth < 32, "source tree exceeds depth bound");
        let children = self.list_children(relative, *remaining_entries)?;
        *remaining_entries = (*remaining_entries)
            .checked_sub(children.len())
            .context("source tree entry bound exceeded")?;
        for (name, kind) in children {
            let child = relative.join(&name);
            match kind {
                EntryKind::Directory => {
                    self.collect_files(&child, extension, remaining_entries, depth + 1, found)?;
                }
                EntryKind::File => {
                    if child.extension().is_some_and(|value| value == extension) {
                        found.push(child);
                    }
                }
                EntryKind::Symlink => bail!("source tree contains a symlink: {}", child.display()),
                EntryKind::Other | EntryKind::Missing => {
                    bail!(
                        "source tree contains a nonregular entry: {}",
                        child.display()
                    );
                }
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
pub(super) fn list_children_at(
    directory: &File,
    max_entries: usize,
) -> Result<Vec<(OsString, EntryKind)>> {
    let mut dir =
        Dir::from_fd(directory.try_clone()?.into()).context("opening directory stream")?;
    let mut children = Vec::new();
    for entry in dir.iter() {
        let entry = entry.context("reading directory entry")?;
        let raw = entry.file_name().to_bytes();
        if raw == b"." || raw == b".." {
            continue;
        }
        let name = OsString::from_vec(raw.to_vec());
        let kind = super::entry_kind_at(directory, &name)?;
        children.push((name, kind));
        ensure!(
            children.len() <= max_entries,
            "directory entry bound exceeded"
        );
    }
    children.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(children)
}
