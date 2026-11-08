// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_trait::async_trait;
use jackin_core::{
    DirEntry, FileIdentity, FileKind, FileLock, FileMode, FileSystem, FsError, HostPath,
    LinkProblem, RegularFile, RegularFileProblem, RepoPath,
};
use tokio::sync::mpsc;

#[derive(Debug)]
pub struct LockSignalFs {
    inner: Arc<dyn FileSystem>,
    contended: mpsc::UnboundedSender<HostPath>,
}

impl LockSignalFs {
    pub fn new(
        inner: Arc<dyn FileSystem>,
        contended: mpsc::UnboundedSender<HostPath>,
    ) -> LockSignalFs {
        LockSignalFs { inner, contended }
    }
}

#[async_trait]
impl FileSystem for LockSignalFs {
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
    async fn identity(&self, path: &HostPath) -> Result<Option<FileIdentity>, FsError> {
        self.inner.identity(path).await
    }
    async fn list(&self, path: &HostPath) -> Result<Vec<DirEntry>, FsError> {
        self.inner.list(path).await
    }
    async fn create_dir_all(&self, path: &HostPath) -> Result<(), FsError> {
        self.inner.create_dir_all(path).await
    }
    async fn create_private_dir_all(&self, path: &HostPath) -> Result<(), FsError> {
        self.inner.create_private_dir_all(path).await
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
    async fn sync_dir(&self, path: &HostPath) -> Result<(), FsError> {
        self.inner.sync_dir(path).await
    }
    async fn lock(&self, path: &HostPath) -> Result<FileLock, FsError> {
        let result = self.inner.lock(path).await;
        if matches!(result, Err(FsError::LockHeld(_))) {
            match self.contended.send(path.clone()) {
                Ok(()) | Err(_) => {}
            }
        }
        result
    }
    async fn read_at(&self, path: &HostPath, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        self.inner.read_at(path, offset, len).await
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
    async fn read_regular(
        &self,
        root: &HostPath,
        file: &RepoPath,
        max_bytes: usize,
    ) -> Result<Result<RegularFile, RegularFileProblem>, FsError> {
        self.inner.read_regular(root, file, max_bytes).await
    }
}
