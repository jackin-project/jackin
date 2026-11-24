// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;

use futures::executor::block_on;
use jackin_core::{FileKind, FileSystem, HostPath};
use jackin_session::error_chain;
use termrock::widgets::file_browser::{FileEntry, FileSource};
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

    fn offered(&self, path: &str, listed: FileKind) -> Option<FileEntry> {
        let kind = match listed {
            FileKind::Symlink => {
                let full = HostPath::parse(Path::new(path)).ok()?;
                block_on(self.fs.stat_following(&full)).ok().flatten()?
            }
            other => other,
        };
        match kind {
            FileKind::Dir => Some(FileEntry::dir(path)),
            FileKind::File => Some(FileEntry::file(path)),
            FileKind::Symlink | FileKind::Socket | FileKind::Other => None,
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
                self.offered(&path, entry.kind)
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
    use std::os::unix::fs::symlink;
    use std::os::unix::net::UnixListener;
    use std::process::Command;

    use jackin_core::StdFileSystem;

    use super::*;

    #[test]
    fn only_directories_and_files_that_can_be_read_are_offered()
    -> Result<(), Box<dyn std::error::Error>> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .build()?;
        let source = HostFiles::new(
            Arc::new(StdFileSystem::default()),
            runtime.handle().clone(),
            String::new(),
        );
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let scratch = std::env::temp_dir().join(format!("jp-{}-{nanos}", std::process::id()));
        std::fs::create_dir(&scratch)?;
        let root = scratch.as_path();
        std::fs::write(root.join("login.json"), b"{}")?;
        std::fs::create_dir(root.join("folder"))?;
        symlink(root.join("login.json"), root.join("to-file"))?;
        symlink(root.join("folder"), root.join("to-folder"))?;
        assert!(
            Command::new("mkfifo")
                .arg(root.join("pipe"))
                .status()?
                .success()
        );
        symlink(root.join("pipe"), root.join("to-pipe"))?;
        let _socket = UnixListener::bind(root.join("s"))?;
        symlink(root.join("missing"), root.join("dangling"))?;
        symlink(root.join("loop-b"), root.join("loop-a"))?;
        symlink(root.join("loop-a"), root.join("loop-b"))?;

        let mut offered: Vec<(String, bool)> = source
            .list(&root.to_string_lossy())?
            .iter()
            .map(|entry| (entry.name().to_owned(), entry.dir))
            .collect();
        offered.sort();
        assert_eq!(
            offered,
            [
                ("folder".to_owned(), true),
                ("login.json".to_owned(), false),
                ("to-file".to_owned(), false),
                ("to-folder".to_owned(), true),
            ]
        );
        std::fs::remove_dir_all(&scratch)?;
        Ok(())
    }
}
