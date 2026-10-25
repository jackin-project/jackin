// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::Write,
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, PermissionsExt, symlink},
    },
    path::Path,
    process::Command,
};

use tempfile::TempDir;

use super::super::process::capture;
use super::{LiveRepository, OwnedSnapshot, validate_link};
use crate::precommit_snapshot::toolchain::QualifiedTools;

fn git(root: &Path, args: &[&str]) {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid");
    let output = capture(&mut command).expect("Git starts");
    assert!(output.status.success(), "git {} failed", args.join(" "));
}

fn fixture() -> TempDir {
    let directory = tempfile::tempdir().expect("fixture directory");
    git(directory.path(), &["init", "--quiet"]);
    fs::write(directory.path().join("staged.txt"), "old\n").expect("fixture file");
    git(directory.path(), &["add", "staged.txt"]);
    git(directory.path(), &["commit", "--quiet", "-m", "initial"]);
    fs::write(directory.path().join("staged.txt"), "staged\n").expect("staged content");
    git(directory.path(), &["add", "staged.txt"]);
    fs::write(directory.path().join("staged.txt"), "unstaged\n").expect("unstaged content");
    directory
}

#[test]
fn materializes_the_index_tree_without_changing_the_live_index_or_worktree() {
    let fixture = fixture();
    let root = fixture.path();
    let live = LiveRepository::capture_at(root).expect("capture fixture");
    let index_before = fs::read(&live.index).expect("live index bytes");
    let file_before = fs::read(root.join("staged.txt")).expect("worktree bytes");
    let metadata_before = fs::metadata(root.join("staged.txt")).expect("worktree metadata");
    let snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    let tree = snapshot.capture_staged_tree().expect("capture staged tree");
    snapshot
        .materialize(&tree, &live)
        .expect("materialize tree");
    snapshot.admit(&tree).expect("admit tree");
    assert_eq!(
        fs::read(snapshot.root.join("staged.txt")).expect("snapshot content"),
        b"staged\n"
    );
    assert_eq!(fs::read(&live.index).expect("index remains"), index_before);
    assert_eq!(
        fs::read(root.join("staged.txt")).expect("worktree remains"),
        file_before
    );
    let metadata_after = fs::metadata(root.join("staged.txt")).expect("worktree metadata remains");
    assert_eq!(
        metadata_after.permissions().mode(),
        metadata_before.permissions().mode()
    );
    assert_eq!(metadata_after.ino(), metadata_before.ino());
    assert_eq!(metadata_after.mtime(), metadata_before.mtime());
    assert_eq!(metadata_after.mtime_nsec(), metadata_before.mtime_nsec());
}

#[test]
fn rejects_live_index_changes_during_capture_and_after_gate() {
    let second_fixture = fixture();
    let live = LiveRepository::capture_at(second_fixture.path()).expect("capture fixture");
    let mut changed = fs::read(&live.index).expect("read live index");
    changed.push(0);
    fs::write(&live.index, changed).expect("change live index");
    assert!(OwnedSnapshot::create(&live).is_err());

    let second_fixture = fixture();
    let live = LiveRepository::capture_at(second_fixture.path()).expect("capture fixture");
    let snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    let tree = snapshot.capture_staged_tree().expect("capture tree");
    snapshot
        .materialize(&tree, &live)
        .expect("materialize tree");
    fs::write(&live.index, b"changed after capture").expect("change live index");
    assert!(OwnedSnapshot::verify_live_index(&live).is_err());
}

#[test]
fn admits_internal_symlinks_and_rejects_escaping_chains() {
    let directory = tempfile::tempdir().expect("fixture directory");
    fs::write(directory.path().join("inside"), b"safe").expect("inside file");
    symlink("inside", directory.path().join("safe-link")).expect("safe link");
    validate_link(directory.path(), &directory.path().join("safe-link")).expect("internal link");

    symlink("../outside", directory.path().join("hop")).expect("escaping hop");
    symlink("hop/../inside", directory.path().join("chained")).expect("chained escape");
    assert!(validate_link(directory.path(), &directory.path().join("chained")).is_err());
}

#[test]
fn collects_tracked_and_new_path_fixes_against_captured_tree() {
    let fixture = fixture();
    let live = LiveRepository::capture_at(fixture.path()).expect("capture fixture");
    let snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    let tree = snapshot.capture_staged_tree().expect("capture staged tree");
    snapshot
        .materialize(&tree, &live)
        .expect("materialize tree");
    fs::write(snapshot.root.join("staged.txt"), "fixed\n").expect("tracked fix");
    fs::write(snapshot.root.join("created file\nname"), "new\n").expect("new fix");
    let patch = snapshot.collect_fixes(&tree).expect("collect patch");
    assert!(patch.starts_with(b"diff --git "));
    assert!(String::from_utf8_lossy(&patch).contains("created file\\nname"));
    assert!(String::from_utf8_lossy(&patch).contains("fixed"));
    assert_eq!(
        fs::read(fixture.path().join("staged.txt")).expect("live file"),
        b"unstaged\n"
    );
}

#[test]
fn collects_new_files_whose_names_look_like_git_pathspec_magic() {
    let fixture = fixture();
    let live = LiveRepository::capture_at(fixture.path()).expect("capture fixture");
    let snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    let tree = snapshot.capture_staged_tree().expect("capture staged tree");
    snapshot
        .materialize(&tree, &live)
        .expect("materialize staged tree");
    let filename = ":(exclude)*";
    fs::write(
        snapshot.root.join(filename),
        "must be included in evidence\n",
    )
    .expect("create pathspec-shaped filename");
    let patch = snapshot.collect_fixes(&tree).expect("collect patch");
    let patch = String::from_utf8_lossy(&patch);
    assert!(patch.contains("must be included in evidence"));
    assert!(patch.contains("(exclude)*"));
}

#[test]
fn runs_gate_with_the_process_supervisor_and_original_hook_arguments() {
    let fixture = fixture();
    let live = LiveRepository::capture_at(fixture.path()).expect("capture fixture");
    let mut snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    let tree = snapshot.capture_staged_tree().expect("capture staged tree");
    snapshot
        .materialize(&tree, &live)
        .expect("materialize tree");
    let tools = QualifiedTools {
        gate: Path::new("/usr/bin/true").to_path_buf(),
        cargo: Path::new("/usr/bin/true").to_path_buf(),
        rustc: Path::new("/usr/bin/true").to_path_buf(),
        environment: BTreeMap::<OsString, OsString>::new(),
    };
    let completion = snapshot
        .run_gate(
            &tools,
            &[OsString::from("fixture argument")],
            &std::sync::atomic::AtomicBool::new(false),
        )
        .expect("supervised gate");
    assert!(completion.success);
    assert!(completion.cleanup.group_drained);
    assert!(completion.cleanup.recovery.is_none());
}

#[test]
fn failed_gate_keeps_bounded_diagnostics_in_private_spools() {
    let fixture = fixture();
    let live = LiveRepository::capture_at(fixture.path()).expect("capture fixture");
    let mut snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    let tree = snapshot.capture_staged_tree().expect("capture staged tree");
    snapshot
        .materialize(&tree, &live)
        .expect("materialize staged tree");
    let fake_gate = snapshot.scratch_root.join("fake-hk");
    fs::write(
        &fake_gate,
        b"#!/bin/sh\nprintf 'gate-out\\n'\nprintf 'gate-error\\n' >&2\nexit 23\n",
    )
    .expect("write fake gate");
    fs::set_permissions(&fake_gate, fs::Permissions::from_mode(0o700))
        .expect("make fake gate executable");
    let tools = QualifiedTools {
        gate: fake_gate,
        cargo: Path::new("/usr/bin/true").to_path_buf(),
        rustc: Path::new("/usr/bin/true").to_path_buf(),
        environment: BTreeMap::<OsString, OsString>::new(),
    };
    let completion = snapshot
        .run_gate(&tools, &[], &std::sync::atomic::AtomicBool::new(false))
        .expect("supervised failing gate");
    assert!(!completion.success);
    assert_eq!(completion.code, Some(23));
    assert_eq!(
        fs::read(snapshot.scratch_root.join("stdout.log")).unwrap(),
        b"gate-out\n"
    );
    assert_eq!(
        fs::read(snapshot.scratch_root.join("stderr.log")).unwrap(),
        b"gate-error\n"
    );
    assert!(completion.cleanup.group_drained);
}

mod index;
