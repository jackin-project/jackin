// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::time::Duration;

use jackin_core::{
    DirEntry, EntryProblem, FileKind, FileMode, FileSystem, FsError, HostPath, PathError,
    RegularFileProblem, RepoPath, StdFileSystem,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn at(dir: &tempfile::TempDir, name: &str) -> Result<HostPath, PathError> {
    HostPath::parse(&dir.path().join(name))
}

#[tokio::test]
async fn std_filesystem_reads_writes_lists_and_stats() -> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    let file = at(&dir, "a.txt")?;

    assert_eq!(fs.read(&file).await?, None);
    fs.write_new(&file, b"hello", FileMode(0o600)).await?;
    assert_eq!(fs.read(&file).await?, Some(b"hello".to_vec()));
    assert!(matches!(
        fs.write_new(&file, b"other", FileMode(0o600)).await,
        Err(FsError::AlreadyExists(_))
    ));
    assert_eq!(fs.read(&file).await?, Some(b"hello".to_vec()));
    #[cfg(unix)]
    {
        let mode = std::fs::metadata(file.as_path())?.permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    let sub = at(&dir, "sub/nested")?;
    fs.create_dir_all(&sub).await?;
    fs.create_dir_all(&sub).await?;
    let link = at(&dir, "link")?;
    symlink(sub.as_path(), link.as_path())?;
    assert_eq!(fs.stat(&file).await?, Some(FileKind::File));
    assert_eq!(fs.stat(&sub).await?, Some(FileKind::Dir));
    assert_eq!(fs.stat(&link).await?, Some(FileKind::Symlink));
    assert_eq!(fs.stat(&at(&dir, "missing")?).await?, None);

    let root = HostPath::parse(dir.path())?;
    let mut entries = fs.list(&root).await?;
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let entry = |name: &str, kind| DirEntry {
        name: name.to_owned(),
        kind,
    };
    assert_eq!(
        entries,
        [
            entry("a.txt", FileKind::File),
            entry("link", FileKind::Symlink),
            entry("sub", FileKind::Dir),
        ]
    );

    let other = at(&dir, "b.txt")?;
    fs.write_new(&other, b"new", FileMode(0o644)).await?;
    fs.rename(&other, &file).await?;
    assert_eq!(fs.read(&file).await?, Some(b"new".to_vec()));
    assert_eq!(fs.read(&other).await?, None);
    Ok(())
}

#[tokio::test]
async fn file_lock_is_exclusive_until_dropped() -> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    let path = at(&dir, "config.toml.lock")?;

    let first = fs.lock(&path).await?;
    match fs.lock(&path).await {
        Err(FsError::LockHeld(held)) => assert_eq!(held, path),
        other => panic!("{other:?}"),
    }
    drop(first);
    fs.lock(&path).await?;
    Ok(())
}

#[tokio::test]
async fn std_filesystem_removes_trees_and_reports_modes() -> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    let outside = at(&dir, "outside")?;
    fs.create_dir_all(&outside).await?;
    let survivor = at(&dir, "outside/keep.txt")?;
    fs.write_new(&survivor, b"keep", FileMode(0o644)).await?;

    let tree = at(&dir, "tree/a/b")?;
    fs.create_dir_all(&tree).await?;
    fs.write_new(&at(&dir, "tree/a/b/deep.txt")?, b"deep", FileMode(0o644))
        .await?;
    symlink(outside.as_path(), dir.path().join("tree/a/link"))?;
    fs.remove_dir_all(&at(&dir, "tree")?).await?;
    assert_eq!(fs.stat(&at(&dir, "tree")?).await?, None);
    assert_eq!(fs.read(&survivor).await?, Some(b"keep".to_vec()));

    let link = at(&dir, "root-link")?;
    symlink(outside.as_path(), link.as_path())?;
    fs.remove_dir_all(&link).await?;
    assert_eq!(fs.stat(&link).await?, None);
    assert_eq!(fs.read(&survivor).await?, Some(b"keep".to_vec()));
    fs.remove_dir_all(&at(&dir, "never-existed")?).await?;

    for (name, mode) in [("exec", 0o755), ("secret", 0o600), ("setuid", 0o4755)] {
        let file = at(&dir, name)?;
        fs.write_new(&file, b"x", FileMode(0o600)).await?;
        std::fs::set_permissions(file.as_path(), std::fs::Permissions::from_mode(mode))?;
        assert_eq!(fs.mode(&file).await?, Some(FileMode(mode)), "{name}");
    }
    assert_eq!(fs.mode(&at(&dir, "missing")?).await?, None);
    let dangling = at(&dir, "dangling")?;
    symlink(dir.path().join("missing"), dangling.as_path())?;
    assert!(fs.mode(&dangling).await?.is_some());
    Ok(())
}

#[tokio::test]
async fn private_dirs_are_created_with_mode_0700_at_every_new_level() -> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    let leaf = at(&dir, "cache/stage/one")?;
    fs.create_private_dir_all(&leaf).await?;
    fs.create_private_dir_all(&leaf).await?;
    for level in ["cache", "cache/stage", "cache/stage/one"] {
        let mode = std::fs::metadata(dir.path().join(level))?
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700, "{level}");
    }
    let file = at(&dir, "plain")?;
    fs.write_new(&file, b"x", FileMode(0o600)).await?;
    assert!(matches!(
        fs.create_private_dir_all(&at(&dir, "plain/inner")?).await,
        Err(FsError::Io { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn a_fifo_is_reported_as_other_and_never_as_a_file() -> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    let pipe = at(&dir, "pipe")?;
    assert!(
        std::process::Command::new("mkfifo")
            .arg(pipe.as_path())
            .status()?
            .success()
    );
    let to_pipe = at(&dir, "to-pipe")?;
    symlink(pipe.as_path(), to_pipe.as_path())?;
    let file = at(&dir, "a.txt")?;
    fs.write_new(&file, b"abc", FileMode(0o600)).await?;

    assert_eq!(fs.stat(&pipe).await?, Some(FileKind::Other));
    assert_eq!(fs.stat_following(&pipe).await?, Some(FileKind::Other));
    assert_eq!(fs.stat_following(&to_pipe).await?, Some(FileKind::Other));
    assert_eq!(fs.stat(&file).await?, Some(FileKind::File));
    let mut entries = fs.list(&HostPath::parse(dir.path())?).await?;
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let kinds: Vec<(&str, FileKind)> = entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("a.txt", FileKind::File),
            ("pipe", FileKind::Other),
            ("to-pipe", FileKind::Symlink),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn stat_following_resolves_links_and_reports_sockets() -> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    let socket = at(&dir, "engine.sock")?;
    let _listener = std::os::unix::net::UnixListener::bind(socket.as_path())?;
    let to_socket = at(&dir, "to-socket")?;
    symlink(socket.as_path(), to_socket.as_path())?;
    let file = at(&dir, "a.txt")?;
    fs.write_new(&file, b"abc", FileMode(0o600)).await?;
    let to_file = at(&dir, "to-file")?;
    symlink(file.as_path(), to_file.as_path())?;
    let dangling = at(&dir, "dangling")?;
    symlink(dir.path().join("gone"), dangling.as_path())?;

    assert_eq!(fs.stat_following(&socket).await?, Some(FileKind::Socket));
    assert_eq!(fs.stat_following(&to_socket).await?, Some(FileKind::Socket));
    assert_eq!(fs.stat(&to_socket).await?, Some(FileKind::Symlink));
    assert_eq!(fs.stat_following(&to_file).await?, Some(FileKind::File));
    assert_eq!(fs.stat_following(&root(&dir)?).await?, Some(FileKind::Dir));
    assert_eq!(fs.stat_following(&dangling).await?, None);
    assert_eq!(fs.stat_following(&at(&dir, "missing")?).await?, None);
    Ok(())
}

#[tokio::test]
async fn file_len_reports_the_size_of_a_local_file() -> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    let file = at(&dir, "a.bin")?;
    fs.write_new(&file, b"12345", FileMode(0o600)).await?;
    assert_eq!(fs.file_len(&file).await?, Some(5));
    assert_eq!(fs.file_len(&at(&dir, "missing")?).await?, None);
    Ok(())
}

fn root(dir: &tempfile::TempDir) -> Result<HostPath, PathError> {
    HostPath::parse(dir.path())
}

#[tokio::test]
async fn read_at_returns_the_requested_slice_never_pads_and_refuses_a_missing_file_or_a_directory()
-> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    let file = at(&dir, "range.bin")?;
    fs.write_new(&file, b"0123456789", FileMode(0o600)).await?;
    for (offset, len, expected) in [
        (3, 4, "3456"),
        (0, 100, "0123456789"),
        (10, 5, ""),
        (11, 5, ""),
        (4, 0, ""),
        (9, usize::MAX, "9"),
    ] {
        assert_eq!(
            fs.read_at(&file, offset, len).await?,
            expected.as_bytes(),
            "read_at({offset}, {len})"
        );
    }
    assert!(matches!(
        fs.read_at(&at(&dir, "none.bin")?, 0, 1).await,
        Err(FsError::Io {
            kind: std::io::ErrorKind::NotFound,
            ..
        })
    ));
    assert!(matches!(
        fs.read_at(&HostPath::parse(dir.path())?, 0, 1).await,
        Err(FsError::Io { .. })
    ));
    Ok(())
}

const CAP: usize = 64 * 1024;

#[tokio::test]
async fn read_regular_returns_at_most_the_cap_plus_one_byte_and_keeps_every_refusal() -> TestResult
{
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    let base = HostPath::parse(&std::fs::canonicalize(dir.path())?)?;
    std::fs::File::create(dir.path().join("sparse"))?.set_len(10 * 1024 * 1024)?;
    std::fs::write(dir.path().join("exact"), vec![7_u8; CAP])?;
    std::fs::write(dir.path().join("over"), vec![7_u8; CAP + 1])?;
    std::fs::write(dir.path().join("small"), b"abc")?;
    symlink("small", dir.path().join("link"))?;
    assert!(
        std::process::Command::new("mkfifo")
            .arg(dir.path().join("pipe"))
            .status()?
            .success()
    );

    for (name, len) in [
        ("sparse", CAP + 1),
        ("exact", CAP),
        ("over", CAP + 1),
        ("small", 3),
    ] {
        let read = fs.read_regular(&base, &RepoPath::parse(name)?, CAP).await?;
        assert_eq!(
            read.map_err(|problem| format!("{problem:?}"))?.bytes.len(),
            len,
            "{name}"
        );
    }
    let uncapped = fs
        .read_regular(&base, &RepoPath::parse("over")?, usize::MAX)
        .await?;
    assert_eq!(
        uncapped
            .map_err(|problem| format!("{problem:?}"))?
            .bytes
            .len(),
        CAP + 1
    );

    for (name, problem) in [
        ("link", RegularFileProblem::Symlink),
        ("pipe", RegularFileProblem::NotARegularFile),
        ("absent", RegularFileProblem::Missing),
    ] {
        let read = fs.read_regular(&base, &RepoPath::parse(name)?, CAP).await?;
        assert_eq!(read.err(), Some(problem), "{name}");
    }
    Ok(())
}

#[tokio::test]
async fn read_regular_holds_a_file_in_one_allocation_near_its_own_size() -> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    let base = HostPath::parse(&std::fs::canonicalize(dir.path())?)?;
    for len in [3_usize, 4096, 100_000, 1_000_000] {
        std::fs::write(dir.path().join("file"), vec![7_u8; len])?;
        let read = fs
            .read_regular(&base, &RepoPath::parse("file")?, 1 << 22)
            .await?
            .map_err(|problem| format!("{problem:?}"))?;
        assert_eq!(read.bytes.len(), len);
        assert!(
            read.bytes.capacity() <= len + len / 16 + 4096,
            "{len} bytes held in {} bytes",
            read.bytes.capacity()
        );
    }
    Ok(())
}

#[test]
fn a_holding_read_keeps_its_hold_until_the_blocking_work_ends() -> TestResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(1)
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("file"), b"data")?;
        let base = HostPath::parse(&std::fs::canonicalize(dir.path())?)?;
        let (release, parked) = std::sync::mpsc::channel::<()>();
        let blocker = tokio::task::spawn_blocking(move || parked.recv());
        tokio::time::sleep(Duration::from_millis(100)).await;
        let hold = std::sync::Arc::new(());
        let fs = StdFileSystem::default();
        let name = RepoPath::parse("file")?;
        let held: jackin_core::Hold = std::sync::Arc::<()>::clone(&hold);
        let waited = tokio::time::timeout(
            Duration::from_millis(100),
            fs.read_regular_holding(&base, &name, 16, held),
        )
        .await;
        assert!(waited.is_err(), "the read finished while the pool was busy");
        assert_eq!(std::sync::Arc::strong_count(&hold), 2);
        release.send(())?;
        blocker.await??;
        for _ in 0..500 {
            if std::sync::Arc::strong_count(&hold) == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(std::sync::Arc::strong_count(&hold), 1);
        Ok(())
    })
}

fn lock_within_ten_seconds(
    path: &HostPath,
) -> Result<Result<(), FsError>, std::sync::mpsc::RecvTimeoutError> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let owned = path.clone();
    std::thread::spawn(move || {
        let outcome = tokio::runtime::Builder::new_current_thread()
            .build()
            .map(|runtime| {
                runtime
                    .block_on(StdFileSystem::default().lock(&owned))
                    .map(drop)
            });
        if let Ok(outcome) = outcome {
            drop(sender.send(outcome));
        }
    });
    let received = receiver.recv_timeout(Duration::from_secs(10));
    if received.is_err() {
        release_pipe_writer(path);
    }
    received
}

fn release_pipe_writer(path: &HostPath) {
    let flags = rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK;
    drop(rustix::fs::open(
        path.as_path(),
        flags,
        rustix::fs::Mode::empty(),
    ));
}

#[tokio::test]
async fn a_lock_never_follows_a_symlink_and_never_creates_through_one() -> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    let victim = at(&dir, "victim")?;
    fs.write_new(&victim, b"keep", FileMode(0o640)).await?;
    let link = at(&dir, "victim.lock")?;
    symlink(victim.as_path(), link.as_path())?;
    assert!(matches!(fs.lock(&link).await, Err(FsError::SymlinkTarget(path)) if path == link));
    assert_eq!(fs.read(&victim).await?, Some(b"keep".to_vec()));
    assert_eq!(
        std::fs::metadata(victim.as_path())?.permissions().mode() & 0o777,
        0o640
    );

    let absent = at(&dir, "absent")?;
    let dangling = at(&dir, "dangling.lock")?;
    symlink(absent.as_path(), dangling.as_path())?;
    assert!(
        matches!(fs.lock(&dangling).await, Err(FsError::SymlinkTarget(path)) if path == dangling)
    );
    assert_eq!(fs.stat(&absent).await?, None);
    Ok(())
}

#[tokio::test]
async fn a_lock_refuses_a_pipe_a_directory_and_a_hard_linked_file_without_blocking() -> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();

    let pipe = at(&dir, "pipe.lock")?;
    assert!(
        std::process::Command::new("mkfifo")
            .arg(pipe.as_path())
            .status()?
            .success()
    );
    let refused =
        lock_within_ten_seconds(&pipe).map_err(|error| format!("lock on a pipe: {error}"))?;
    assert!(matches!(
        refused,
        Err(FsError::UnsafeEntry {
            problem: EntryProblem::NotARegularFile,
            ..
        })
    ));

    let folder = at(&dir, "folder.lock")?;
    fs.create_dir_all(&folder).await?;
    assert!(matches!(
        fs.lock(&folder).await,
        Err(FsError::UnsafeEntry {
            problem: EntryProblem::NotARegularFile,
            ..
        })
    ));

    let original = at(&dir, "original")?;
    fs.write_new(&original, b"x", FileMode(0o600)).await?;
    let twin = at(&dir, "twin.lock")?;
    std::fs::hard_link(original.as_path(), twin.as_path())?;
    assert!(matches!(
        fs.lock(&twin).await,
        Err(FsError::UnsafeEntry {
            problem: EntryProblem::MultipleLinks,
            ..
        })
    ));
    Ok(())
}

#[tokio::test]
async fn a_lock_still_works_through_a_symlinked_ancestor_and_stays_exclusive() -> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    fs.create_dir_all(&at(&dir, "real")?).await?;
    symlink(dir.path().join("real"), dir.path().join("via"))?;
    let through = at(&dir, "via/state.lock")?;
    let held = fs.lock(&through).await?;
    assert!(matches!(
        fs.lock(&at(&dir, "real/state.lock")?).await,
        Err(FsError::LockHeld(_))
    ));
    drop(held);
    fs.lock(&at(&dir, "real/state.lock")?).await?;
    Ok(())
}

#[tokio::test]
async fn an_existing_private_leaf_is_tightened_and_its_parent_is_left_alone() -> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    let parent = dir.path().join("parent");
    std::fs::create_dir(&parent)?;
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755))?;
    let leaf = parent.join("leaf");
    std::fs::create_dir(&leaf)?;
    std::fs::set_permissions(&leaf, std::fs::Permissions::from_mode(0o755))?;

    fs.create_private_dir_all(&at(&dir, "parent/leaf")?).await?;
    assert_eq!(
        std::fs::metadata(&leaf)?.permissions().mode() & 0o7777,
        0o700
    );

    let sealed = parent.join("sealed");
    std::fs::create_dir(&sealed)?;
    std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o555))?;
    fs.create_private_dir_all(&at(&dir, "parent/sealed")?)
        .await?;
    assert_eq!(
        std::fs::metadata(&sealed)?.permissions().mode() & 0o7777,
        0o500
    );
    std::fs::set_permissions(&sealed, std::fs::Permissions::from_mode(0o700))?;
    assert_eq!(
        std::fs::metadata(&parent)?.permissions().mode() & 0o7777,
        0o755
    );
    Ok(())
}

#[tokio::test]
async fn a_private_leaf_that_is_a_symlink_is_refused_and_its_target_is_not_touched() -> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    let target = dir.path().join("target");
    std::fs::create_dir(&target)?;
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))?;
    let link = at(&dir, "leaf")?;
    symlink(&target, link.as_path())?;
    assert!(matches!(
        fs.create_private_dir_all(&link).await,
        Err(FsError::SymlinkTarget(path)) if path == link
    ));
    assert_eq!(
        std::fs::metadata(&target)?.permissions().mode() & 0o7777,
        0o755
    );

    let absent = dir.path().join("absent");
    let dangling = at(&dir, "dangling")?;
    symlink(&absent, dangling.as_path())?;
    assert!(matches!(
        fs.create_private_dir_all(&dangling).await,
        Err(FsError::SymlinkTarget(path)) if path == dangling
    ));
    assert!(!absent.exists());
    Ok(())
}

#[tokio::test]
async fn a_private_leaf_owned_by_another_user_is_refused_and_keeps_its_mode() -> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    let foreign = if rustix::process::geteuid().is_root() {
        let owned = at(&dir, "foreign")?;
        fs.create_dir_all(&owned).await?;
        rustix::fs::chown(
            owned.as_path(),
            Some(rustix::fs::Uid::from_raw(65534)),
            None,
        )?;
        owned
    } else {
        HostPath::parse(std::path::Path::new("/usr"))?
    };
    let before = std::fs::metadata(foreign.as_path())?;
    assert_ne!(before.uid(), rustix::process::geteuid().as_raw());
    assert!(matches!(
        fs.create_private_dir_all(&foreign).await,
        Err(FsError::UnsafeEntry {
            problem: EntryProblem::ForeignOwner,
            ..
        })
    ));
    assert_eq!(std::fs::metadata(foreign.as_path())?.mode(), before.mode());
    Ok(())
}

#[tokio::test]
async fn listing_keeps_unicode_names_exact() -> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    for name in ["日本語.txt", "🦀"] {
        fs.write_new(&at(&dir, name)?, b"x", FileMode(0o600))
            .await?;
    }
    let listed = fs.list(&root(&dir)?).await?;
    let names: Vec<&str> = listed.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, ["日本語.txt", "🦀"]);
    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn listing_refuses_a_name_that_is_not_utf8_instead_of_aliasing_it() -> TestResult {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    for name in [&b"bad\xff1"[..], &b"bad\xff2"[..]] {
        std::fs::write(dir.path().join(OsStr::from_bytes(name)), b"x")?;
    }
    let listed_dir = root(&dir)?;
    assert!(matches!(
        fs.list(&listed_dir).await,
        Err(FsError::Io { kind: std::io::ErrorKind::InvalidData, path, .. }) if path == listed_dir
    ));
    Ok(())
}

#[tokio::test]
async fn a_lock_on_a_file_owned_by_another_user_is_refused_and_leaves_it_alone() -> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    let foreign = if rustix::process::geteuid().is_root() {
        let owned = at(&dir, "foreign.lock")?;
        fs.write_new(&owned, b"", FileMode(0o600)).await?;
        rustix::fs::chown(
            owned.as_path(),
            Some(rustix::fs::Uid::from_raw(65534)),
            None,
        )?;
        owned
    } else {
        HostPath::parse(std::path::Path::new("/etc/hosts"))?
    };
    let before = std::fs::metadata(foreign.as_path())?;
    assert_ne!(before.uid(), rustix::process::geteuid().as_raw());
    let refused = fs.lock(&foreign).await;
    assert!(
        matches!(
            refused,
            Err(FsError::UnsafeEntry {
                problem: EntryProblem::ForeignOwner,
                ..
            } | FsError::Io {
                kind: std::io::ErrorKind::PermissionDenied,
                ..
            })
        ),
        "{refused:?}"
    );
    let after = std::fs::metadata(foreign.as_path())?;
    assert_eq!(
        (after.mode(), after.mtime()),
        (before.mode(), before.mtime())
    );
    Ok(())
}

#[tokio::test]
async fn a_lock_on_a_socket_and_a_private_directory_leaf_that_is_a_file_are_refused_by_kind()
-> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    let socket = at(&dir, "s.lock")?;
    let _listener = std::os::unix::net::UnixListener::bind(socket.as_path())?;
    assert!(matches!(
        fs.lock(&socket).await,
        Err(FsError::UnsafeEntry {
            problem: EntryProblem::NotARegularFile,
            ..
        })
    ));

    let file = at(&dir, "plain")?;
    fs.write_new(&file, b"x", FileMode(0o600)).await?;
    assert!(matches!(
        fs.create_private_dir_all(&file).await,
        Err(FsError::UnsafeEntry {
            problem: EntryProblem::NotADirectory,
            ..
        })
    ));
    Ok(())
}
