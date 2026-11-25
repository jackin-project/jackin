// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use jackin_core::{
    DirEntry, FileIdentity, FileKind, FileLock, FileMode, FileSystem, FsError, HostPath,
    LinkProblem, RegularFile, RegularFileProblem, RepoPath,
};

use tokio::sync::watch;

use super::FakeClock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsFault {
    WriteNew,
    Rename,
    SyncDir,
}

#[derive(Debug)]
struct Parked {
    file: String,
    arrived: Arc<AtomicBool>,
    open: watch::Receiver<bool>,
}

#[derive(Debug)]
pub struct ReadHold {
    arrived: Arc<AtomicBool>,
    open: watch::Sender<bool>,
}

impl ReadHold {
    pub fn has_arrived(&self) -> bool {
        self.arrived.load(Ordering::SeqCst)
    }

    pub fn release(&self) {
        self.open.send_replace(true);
    }
}

#[derive(Debug)]
pub struct WorkingFs {
    inner: Arc<dyn FileSystem>,
    clock: Arc<FakeClock>,
    fault: Mutex<Option<FsFault>>,
    cancel: Mutex<Option<(FsFault, CancellationToken)>>,
    parked: Mutex<Option<Parked>>,
    reads: Mutex<std::collections::BTreeMap<String, usize>>,
}

impl WorkingFs {
    pub fn new(inner: Arc<dyn FileSystem>, clock: &Arc<FakeClock>) -> WorkingFs {
        WorkingFs {
            inner,
            clock: Arc::clone(clock),
            fault: Mutex::new(None),
            cancel: Mutex::new(None),
            parked: Mutex::new(None),
            reads: Mutex::new(std::collections::BTreeMap::new()),
        }
    }

    pub fn hold_read(&self, file: &str) -> ReadHold {
        let arrived = Arc::new(AtomicBool::new(false));
        let (open, receiver) = watch::channel(false);
        let mut armed = self.parked.lock().unwrap_or_else(PoisonError::into_inner);
        assert!(armed.is_none(), "a read is already held");
        *armed = Some(Parked {
            file: file.to_owned(),
            arrived: Arc::clone(&arrived),
            open: receiver,
        });
        drop(armed);
        ReadHold { arrived, open }
    }

    pub fn reads_of(&self, root: &HostPath, file: &str) -> usize {
        let reads = self.reads.lock().unwrap_or_else(PoisonError::into_inner);
        reads.get(&Self::key(root, file)).copied().unwrap_or(0)
    }

    fn key(root: &HostPath, file: &str) -> String {
        root.as_path().join(file).display().to_string()
    }

    fn counted(&self, root: &HostPath, file: &RepoPath) {
        let mut reads = self.reads.lock().unwrap_or_else(PoisonError::into_inner);
        *reads.entry(Self::key(root, file.as_str())).or_insert(0) += 1;
    }

    async fn parked_at(&self, file: &RepoPath) {
        let parked = {
            let mut armed = self.parked.lock().unwrap_or_else(PoisonError::into_inner);
            if armed
                .as_ref()
                .is_some_and(|hold| hold.file == file.as_str())
            {
                armed.take()
            } else {
                None
            }
        };
        if let Some(mut hold) = parked {
            hold.arrived.store(true, Ordering::SeqCst);
            drop(hold.open.wait_for(|open| *open).await);
        }
    }

    pub fn cancel_next(&self, fault: FsFault, token: &CancellationToken) {
        *self.cancel.lock().unwrap_or_else(PoisonError::into_inner) = Some((fault, token.clone()));
    }

    async fn cancelling(&self, operation: FsFault) {
        let armed = {
            let mut armed = self.cancel.lock().unwrap_or_else(PoisonError::into_inner);
            match armed.take() {
                Some((fault, token)) if fault == operation => Some(token),
                other => {
                    *armed = other;
                    None
                }
            }
        };
        if let Some(token) = armed {
            token.cancel();
            tokio::task::yield_now().await;
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
        self.cancelling(FsFault::WriteNew).await;
        self.inner.write_new(path, bytes, mode).await
    }

    async fn rename(&self, from: &HostPath, to: &HostPath) -> Result<(), FsError> {
        let _working = self.clock.working();
        if let Some(error) = self.failed(FsFault::Rename, to) {
            return Err(error);
        }
        self.cancelling(FsFault::Rename).await;
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
        self.cancelling(FsFault::SyncDir).await;
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
        self.parked_at(file).await;
        let _working = self.clock.working();
        let read = self.inner.read_regular(root, file, max_bytes).await;
        self.counted(root, file);
        read
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
