// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;

use jackin_core::{FileSystem, HostPath, LinkProblem, RepoPath, StdFileSystem};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn read_link_reports_a_non_utf8_target_as_a_problem_and_a_utf8_target_as_text() -> TestResult
{
    let dir = tempfile::tempdir()?;
    let root = HostPath::parse(&dir.path().canonicalize()?)?;
    symlink(OsStr::from_bytes(b"b\xFF"), dir.path().join("bad"))?;
    symlink("t.txt", dir.path().join("ok"))?;
    let fs = StdFileSystem::default();
    assert_eq!(
        fs.read_link(&root, &RepoPath::parse("bad")?).await?,
        Err(LinkProblem::NonUtf8)
    );
    assert_eq!(
        fs.read_link(&root, &RepoPath::parse("ok")?).await?,
        Ok("t.txt".to_owned())
    );
    Ok(())
}
