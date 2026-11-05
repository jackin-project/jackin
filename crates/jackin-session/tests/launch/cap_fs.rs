// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use jackin_core::{
    DirEntry, FileKind, FileLock, FileMode, FileSystem, FsError, HostPath, LinkProblem,
    RegularFile, RegularFileProblem, RepoPath,
};

struct Hook {
    file: String,
    skip: usize,
    action: Box<dyn FnOnce() + Send>,
}

impl std::fmt::Debug for Hook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hook")
            .field("file", &self.file)
            .field("skip", &self.skip)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub(crate) struct RecordsCaps {
    inner: Arc<dyn FileSystem>,
    seen: Mutex<Vec<(String, usize)>>,
    after: Mutex<Option<Hook>>,
}

impl RecordsCaps {
    pub(crate) fn new(inner: Arc<dyn FileSystem>) -> RecordsCaps {
        RecordsCaps {
            inner,
            seen: Mutex::new(Vec::new()),
            after: Mutex::new(None),
        }
    }

    pub(crate) fn after_read(
        &self,
        file: &str,
        skip: usize,
        action: impl FnOnce() + Send + 'static,
    ) {
        *self.after.lock().unwrap_or_else(PoisonError::into_inner) = Some(Hook {
            file: file.to_owned(),
            skip,
            action: Box::new(action),
        });
    }

    pub(crate) fn seen(&self) -> Vec<(String, usize)> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait]
impl FileSystem for RecordsCaps {
    async fn read(&self, path: &HostPath) -> Result<Option<Vec<u8>>, FsError> {
        self.inner.read(path).await
    }
    async fn stat(&self, path: &HostPath) -> Result<Option<FileKind>, FsError> {
        self.inner.stat(path).await
    }
    async fn stat_following(&self, path: &HostPath) -> Result<Option<FileKind>, FsError> {
        self.inner.stat_following(path).await
    }
    async fn canonicalize(&self, path: &HostPath) -> Result<Option<HostPath>, FsError> {
        self.inner.canonicalize(path).await
    }
    async fn file_len(&self, path: &HostPath) -> Result<Option<u64>, FsError> {
        self.inner.file_len(path).await
    }
    async fn list(&self, dir: &HostPath) -> Result<Vec<DirEntry>, FsError> {
        self.inner.list(dir).await
    }
    async fn create_dir_all(&self, dir: &HostPath) -> Result<(), FsError> {
        self.inner.create_dir_all(dir).await
    }
    async fn create_private_dir_all(&self, dir: &HostPath) -> Result<(), FsError> {
        self.inner.create_private_dir_all(dir).await
    }
    async fn write_new(
        &self,
        path: &HostPath,
        bytes: &[u8],
        mode: FileMode,
    ) -> Result<(), FsError> {
        self.inner.write_new(path, bytes, mode).await
    }
    async fn rename(&self, from: &HostPath, to: &HostPath) -> Result<(), FsError> {
        self.inner.rename(from, to).await
    }
    async fn remove_file(&self, path: &HostPath) -> Result<(), FsError> {
        self.inner.remove_file(path).await
    }
    async fn remove_dir_all(&self, path: &HostPath) -> Result<(), FsError> {
        self.inner.remove_dir_all(path).await
    }
    async fn mode(&self, path: &HostPath) -> Result<Option<FileMode>, FsError> {
        self.inner.mode(path).await
    }
    async fn sync_dir(&self, dir: &HostPath) -> Result<(), FsError> {
        self.inner.sync_dir(dir).await
    }
    async fn lock(&self, path: &HostPath) -> Result<FileLock, FsError> {
        self.inner.lock(path).await
    }
    async fn read_regular(
        &self,
        root: &HostPath,
        file: &RepoPath,
        max_bytes: usize,
    ) -> Result<Result<RegularFile, RegularFileProblem>, FsError> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((file.as_str().to_owned(), max_bytes));
        let read = self.inner.read_regular(root, file, max_bytes).await;
        let hook = {
            let mut after = self.after.lock().unwrap_or_else(PoisonError::into_inner);
            match after.as_mut() {
                Some(hook) if hook.file == file.as_str() && hook.skip > 0 => {
                    hook.skip -= 1;
                    None
                }
                Some(hook) if hook.file == file.as_str() => after.take(),
                _ => None,
            }
        };
        if let Some(hook) = hook {
            (hook.action)();
        }
        read
    }
    async fn read_link(
        &self,
        root: &HostPath,
        file: &RepoPath,
    ) -> Result<Result<String, LinkProblem>, FsError> {
        self.inner.read_link(root, file).await
    }
    async fn write_symlink(&self, path: &HostPath, target: &str) -> Result<(), FsError> {
        self.inner.write_symlink(path, target).await
    }
    async fn read_at(&self, path: &HostPath, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        self.inner.read_at(path, offset, len).await
    }
}
