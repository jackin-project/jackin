// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use jackin_core::{
    DirEntry, FileKind, FileLock, FileMode, FileSystem, FsError, HostPath, LinkProblem,
    RegularFile, RegularFileProblem, RepoPath,
};
use std::sync::Arc;

pub(crate) type Touch = Box<dyn Fn(&HostPath) -> Result<(), FsError> + Send + Sync>;
pub(crate) type Held = Box<dyn Fn() + Send + Sync>;

pub(crate) struct HookFs {
    inner: Arc<dyn FileSystem>,
    touch: Touch,
    held: Held,
}

impl std::fmt::Debug for HookFs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HookFs")
    }
}

impl HookFs {
    pub(crate) fn new(inner: Arc<dyn FileSystem>, touch: Touch, held: Held) -> HookFs {
        HookFs { inner, touch, held }
    }
}

#[async_trait]
impl FileSystem for HookFs {
    async fn read(&self, p: &HostPath) -> Result<Option<Vec<u8>>, FsError> {
        let found = self.inner.read(p).await;
        (self.touch)(p)?;
        found
    }
    async fn stat(&self, p: &HostPath) -> Result<Option<FileKind>, FsError> {
        let found = self.inner.stat(p).await;
        (self.touch)(p)?;
        found
    }
    async fn stat_following(&self, p: &HostPath) -> Result<Option<FileKind>, FsError> {
        self.inner.stat_following(p).await
    }
    async fn file_len(&self, p: &HostPath) -> Result<Option<u64>, FsError> {
        self.inner.file_len(p).await
    }
    async fn list(&self, d: &HostPath) -> Result<Vec<DirEntry>, FsError> {
        self.inner.list(d).await
    }
    async fn create_dir_all(&self, d: &HostPath) -> Result<(), FsError> {
        self.inner.create_dir_all(d).await
    }
    async fn create_private_dir_all(&self, d: &HostPath) -> Result<(), FsError> {
        self.inner.create_private_dir_all(d).await
    }
    async fn write_new(&self, p: &HostPath, b: &[u8], m: FileMode) -> Result<(), FsError> {
        self.inner.write_new(p, b, m).await
    }
    async fn rename(&self, f: &HostPath, t: &HostPath) -> Result<(), FsError> {
        self.inner.rename(f, t).await
    }
    async fn remove_file(&self, p: &HostPath) -> Result<(), FsError> {
        self.inner.remove_file(p).await
    }
    async fn remove_dir_all(&self, p: &HostPath) -> Result<(), FsError> {
        self.inner.remove_dir_all(p).await
    }
    async fn mode(&self, p: &HostPath) -> Result<Option<FileMode>, FsError> {
        self.inner.mode(p).await
    }
    async fn sync_dir(&self, d: &HostPath) -> Result<(), FsError> {
        self.inner.sync_dir(d).await
    }
    async fn lock(&self, p: &HostPath) -> Result<FileLock, FsError> {
        let found = self.inner.lock(p).await;
        if matches!(found, Err(FsError::LockHeld(_))) {
            (self.held)();
        }
        found
    }
    async fn read_at(&self, p: &HostPath, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        self.inner.read_at(p, offset, len).await
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
        let found = self.inner.read_regular(root, file, max_bytes).await;
        let leaf =
            HostPath::parse(&root.as_path().join(file.as_str())).map_err(|error| FsError::Io {
                path: root.clone(),
                kind: std::io::ErrorKind::InvalidInput,
                message: error.to_string(),
            })?;
        (self.touch)(&leaf)?;
        found
    }
}
