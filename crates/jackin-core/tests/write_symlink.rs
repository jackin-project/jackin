// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{FileSystem, FsError, HostPath, StdFileSystem};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn write_symlink_creates_the_exact_link_text_and_refuses_an_existing_path() -> TestResult {
    let dir = tempfile::tempdir()?;
    let root = dir.path().canonicalize()?;
    let fs = StdFileSystem::default();
    let dangling = HostPath::parse(&root.join("dangling"))?;
    fs.write_symlink(&dangling, "nowhere/../x").await?;
    assert_eq!(
        std::fs::read_link(dangling.as_path())?.to_str(),
        Some("nowhere/../x")
    );
    match fs.write_symlink(&dangling, "other").await {
        Err(FsError::AlreadyExists(path)) => assert_eq!(path, dangling),
        other => return Err(format!("expected AlreadyExists, got {other:?}").into()),
    }
    assert_eq!(
        std::fs::read_link(dangling.as_path())?.to_str(),
        Some("nowhere/../x")
    );
    let file = HostPath::parse(&root.join("file"))?;
    std::fs::write(file.as_path(), b"kept")?;
    assert!(matches!(
        fs.write_symlink(&file, "other").await,
        Err(FsError::AlreadyExists(_))
    ));
    assert_eq!(std::fs::read(file.as_path())?, b"kept");
    Ok(())
}
