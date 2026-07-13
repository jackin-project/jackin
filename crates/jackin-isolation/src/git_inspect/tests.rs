// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{changed_files_sync, head_content_sync, parse_porcelain, working_content_sync};
use std::process::{Command, Stdio};

/// Run a git subcommand in `dir`, asserting success. Used only by the
/// end-to-end fixtures below.
fn git(dir: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .args(["-C", dir.to_str().unwrap()])
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git available");
    assert!(status.success(), "git {args:?} failed");
}

/// Regression: the sync helpers must pipe stdout, else `wait_with_output`
/// returns an empty buffer and every worktree falsely reports zero changes.
#[test]
fn changed_files_sync_reports_real_edits_against_a_repo() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path();
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "t@example.com"]);
    git(dir, &["config", "user.name", "Test"]);
    std::fs::write(dir.join("tracked.txt"), b"original\n").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "init"]);
    // Modify the tracked file and add an untracked one.
    std::fs::write(dir.join("tracked.txt"), b"changed\n").unwrap();
    std::fs::write(dir.join("untracked.txt"), b"new\n").unwrap();

    let changed = changed_files_sync(dir.to_str().unwrap());
    assert!(
        changed
            .iter()
            .any(|f| f.path == "tracked.txt" && f.status == 'M'),
        "modified tracked file must be reported: {changed:?}"
    );
    assert!(
        changed
            .iter()
            .any(|f| f.path == "untracked.txt" && f.status == '?'),
        "untracked file must be reported: {changed:?}"
    );

    // HEAD vs working content must come back non-empty (the piped-stdout fix).
    assert_eq!(
        head_content_sync(dir.to_str().unwrap(), "tracked.txt").as_deref(),
        Some("original\n")
    );
    assert_eq!(
        working_content_sync(dir.to_str().unwrap(), "tracked.txt").as_deref(),
        Some("changed\n")
    );
    // A path absent from HEAD (untracked) has no HEAD content.
    assert_eq!(
        head_content_sync(dir.to_str().unwrap(), "untracked.txt"),
        None
    );
}

#[test]
fn parse_porcelain_basic() {
    let text = " M src/foo.rs\nA  src/bar.rs\n?? scratch.txt\n";
    let files = parse_porcelain(text);
    assert_eq!(files.len(), 3);
    assert_eq!(files[0].status, 'M');
    assert_eq!(files[0].path, "src/foo.rs");
    assert_eq!(files[1].status, 'A');
    assert_eq!(files[1].path, "src/bar.rs");
    assert_eq!(files[2].status, '?');
    assert_eq!(files[2].path, "scratch.txt");
}

#[test]
fn parse_porcelain_empty() {
    assert!(parse_porcelain("").is_empty());
}
