// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use jackin_core::{
    DirEntry, FileIdentity, FileKind, FileLock, FileMode, FileSystem, FsError, HostPath,
    LinkProblem, RegularFile, RegularFileProblem, RepoPath,
};

use super::FakeClock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsFault {
    WriteNew,
    Rename,
    SyncDir,
}

#[derive(Debug)]
pub struct WorkingFs {
    inner: Arc<dyn FileSystem>,
    clock: Arc<FakeClock>,
    fault: Mutex<Option<FsFault>>,
}

impl WorkingFs {
    pub fn new(inner: Arc<dyn FileSystem>, clock: &Arc<FakeClock>) -> WorkingFs {
        WorkingFs {
            inner,
            clock: Arc::clone(clock),
            fault: Mutex::new(None),
        }
    }

    pub fn fail_next(&self, fault: FsFault) {
        *self.fault.lock().unwrap_or_else(PoisonError::into_inner) = Some(fault);
    }

    fn failed(&self, operation: FsFault, path: &HostPath) -> Option<FsError> {
        let mut armed = self.fault.lock().unwrap_or_else(PoisonError::into_inner);
        if *armed != Some(operation) {
            return None;
        }
        *armed = None;
        Some(FsError::Io {
            path: path.clone(),
            kind: std::io::ErrorKind::Other,
            message: format!("injected {operation:?} failure"),
        })
    }
}

#[async_trait]
impl FileSystem for WorkingFs {
    async fn read(&self, path: &HostPath) -> Result<Option<Vec<u8>>, FsError> {
        let _working = self.clock.working();
        self.inner.read(path).await
    }

    async fn stat(&self, path: &HostPath) -> Result<Option<FileKind>, FsError> {
        let _working = self.clock.working();
        self.inner.stat(path).await
    }

    async fn stat_following(&self, path: &HostPath) -> Result<Option<FileKind>, FsError> {
        let _working = self.clock.working();
        self.inner.stat_following(path).await
    }

    async fn canonicalize(&self, path: &HostPath) -> Result<Option<HostPath>, FsError> {
        let _working = self.clock.working();
        self.inner.canonicalize(path).await
    }

    async fn file_len(&self, path: &HostPath) -> Result<Option<u64>, FsError> {
        let _working = self.clock.working();
        self.inner.file_len(path).await
    }

    async fn identity(&self, path: &HostPath) -> Result<Option<FileIdentity>, FsError> {
        let _working = self.clock.working();
        self.inner.identity(path).await
    }

    async fn list(&self, dir: &HostPath) -> Result<Vec<DirEntry>, FsError> {
        let _working = self.clock.working();
        self.inner.list(dir).await
    }

    async fn create_dir_all(&self, dir: &HostPath) -> Result<(), FsError> {
        let _working = self.clock.working();
        self.inner.create_dir_all(dir).await
    }

    async fn create_private_dir_all(&self, dir: &HostPath) -> Result<(), FsError> {
        let _working = self.clock.working();
        self.inner.create_private_dir_all(dir).await
    }

    async fn write_new(
        &self,
        path: &HostPath,
        bytes: &[u8],
        mode: FileMode,
    ) -> Result<(), FsError> {
        let _working = self.clock.working();
        if let Some(error) = self.failed(FsFault::WriteNew, path) {
            return Err(error);
        }
        self.inner.write_new(path, bytes, mode).await
    }

    async fn rename(&self, from: &HostPath, to: &HostPath) -> Result<(), FsError> {
        let _working = self.clock.working();
        if let Some(error) = self.failed(FsFault::Rename, to) {
            return Err(error);
        }
        self.inner.rename(from, to).await
    }

    async fn remove_file(&self, path: &HostPath) -> Result<(), FsError> {
        let _working = self.clock.working();
        self.inner.remove_file(path).await
    }

    async fn remove_dir_all(&self, path: &HostPath) -> Result<(), FsError> {
        let _working = self.clock.working();
        self.inner.remove_dir_all(path).await
    }

    async fn mode(&self, path: &HostPath) -> Result<Option<FileMode>, FsError> {
        let _working = self.clock.working();
        self.inner.mode(path).await
    }

    async fn sync_dir(&self, dir: &HostPath) -> Result<(), FsError> {
        let _working = self.clock.working();
        if let Some(error) = self.failed(FsFault::SyncDir, dir) {
            return Err(error);
        }
        self.inner.sync_dir(dir).await
    }

    async fn lock(&self, path: &HostPath) -> Result<FileLock, FsError> {
        let _working = self.clock.working();
        self.inner.lock(path).await
    }

    async fn read_regular(
        &self,
        root: &HostPath,
        file: &RepoPath,
        max_bytes: usize,
    ) -> Result<Result<RegularFile, RegularFileProblem>, FsError> {
        let _working = self.clock.working();
        self.inner.read_regular(root, file, max_bytes).await
    }

    async fn read_link(
        &self,
        root: &HostPath,
        file: &RepoPath,
    ) -> Result<Result<String, LinkProblem>, FsError> {
        let _working = self.clock.working();
        self.inner.read_link(root, file).await
    }

    async fn write_symlink(&self, path: &HostPath, target: &str) -> Result<(), FsError> {
        let _working = self.clock.working();
        self.inner.write_symlink(path, target).await
    }

    async fn read_at(&self, path: &HostPath, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        let _working = self.clock.working();
        self.inner.read_at(path, offset, len).await
    }
}
