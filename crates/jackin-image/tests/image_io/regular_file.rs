// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::guard;
use std::error::Error;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{
    DirEntry, FileIdentity, FileKind, FileLock, FileMode, FileSystem, FsError, HostPath,
    LinkProblem, RegularFile, RegularFileProblem, RepoPath, StdFileSystem,
};
use jackin_image::{ContextProblem, EngineError, read_regular_file};
use tempfile::TempDir;
use tokio::time::timeout;

use crate::support::TestResult;

const SECRET: &[u8] = b"outside secret bytes";

struct Tree {
    _dir: TempDir,
    root: PathBuf,
    outside: PathBuf,
}

fn chmod(path: &Path, mode: u32) -> TestResult {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}

impl Tree {
    fn new() -> Result<Tree, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let base = dir.path().canonicalize()?;
        let root = base.join("role");
        let outside = base.join("outside");
        fs::create_dir_all(root.join("nested/deeper"))?;
        fs::create_dir_all(&outside)?;
        fs::write(outside.join("secret.txt"), SECRET)?;
        fs::write(root.join("nested/deeper/file.txt"), b"nested bytes")?;
        fs::write(root.join("run.sh"), b"#!/bin/sh\n")?;
        chmod(&root.join("run.sh"), 0o755)?;
        fs::write(root.join("setuid.sh"), b"suid")?;
        chmod(&root.join("setuid.sh"), 0o4755)?;
        fs::write(root.join("plain.txt"), b"plain")?;
        chmod(&root.join("plain.txt"), 0o644)?;
        fs::create_dir(root.join("dir"))?;
        symlink("run.sh", root.join("link_inside"))?;
        symlink(outside.join("secret.txt"), root.join("link_outside"))?;
        symlink(base.join("nowhere"), root.join("link_dangling"))?;
        symlink(&outside, root.join("link_dir"))?;
        symlink("run.sh", root.join("Dockerfile"))?;
        Ok(Tree {
            _dir: dir,
            root,
            outside,
        })
    }

    fn host(&self) -> Result<HostPath, Box<dyn Error>> {
        Ok(HostPath::parse(&self.root)?)
    }
}

async fn read(root: &HostPath, name: &str) -> Result<RegularFile, EngineError> {
    let file = RepoPath::parse(name).map_err(|error| {
        EngineError::Fs(FsError::Io {
            path: root.clone(),
            kind: std::io::ErrorKind::InvalidInput,
            message: error.to_string(),
        })
    })?;
    read_regular_file(&StdFileSystem::default(), root, &file).await
}

fn refused(result: Result<RegularFile, EngineError>) -> Result<ContextProblem, Box<dyn Error>> {
    match result {
        Err(EngineError::ContextEntry { problem, .. }) => Ok(problem),
        other => Err(format!("expected a ContextEntry refusal, got {other:?}").into()),
    }
}

#[tokio::test]
async fn read_regular_file_returns_bytes_and_mode_and_refuses_every_other_entry() -> TestResult {
    guard(async {
        let tree = Tree::new()?;
        let root = tree.host()?;
        let accepted: [(&str, &[u8], u32); 4] = [
            ("nested/deeper/file.txt", b"nested bytes", 0o644),
            ("run.sh", b"#!/bin/sh\n", 0o755),
            ("setuid.sh", b"suid", 0o755),
            ("plain.txt", b"plain", 0o644),
        ];
        for (name, bytes, mode) in accepted {
            let found = read(&root, name).await?;
            assert_eq!(found.bytes, bytes, "bytes of {name}");
            assert_eq!(found.mode, FileMode(mode), "mode of {name}");
        }
        let rows = [
            ("link_inside", ContextProblem::Symlink),
            ("link_outside", ContextProblem::Symlink),
            ("link_dangling", ContextProblem::Symlink),
            ("link_dir/secret.txt", ContextProblem::Symlink),
            ("Dockerfile", ContextProblem::Symlink),
            ("dir", ContextProblem::NotARegularFile),
            ("nested", ContextProblem::NotARegularFile),
            ("gone.txt", ContextProblem::Missing),
            ("gone/deeper.txt", ContextProblem::Missing),
            ("plain.txt/below", ContextProblem::Missing),
        ];
        for (name, problem) in rows {
            let result = read(&root, name).await;
            if let Ok(found) = &result {
                assert_ne!(found.bytes, SECRET, "the outside bytes arrived for {name}");
            }
            assert_eq!(refused(result)?, problem, "problem for {name}");
        }
        assert_eq!(fs::read(tree.outside.join("secret.txt"))?, SECRET);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn read_regular_file_refuses_a_fifo_without_blocking() -> TestResult {
    let tree = Tree::new()?;
    let fifo = tree.root.join("pipe");
    assert!(Command::new("mkfifo").arg(&fifo).status()?.success());
    let root = tree.host()?;
    let reading = tokio::spawn(async move { read(&root, "pipe").await });
    let finished = timeout(Duration::from_secs(10), reading).await;
    if finished.is_err() {
        tokio::task::spawn_blocking(move || fs::write(fifo, b"")).await??;
        return Err("a FIFO blocked the reader".into());
    }
    assert_eq!(refused(finished??)?, ContextProblem::NotARegularFile);
    Ok(())
}

#[derive(Debug)]
struct SwapFs {
    inner: StdFileSystem,
    leaf: HostPath,
    secret: PathBuf,
    swapped: AtomicBool,
}

impl SwapFs {
    fn swap_after(&self, path: &HostPath) -> Result<(), FsError> {
        if *path == self.leaf && !self.swapped.swap(true, Ordering::SeqCst) {
            let failure = |source: std::io::Error| FsError::Io {
                path: self.leaf.clone(),
                kind: source.kind(),
                message: source.to_string(),
            };
            fs::remove_file(self.leaf.as_path()).map_err(failure)?;
            symlink(&self.secret, self.leaf.as_path()).map_err(failure)?;
        }
        Ok(())
    }
}

#[async_trait]
impl FileSystem for SwapFs {
    async fn read(&self, p: &HostPath) -> Result<Option<Vec<u8>>, FsError> {
        let found = self.inner.read(p).await;
        self.swap_after(p)?;
        found
    }
    async fn stat(&self, p: &HostPath) -> Result<Option<FileKind>, FsError> {
        let found = self.inner.stat(p).await;
        self.swap_after(p)?;
        found
    }
    async fn stat_following(&self, p: &HostPath) -> Result<Option<FileKind>, FsError> {
        let found = self.inner.stat_following(p).await;
        self.swap_after(p)?;
        found
    }
    async fn canonicalize(&self, p: &HostPath) -> Result<Option<HostPath>, FsError> {
        self.inner.canonicalize(p).await
    }
    async fn file_len(&self, p: &HostPath) -> Result<Option<u64>, FsError> {
        self.inner.file_len(p).await
    }
    async fn identity(&self, p: &HostPath) -> Result<Option<FileIdentity>, FsError> {
        self.inner.identity(p).await
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
        let found = self.inner.mode(p).await;
        self.swap_after(p)?;
        found
    }
    async fn sync_dir(&self, d: &HostPath) -> Result<(), FsError> {
        self.inner.sync_dir(d).await
    }
    async fn lock(&self, p: &HostPath) -> Result<FileLock, FsError> {
        self.inner.lock(p).await
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
        self.swap_after(
            &HostPath::parse(&root.as_path().join(file.as_str())).map_err(|error| FsError::Io {
                path: root.clone(),
                kind: std::io::ErrorKind::InvalidInput,
                message: error.to_string(),
            })?,
        )?;
        found
    }
}

#[tokio::test]
async fn a_listed_file_swapped_for_a_symlink_between_calls_never_yields_the_outside_bytes()
-> TestResult {
    guard(async {
        let tree = Tree::new()?;
        let leaf = HostPath::parse(&tree.root.join("plain.txt"))?;
        let swapping = Arc::new(SwapFs {
            inner: StdFileSystem::default(),
            leaf: leaf.clone(),
            secret: tree.outside.join("secret.txt"),
            swapped: AtomicBool::new(false),
        });
        let root = tree.host()?;
        let file = RepoPath::parse("plain.txt")?;
        match read_regular_file(swapping.as_ref(), &root, &file).await {
            Ok(found) => assert_eq!(found.bytes, b"plain"),
            Err(EngineError::ContextEntry { .. }) => {}
            Err(other) => return Err(other.into()),
        }
        assert!(
            swapping.swapped.load(Ordering::SeqCst),
            "the swap never ran, so the test proved nothing"
        );
        assert_eq!(
            fs::read(leaf.as_path())?,
            SECRET,
            "the leaf is now the symlink"
        );
        Ok(())
    })
    .await
}
