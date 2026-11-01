// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    DirEntry, FileKind, FileMode, FileSystem, FsError, HostPath, PathError, StdFileSystem,
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
