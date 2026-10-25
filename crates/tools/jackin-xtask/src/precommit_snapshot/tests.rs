use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use super::{
    OwnerSignal, SetupSignals, SignalRoute, completion_exit_code, retain_snapshot_failure,
    retain_unsuccessful_completion, write_private_fixes_spool,
};
use crate::precommit_snapshot::{evidence, git, process, toolchain::QualifiedTools};

fn fixture() -> (tempfile::TempDir, git::LiveRepository, git::OwnedSnapshot) {
    let home = fs::canonicalize(std::env::var_os("HOME").expect("HOME is set"))
        .expect("canonicalizing HOME");
    let repository = tempfile::Builder::new()
        .prefix("jackin-f2-retention-")
        .tempdir_in(home)
        .expect("creating fixture repository");
    git_command(repository.path(), &["init", "--quiet"]);
    git_command(
        repository.path(),
        &["config", "user.name", "Retention Test"],
    );
    git_command(
        repository.path(),
        &["config", "user.email", "retention@example.invalid"],
    );
    fs::write(repository.path().join("tracked.txt"), b"fixture\n").expect("writing fixture source");
    git_command(repository.path(), &["add", "tracked.txt"]);
    git_command(repository.path(), &["commit", "--quiet", "-m", "fixture"]);

    let live = git::LiveRepository::capture_at(repository.path()).expect("capturing fixture");
    let snapshot = git::OwnedSnapshot::create(&live).expect("creating private snapshot");
    (repository, live, snapshot)
}

fn git_command(root: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .status()
        .expect("running fixture Git command");
    assert!(status.success(), "git {arguments:?} failed: {status}");
}

fn seed_diagnostics(snapshot: &git::OwnedSnapshot) -> PathBuf {
    fs::write(
        snapshot.scratch_root.join("stdout.log"),
        b"original stdout\n",
    )
    .expect("writing stdout fixture");
    fs::write(
        snapshot.scratch_root.join("stderr.log"),
        b"original stderr\n",
    )
    .expect("writing stderr fixture");
    write_private_fixes_spool(snapshot, b"original fixes\n").expect("spooling fixes")
}

fn assert_retained_diagnostics(root: &Path, fixes: &[u8]) {
    assert_eq!(
        fs::read(root.join("stdout.log")).expect("retained stdout"),
        b"original stdout\n"
    );
    assert_eq!(
        fs::read(root.join("stderr.log")).expect("retained stderr"),
        b"original stderr\n"
    );
    assert_eq!(
        fs::read(root.join("fixes.patch")).expect("retained fixes"),
        fixes
    );
}

fn test_signals() -> SetupSignals {
    SetupSignals {
        route: SignalRoute {
            recovery_armed: Arc::new(AtomicBool::new(false)),
            setup_gate_cancellation: Arc::new(AtomicBool::new(false)),
            recovery_cancellation: Arc::new(AtomicBool::new(false)),
            hup: Arc::new(AtomicBool::new(false)),
            int: Arc::new(AtomicBool::new(false)),
            term: Arc::new(AtomicBool::new(false)),
        },
    }
}

#[test]
fn signal_route_classifies_one_event_before_and_after_recovery_arm() {
    let signals = test_signals();

    signals.route.record(OwnerSignal::Hup);
    assert!(signals.route.setup_gate_cancellation.load(Ordering::SeqCst));
    assert!(!signals.route.recovery_cancellation.load(Ordering::SeqCst));

    let recovery = signals
        .arm_recovery_with_budget(std::time::Duration::from_secs(30))
        .expect("recovery phase arms once");
    assert!(!recovery.cancellation().load(Ordering::SeqCst));
    signals.route.record(OwnerSignal::Term);
    assert!(signals.route.setup_gate_cancellation.load(Ordering::SeqCst));
    assert!(recovery.cancellation().load(Ordering::SeqCst));
    assert_eq!(signals.signal_status(), Some(143));
}

#[test]
fn an_initial_sticky_signal_does_not_mask_recovery_deadline_failure() {
    let signals = test_signals();
    signals.route.record(OwnerSignal::Term);
    let recovery = signals
        .arm_recovery_with_budget(std::time::Duration::ZERO)
        .expect("recovery phase arms");
    let error = recovery
        .check_start()
        .expect_err("expired recovery phase remains an error");
    assert_eq!(
        error
            .downcast_ref::<process::CaptureFailure>()
            .unwrap()
            .kind,
        process::CaptureFailureKind::Deadline
    );
    assert_eq!(signals.signal_status(), Some(143));
    assert!(!process::is_clean_cancellation(&error));
}

#[test]
fn completion_exit_code_preserves_strict_outcome_precedence() {
    assert_eq!(completion_exit_code(false, Some(0), None).unwrap(), 1);
    assert_eq!(completion_exit_code(true, Some(0), None).unwrap(), 0);
    assert_eq!(completion_exit_code(false, Some(23), None).unwrap(), 23);
    assert_eq!(completion_exit_code(false, Some(23), Some(15)).unwrap(), 23);
    assert_eq!(completion_exit_code(false, None, Some(15)).unwrap(), 143);
    assert_eq!(
        completion_exit_code(false, Some(256), Some(15)).unwrap(),
        143
    );
}

#[test]
fn completion_exit_code_errors_without_valid_code_or_signal() {
    assert_eq!(
        completion_exit_code(false, None, None)
            .unwrap_err()
            .to_string(),
        "native pre-commit gate did not complete successfully"
    );
}

#[test]
fn live_index_error_retains_original_diagnostics_after_owner_drop() {
    let (_repository, live, mut snapshot) = fixture();
    let root = snapshot.scratch_root.clone();
    let fixes = b"original fixes\n";
    seed_diagnostics(&snapshot);
    fs::write(&live.index, b"changed index").expect("changing fixture index");

    let error = git::OwnedSnapshot::verify_live_index(&live).unwrap_err();
    let error = retain_snapshot_failure(&mut snapshot, error);
    let detail = format!("{error:#}");
    assert!(detail.contains("live Git index or split-index backing file changed"));
    assert!(detail.contains("private snapshot retained at"));
    drop(snapshot);

    assert_retained_diagnostics(&root, fixes);
    fs::remove_dir_all(root).expect("removing retained index-error snapshot");
}

#[test]
fn fixes_collection_error_retains_new_paths_and_diagnostics_after_owner_drop() {
    let (_repository, _live, mut snapshot) = fixture();
    let root = snapshot.scratch_root.clone();
    let fixes = b"original fixes\n";
    seed_diagnostics(&snapshot);
    let error = snapshot
        .collect_fixes(OsStr::new("not-a-tree-object"), &snapshot.controls)
        .unwrap_err();
    let error = retain_snapshot_failure(&mut snapshot, error);
    assert!(format!("{error:#}").contains("private snapshot retained at"));
    drop(snapshot);

    assert!(root.join("new-paths").is_file());
    assert_retained_diagnostics(&root, fixes);
    fs::remove_dir_all(root).expect("removing retained fixes-error snapshot");
}

#[test]
fn evidence_persist_error_retains_fixes_before_owner_drop() {
    let (_repository, live, mut snapshot) = fixture();
    let root = snapshot.scratch_root.clone();
    let fixes = b"diff --git a/tracked.txt b/tracked.txt\n";
    let spool = write_private_fixes_spool(&snapshot, fixes).expect("spooling collected fixes");
    let collision = live.common_git_dir.join("jackin-hook-evidence");
    fs::write(collision, b"not a directory").expect("creating evidence-path collision");

    let error = evidence::persist(&live.common_git_dir, fixes).unwrap_err();
    let error = retain_snapshot_failure(&mut snapshot, error);
    let detail = format!("{error:#}");
    assert!(detail.contains("hook evidence root is not a physical directory"));
    assert!(detail.contains("private snapshot retained at"));
    drop(snapshot);

    assert_eq!(fs::read(spool).expect("retained fixes spool"), fixes);
    fs::remove_dir_all(root).expect("removing retained evidence-error snapshot");
}

#[test]
fn gate_spawn_error_without_recovery_retains_snapshot_after_owner_drop() {
    let (repository, _live, mut snapshot) = fixture();
    let root = snapshot.scratch_root.clone();
    let fixes = b"original fixes\n";
    let spool = write_private_fixes_spool(&snapshot, fixes).expect("spooling fixes");
    let tools = QualifiedTools {
        gate: repository.path().join("missing-gate"),
        cargo: PathBuf::new(),
        rustc: PathBuf::new(),
        environment: std::collections::BTreeMap::new(),
    };

    let controls = snapshot.controls.clone();
    let Err(error) = snapshot.run_gate(&tools, &[], &controls) else {
        panic!("missing gate unexpectedly completed");
    };
    let original = format!("{error:#}");
    assert!(!original.is_empty());
    let error = retain_snapshot_failure(&mut snapshot, error);
    let detail = format!("{error:#}");
    assert!(detail.contains(&original));
    assert!(detail.contains("private snapshot retained at"));
    drop(snapshot);

    assert!(root.join("stdout.log").is_file());
    assert!(root.join("stderr.log").is_file());
    assert_eq!(fs::read(spool).expect("retained fixes spool"), fixes);
    fs::remove_dir_all(root).expect("removing retained gate-error snapshot");
}

#[test]
fn unsuccessful_completion_retains_spools_after_owner_drop() {
    let (_repository, live, mut snapshot) = fixture();
    let root = snapshot.scratch_root.clone();
    let fixes = b"original fixes\n";
    seed_diagnostics(&snapshot);

    let retained = retain_unsuccessful_completion(&mut snapshot, false)
        .expect("retaining unsuccessful completion");
    assert_eq!(retained, root);
    drop(snapshot);

    assert_retained_diagnostics(&root, fixes);
    fs::remove_dir_all(root).expect("removing unsuccessful-completion snapshot");

    let mut successful_snapshot = git::OwnedSnapshot::create(&live).expect("new snapshot");
    let successful_root = successful_snapshot.scratch_root.clone();
    assert!(retain_unsuccessful_completion(&mut successful_snapshot, true).is_none());
    drop(successful_snapshot);
    assert!(!successful_root.exists());
}
