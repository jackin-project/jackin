use std::{
    env,
    ffi::OsStr,
    fs, io,
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, PermissionsExt, symlink},
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitCode, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use super::{
    OwnerBudgets, OwnerSignal, SetupSignals, SignalRoute, completion_exit_code,
    retain_snapshot_failure, retain_unsuccessful_completion, run_installed_from_hook_with_budgets,
    write_private_fixes_spool,
};
use crate::precommit_snapshot::{evidence, git, process, toolchain::QualifiedTools};
use sha2::Digest;

const OWNER_DRIVER_CHILD: &str = "JACKIN_F3_OWNER_DRIVER_CHILD";
const OWNER_DRIVER_GENERATION: &str = "JACKIN_F3_OWNER_DRIVER_GENERATION";
const OWNER_DRIVER_EXPECTATION: &str = "JACKIN_F3_OWNER_DRIVER_EXPECTATION";
const OWNER_DRIVER_STATUS: &str = "JACKIN_F3_OWNER_DRIVER_STATUS";
const OWNER_DRIVER_SETUP_MS: &str = "JACKIN_F3_OWNER_DRIVER_SETUP_MS";
const OWNER_DRIVER_GATE_MS: &str = "JACKIN_F3_OWNER_DRIVER_GATE_MS";
const OWNER_DRIVER_RECOVERY_MS: &str = "JACKIN_F3_OWNER_DRIVER_RECOVERY_MS";
const OWNER_DRIVER_GIT_PROGRAM: &str = "JACKIN_F3_OWNER_DRIVER_GIT_PROGRAM";
const OWNER_DRIVER_REPOSITORY: &str = "JACKIN_F3_OWNER_DRIVER_REPOSITORY";
const OWNER_DRIVER_EXPECTED_TREE: &str = "JACKIN_F3_OWNER_DRIVER_EXPECTED_TREE";
const OWNER_DRIVER_PROBE_EXE: &str = "JACKIN_F3_OWNER_DRIVER_PROBE_EXE";
const OWNER_DRIVER_PROBE_OUTPUT: &str = "JACKIN_F3_OWNER_DRIVER_PROBE_OUTPUT";
const OWNER_DRIVER_ARTIFACT_REPORT: &str = "JACKIN_F3_OWNER_DRIVER_ARTIFACT_REPORT";
const OWNER_DRIVER_EXPECTED_RUSTC: &str = "JACKIN_F3_OWNER_DRIVER_EXPECTED_RUSTC";
const OWNER_DRIVER_EXPECTED_RUSTDOC: &str = "JACKIN_F3_OWNER_DRIVER_EXPECTED_RUSTDOC";
const OWNER_DRIVER_EXPECTED_CLIPPY: &str = "JACKIN_F3_OWNER_DRIVER_EXPECTED_CLIPPY";
const OWNER_DRIVER_EXPECTED_RUSTFMT: &str = "JACKIN_F3_OWNER_DRIVER_EXPECTED_RUSTFMT";
const LIVE_CANARY_PATHS: &[&str] = &[
    "tracked.txt",
    "untracked-canary.txt",
    "ignored-canary.txt",
    ".gitignore",
    ".git/config",
    ".git/index",
    ".git/refs/stash",
    ".git/logs/refs/stash",
    ".git/packed-refs",
    "target",
    ".git/f3-test/live-artifacts",
    ".git/f3-test/live-artifacts/existing-canary.bin",
    ".git/f3-test/live-artifacts/f3-owner-artifact",
    ".git/f3-test/live-artifacts/build",
    ".git/f3-test/live-artifacts/build/f3-owner-build-artifact",
];

fn fixture() -> (tempfile::TempDir, git::LiveRepository, git::OwnedSnapshot) {
    let home =
        fs::canonicalize(env::var_os("HOME").expect("HOME is set")).expect("canonicalizing HOME");
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

#[test]
fn installed_owner_driver_child() {
    if env::var_os(OWNER_DRIVER_CHILD).is_none() {
        return;
    }

    let generation =
        PathBuf::from(env::var_os(OWNER_DRIVER_GENERATION).expect("installed generation path"));
    let setup = owner_budget(OWNER_DRIVER_SETUP_MS);
    let gate = owner_budget(OWNER_DRIVER_GATE_MS);
    let recovery = owner_budget(OWNER_DRIVER_RECOVERY_MS);
    let result = run_installed_from_hook_with_budgets(
        &generation,
        &[],
        OwnerBudgets {
            setup,
            gate,
            recovery,
            test_git_program: env::var_os(OWNER_DRIVER_GIT_PROGRAM).map(PathBuf::from),
        },
    );
    let expectation = env::var(OWNER_DRIVER_EXPECTATION).expect("driver expectation");
    match expectation.as_str() {
        "status" => {
            let status = result.expect("installed owner driver returns a status");
            let expected = env::var(OWNER_DRIVER_STATUS)
                .expect("expected status")
                .parse::<u8>()
                .expect("numeric status");
            assert_eq!(status, ExitCode::from(expected));
        }
        "setup-deadline" | "recovery-deadline" => {
            let Err(error) = result else {
                panic!("expired {expectation} phase unexpectedly completed successfully");
            };
            let detail = format!("{error:#}");
            assert!(
                !process::is_clean_cancellation(&error),
                "{expectation} deadline must not be reported as clean cancellation: {detail}"
            );
            assert!(
                detail.contains("private snapshot retained at"),
                "{expectation} deadline must retain its installed-owner snapshot: {detail}"
            );
            assert!(
                detail.contains("timed_out=true") || detail.contains("phase deadline expired"),
                "{expectation} deadline must retain a deadline-specific failure: {detail}"
            );
            eprintln!("installed owner {expectation} result: {detail}");
        }
        other => panic!("unknown installed owner test expectation: {other}"),
    }
}

#[test]
fn installed_owner_live_state_probe_child() {
    let Some(output) = env::var_os(OWNER_DRIVER_PROBE_OUTPUT) else {
        return;
    };
    let repository = PathBuf::from(
        env::var_os(OWNER_DRIVER_REPOSITORY).expect("live repository path for state probe"),
    );
    fs::write(output, capture_live_canary_state(&repository))
        .expect("write live repository state observed during gate");
}

fn capture_live_canary_state(repository: &Path) -> Vec<u8> {
    let mut state = String::new();
    for relative in LIVE_CANARY_PATHS {
        let path = repository.join(relative);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                let bytes = if metadata.file_type().is_symlink() {
                    fs::read_link(&path)
                        .expect("read target symlink literal")
                        .as_os_str()
                        .as_bytes()
                        .to_vec()
                } else if metadata.is_file() {
                    fs::read(&path).expect("read live canary file bytes")
                } else {
                    Vec::new()
                };
                state.push_str(&format!(
                    "{relative}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                    metadata.dev(),
                    metadata.ino(),
                    metadata.mode(),
                    metadata.len(),
                    metadata.mtime(),
                    metadata.mtime_nsec(),
                    metadata.ctime(),
                    metadata.ctime_nsec(),
                ));
                state.push_str(&hex::encode(bytes));
                state.push('\n');
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                state.push_str(&format!("{relative}\tmissing\n\n"));
            }
            Err(error) => panic!("reading live canary metadata for {relative}: {error}"),
        }
    }
    state.into_bytes()
}

#[test]
fn installed_owner_signal_during_qualification_drains_owned_group() {
    for (signal, expected_status) in [
        (nix::sys::signal::Signal::SIGHUP, 129),
        (nix::sys::signal::Signal::SIGINT, 130),
        (nix::sys::signal::Signal::SIGTERM, 143),
    ] {
        let fixture = InstalledOwnerFixture::new(OwnerFixtureMode::BlockedQualification);
        let original_index = fs::read(fixture.index()).expect("read original index");
        let original_file =
            fs::read(fixture.repository.join("tracked.txt")).expect("read original worktree file");
        let mut owner = fixture.spawn_owner(
            "status",
            expected_status,
            OwnerFixtureBudgets::uniform(20_000),
        );
        wait_for_owner_readiness(&mut owner, &fixture.ready, &fixture.ticks);
        send_owner_signal(&mut owner, signal);
        let output = owner.wait_output(std::time::Duration::from_secs(8));
        assert!(
            output.status.success(),
            "installed-owner test helper failed for {signal:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("native hook recovery stopped after strict child drain"),
            "signal must be handled only after strict qualification-child drain: {stderr}"
        );
        let retained = retained_snapshot(&stderr);
        assert!(
            retained.is_dir(),
            "retained owner snapshot must survive return"
        );
        assert_eq!(
            fs::read(fixture.index()).expect("index after signal"),
            original_index
        );
        assert_eq!(
            fs::read(fixture.repository.join("tracked.txt")).expect("worktree after signal"),
            original_file
        );
        assert_ticks_stopped(&fixture.ticks);
        fs::remove_dir_all(retained).expect("remove verified drained snapshot");
    }
}

#[test]
fn installed_owner_setup_deadline_drains_and_retains_without_signal() {
    let fixture = InstalledOwnerFixture::new(OwnerFixtureMode::BlockedQualification);
    let mut owner = fixture.spawn_owner(
        "setup-deadline",
        0,
        OwnerFixtureBudgets {
            setup_ms: 3_000,
            gate_ms: 20_000,
            recovery_ms: 20_000,
        },
    );
    wait_for_owner_readiness(&mut owner, &fixture.ready, &fixture.ticks);
    let output = owner.wait_output(std::time::Duration::from_secs(8));
    assert!(
        output.status.success(),
        "installed-owner deadline assertion failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("installed owner setup-deadline result:"),
        "deadline failure should be observed by the installed-owner driver harness: {stderr}"
    );
    let retained = retained_snapshot(&stderr);
    assert!(
        retained.is_dir(),
        "deadline failure must retain the snapshot at {}: {stderr}",
        retained.display()
    );
    assert_ticks_stopped(&fixture.ticks);
    fs::remove_dir_all(retained).expect("remove verified drained deadline snapshot");
}

#[test]
fn installed_owner_first_gate_signal_collects_tracked_and_new_file_fixes() {
    let fixture = InstalledOwnerFixture::new(OwnerFixtureMode::BlockedGate);
    let original_index = fs::read(fixture.index()).expect("read original index");
    let original_file =
        fs::read(fixture.repository.join("tracked.txt")).expect("read original worktree file");
    let mut owner = fixture.spawn_owner("status", 130, OwnerFixtureBudgets::uniform(20_000));
    wait_for_owner_readiness(&mut owner, &fixture.ready, &fixture.ticks);
    send_owner_signal(&mut owner, nix::sys::signal::Signal::SIGINT);
    let output = owner.wait_output(std::time::Duration::from_secs(8));
    assert!(
        output.status.success(),
        "installed-owner gate-signal assertion failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("native pre-commit gate was unsuccessful; private snapshot retained at"),
        "gate cancellation must retain the immutable-owner snapshot: {stderr}"
    );
    assert!(
        stderr.contains("review private patch:"),
        "fix evidence must be reported: {stderr}"
    );
    let retained = retained_snapshot(&stderr);
    let snapshot_fix = fs::read(retained.join("fixes.patch")).expect("snapshot fix spool");
    let evidence = find_evidence_patch(&fixture.repository.join(".git"));
    let evidence_fix = fs::read(&evidence).expect("persisted hook evidence");
    for patch in [&snapshot_fix, &evidence_fix] {
        let patch = String::from_utf8_lossy(patch);
        assert!(
            patch.contains("diff --git a/tracked.txt b/tracked.txt"),
            "{patch}"
        );
        assert!(
            patch.contains("diff --git a/new-file.txt b/new-file.txt"),
            "{patch}"
        );
        assert!(patch.contains("fixed worktree bytes"), "{patch}");
        assert!(patch.contains("new staged-tree file"), "{patch}");
    }
    assert!(retained.is_dir(), "first gate signal preserves diagnostics");
    assert_eq!(
        fs::read(fixture.index()).expect("index after gate signal"),
        original_index
    );
    assert_eq!(
        fs::read(fixture.repository.join("tracked.txt")).expect("worktree after gate signal"),
        original_file
    );
    assert_ticks_stopped(&fixture.ticks);
    fs::remove_dir_all(retained).expect("remove verified drained gate snapshot");
    let evidence_dir = evidence.parent().expect("evidence parent").to_path_buf();
    fs::remove_dir_all(evidence_dir).expect("remove verified hook evidence fixture");
}

#[test]
fn installed_owner_rejects_ignored_source_from_successful_gate_with_complete_patch() {
    let fixture = InstalledOwnerFixture::new(OwnerFixtureMode::IgnoredSourceGate);
    let before = capture_live_canary_state(&fixture.repository);
    let mut owner = fixture.spawn_owner("status", 1, OwnerFixtureBudgets::uniform(20_000));
    let output = owner.wait_output(std::time::Duration::from_secs(8));
    assert!(
        output.status.success(),
        "installed-owner ignored-source assertion failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error: hook changed staged source; review private patch:"),
        "a successful gate that creates ignored staged-source evidence must be rejected: {stderr}"
    );
    let evidence = find_evidence_patch(&fixture.repository.join(".git"));
    let patch = fs::read(&evidence).expect("complete ignored-source evidence patch");
    let patch = String::from_utf8_lossy(&patch);
    assert!(patch.contains("diff --git a/ignored-extra.rs b/ignored-extra.rs"));
    assert!(patch.contains("source ignored by staged ignore rule"));
    assert_eq!(patch.matches("diff --git ").count(), 1, "{patch}");
    assert!(
        fs::read(&fixture.probe_output).expect("state observed during gate") == before,
        "the live repository canaries must remain unchanged while the gate runs"
    );
    assert_eq!(
        capture_live_canary_state(&fixture.repository),
        before,
        "the live repository canaries must remain unchanged after the owner returns"
    );
    assert!(
        !fixture.guard_marker.exists(),
        "the admitted guard is qualified for the hook contract but must not be invoked by this fake gate"
    );
    assert_private_artifacts_were_isolated(&fixture);
    let evidence_dir = evidence.parent().expect("evidence parent").to_path_buf();
    fs::remove_dir_all(evidence_dir).expect("remove verified hook evidence fixture");
}

#[test]
fn installed_owner_scrubs_injected_git_tools_aliases_and_live_artifact_paths() {
    let fixture = InstalledOwnerFixture::new(OwnerFixtureMode::IsolationGate);
    let before = capture_live_canary_state(&fixture.repository);
    let injection = fixture.poisoned_environment();
    let mut owner = fixture.spawn_owner_with_environment(
        "status",
        0,
        OwnerFixtureBudgets::uniform(20_000),
        None,
        &injection,
    );
    let output = owner.wait_output(std::time::Duration::from_secs(8));
    assert!(
        output.status.success(),
        "installed-owner isolation assertion failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(&fixture.probe_output).expect("state observed during gate"),
        before,
        "config, index, stash, live files, and the target symlink must stay unchanged during the gate"
    );
    assert_eq!(
        capture_live_canary_state(&fixture.repository),
        before,
        "config, index, stash, live files, and the target symlink must stay unchanged after the owner returns"
    );
    assert!(
        !fixture.guard_marker.exists(),
        "the admitted guard is qualified for the hook contract but must not be invoked by this fake gate"
    );
    assert!(
        !fixture.poison_marker.exists(),
        "no injected compiler or Cargo wrapper may run"
    );
    assert!(
        !fixture.file_alias_marker.exists() && !fixture.count_alias_marker.exists(),
        "neither injected Git alias may run inside the gate"
    );
    assert_private_artifacts_were_isolated(&fixture);
}

fn assert_private_artifacts_were_isolated(fixture: &InstalledOwnerFixture) {
    let report = fs::read_to_string(&fixture.artifact_report)
        .expect("gate records successfully created private build artifacts");
    let lines = report.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 5, "artifact report: {report:?}");
    let private_root = PathBuf::from(lines[0]);
    let target_artifacts = PathBuf::from(lines[1]);
    let build_artifacts = PathBuf::from(lines[2]);
    let cargo = PathBuf::from(lines[3]);
    assert_eq!(lines[4], "verified", "artifact report: {report:?}");
    assert!(!private_root.starts_with(&fixture.repository));
    assert!(target_artifacts.starts_with(&private_root), "{report}");
    assert!(build_artifacts.starts_with(&private_root), "{report}");
    assert!(cargo.starts_with(&private_root), "{report}");
    assert!(
        !target_artifacts.starts_with(&fixture.live_artifacts),
        "{report}"
    );
    assert!(
        !build_artifacts.starts_with(&fixture.live_artifacts),
        "{report}"
    );
    assert!(
        !target_artifacts.starts_with(&fixture.repository),
        "{report}"
    );
    assert!(
        !build_artifacts.starts_with(&fixture.repository),
        "{report}"
    );
    assert!(
        !private_root.exists(),
        "successful installed-owner completion must remove its private build directory"
    );
}

#[test]
fn installed_owner_second_recovery_signal_cancels_new_file_capture() {
    let fixture = InstalledOwnerFixture::new(OwnerFixtureMode::BlockedGate);
    let original_index = fs::read(fixture.index()).expect("read original index");
    let original_file =
        fs::read(fixture.repository.join("tracked.txt")).expect("read original worktree file");
    let git_wrapper = fixture.recovery_git_wrapper();
    let mut owner = fixture.spawn_owner_with_git_program(
        "status",
        143,
        OwnerFixtureBudgets::uniform(20_000),
        Some(&git_wrapper),
    );
    wait_for_owner_readiness(&mut owner, &fixture.ready, &fixture.ticks);
    send_owner_signal(&mut owner, nix::sys::signal::Signal::SIGHUP);
    wait_for_owner_readiness(&mut owner, &fixture.recovery_ready, &fixture.recovery_ticks);
    send_owner_signal(&mut owner, nix::sys::signal::Signal::SIGTERM);
    let output = owner.wait_output(std::time::Duration::from_secs(8));
    assert!(
        output.status.success(),
        "installed-owner recovery-signal assertion failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("native hook recovery stopped after strict child drain"),
        "a second signal must cancel genuine fix collection after the gate drains: {stderr}"
    );
    let retained = retained_snapshot(&stderr);
    assert!(retained.is_dir(), "recovery cancellation retains snapshot");
    assert_eq!(
        fs::read(retained.join("new-paths")).expect("retained new-path inventory"),
        b"new-file.txt\0"
    );
    assert!(
        !retained.join("fixes.patch").exists(),
        "cancelled recovery must not report an incomplete patch"
    );
    assert_eq!(
        fs::read(fixture.index()).expect("index after recovery signal"),
        original_index
    );
    assert_eq!(
        fs::read(fixture.repository.join("tracked.txt")).expect("worktree after recovery signal"),
        original_file
    );
    assert!(!fixture.repository.join("new-file.txt").exists());
    assert!(
        !fixture
            .repository
            .join(".git/jackin-hook-evidence")
            .exists(),
        "incomplete recovery must not publish evidence"
    );
    assert_ticks_stopped(&fixture.recovery_ticks);
    fs::remove_dir_all(retained).expect("remove verified drained recovery snapshot");
}

#[test]
fn installed_owner_recovery_deadline_is_not_masked_by_first_signal() {
    let fixture = InstalledOwnerFixture::new(OwnerFixtureMode::BlockedGate);
    let original_index = fs::read(fixture.index()).expect("read original index");
    let original_file =
        fs::read(fixture.repository.join("tracked.txt")).expect("read original worktree file");
    let git_wrapper = fixture.recovery_git_wrapper();
    let mut owner = fixture.spawn_owner_with_git_program(
        "recovery-deadline",
        0,
        OwnerFixtureBudgets {
            setup_ms: 20_000,
            gate_ms: 20_000,
            recovery_ms: 1_500,
        },
        Some(&git_wrapper),
    );
    wait_for_owner_readiness(&mut owner, &fixture.ready, &fixture.ticks);
    send_owner_signal(&mut owner, nix::sys::signal::Signal::SIGHUP);
    wait_for_owner_readiness(&mut owner, &fixture.recovery_ready, &fixture.recovery_ticks);
    let output = owner.wait_output(std::time::Duration::from_secs(8));
    assert!(
        output.status.success(),
        "installed-owner recovery-deadline assertion failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("installed owner recovery-deadline result:"),
        "recovery timeout must reach the installed-owner driver as an error: {stderr}"
    );
    assert!(
        !stderr.contains("native hook recovery stopped after strict child drain"),
        "the first sticky HUP must not mask a later recovery deadline: {stderr}"
    );
    let retained = retained_snapshot(&stderr);
    assert!(retained.is_dir(), "recovery deadline retains snapshot");
    assert_eq!(
        fs::read(retained.join("new-paths")).expect("retained new-path inventory"),
        b"new-file.txt\0"
    );
    assert!(
        !retained.join("fixes.patch").exists(),
        "timed out recovery must not report an incomplete patch"
    );
    assert_eq!(
        fs::read(fixture.index()).expect("index after recovery timeout"),
        original_index
    );
    assert_eq!(
        fs::read(fixture.repository.join("tracked.txt")).expect("worktree after recovery timeout"),
        original_file
    );
    assert!(!fixture.repository.join("new-file.txt").exists());
    assert!(
        !fixture
            .repository
            .join(".git/jackin-hook-evidence")
            .exists(),
        "incomplete recovery must not publish evidence"
    );
    assert_ticks_stopped(&fixture.recovery_ticks);
    fs::remove_dir_all(retained).expect("remove verified drained recovery snapshot");
}

fn owner_budget(name: &str) -> std::time::Duration {
    std::time::Duration::from_millis(
        env::var(name)
            .expect("owner budget")
            .parse::<u64>()
            .expect("numeric owner budget"),
    )
}

#[derive(Clone, Copy)]
enum OwnerFixtureMode {
    BlockedQualification,
    BlockedGate,
    IgnoredSourceGate,
    IsolationGate,
}

fn create_owner_fixture_repository(repository: &Path, has_live_canaries: bool) {
    git_command(repository, &["init", "--quiet"]);
    git_command(repository, &["config", "user.name", "Installed Owner Test"]);
    git_command(
        repository,
        &["config", "user.email", "installed-owner@example.invalid"],
    );
    fs::write(
        repository.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"1.99.1\"\nprofile = \"minimal\"\ncomponents = [\"clippy\", \"rustfmt\"]\n",
    )
    .expect("write staged toolchain pin");
    fs::write(repository.join("tracked.txt"), b"base worktree bytes\n")
        .expect("write tracked fixture");
    git_command(repository, &["add", "rust-toolchain.toml", "tracked.txt"]);
    git_command(repository, &["commit", "--quiet", "-m", "fixture"]);
    fs::write(repository.join("tracked.txt"), b"staged worktree bytes\n")
        .expect("stage a live worktree difference");
    git_command(repository, &["add", "tracked.txt"]);

    if has_live_canaries {
        fs::write(
            repository.join(".gitignore"),
            b"ignored-canary.txt\ntarget\nignored-extra.rs\n",
        )
        .expect("write staged ignore rules");
        git_command(repository, &["add", ".gitignore"]);
        fs::write(
            repository.join("untracked-canary.txt"),
            b"live untracked canary\n",
        )
        .expect("write live untracked canary");
        fs::write(
            repository.join("ignored-canary.txt"),
            b"live ignored canary\n",
        )
        .expect("write live ignored canary");
    }
}

#[derive(Clone, Copy)]
struct OwnerFixtureBudgets {
    setup_ms: u64,
    gate_ms: u64,
    recovery_ms: u64,
}

impl OwnerFixtureBudgets {
    fn uniform(milliseconds: u64) -> Self {
        Self {
            setup_ms: milliseconds,
            gate_ms: milliseconds,
            recovery_ms: milliseconds,
        }
    }
}

struct InstalledOwnerFixture {
    _temporary: tempfile::TempDir,
    repository: PathBuf,
    generation: PathBuf,
    owner: PathBuf,
    path: std::ffi::OsString,
    ready: PathBuf,
    ticks: PathBuf,
    recovery_ready: PathBuf,
    recovery_ticks: PathBuf,
    probe_output: PathBuf,
    artifact_report: PathBuf,
    guard_marker: PathBuf,
    expected_tree: String,
    expected_rustc: PathBuf,
    live_artifacts: PathBuf,
    poison_marker: PathBuf,
    poison: PathBuf,
    file_alias_config: PathBuf,
    file_alias_marker: PathBuf,
    count_alias_marker: PathBuf,
}

impl InstalledOwnerFixture {
    fn new(mode: OwnerFixtureMode) -> Self {
        let home = fs::canonicalize(env::var_os("HOME").expect("HOME is set"))
            .expect("canonicalizing HOME");
        let temporary = tempfile::Builder::new()
            .prefix("jackin-installed-owner-f3-")
            .tempdir_in(home)
            .expect("creating installed-owner repository");
        let repository = temporary
            .path()
            .canonicalize()
            .expect("canonical repository");
        let has_live_canaries = matches!(
            mode,
            OwnerFixtureMode::IgnoredSourceGate | OwnerFixtureMode::IsolationGate
        );
        create_owner_fixture_repository(&repository, has_live_canaries);

        let scratch = repository.join(".git/f3-test");
        fs::create_dir(&scratch).expect("create private test support directory");
        fs::set_permissions(&scratch, fs::Permissions::from_mode(0o700))
            .expect("protect test support directory");
        let live_artifacts = scratch.join("live-artifacts");
        fs::create_dir(&live_artifacts).expect("create live artifact canary directory");
        fs::set_permissions(&live_artifacts, fs::Permissions::from_mode(0o700))
            .expect("protect live artifact canary directory");
        fs::write(
            live_artifacts.join("existing-canary.bin"),
            b"preserve the live artifact directory\n",
        )
        .expect("write live artifact canary");
        if has_live_canaries {
            symlink(".git/f3-test/live-artifacts", repository.join("target"))
                .expect("create ignored live target symlink");
            git_command(&repository, &["check-ignore", "--quiet", "target"]);
            git_command(
                &repository,
                &["check-ignore", "--quiet", "ignored-canary.txt"],
            );
        }
        let ready = scratch.join("ready");
        let ticks = scratch.join("ticks");
        let recovery_ready = scratch.join("recovery-ready");
        let recovery_ticks = scratch.join("recovery-ticks");
        let probe_output = scratch.join("gate-live-state");
        let artifact_report = scratch.join("private-artifact-report");
        let guard_marker = scratch.join("rust-build-guard-invoked");
        let poison_marker = scratch.join("poison-wrapper-ran");
        let file_alias_marker = scratch.join("file-alias-ran");
        let count_alias_marker = scratch.join("count-alias-ran");
        let poison = write_poison_tools(&scratch, &poison_marker);
        let file_alias_config = scratch.join("poison-git-config");
        fs::write(
            &file_alias_config,
            format!(
                "[alias]\nf3-poison-file = \"!touch {}\"\n",
                shell_quote(&file_alias_marker)
            ),
        )
        .expect("write injected Git config alias");
        let bin = scratch.join("bin");
        fs::create_dir(&bin).expect("create private fixture tools");
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o700))
            .expect("protect fixture tools");
        let expected_rustc = bin.join("rustc");
        write_owner_toolchain(&bin, &ready, &ticks, &guard_marker, mode);

        if has_live_canaries {
            let stash = git_output(&repository, &["stash", "create"]);
            assert!(!stash.is_empty(), "fixture changes produce a stash commit");
            git_command(
                &repository,
                &["stash", "store", "-m", "state canary", &stash],
            );
        }

        let head_tree = git_output(&repository, &["rev-parse", "HEAD^{tree}"]);
        let expected_tree = git_output(&repository, &["write-tree"]);
        let common = repository.join(".git");
        let owner_root = common.join("jackin-precommit-owner");
        let generations = owner_root.join("generations");
        let generation_id = "a".repeat(64);
        let generation = generations.join(&generation_id);
        fs::create_dir_all(&generations).expect("create owner generations");
        fs::set_permissions(&owner_root, fs::Permissions::from_mode(0o700))
            .expect("protect owner root");
        fs::set_permissions(&generations, fs::Permissions::from_mode(0o700))
            .expect("protect generations");
        fs::create_dir(&generation).expect("create selected generation");
        let owner = generation.join("owner");
        fs::copy(
            env::current_exe().expect("resolve owner test executable"),
            &owner,
        )
        .expect("copy actual owner test harness into selected generation");
        fs::set_permissions(&owner, fs::Permissions::from_mode(0o500))
            .expect("seal selected owner executable");

        let owner_hash = digest(&fs::read(&owner).expect("read copied owner"));
        let cargo_path = bin.join("cargo");
        let rustc_path = bin.join("rustc");
        let manifest = format!(
            "jackin-native-precommit-install/1\ngeneration={generation_id}\nowner-sha256={owner_hash}\nsource-tree={head_tree}\nbuild-commit={}\nsource-sha256={}\ncargo-sha256={}\nrustc-sha256={}\ntarget-sha256={}\nbuild-sha256={}\n",
            "b".repeat(40),
            "c".repeat(64),
            digest(&fs::read(&cargo_path).expect("read fixture cargo")),
            digest(&fs::read(&rustc_path).expect("read fixture rustc")),
            "d".repeat(64),
            "e".repeat(64),
        );
        fs::write(generation.join("install.manifest"), manifest)
            .expect("write selected-owner manifest");
        fs::set_permissions(
            generation.join("install.manifest"),
            fs::Permissions::from_mode(0o400),
        )
        .expect("seal selected-owner manifest");
        fs::set_permissions(&generation, fs::Permissions::from_mode(0o500))
            .expect("seal immutable selected generation");
        let pending = owner_root.join("pending");
        fs::create_dir(&pending).expect("create pending tree directory");
        fs::set_permissions(&pending, fs::Permissions::from_mode(0o700))
            .expect("protect pending tree directory");

        Self {
            _temporary: temporary,
            repository,
            generation,
            owner,
            path: env::join_paths(
                std::iter::once(bin)
                    .chain(env::split_paths(&env::var_os("PATH").unwrap_or_default())),
            )
            .expect("join installed-owner test PATH"),
            ready,
            ticks,
            recovery_ready,
            recovery_ticks,
            probe_output,
            artifact_report,
            guard_marker,
            expected_tree,
            expected_rustc,
            live_artifacts,
            poison_marker,
            poison,
            file_alias_config,
            file_alias_marker,
            count_alias_marker,
        }
    }

    fn index(&self) -> PathBuf {
        self.repository.join(".git/index")
    }

    fn spawn_owner(
        &self,
        expectation: &str,
        expected_status: u8,
        budgets: OwnerFixtureBudgets,
    ) -> OwnerChild {
        self.spawn_owner_with_git_program(expectation, expected_status, budgets, None)
    }

    fn spawn_owner_with_git_program(
        &self,
        expectation: &str,
        expected_status: u8,
        budgets: OwnerFixtureBudgets,
        git_program: Option<&Path>,
    ) -> OwnerChild {
        self.spawn_owner_with_environment(expectation, expected_status, budgets, git_program, &[])
    }

    fn spawn_owner_with_environment(
        &self,
        expectation: &str,
        expected_status: u8,
        budgets: OwnerFixtureBudgets,
        git_program: Option<&Path>,
        injected_environment: &[(std::ffi::OsString, std::ffi::OsString)],
    ) -> OwnerChild {
        let home = env::var_os("HOME").expect("HOME is set");
        let mut command = Command::new(&self.owner);
        command
            .args([
                "--exact",
                "precommit_snapshot::tests::installed_owner_driver_child",
                "--nocapture",
            ])
            .current_dir(&self.repository)
            .env_clear()
            .env("HOME", home)
            .env("PATH", &self.path)
            .env(OWNER_DRIVER_CHILD, "1")
            .env(OWNER_DRIVER_GENERATION, &self.generation)
            .env(OWNER_DRIVER_EXPECTATION, expectation)
            .env(OWNER_DRIVER_STATUS, expected_status.to_string())
            .env(OWNER_DRIVER_SETUP_MS, budgets.setup_ms.to_string())
            .env(OWNER_DRIVER_GATE_MS, budgets.gate_ms.to_string())
            .env(OWNER_DRIVER_RECOVERY_MS, budgets.recovery_ms.to_string())
            .env(OWNER_DRIVER_REPOSITORY, &self.repository)
            .env(OWNER_DRIVER_EXPECTED_TREE, &self.expected_tree)
            .env(OWNER_DRIVER_PROBE_EXE, &self.owner)
            .env(OWNER_DRIVER_PROBE_OUTPUT, &self.probe_output)
            .env(OWNER_DRIVER_ARTIFACT_REPORT, &self.artifact_report)
            .env(OWNER_DRIVER_EXPECTED_RUSTC, &self.expected_rustc)
            .env(
                OWNER_DRIVER_EXPECTED_RUSTDOC,
                self.expected_rustc.with_file_name("rustdoc"),
            )
            .env(
                OWNER_DRIVER_EXPECTED_CLIPPY,
                self.expected_rustc.with_file_name("clippy-driver"),
            )
            .env(
                OWNER_DRIVER_EXPECTED_RUSTFMT,
                self.expected_rustc.with_file_name("rustfmt"),
            )
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES");
        if let Some(program) = git_program {
            command.env(OWNER_DRIVER_GIT_PROGRAM, program);
        } else {
            command.env_remove(OWNER_DRIVER_GIT_PROGRAM);
        }
        for (key, value) in injected_environment {
            command.env(key, value);
        }
        let child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn selected immutable installed owner");
        OwnerChild { child: Some(child) }
    }

    fn poisoned_environment(&self) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
        let poisoned = |name: &str| self.poison.join(name).into_os_string();
        vec![
            (
                "GIT_CONFIG".into(),
                self.file_alias_config.as_os_str().to_owned(),
            ),
            ("GIT_CONFIG_COUNT".into(), "1".into()),
            ("GIT_CONFIG_KEY_0".into(), "alias.f3-poison-count".into()),
            (
                "GIT_CONFIG_VALUE_0".into(),
                format!("!touch {}", shell_quote(&self.count_alias_marker)).into(),
            ),
            ("CARGO".into(), poisoned("cargo")),
            ("RUSTC".into(), poisoned("rustc")),
            ("RUSTDOC".into(), poisoned("rustdoc")),
            ("CLIPPY_DRIVER".into(), poisoned("clippy-driver")),
            ("RUSTFMT".into(), poisoned("rustfmt")),
            ("RUSTC_WRAPPER".into(), poisoned("rustc-wrapper")),
            ("RUSTC_WORKSPACE_WRAPPER".into(), poisoned("rustc-wrapper")),
            ("CARGO_BUILD_RUSTC".into(), poisoned("rustc")),
            ("CARGO_BUILD_RUSTDOC".into(), poisoned("rustdoc")),
            (
                "CARGO_BUILD_RUSTC_WRAPPER".into(),
                poisoned("rustc-wrapper"),
            ),
            (
                "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER".into(),
                poisoned("rustc-wrapper"),
            ),
            ("CARGO_ALIAS_F3_POISON".into(), "poisoned alias".into()),
            (
                "CARGO_TARGET_DIR".into(),
                self.repository.join("target").into_os_string(),
            ),
            (
                "CARGO_BUILD_BUILD_DIR".into(),
                self.repository.join("target/build").into_os_string(),
            ),
        ]
    }

    fn recovery_git_wrapper(&self) -> PathBuf {
        let wrapper = self.repository.join(".git/f3-test/bin/git-wrapper");
        let holder = self.repository.join(".git/f3-test/bin/holder");
        let real_git = real_git_executable();
        let body = format!(
            "#!/bin/sh\nif [ \"$#\" -eq 6 ] && [ \"$1\" = \"--literal-pathspecs\" ] && [ \"$2\" = \"add\" ] && [ \"$3\" = \"--force\" ] && [ \"$4\" = \"--intent-to-add\" ] && [ \"$5\" != \"\" ] && [ \"$6\" = \"--pathspec-file-nul\" ]; then\n  {} {} {} &\n  wait $!\n  exit 97\nfi\nexec {} \"$@\"\n",
            shell_quote(&holder),
            shell_quote(&self.recovery_ready),
            shell_quote(&self.recovery_ticks),
            shell_quote(&real_git),
        );
        write_executable(&wrapper, body.as_bytes());
        wrapper
    }
}

impl Drop for InstalledOwnerFixture {
    fn drop(&mut self) {
        drop(fs::set_permissions(
            &self.generation,
            fs::Permissions::from_mode(0o700),
        ));
    }
}

struct OwnerChild {
    child: Option<Child>,
}

impl OwnerChild {
    fn id(&self) -> u32 {
        self.child.as_ref().expect("owner child remains").id()
    }

    fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        self.child.as_mut().expect("owner child remains").try_wait()
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "the synchronous owner harness uses a bounded child-status polling loop"
    )]
    fn wait_output(&mut self, timeout: std::time::Duration) -> std::process::Output {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Some(status) = self.try_wait().expect("check installed owner status") {
                assert!(
                    status.code().is_some(),
                    "owner test harness was signaled: {status}"
                );
                let child = self.child.take().expect("take completed owner child");
                return child.wait_with_output().expect("collect owner diagnostics");
            }
            assert!(
                std::time::Instant::now() < deadline,
                "installed owner did not finish within {timeout:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

impl Drop for OwnerChild {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            drop(child.kill());
            drop(child.wait());
        }
    }
}

fn write_owner_toolchain(
    bin: &Path,
    ready: &Path,
    ticks: &Path,
    guard_marker: &Path,
    mode: OwnerFixtureMode,
) {
    let ready = shell_quote(ready);
    let ticks = shell_quote(ticks);
    let holder = bin.join("holder");
    write_executable(
        &holder,
        b"#!/bin/sh\ntrap '' HUP INT TERM\nprintf x >> \"$2\"\n: > \"$1\"\ncount=0\nwhile [ \"$count\" -lt 400 ]; do\n  /bin/sleep 0.05 || :\n  printf x >> \"$2\"\n  count=$((count + 1))\ndone\n",
    );
    let rust_tools = String::from(
        "#!/bin/sh\nname=${0##*/}\ncase \"$name\" in\n  cargo) printf 'cargo 1.99.1 (fixture)\\n' ;;\n  rustc) printf 'rustc 1.99.1 (fixture)\\nbinary: rustc\\ncommit-hash: 0123456789abcdef0123456789abcdef01234567\\nrelease: 1.99.1\\n' ;;\n  rustdoc) printf 'rustdoc 1.99.1 (fixture)\\nbinary: rustdoc\\ncommit-hash: 0123456789abcdef0123456789abcdef01234567\\nrelease: 1.99.1\\n' ;;\n  clippy-driver) printf 'clippy 0.1.99 (0123456789abcdef0123456789abcdef01234567 fixture)\\n' ;;\n  rustfmt) printf 'rustfmt 1.99.1-stable (0123456789abcdef0123456789abcdef01234567 fixture)\\n' ;;\n  *) exit 91 ;;\nesac\n",
    );
    for name in ["rustc", "rustdoc", "clippy-driver", "rustfmt"] {
        write_executable(&bin.join(name), rust_tools.as_bytes());
    }

    let build_mode = matches!(
        mode,
        OwnerFixtureMode::IgnoredSourceGate | OwnerFixtureMode::IsolationGate
    );
    let mut cargo_body = rust_tools.clone();
    if build_mode {
        cargo_body = String::from(
            "#!/bin/sh\nname=${0##*/}\nif [ \"$name\" = cargo ] && [ \"${1-}\" = build ]; then\ntarget=\"${CARGO_TARGET_DIR:?missing effective Cargo target directory}\"\nbuild=\"${CARGO_BUILD_BUILD_DIR:?missing effective Cargo build directory}\"\nmkdir -p \"$target\" \"$build\"\nprintf 'private target artifact\\n' > \"$target/f3-owner-artifact\"\nprintf 'private build artifact\\n' > \"$build/f3-owner-build-artifact\"\nscratch=\"$(cd .. && pwd -P)\"\ntarget=\"$(cd \"$target\" && pwd -P)\"\nbuild=\"$(cd \"$build\" && pwd -P)\"\ncargo_command=\"$(command -v cargo)\"\ncargo_dir=\"$(cd \"$(dirname \"$cargo_command\")\" && pwd -P)\"\ncargo=\"$cargo_dir/$(basename \"$cargo_command\")\"\ntest -f \"$target/f3-owner-artifact\"\ntest -f \"$build/f3-owner-build-artifact\"\nprintf '%s\\n%s\\n%s\\n%s\\nverified\\n' \"$scratch\" \"$target\" \"$build\" \"$cargo\" > \"$JACKIN_F3_OWNER_DRIVER_ARTIFACT_REPORT\"\nexit 0\nfi\ncase \"$name\" in\n  cargo) printf 'cargo 1.99.1 (fixture)\\n' ;;\n  *) exit 91 ;;\nesac\n",
        );
    }
    write_executable(&bin.join("cargo"), cargo_body.as_bytes());

    let gate_body = if build_mode {
        let mut gate = String::from(
            "#!/bin/sh\nset -eu\ntest \"$(git write-tree)\" = \"$JACKIN_F3_OWNER_DRIVER_EXPECTED_TREE\" || exit 93\ntest \"$(git show :tracked.txt)\" = 'staged worktree bytes' || exit 94\ntest \"$(pwd)\" != \"$JACKIN_F3_OWNER_DRIVER_REPOSITORY\" || exit 95\ntest \"$RUSTC\" = \"$JACKIN_F3_OWNER_DRIVER_EXPECTED_RUSTC\" || exit 96\ntest \"$RUSTDOC\" = \"$JACKIN_F3_OWNER_DRIVER_EXPECTED_RUSTDOC\" || exit 97\ntest \"$CLIPPY_DRIVER\" = \"$JACKIN_F3_OWNER_DRIVER_EXPECTED_CLIPPY\" || exit 98\ntest \"$RUSTFMT\" = \"$JACKIN_F3_OWNER_DRIVER_EXPECTED_RUSTFMT\" || exit 99\ntest \"$CARGO_BUILD_RUSTC\" = \"$JACKIN_F3_OWNER_DRIVER_EXPECTED_RUSTC\" || exit 100\ntest \"$CARGO_BUILD_RUSTDOC\" = \"$JACKIN_F3_OWNER_DRIVER_EXPECTED_RUSTDOC\" || exit 101\ntest -x \"$(command -v cargo)\" || exit 102\ntest -z \"${CARGO-}\" || exit 103\ntest -z \"${CARGO_ALIAS_F3_POISON-}\" || exit 104\ntest -z \"${RUSTC_WRAPPER-}\" || exit 105\ntest -z \"${RUSTC_WORKSPACE_WRAPPER-}\" || exit 106\ntest -z \"${CARGO_BUILD_RUSTC_WRAPPER-}\" || exit 107\ntest -z \"${CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER-}\" || exit 108\nif git f3-poison-file >/dev/null 2>&1; then exit 109; fi\nif git f3-poison-count >/dev/null 2>&1; then exit 110; fi\n",
        );
        if matches!(mode, OwnerFixtureMode::IgnoredSourceGate) {
            gate.push_str(
                "printf 'source ignored by staged ignore rule\\n' > ignored-extra.rs\ngit check-ignore -q ignored-extra.rs || exit 111\n",
            );
        }
        gate.push_str(
            "cargo build --f3-owner-isolation-probe\n\"$JACKIN_F3_OWNER_DRIVER_PROBE_EXE\" --exact precommit_snapshot::tests::installed_owner_live_state_probe_child --nocapture --test-threads=1 >/dev/null 2>&1\n",
        );
        gate.push_str("exit 0\n");
        gate
    } else {
        format!(
            "#!/bin/sh\nprintf 'fixed worktree bytes\\n' > tracked.txt\nprintf 'new staged-tree file\\n' > new-file.txt\n{} {} {} &\nwait $!\nexit 97\n",
            shell_quote(&holder),
            ready,
            ticks
        )
    };
    write_executable(&bin.join("hk"), gate_body.as_bytes());

    let block_qualification = matches!(mode, OwnerFixtureMode::BlockedQualification);
    let mise_body = if block_qualification {
        format!(
            "#!/bin/sh\ncase \"$1\" in\n  which) {} {} {} &\n    wait $!\n    exit 97 ;;\n  exec)\n    case \"$*\" in\n      *'cargo --version'*) printf 'cargo 1.99.1 (fixture)\\n' ;;\n      *'command -v cargo'*) printf '%s\\n' {} ;;\n      *) exit 92 ;;\n    esac ;;\n  *) exit 90 ;;\nesac\n",
            shell_quote(&holder),
            ready,
            ticks,
            shell_quote(&bin.join("cargo"))
        )
    } else {
        format!(
            "#!/bin/sh\ncase \"$1\" in\n  which)\n    if [ \"$2\" = hk ]; then printf '%s\\n' {}; else printf '%s\\n' /usr/bin/true; fi ;;\n  exec)\n    case \"$*\" in\n      *'cargo --version'*) printf 'cargo 1.99.1 (fixture)\\n' ;;\n      *'command -v cargo'*) printf '%s\\n' {} ;;\n      *) exit 92 ;;\n    esac ;;\n  *) exit 90 ;;\nesac\n",
            shell_quote(&bin.join("hk")),
            shell_quote(&bin.join("cargo"))
        )
    };
    write_executable(&bin.join("mise"), mise_body.as_bytes());
    let rustup_body = format!(
        "#!/bin/sh\n[ \"$1\" = which ] && [ \"$3\" = 1.99.1 ] || exit 90\ncase \"$4\" in rustc|rustdoc|clippy-driver|rustfmt) printf '%s/%s\\n' {} \"$4\" ;; *) exit 91 ;; esac\n",
        shell_quote(bin)
    );
    write_executable(&bin.join("rustup"), rustup_body.as_bytes());
    write_executable(
        &bin.join("rust-build-guard"),
        format!("#!/bin/sh\n: > {}\nexit 97\n", shell_quote(guard_marker)).as_bytes(),
    );
}

fn write_executable(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).expect("write installed-owner fixture executable");
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .expect("make installed-owner fixture executable");
}

fn write_poison_tools(scratch: &Path, marker: &Path) -> PathBuf {
    let poison = scratch.join("poison");
    fs::create_dir(&poison).expect("create poison tool directory");
    let body = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$0 $*\" >> {}\nexit 87\n",
        shell_quote(marker)
    );
    for name in [
        "cargo",
        "rustc",
        "rustdoc",
        "clippy-driver",
        "rustfmt",
        "rustc-wrapper",
        "rustdoc-wrapper",
    ] {
        write_executable(&poison.join(name), body.as_bytes());
    }
    poison
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

fn real_git_executable() -> PathBuf {
    env::split_paths(&env::var_os("PATH").expect("test PATH is set"))
        .map(|directory| directory.join("git"))
        .find(|path| {
            fs::metadata(path).is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
        .and_then(|path| fs::canonicalize(path).ok())
        .expect("resolve absolute real Git executable for wrapper delegation")
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(sha2::Sha256::digest(bytes))
}

#[expect(
    clippy::disallowed_methods,
    reason = "fixture setup runs a short synchronous Git command outside async runtime code"
)]
fn git_output(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
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
        .output()
        .expect("run fixture Git command");
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("fixture Git output is UTF-8")
        .trim()
        .to_owned()
}

#[expect(
    clippy::disallowed_methods,
    reason = "the synchronous owner harness polls readiness with a hard deadline"
)]
fn wait_for_owner_readiness(owner: &mut OwnerChild, ready: &Path, ticks: &Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while std::time::Instant::now() < deadline {
        if let Some(status) = owner.try_wait().expect("check owner before readiness") {
            let output = owner.wait_output(std::time::Duration::ZERO);
            panic!(
                "owner exited before blocked-child readiness ({status}): stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        if ready.exists() && fs::metadata(ticks).is_ok_and(|metadata| metadata.len() > 0) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("installed-owner child never published readiness and a live tick");
}

fn send_owner_signal(owner: &mut OwnerChild, signal: nix::sys::signal::Signal) {
    assert!(
        owner
            .try_wait()
            .expect("check owner before signal")
            .is_none(),
        "owner must remain alive until the test delivers {signal:?}"
    );
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(owner.id().try_into().expect("owner PID fits i32")),
        signal,
    )
    .expect("deliver signal to live installed owner child");
}

fn retained_snapshot(output: &str) -> PathBuf {
    let path = output
        .split("private snapshot retained at ")
        .nth(1)
        .and_then(|remainder| remainder.split_whitespace().next())
        .expect("owner output reports retained snapshot");
    PathBuf::from(path.trim_end_matches([';', ',', '.', ':', ')']))
}

#[expect(
    clippy::disallowed_methods,
    reason = "the fixture checks for post-return child activity over a short bounded interval"
)]
fn assert_ticks_stopped(ticks: &Path) {
    let after_return = fs::read(ticks).expect("read strict-child ticks after owner return");
    std::thread::sleep(std::time::Duration::from_millis(250));
    assert_eq!(
        fs::read(ticks).expect("re-read strict-child ticks"),
        after_return,
        "strict supervisor must drain the process group before owner return"
    );
}

fn find_evidence_patch(common_git_dir: &Path) -> PathBuf {
    let root = common_git_dir.join("jackin-hook-evidence");
    for invocation in crate::fs_util::read_dir_sorted(&root).expect("read retained evidence root") {
        let patch = invocation.path().join("fixes.patch");
        if patch.is_file() {
            return patch;
        }
    }
    panic!("installed owner did not persist a staged-source patch");
}
