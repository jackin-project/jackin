// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_trait::async_trait;
use jackin_core::{
    DirEntry, FileKind, FileLock, FileMode, FileSystem, FsError, HostPath, LinkProblem,
    ProcessRunner, RegularFile, RegularFileProblem, RepoPath,
};
use jackin_session::ExportError;
use jackin_testkit::{TestResult, default_limits, guard};
use tokio_util::sync::CancellationToken;

use super::lab::Lab;
use super::support::{Sees, bundles_left, exports, snapshot, staging_left};

#[derive(Debug)]
struct CancelsOnRead {
    inner: Arc<dyn FileSystem>,
    token: CancellationToken,
}

#[async_trait]
impl FileSystem for CancelsOnRead {
    async fn read(&self, path: &HostPath) -> Result<Option<Vec<u8>>, FsError> {
        self.inner.read(path).await
    }
    async fn stat(&self, path: &HostPath) -> Result<Option<FileKind>, FsError> {
        self.inner.stat(path).await
    }
    async fn stat_following(&self, path: &HostPath) -> Result<Option<FileKind>, FsError> {
        self.inner.stat_following(path).await
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
    ) -> Result<Result<RegularFile, RegularFileProblem>, FsError> {
        self.inner.read_regular(root, file).await
    }
    async fn read_at(&self, path: &HostPath, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        let read = self.inner.read_at(path, offset, len).await;
        if offset > 0 {
            self.token.cancel();
        }
        read
    }
}

#[tokio::test]
async fn a_cancel_during_the_first_read_of_a_long_walk_ends_the_export_with_nothing_left()
-> TestResult {
    guard(async {
        let lab = Lab::armed().await?;
        let noise: Vec<u8> = (0..2 * 1024 * 1024_u64)
            .map(|index| (index.wrapping_mul(2_654_435_761) >> 13) as u8)
            .collect();
        lab.handle
            .commit_in_project("noise.bin", &noise, "noise")
            .await?;
        let before = snapshot(&lab.rig).await?;
        let token = CancellationToken::new();
        let fs = Arc::new(CancelsOnRead {
            inner: Arc::clone(&lab.rig.world.fs),
            token: token.clone(),
        });
        let runner: Arc<dyn ProcessRunner> = Arc::<Sees>::clone(&lab.sees);
        let session = lab
            .tap
            .session_over_fs(&lab.rig, default_limits(), runner, fs);
        let mark = lab.sees.mark();

        let cancelled = session.export(&lab.rig.config, &lab.name, token).await;

        assert!(
            matches!(cancelled, Err(ExportError::Cancelled)),
            "{cancelled:?}"
        );
        let calls = lab.sees.since(mark);
        assert!(
            calls.iter().all(|call| !call
                .args
                .iter()
                .any(|arg| arg == "fetch" || arg == "index-pack")),
            "{calls:?}"
        );
        let after = snapshot(&lab.rig).await?;
        assert!(exports(&after).is_empty());
        assert_eq!(after.rest, before.rest);
        assert_eq!(staging_left(&lab.rig)?, Vec::<String>::new());
        assert_eq!(bundles_left(&lab.handle), Vec::<String>::new());
        let landed = lab.export().await?;
        assert_eq!(landed.branches.len(), 1);
        Ok(())
    })
    .await
}
