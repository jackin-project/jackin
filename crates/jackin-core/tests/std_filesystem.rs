// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    DirEntry, FileKind, FileMode, FileSystem, FsError, HostPath, PathError, RegularFileProblem,
    RepoPath, StdFileSystem,
};
use std::io::ErrorKind;

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
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(file.as_path())?.permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    let sub = at(&dir, "sub/nested")?;
    fs.create_dir_all(&sub).await?;
    fs.create_dir_all(&sub).await?;
    let link = at(&dir, "link")?;
    std::os::unix::fs::symlink(sub.as_path(), link.as_path())?;
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
    let replacement = at(&dir, "\u{fffd}")?;
    std::fs::write(replacement.as_path(), b"valid replacement character")?;
    assert!(
        fs.list(&root)
            .await?
            .iter()
            .any(|entry| entry.name == "\u{fffd}")
    );
    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn directory_listing_refuses_a_non_utf8_child_name() -> TestResult {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let dir = tempfile::tempdir()?;
    let root = HostPath::parse(dir.path())?;
    let invalid = dir.path().join(OsString::from_vec(vec![
        b'n', b'o', b'n', b'u', b't', b'f', b'8', 0xff,
    ]));
    std::fs::write(invalid, b"data")?;
    let fs = StdFileSystem::default();
    match fs.list(&root).await {
        Err(FsError::Io { kind, message, .. }) => {
            assert_eq!(kind, ErrorKind::InvalidData);
            assert_eq!(message, "directory entry name is not valid UTF-8");
        }
        result => panic!("expected an invalid-data listing error, got {result:?}"),
    }
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
    use std::os::unix::fs::{PermissionsExt, symlink};

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
    use std::os::unix::fs::PermissionsExt;

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
    std::os::unix::fs::symlink(pipe.as_path(), to_pipe.as_path())?;
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
    use std::os::unix::fs::symlink;

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
            kind: ErrorKind::NotFound,
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
    std::os::unix::fs::symlink("small", dir.path().join("link"))?;
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
async fn a_lock_file_that_is_a_symlink_is_refused_and_creates_no_target() -> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    let lock = at(&dir, "usage.json.lock")?;
    let target = dir.path().join("elsewhere");
    std::os::unix::fs::symlink(&target, lock.as_path())?;
    let refused = fs.lock(&lock).await;
    assert!(matches!(refused, Err(FsError::Io { .. })), "{refused:?}");
    assert!(
        !target.exists(),
        "the lock followed the link and created its target"
    );
    Ok(())
}
