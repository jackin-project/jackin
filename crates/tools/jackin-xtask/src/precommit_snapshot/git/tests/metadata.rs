// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[test]
fn copies_shallow_and_merge_metadata_as_raw_bytes_and_skips_absent_optional_files() {
    let fixture = fixture();
    let root = fixture.path();
    let head = git_output(root, &["rev-parse", "HEAD"]);
    fs::write(root.join(".git/shallow"), &head).expect("write shallow boundary");
    let live = LiveRepository::capture_at(root).expect("capture shallow fixture");
    fs::write(live.worktree_git_dir.join("MERGE_HEAD"), &head).expect("write merge head");
    let merge_message = b"merge message\r\n\xff";
    fs::write(live.worktree_git_dir.join("MERGE_MSG"), merge_message).expect("write merge message");

    let snapshot = OwnedSnapshot::create(&live).expect("private shallow snapshot");
    let tree = snapshot.capture_staged_tree().expect("capture staged tree");
    snapshot
        .materialize(&tree, &live)
        .expect("materialize merge metadata");

    assert_eq!(
        fs::read(snapshot.git_dir.join("shallow")).expect("copied shallow boundary"),
        head
    );
    assert_eq!(
        fs::read(snapshot.git_dir.join("MERGE_HEAD")).expect("copied merge head"),
        head
    );
    assert_eq!(
        fs::read(snapshot.git_dir.join("MERGE_MSG")).expect("copied merge message"),
        merge_message
    );
    assert!(
        !snapshot.git_dir.join("MERGE_MODE").exists(),
        "absent optional merge metadata must remain absent"
    );
}

#[test]
fn rejects_oversized_shallow_metadata_before_materialization() {
    let fixture = fixture();
    let root = fixture.path();
    let shallow = root.join(".git/shallow");
    let file = super::super::super::process::create_private_new(&shallow)
        .expect("create oversized shallow boundary");
    file.set_len(super::super::super::process::METADATA_OUTPUT_LIMIT as u64 + 1)
        .expect("extend shallow boundary sparsely");
    drop(file);

    let live = LiveRepository::capture_at(root).expect("capture oversized metadata fixture");
    let index_before = fs::read(&live.index).expect("read live index");
    let worktree_before = fs::read(root.join("staged.txt")).expect("read live worktree");
    let result = OwnedSnapshot::create(&live);
    let Err(error) = result else {
        panic!("oversized shallow metadata must fail closed");
    };

    assert!(
        format!("{error:#}").contains("shallow boundary exceeds its byte limit"),
        "unexpected metadata size error: {error:#}"
    );
    assert_eq!(
        fs::read(&live.index).expect("live index remains"),
        index_before
    );
    assert_eq!(
        fs::read(root.join("staged.txt")).expect("live worktree remains"),
        worktree_before
    );
}

#[test]
fn rejects_oversized_merge_metadata_before_checkout() {
    let fixture = fixture();
    let root = fixture.path();
    let live = LiveRepository::capture_at(root).expect("capture fixture");
    let index_before = fs::read(&live.index).expect("read live index");
    let worktree_before = fs::read(root.join("staged.txt")).expect("read live worktree");
    let snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    let scratch_root = snapshot.scratch_root.clone();
    let tree = snapshot.capture_staged_tree().expect("capture staged tree");
    let merge_message = live.worktree_git_dir.join("MERGE_MSG");
    let file = super::super::super::process::create_private_new(&merge_message)
        .expect("create oversized merge message");
    file.set_len(super::super::super::process::METADATA_OUTPUT_LIMIT as u64 + 1)
        .expect("extend merge message sparsely");
    drop(file);

    let error = snapshot
        .materialize(&tree, &live)
        .expect_err("oversized merge metadata must fail closed");
    assert!(
        format!("{error:#}").contains("MERGE_MSG exceeds its byte limit"),
        "unexpected metadata size error: {error:#}"
    );
    assert!(
        !snapshot.root.join("staged.txt").exists(),
        "checkout must not start after metadata admission fails"
    );
    assert!(!snapshot.git_dir.join("MERGE_MSG").exists());
    assert_eq!(
        fs::read(&live.index).expect("live index remains"),
        index_before
    );
    assert_eq!(
        fs::read(root.join("staged.txt")).expect("live worktree remains"),
        worktree_before
    );
    drop(snapshot);
    assert!(
        !scratch_root.exists(),
        "aborted snapshot scratch must be removed"
    );
}

#[test]
fn cancelled_metadata_copy_does_not_create_a_destination() {
    let fixture = tempfile::tempdir().expect("metadata copy fixture");
    let source = fixture.path().join("source");
    let destination = fixture.path().join("destination");
    fs::write(&source, b"raw metadata").expect("write metadata source");
    let cancellation = Arc::new(AtomicBool::new(true));
    let controls = SetupControls::new(cancellation, Duration::from_secs(30));

    let error = super::super::snapshot::copy_git_metadata(
        &source,
        &destination,
        "fixture metadata",
        &controls,
    )
    .expect_err("cancelled copy must stop before writing");
    assert!(format!("{error:#}").contains("cancelled"));
    assert!(!destination.exists());
    assert!(controls.cancellation().load(Ordering::SeqCst));
}

fn git_output(root: &Path, args: &[&str]) -> Vec<u8> {
    let mut command = Command::new("git");
    command.args(args).current_dir(root);
    let output = capture(&mut command).expect("run Git fixture command");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
