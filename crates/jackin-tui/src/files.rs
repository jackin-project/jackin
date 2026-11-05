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
            .filter_map(|entry| {
                let path = Path::new(dir)
                    .join(&entry.name)
                    .to_string_lossy()
                    .into_owned();
                let kind = HostPath::parse(Path::new(&path))
                    .map_or(entry.kind, |full| self.kind(&full, entry.kind));
                match kind {
                    FileKind::Dir => Some(FileEntry::dir(&path)),
                    FileKind::File | FileKind::Symlink => Some(FileEntry::file(&path)),
                    FileKind::Socket | FileKind::Other => None,
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

#[cfg(test)]
mod tests {
    use std::process::Command;

    use jackin_core::StdFileSystem;

    use super::*;

    fn names(source: &HostFiles, dir: &Path) -> Vec<(String, bool)> {
        let mut listed: Vec<(String, bool)> = source
            .list(&dir.to_string_lossy())
            .unwrap_or_default()
            .iter()
            .map(|entry| (entry.name().to_owned(), entry.dir))
            .collect();
        listed.sort();
        listed
    }

    #[test]
    fn a_pipe_or_a_link_to_one_is_not_offered_as_a_file() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime");
        let source = HostFiles::new(
            Arc::new(StdFileSystem::default()),
            runtime.handle().clone(),
            String::new(),
        );
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::write(root.join("login.json"), b"{}").expect("file");
        std::fs::create_dir(root.join("folder")).expect("dir");
        std::os::unix::fs::symlink(root.join("login.json"), root.join("link")).expect("link");
        assert!(
            Command::new("mkfifo")
                .arg(root.join("pipe"))
                .status()
                .expect("mkfifo")
                .success()
        );
        std::os::unix::fs::symlink(root.join("pipe"), root.join("to-pipe")).expect("link");
        assert_eq!(
            names(&source, root),
            [
                ("folder".to_owned(), true),
                ("link".to_owned(), false),
                ("login.json".to_owned(), false),
            ]
        );
    }
}
