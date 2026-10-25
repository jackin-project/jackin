// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::Write,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{MetadataExt, PermissionsExt, symlink},
    },
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::Ordering,
    thread,
};

use tempfile::TempDir;

use super::super::SetupSignals;
use super::super::process::{SetupControls, capture};
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
fn rejects_overflowed_staged_inventory_without_changing_live_state() {
    let fixture = fixture();
    let root = fixture.path();
    let live = LiveRepository::capture_at(root).expect("capture fixture");
    let index_before = fs::read(&live.index).expect("live index bytes");
    let worktree_before = fs::read(root.join("staged.txt")).expect("worktree bytes");
    let snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    let tree = snapshot.capture_staged_tree().expect("capture staged tree");
    snapshot
        .materialize(&tree, &live)
        .expect("materialize staged tree");

    let error = snapshot
        .admit_with_inventory_limit(&tree, 1)
        .expect_err("staged inventory must fail closed when output exceeds its limit");
    assert!(
        format!("{error:#}").contains("stdout output exceeded spool budget 1"),
        "expected bounded-capture overflow, got: {error:#}"
    );
    assert_eq!(
        fs::read(&live.index).expect("live index remains"),
        index_before
    );
    assert_eq!(
        fs::read(root.join("staged.txt")).expect("live worktree remains"),
        worktree_before
    );
    OwnedSnapshot::verify_live_index(&live).expect("live index custody remains valid");
}

#[test]
fn git_path_record_parser_preserves_non_utf8_and_trailing_newline_bytes() {
    assert_eq!(
        super::without_record_terminator(b"/repo/\xff\n\n").expect("one record terminator"),
        b"/repo/\xff\n"
    );
    super::without_record_terminator(b"/repo/\xff")
        .expect_err("missing record terminator must fail");
}

#[test]
fn spool_cleanup_error_reports_the_retained_snapshot_path() {
    let fixture = fixture();
    let live = LiveRepository::capture_at(fixture.path()).expect("capture fixture");
    let mut snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    let retained_path = snapshot.scratch_root.clone();
    let error = snapshot.spool_failure_error(
        "fixture cleanup uncertainty",
        Some(jackin_process::SpoolRecovery::default()),
    );

    assert!(
        error
            .to_string()
            .contains(&retained_path.display().to_string())
    );
    assert!(retained_path.is_dir(), "snapshot custody must be retained");
    drop(snapshot);
    fs::remove_dir_all(retained_path).expect("remove retained test snapshot");
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
    let patch = snapshot
        .collect_fixes(&tree, &snapshot.controls)
        .expect("collect patch");
    assert!(patch.starts_with(b"diff --git "));
    assert!(String::from_utf8_lossy(&patch).contains("created file\\nname"));
    assert!(String::from_utf8_lossy(&patch).contains("fixed"));
    assert_eq!(
        fs::read(fixture.path().join("staged.txt")).expect("live file"),
        b"unstaged\n"
    );
}

#[test]
fn recovery_controls_collect_new_path_fixes_after_setup_cancellation() {
    let fixture = fixture();
    let live = LiveRepository::capture_at(fixture.path()).expect("capture fixture");
    let snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    let tree = snapshot.capture_staged_tree().expect("capture staged tree");
    snapshot
        .materialize(&tree, &live)
        .expect("materialize staged tree");

    let new_path = snapshot.root.join("recovery-created.txt");
    fs::write(&new_path, "recovery patch\n").expect("write recovery-created file");
    snapshot
        .controls
        .cancellation()
        .store(true, Ordering::SeqCst);
    let recovery_controls = SetupControls::for_test(std::time::Duration::from_secs(10));
    assert!(snapshot.controls.cancellation().load(Ordering::SeqCst));
    assert!(!recovery_controls.cancellation().load(Ordering::SeqCst));
    recovery_controls
        .check_start()
        .expect("fresh recovery deadline remains live after setup cancellation");
    let patch = snapshot
        .collect_fixes(&tree, &recovery_controls)
        .expect("fresh recovery controls admit new-path staging and diff capture");
    assert!(
        String::from_utf8_lossy(&patch).contains("recovery patch"),
        "new-file fix remains available after setup cancellation"
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
    let patch = snapshot
        .collect_fixes(&tree, &snapshot.controls)
        .expect("collect patch");
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
    let controls = snapshot.controls.clone();
    let completion = snapshot
        .run_gate(&tools, &[OsString::from("fixture argument")], &controls)
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
    let controls = snapshot.controls.clone();
    let completion = snapshot
        .run_gate(&tools, &[], &controls)
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

#[test]
fn setup_signal_after_snapshot_allocation_drops_private_snapshot() {
    const CHILD_MARKER: &str = "JACKIN_SETUP_SIGNAL_CHILD";
    const FIXTURE_ROOT: &str = "JACKIN_SETUP_SIGNAL_FIXTURE";
    const READY_PATH: &str = "JACKIN_SETUP_SIGNAL_READY";

    if std::env::var_os(CHILD_MARKER).is_some() {
        let signals = SetupSignals::register().expect("register setup signals");
        let root = PathBuf::from(std::env::var_os(FIXTURE_ROOT).expect("fixture root"));
        let live = LiveRepository::capture_at(&root).expect("capture fixture");
        let index_before = fs::read(&live.index).expect("read live index");
        let file_before = fs::read(root.join("staged.txt")).expect("read live worktree");
        let snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
        let scratch_root = snapshot.scratch_root.clone();
        let tree = snapshot.capture_staged_tree().expect("capture staged tree");
        snapshot
            .materialize(&tree, &live)
            .expect("materialize staged tree");
        let ready = PathBuf::from(std::env::var_os(READY_PATH).expect("readiness path"));
        let pending = ready.with_extension("pending");
        fs::write(&pending, scratch_root.as_os_str().as_bytes()).expect("write readiness record");
        fs::rename(pending, ready).expect("publish readiness record");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while (signals.signal_status().is_none()
            || !signals
                .route
                .setup_gate_cancellation
                .load(Ordering::Relaxed))
            && std::time::Instant::now() < deadline
        {
            thread::park_timeout(std::time::Duration::from_millis(10));
        }
        assert_eq!(signals.signal_status(), Some(143));
        assert!(signals.route.term.load(Ordering::Relaxed));
        assert!(
            signals
                .route
                .setup_gate_cancellation
                .load(Ordering::Relaxed)
        );
        drop(snapshot);
        assert_eq!(
            fs::read(&live.index).expect("live index remains"),
            index_before
        );
        assert_eq!(
            fs::read(root.join("staged.txt")).expect("live worktree remains"),
            file_before
        );
        return;
    }

    let fixture = fixture();
    let root = fixture.path();
    let live = LiveRepository::capture_at(root).expect("capture fixture");
    let index_before = fs::read(&live.index).expect("read live index");
    let file_before = fs::read(root.join("staged.txt")).expect("read live worktree");
    let control = tempfile::tempdir().expect("control directory");
    let ready = control.path().join("ready");
    struct ChildCleanup {
        child: std::process::Child,
        complete: bool,
    }
    impl Drop for ChildCleanup {
        fn drop(&mut self) {
            if !self.complete {
                drop(self.child.kill());
                drop(self.child.wait());
            }
        }
    }
    let mut child = ChildCleanup {
        child: Command::new(
            std::env::current_exe().expect("test executable"),
        )
        .args([
            "--exact",
            "precommit_snapshot::git::tests::setup_signal_after_snapshot_allocation_drops_private_snapshot",
            "--nocapture",
        ])
        .env(CHILD_MARKER, "1")
        .env(FIXTURE_ROOT, root)
        .env(READY_PATH, &ready)
        .spawn()
        .expect("spawn isolated setup-signal test process"),
        complete: false,
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !ready.exists() && std::time::Instant::now() < deadline {
        if let Some(status) = child.child.try_wait().expect("check child status") {
            child.complete = true;
            panic!("child exited before setup readiness: {status}");
        }
        thread::park_timeout(std::time::Duration::from_millis(10));
    }
    assert!(
        ready.exists(),
        "child must finish snapshot setup before signal"
    );
    let scratch_root = PathBuf::from(OsString::from_vec(
        fs::read(&ready).expect("read private snapshot path"),
    ));
    let child_pid = child.child.id().to_string();
    let signal = Command::new("/bin/kill")
        .arg("-TERM")
        .arg(child_pid)
        .status()
        .expect("send TERM to child");
    assert!(signal.success(), "TERM delivery must succeed");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.child.try_wait().expect("check child completion") {
            child.complete = true;
            break status;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "child did not complete setup cancellation"
        );
        thread::park_timeout(std::time::Duration::from_millis(10));
    };
    assert!(status.success(), "signal must be handled: {status}");
    assert!(!scratch_root.exists(), "private snapshot must be removed");
    assert_eq!(
        fs::read(&live.index).expect("live index remains"),
        index_before
    );
    assert_eq!(
        fs::read(root.join("staged.txt")).expect("live worktree remains"),
        file_before
    );
}

mod index;
