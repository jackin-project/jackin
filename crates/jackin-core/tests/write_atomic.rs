// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::ErrorKind;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use jackin_core::{
    DirEntry, FileKind, FileLock, FileMode, FileSystem, FsError, HostPath, LinkProblem,
    RegularFile, RegularFileProblem, RepoPath, StdFileSystem, write_atomic, write_atomic_locked,
};
use tokio::sync::Notify;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Fault {
    None,
    WriteNew,
    Rename,
    SyncDir,
}

#[derive(Debug, Default)]
struct Gate {
    taken: AtomicBool,
    parked: Notify,
    release: Notify,
}

#[derive(Debug)]
struct FaultFs {
    inner: StdFileSystem,
    fault: Fault,
    kind: ErrorKind,
    gate: Option<Gate>,
}

impl FaultFs {
    fn fail(&self, at: Fault, path: &HostPath) -> Result<(), FsError> {
        if self.fault != at {
            return Ok(());
        }
        let (kind, message) = (self.kind, "injected".to_owned());
        Err(FsError::Io {
            path: path.clone(),
            kind,
            message,
        })
    }
}

#[async_trait]
impl FileSystem for FaultFs {
    async fn read(&self, p: &HostPath) -> Result<Option<Vec<u8>>, FsError> {
        self.inner.read(p).await
    }
    async fn stat(&self, p: &HostPath) -> Result<Option<FileKind>, FsError> {
        self.inner.stat(p).await
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
    async fn remove_file(&self, p: &HostPath) -> Result<(), FsError> {
        self.inner.remove_file(p).await
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
        r: &HostPath,
        f: &RepoPath,
        max_bytes: usize,
    ) -> Result<Result<RegularFile, RegularFileProblem>, FsError> {
        self.inner.read_regular(r, f, max_bytes).await
    }
    async fn lock(&self, p: &HostPath) -> Result<FileLock, FsError> {
        self.inner.lock(p).await
    }
    async fn write_new(&self, p: &HostPath, b: &[u8], m: FileMode) -> Result<(), FsError> {
        let half = if self.fault == Fault::WriteNew {
            &b[..b.len() / 2]
        } else {
            b
        };
        self.inner.write_new(p, half, m).await?;
        self.fail(Fault::WriteNew, p)
    }
    async fn rename(&self, f: &HostPath, t: &HostPath) -> Result<(), FsError> {
        self.fail(Fault::Rename, f)?;
        if let Some(gate) = &self.gate
            && !gate.taken.swap(true, Ordering::SeqCst)
        {
            gate.parked.notify_one();
            gate.release.notified().await;
        }
        self.inner.rename(f, t).await
    }
    async fn remove_dir_all(&self, p: &HostPath) -> Result<(), FsError> {
        self.inner.remove_dir_all(p).await
    }
    async fn mode(&self, p: &HostPath) -> Result<Option<FileMode>, FsError> {
        self.inner.mode(p).await
    }
    async fn sync_dir(&self, d: &HostPath) -> Result<(), FsError> {
        self.fail(Fault::SyncDir, d)?;
        self.inner.sync_dir(d).await
    }
}

fn faulty(fault: Fault) -> FaultFs {
    FaultFs {
        inner: StdFileSystem::default(),
        fault,
        kind: ErrorKind::StorageFull,
        gate: None,
    }
}

fn gated() -> Arc<FaultFs> {
    Arc::new(FaultFs {
        gate: Some(Gate::default()),
        ..faulty(Fault::None)
    })
}

fn names(entries: Vec<DirEntry>) -> Vec<String> {
    entries.into_iter().map(|e| e.name).collect()
}

#[tokio::test]
async fn write_atomic_replaces_the_target_and_leaves_no_temp_file() -> TestResult {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir()?;
    let root = HostPath::parse(dir.path())?;
    let target = HostPath::parse(&dir.path().join("config.toml"))?;
    let fs = faulty(Fault::None);

    std::fs::write(target.as_path(), b"old")?;
    std::fs::set_permissions(target.as_path(), std::fs::Permissions::from_mode(0o644))?;
    write_atomic(&fs, &target, b"first", FileMode(0o600)).await?;
    assert_eq!(fs.read(&target).await?, Some(b"first".to_vec()));
    let mode = std::fs::metadata(target.as_path())?.permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    assert_eq!(names(fs.list(&root).await?), ["config.toml"]);

    write_atomic(&fs, &target, b"second", FileMode(0o600)).await?;
    assert_eq!(fs.read(&target).await?, Some(b"second".to_vec()));
    assert_eq!(names(fs.list(&root).await?), ["config.toml"]);

    let fresh = HostPath::parse(&dir.path().join("fresh.toml"))?;
    write_atomic(&fs, &fresh, b"created", FileMode(0o600)).await?;
    assert_eq!(fs.read(&fresh).await?, Some(b"created".to_vec()));
    assert_eq!(names(fs.list(&root).await?), ["config.toml", "fresh.toml"]);
    Ok(())
}

#[tokio::test]
async fn write_atomic_failure_keeps_the_old_file_and_leaves_no_temp_file() -> TestResult {
    for fault in [Fault::WriteNew, Fault::Rename, Fault::SyncDir] {
        for existing in [true, false] {
            let dir = tempfile::tempdir()?;
            let root = HostPath::parse(dir.path())?;
            let target = HostPath::parse(&dir.path().join("config.toml"))?;
            if existing {
                std::fs::write(target.as_path(), b"old bytes")?;
            }
            let fs = faulty(fault);
            let outcome = write_atomic(&fs, &target, b"new bytes", FileMode(0o600)).await;
            match outcome {
                Err(FsError::Io { kind, .. }) => assert_eq!(kind, ErrorKind::StorageFull),
                other => panic!("{fault:?}: {other:?}"),
            }
            let after = fs.read(&target).await?;
            let listed = names(fs.list(&root).await?);
            if fault == Fault::SyncDir {
                assert_eq!(after, Some(b"new bytes".to_vec()), "{fault:?} {existing}");
                assert_eq!(listed, ["config.toml"]);
            } else if existing {
                assert_eq!(after, Some(b"old bytes".to_vec()), "{fault:?}");
                assert_eq!(listed, ["config.toml"]);
            } else {
                assert_eq!(after, None, "{fault:?}");
                assert!(listed.is_empty(), "{listed:?}");
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn write_atomic_refuses_a_symlink_target_and_replaces_a_regular_file() -> TestResult {
    let dir = tempfile::tempdir()?;
    let root = dir.path();
    let fs = StdFileSystem::default();
    let at = |name: &str| HostPath::parse(&root.join(name));

    std::fs::write(root.join("real.txt"), b"keep")?;
    std::os::unix::fs::symlink(root.join("real.txt"), root.join("link-file"))?;
    let outcome = write_atomic(&fs, &at("link-file")?, b"new", FileMode(0o600)).await;
    assert!(
        matches!(outcome, Err(FsError::SymlinkTarget(_))),
        "{outcome:?}"
    );
    assert_eq!(std::fs::read(root.join("real.txt"))?, b"keep");
    assert!(std::fs::symlink_metadata(root.join("link-file"))?.is_symlink());
    assert!(!root.join(".link-file.tmp").exists());

    std::os::unix::fs::symlink(root.join("missing.txt"), root.join("dangling"))?;
    let outcome = write_atomic(&fs, &at("dangling")?, b"new", FileMode(0o600)).await;
    assert!(
        matches!(outcome, Err(FsError::SymlinkTarget(_))),
        "{outcome:?}"
    );
    assert!(!root.join("missing.txt").exists());
    assert!(!root.join(".dangling.tmp").exists());

    std::fs::create_dir(root.join("a-dir"))?;
    std::os::unix::fs::symlink(root.join("a-dir"), root.join("link-dir"))?;
    let outcome = write_atomic(&fs, &at("link-dir")?, b"new", FileMode(0o600)).await;
    assert!(
        matches!(outcome, Err(FsError::SymlinkTarget(_))),
        "{outcome:?}"
    );
    assert!(std::fs::symlink_metadata(root.join("link-dir"))?.is_symlink());

    std::fs::write(root.join("regular.txt"), b"old")?;
    write_atomic(&fs, &at("regular.txt")?, b"new", FileMode(0o600)).await?;
    assert_eq!(std::fs::read(root.join("regular.txt"))?, b"new");

    std::fs::create_dir(root.join("real_dir"))?;
    std::os::unix::fs::symlink(root.join("real_dir"), root.join("link_dir"))?;
    write_atomic(&fs, &at("link_dir/config.toml")?, b"in", FileMode(0o600)).await?;
    assert_eq!(std::fs::read(root.join("real_dir/config.toml"))?, b"in");
    Ok(())
}

#[tokio::test]
async fn two_writers_of_one_target_both_succeed_and_leave_one_whole_file() -> TestResult {
    let dir = tempfile::tempdir()?;
    let root = HostPath::parse(dir.path())?;
    let target = HostPath::parse(&dir.path().join("usage.json"))?;
    let fs = gated();
    let (a_bytes, b_bytes) = (vec![b'a'; 4096], vec![b'b'; 8192]);

    let mut first = tokio::spawn({
        let (fs, target, bytes) = (Arc::clone(&fs), target.clone(), a_bytes.clone());
        async move { write_atomic(&*fs, &target, &bytes, FileMode(0o600)).await }
    });
    let gate = fs.gate.as_ref().ok_or("no gate")?;
    tokio::select! {
        () = gate.parked.notified() => {}
        done = &mut first => return Err(format!("the first writer ended before its rename: {done:?}").into()),
    }
    let second = write_atomic(&*fs, &target, &b_bytes, FileMode(0o600)).await;
    gate.release.notify_one();
    let first = first.await?;

    assert!(second.is_ok(), "second writer: {second:?}");
    assert!(first.is_ok(), "first writer: {first:?}");
    assert_eq!(names(fs.list(&root).await?), ["usage.json"]);
    let after = fs.read(&target).await?;
    assert!(
        after == Some(a_bytes) || after == Some(b_bytes),
        "the file is neither writer's bytes"
    );

    write_atomic(&*fs, &target, b"third", FileMode(0o600)).await?;
    assert_eq!(fs.read(&target).await?, Some(b"third".to_vec()));
    assert_eq!(names(fs.list(&root).await?), ["usage.json"]);
    Ok(())
}

#[tokio::test]
async fn a_locked_write_refuses_a_lock_that_is_not_the_targets_own() -> TestResult {
    let dir = tempfile::tempdir()?;
    let root = dir.path();
    let fs = StdFileSystem::default();
    let target = HostPath::parse(&root.join("data.json"))?;
    let foreign_path = HostPath::parse(&root.join("other.json.lock"))?;
    let foreign = fs.lock(&foreign_path).await?;

    let outcome = write_atomic_locked(&fs, &target, b"new", FileMode(0o600), &foreign).await;
    assert!(
        matches!(&outcome, Err(FsError::WrongLock { lock, target: refused }) if *lock == foreign_path && *refused == target),
        "{outcome:?}"
    );
    assert!(!root.join("data.json").exists());

    let own = fs
        .lock(&HostPath::parse(&root.join("data.json.lock"))?)
        .await?;
    write_atomic_locked(&fs, &target, b"new", FileMode(0o600), &own).await?;
    assert_eq!(std::fs::read(root.join("data.json"))?, b"new");
    Ok(())
}
