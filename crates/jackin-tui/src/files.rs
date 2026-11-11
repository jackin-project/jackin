// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;

use futures::executor::block_on;
use jackin_core::{FileKind, FileSystem, HostPath};
use jackin_session::error_chain;
use junie_tui::widgets::file_browser::{FileEntry, FileSource};
use tokio::runtime::Handle;

pub(crate) struct HostFiles {
    fs: Arc<dyn FileSystem>,
    handle: Handle,
    home: String,
}

impl std::fmt::Debug for HostFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostFiles").finish_non_exhaustive()
    }
}

impl HostFiles {
    pub(crate) fn new(fs: Arc<dyn FileSystem>, handle: Handle, home: String) -> HostFiles {
        HostFiles { fs, handle, home }
    }

    fn kind(&self, path: &HostPath, listed: FileKind) -> FileKind {
        match listed {
            FileKind::Symlink => block_on(self.fs.stat_following(path))
                .ok()
                .flatten()
                .unwrap_or(FileKind::Symlink),
            other => other,
        }
    }
}

impl FileSource for HostFiles {
    fn list(&self, dir: &str) -> Result<Vec<FileEntry>, String> {
        let _entered = self.handle.enter();
        let root = HostPath::parse(Path::new(dir)).map_err(|error| error_chain(&error))?;
        let listed = block_on(self.fs.list(&root)).map_err(|error| error_chain(&error))?;
        Ok(listed
            .into_iter()
            .map(|entry| {
                let path = Path::new(dir)
                    .join(&entry.name)
                    .to_string_lossy()
                    .into_owned();
                let kind = HostPath::parse(Path::new(&path))
                    .map_or(entry.kind, |full| self.kind(&full, entry.kind));
                match kind {
                    FileKind::Dir => FileEntry::dir(&path),
                    _ => FileEntry::file(&path),
                }
            })
            .collect())
    }

    fn is_dir(&self, path: &str) -> bool {
        let _entered = self.handle.enter();
        HostPath::parse(Path::new(path)).is_ok_and(|path| {
            block_on(self.fs.stat_following(&path))
                .ok()
                .flatten()
                .is_some_and(|kind| kind == FileKind::Dir)
        })
    }

    fn home(&self) -> Option<String> {
        Some(self.home.clone())
    }
}
