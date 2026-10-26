use std::{
    env,
    ffi::OsStr,
    fs, io,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use super::{
    OwnerBudgets, OwnerSignal, SetupSignals, SignalRoute, completion_exit_code,
    retain_snapshot_failure, retain_unsuccessful_completion, run_with_budgets,
    write_private_fixes_spool,
};
use crate::precommit_snapshot::{evidence, git, process, toolchain::QualifiedTools};

const OWNER_DRIVER_CHILD: &str = "JACKIN_F3_OWNER_DRIVER_CHILD";
const OWNER_DRIVER_EXPECTATION: &str = "JACKIN_F3_OWNER_DRIVER_EXPECTATION";
const OWNER_DRIVER_STATUS: &str = "JACKIN_F3_OWNER_DRIVER_STATUS";
const OWNER_DRIVER_SETUP_MS: &str = "JACKIN_F3_OWNER_DRIVER_SETUP_MS";
const OWNER_DRIVER_GATE_MS: &str = "JACKIN_F3_OWNER_DRIVER_GATE_MS";
const OWNER_DRIVER_RECOVERY_MS: &str = "JACKIN_F3_OWNER_DRIVER_RECOVERY_MS";
const OWNER_DRIVER_GIT_PROGRAM: &str = "JACKIN_F3_OWNER_DRIVER_GIT_PROGRAM";
const OWNER_DRIVER_PROBE_OUTPUT: &str = "JACKIN_F3_OWNER_DRIVER_PROBE_OUTPUT";
const OWNER_DRIVER_GATE_RELEASE: &str = "JACKIN_F3_OWNER_DRIVER_GATE_RELEASE";
const OWNER_DRIVER_GATE_READY: &str = "JACKIN_F3_OWNER_DRIVER_GATE_READY";

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
        .env_remove("GIT_CONFIG")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("GIT_CONFIG_COUNT")
        .status()
        .expect("running fixture Git command");
    assert!(status.success(), "git {arguments:?} failed: {status}");
}

fn git_output(root: &Path, arguments: &[&str]) -> Vec<u8> {
    let mut command = Command::new("git");
    command
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
        .env_remove("GIT_CONFIG")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("GIT_CONFIG_COUNT");
    process::capture_strict(
        &mut command,
        &process::SetupControls::for_test(std::time::Duration::from_secs(10)),
        process::EnvironmentMode::Inherit,
        1024,
        1024,
        process::CommandBudget::Probe,
    )
    .expect("capture fixture Git output")
    .stdout
}

fn capture_live_canary_state(repository: &Path) -> Vec<u8> {
    let paths = [
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
        ".git/f3-test/live-artifacts/sentinel.bin",
    ];
    let mut state = String::new();
    for relative in paths {
        let path = repository.join(relative);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                let bytes = if metadata.file_type().is_symlink() {
                    fs::read_link(&path)
                        .expect("read live canary symlink target")
                        .as_os_str()
                        .as_encoded_bytes()
                        .to_vec()
                } else if metadata.is_file() {
                    fs::read(&path).expect("read live canary file")
                } else {
                    Vec::new()
                };
                use std::fmt::Write as _;
                writeln!(
                    state,
                    "{relative}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    metadata.dev(),
                    metadata.ino(),
                    metadata.mode(),
                    metadata.len(),
                    metadata.mtime(),
                    metadata.mtime_nsec(),
                    metadata.ctime(),
                    metadata.ctime_nsec(),
                    hex::encode(bytes),
                )
                .expect("append live canary state");
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                use std::fmt::Write as _;
                writeln!(state, "{relative}\tmissing").expect("append missing canary state");
            }
            Err(error) => panic!("reading live canary {relative}: {error}"),
        }
    }
    state.into_bytes()
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
            recovery_armed: AtomicBool::new(false),
            cancellation: Arc::new(AtomicBool::new(false)),
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
    assert!(signals.route.cancellation.load(Ordering::SeqCst));

    let recovery = signals
        .arm_recovery_with_budget(std::time::Duration::from_secs(30))
        .expect("recovery phase arms once");
    assert!(!recovery.cancellation().load(Ordering::SeqCst));
    signals.route.record(OwnerSignal::Term);
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

    let error = git::OwnedSnapshot::verify_live_index(&live, &live.controls).unwrap_err();
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

    let setup = owner_budget(OWNER_DRIVER_SETUP_MS);
    let gate = owner_budget(OWNER_DRIVER_GATE_MS);
    let recovery = owner_budget(OWNER_DRIVER_RECOVERY_MS);
    let result = run_with_budgets(
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
            assert_eq!(status, expected);
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
        "qualification-error" => {
            let Err(error) = result else {
                panic!("injected Mise environment unexpectedly qualified");
            };
            let detail = format!("{error:#}");
            assert!(
                detail.contains("reintroduces RUSTC_BOOTSTRAP"),
                "unexpected qualification failure: {detail}"
            );
            eprintln!("installed owner {expectation} result: {detail}");
        }
        other => panic!("unknown installed owner test expectation: {other}"),
    }
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
        let mut owner = fixture.spawn_owner("status", expected_status, 20_000, 20_000, 20_000);
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
    let mut owner = fixture.spawn_owner("setup-deadline", 0, 3_000, 20_000, 20_000);
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
fn generic_owner_rejects_successful_gate_that_changes_ignored_staged_source() {
    let fixture = InstalledOwnerFixture::new(OwnerFixtureMode::IgnoredSourceGate);
    let original_index = fs::read(fixture.index()).expect("read original index");
    let original_file =
        fs::read(fixture.repository.join("tracked.txt")).expect("read original worktree file");
    let mut owner = fixture.spawn_owner("status", 1, 20_000, 20_000, 20_000);
    let output = owner.wait_output(std::time::Duration::from_secs(8));
    assert!(
        output.status.success(),
        "generic-owner fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error: hook changed staged source; review private patch:"),
        "successful hook edits to ignored staged-source data must be rejected: {stderr}"
    );
    let patch = fs::read(find_evidence_patch(&fixture.repository.join(".git")))
        .expect("retained ignored-source patch");
    let patch = String::from_utf8_lossy(&patch);
    assert!(patch.contains("diff --git a/ignored-extra.txt b/ignored-extra.txt"));
    assert!(patch.contains("ignored staged-source fixture"));
    assert_eq!(patch.matches("diff --git ").count(), 1, "{patch}");
    assert_eq!(
        fs::read(fixture.index()).expect("index after generic owner"),
        original_index
    );
    assert_eq!(
        fs::read(fixture.repository.join("tracked.txt")).expect("worktree after generic owner"),
        original_file
    );
    assert!(
        !fixture.repository.join("ignored-extra.txt").exists(),
        "gate output must stay in the private snapshot"
    );
}

#[test]
fn generic_owner_keeps_live_canaries_while_gate_uses_private_tools_and_artifacts() {
    let fixture = InstalledOwnerFixture::new(OwnerFixtureMode::IsolationGate);
    let before = capture_live_canary_state(&fixture.repository);
    let mut owner = fixture.spawn_owner("status", 0, 20_000, 20_000, 20_000);
    let release_gate = GateReleaseOnDrop(fixture.gate_release.clone());
    wait_for_owner_readiness(&mut owner, &fixture.gate_ready, &fixture.ticks);
    assert_eq!(
        capture_live_canary_state(&fixture.repository),
        before,
        "live Git/worktree canaries remain unchanged while the gate is blocked"
    );
    release_gate.release();
    let output = owner.wait_output(std::time::Duration::from_secs(8));
    assert!(
        output.status.success(),
        "generic-owner isolation gate failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        capture_live_canary_state(&fixture.repository),
        before,
        "live Git/worktree canaries remain unchanged after the gate"
    );
    assert_eq!(
        fs::read(fixture.live_artifacts.join("sentinel.bin"))
            .expect("external artifact sentinel remains"),
        b"preserve external artifact sentinel\n"
    );
    assert!(!fixture.repository.join("ignored-extra.txt").exists());
    for marker in [
        &fixture.poison_marker,
        &fixture.git_alias_file_marker,
        &fixture.git_alias_count_marker,
        &fixture.cargo_alias_marker,
    ] {
        assert!(!marker.exists(), "poison marker ran: {}", marker.display());
    }
    let report = fs::read_to_string(&fixture.probe_output).expect("gate tool report");
    let mut lines = report.lines();
    let target = PathBuf::from(lines.next().expect("private target directory"));
    let build = PathBuf::from(lines.next().expect("private build directory"));
    assert_eq!(
        PathBuf::from(lines.next().expect("selected compiler")),
        fixture.expected_rustc
    );
    assert!(
        PathBuf::from(lines.next().expect("selected Cargo"))
            .starts_with(target.parent().unwrap_or(&target)),
        "Cargo resolves from private qualified tooling: {report}"
    );
    assert!(
        lines.next().is_none(),
        "unexpected gate report fields: {report}"
    );
    assert!(!target.starts_with(&fixture.repository), "{report}");
    assert!(!build.starts_with(&fixture.repository), "{report}");
    assert!(
        !target.exists(),
        "successful gate scratch is removed: {report}"
    );
    assert!(
        !build.exists(),
        "successful gate scratch is removed: {report}"
    );
}

struct GateReleaseOnDrop(PathBuf);

impl GateReleaseOnDrop {
    fn release(&self) {
        fs::write(&self.0, b"release\n").expect("release blocked gate");
    }
}

impl Drop for GateReleaseOnDrop {
    fn drop(&mut self) {
        drop(fs::write(&self.0, b"release\n"));
    }
}

#[test]
fn generic_owner_observes_bootstrap_through_final_mise_cargo_and_gate_environment() {
    let clean = InstalledOwnerFixture::new(OwnerFixtureMode::BootstrapClean);
    let mut owner = clean.spawn_owner("status", 0, 20_000, 20_000, 20_000);
    let output = owner.wait_output(std::time::Duration::from_secs(8));
    assert!(
        output.status.success(),
        "clean generic owner failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(clean.cargo_marker).expect("Cargo environment marker"),
        "absent\n"
    );
    assert_eq!(
        fs::read_to_string(clean.gate_marker).expect("gate environment marker"),
        "absent\n"
    );

    let injected = InstalledOwnerFixture::new(OwnerFixtureMode::BootstrapInjected);
    let mut owner = injected.spawn_owner("qualification-error", 0, 20_000, 20_000, 20_000);
    let output = owner.wait_output(std::time::Duration::from_secs(8));
    assert!(
        output.status.success(),
        "bootstrap-injection driver failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("reintroduces RUSTC_BOOTSTRAP"),
        "injected Mise variable must be rejected during qualification: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!injected.cargo_marker.exists());
    assert!(!injected.gate_marker.exists());
}

#[test]
fn installed_owner_first_gate_signal_collects_tracked_and_new_file_fixes() {
    let fixture = InstalledOwnerFixture::new(OwnerFixtureMode::BlockedGate);
    let original_index = fs::read(fixture.index()).expect("read original index");
    let original_file =
        fs::read(fixture.repository.join("tracked.txt")).expect("read original worktree file");
    let mut owner = fixture.spawn_owner("status", 130, 20_000, 20_000, 20_000);
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
fn installed_owner_second_recovery_signal_cancels_new_file_capture() {
    let fixture = InstalledOwnerFixture::new(OwnerFixtureMode::BlockedGate);
    let original_index = fs::read(fixture.index()).expect("read original index");
    let original_file =
        fs::read(fixture.repository.join("tracked.txt")).expect("read original worktree file");
    let git_wrapper = fixture.recovery_git_wrapper();
    let mut owner = fixture.spawn_owner_with_git_program(
        "status",
        143,
        20_000,
        20_000,
        20_000,
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
        20_000,
        20_000,
        1_500,
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
    BootstrapClean,
    BootstrapInjected,
}

struct InstalledOwnerFixture {
    _temporary: tempfile::TempDir,
    repository: PathBuf,
    owner: PathBuf,
    path: std::ffi::OsString,
    ready: PathBuf,
    ticks: PathBuf,
    recovery_ready: PathBuf,
    recovery_ticks: PathBuf,
    cargo_marker: PathBuf,
    gate_marker: PathBuf,
    inherited_bootstrap: bool,
    probe_output: PathBuf,
    gate_ready: PathBuf,
    gate_release: PathBuf,
    poison_marker: PathBuf,
    git_alias_file_marker: PathBuf,
    git_alias_count_marker: PathBuf,
    cargo_alias_marker: PathBuf,
    poison_bin: PathBuf,
    git_alias_config: PathBuf,
    expected_rustc: PathBuf,
    live_artifacts: PathBuf,
    mode: OwnerFixtureMode,
}

impl InstalledOwnerFixture {
    fn new(mode: OwnerFixtureMode) -> Self {
        let (temporary, repository) = initialize_fixture_repository(mode);
        let scratch = repository.join(".git/f3-test");
        fs::create_dir(&scratch).expect("create private test support directory");
        fs::set_permissions(&scratch, fs::Permissions::from_mode(0o700))
            .expect("protect test support directory");
        let ready = scratch.join("ready");
        let ticks = scratch.join("ticks");
        let recovery_ready = scratch.join("recovery-ready");
        let recovery_ticks = scratch.join("recovery-ticks");
        let probe_output = scratch.join("gate-probe");
        let gate_ready = scratch.join("gate-ready");
        let gate_release = scratch.join("gate-release");
        let canaries = IsolationCanaries::create(&repository, &scratch, mode);
        let cargo_marker = scratch.join("cargo-environment");
        let gate_marker = scratch.join("gate-environment");
        let bin = scratch.join("bin");
        fs::create_dir(&bin).expect("create private fixture tools");
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o700))
            .expect("protect fixture tools");
        let expected_rustc = bin.join("rustc");
        write_owner_toolchain(&bin, &ready, &ticks, &gate_ready, mode);

        let owner = scratch.join("owner");
        fs::copy(
            env::current_exe().expect("resolve owner test executable"),
            &owner,
        )
        .expect("copy actual owner test harness into fixture");
        fs::set_permissions(&owner, fs::Permissions::from_mode(0o500))
            .expect("protect generic owner executable");

        Self {
            _temporary: temporary,
            repository,
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
            cargo_marker,
            gate_marker,
            inherited_bootstrap: matches!(
                mode,
                OwnerFixtureMode::BootstrapClean | OwnerFixtureMode::BootstrapInjected
            ),
            probe_output,
            gate_ready,
            gate_release,
            poison_marker: canaries.poison_marker,
            git_alias_file_marker: canaries.git_alias_file_marker,
            git_alias_count_marker: canaries.git_alias_count_marker,
            cargo_alias_marker: canaries.cargo_alias_marker,
            poison_bin: canaries.poison_bin,
            git_alias_config: canaries.git_alias_config,
            expected_rustc,
            live_artifacts: canaries.live_artifacts,
            mode,
        }
    }

    fn index(&self) -> PathBuf {
        self.repository.join(".git/index")
    }

    fn spawn_owner(
        &self,
        expectation: &str,
        expected_status: u8,
        setup_ms: u64,
        gate_ms: u64,
        recovery_ms: u64,
    ) -> OwnerChild {
        self.spawn_owner_with_git_program(
            expectation,
            expected_status,
            setup_ms,
            gate_ms,
            recovery_ms,
            None,
        )
    }

    fn spawn_owner_with_git_program(
        &self,
        expectation: &str,
        expected_status: u8,
        setup_ms: u64,
        gate_ms: u64,
        recovery_ms: u64,
        git_program: Option<&Path>,
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
            .env(OWNER_DRIVER_EXPECTATION, expectation)
            .env(OWNER_DRIVER_STATUS, expected_status.to_string())
            .env(OWNER_DRIVER_SETUP_MS, setup_ms.to_string())
            .env(OWNER_DRIVER_GATE_MS, gate_ms.to_string())
            .env(OWNER_DRIVER_RECOVERY_MS, recovery_ms.to_string())
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
        if self.inherited_bootstrap {
            command.env("RUSTC_BOOTSTRAP", "1");
        }
        if matches!(self.mode, OwnerFixtureMode::IsolationGate) {
            command
                .env(OWNER_DRIVER_PROBE_OUTPUT, &self.probe_output)
                .env(OWNER_DRIVER_GATE_READY, &self.gate_ready)
                .env(OWNER_DRIVER_GATE_RELEASE, &self.gate_release)
                .env("GIT_CONFIG", &self.git_alias_config)
                .env("GIT_CONFIG_COUNT", "1")
                .env("GIT_CONFIG_KEY_0", "alias.poison")
                .env(
                    "GIT_CONFIG_VALUE_0",
                    format!("!touch {}", shell_quote(&self.git_alias_count_marker)),
                )
                .env("CARGO_ALIAS_F3_POISON", "!touch marker")
                .env("CARGO", self.poison_bin.join("cargo"))
                .env("RUSTC", self.poison_bin.join("rustc"))
                .env("RUSTC_WRAPPER", self.poison_bin.join("rustc-wrapper"))
                .env("CARGO_TARGET_DIR", self.repository.join("target"))
                .env("CARGO_BUILD_BUILD_DIR", self.repository.join("target"));
        }
        let child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn selected immutable installed owner");
        OwnerChild { child: Some(child) }
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

fn initialize_fixture_repository(mode: OwnerFixtureMode) -> (tempfile::TempDir, PathBuf) {
    let home =
        fs::canonicalize(env::var_os("HOME").expect("HOME is set")).expect("canonicalizing HOME");
    let temporary = tempfile::Builder::new()
        .prefix("jackin-installed-owner-f3-")
        .tempdir_in(home)
        .expect("creating installed-owner repository");
    let repository = temporary
        .path()
        .canonicalize()
        .expect("canonical repository");
    let has_canaries = matches!(
        mode,
        OwnerFixtureMode::IgnoredSourceGate | OwnerFixtureMode::IsolationGate
    );
    git_command(&repository, &["init", "--quiet"]);
    git_command(
        &repository,
        &["config", "user.name", "Installed Owner Test"],
    );
    git_command(
        &repository,
        &["config", "user.email", "installed-owner@example.invalid"],
    );
    fs::write(
        repository.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"1.99.1\"\nprofile = \"minimal\"\ncomponents = [\"clippy\", \"rustfmt\"]\n",
    )
    .expect("write staged toolchain pin");
    fs::write(repository.join("tracked.txt"), b"base worktree bytes\n")
        .expect("write tracked fixture");
    if has_canaries {
        let ignore_rules: &[u8] = if matches!(mode, OwnerFixtureMode::IsolationGate) {
            b"ignored-extra.txt\nignored-canary.txt\ntarget\n"
        } else {
            b"ignored-extra.txt\n"
        };
        fs::write(repository.join(".gitignore"), ignore_rules).expect("write fixture ignore rule");
        git_command(
            &repository,
            &["add", "rust-toolchain.toml", "tracked.txt", ".gitignore"],
        );
    } else {
        git_command(&repository, &["add", "rust-toolchain.toml", "tracked.txt"]);
    }
    git_command(&repository, &["commit", "--quiet", "-m", "fixture"]);
    fs::write(repository.join("tracked.txt"), b"staged worktree bytes\n")
        .expect("stage a live worktree difference");
    git_command(&repository, &["add", "tracked.txt"]);
    (temporary, repository)
}

struct IsolationCanaries {
    poison_marker: PathBuf,
    git_alias_file_marker: PathBuf,
    git_alias_count_marker: PathBuf,
    cargo_alias_marker: PathBuf,
    poison_bin: PathBuf,
    git_alias_config: PathBuf,
    live_artifacts: PathBuf,
}

impl IsolationCanaries {
    fn create(repository: &Path, scratch: &Path, mode: OwnerFixtureMode) -> Self {
        let poison_marker = scratch.join("poison-ran");
        let git_alias_file_marker = scratch.join("git-alias-file-ran");
        let git_alias_count_marker = scratch.join("git-alias-count-ran");
        let cargo_alias_marker = scratch.join("cargo-alias-ran");
        let poison_bin = scratch.join("poison-bin");
        let git_alias_config = scratch.join("git-alias-config");
        let live_artifacts = scratch.join("live-artifacts");
        fs::create_dir(&live_artifacts).expect("create external artifact canary");
        fs::set_permissions(&live_artifacts, fs::Permissions::from_mode(0o700))
            .expect("protect external artifact canary");
        fs::write(
            live_artifacts.join("sentinel.bin"),
            b"preserve external artifact sentinel\n",
        )
        .expect("write external artifact sentinel");
        if matches!(mode, OwnerFixtureMode::IsolationGate) {
            fs::write(repository.join("tracked.txt"), b"live unstaged canary\n")
                .expect("write unstaged tracked canary");
            fs::write(
                repository.join("untracked-canary.txt"),
                b"live untracked canary\n",
            )
            .expect("write untracked canary");
            fs::write(
                repository.join("ignored-canary.txt"),
                b"live ignored canary\n",
            )
            .expect("write ignored canary");
            symlink(".git/f3-test/live-artifacts", repository.join("target"))
                .expect("create ignored target symlink canary");
            git_command(repository, &["check-ignore", "--quiet", "target"]);
            git_command(
                repository,
                &["check-ignore", "--quiet", "ignored-canary.txt"],
            );
            let stash = String::from_utf8(git_output(repository, &["stash", "create"]))
                .expect("decode fixture stash object");
            let stash = stash.trim();
            assert!(
                !stash.is_empty(),
                "fixture contains stashed tracked changes"
            );
            git_command(repository, &["stash", "store", "-m", "state canary", stash]);
        }
        fs::create_dir(&poison_bin).expect("create injected executable directory");
        fs::set_permissions(&poison_bin, fs::Permissions::from_mode(0o700))
            .expect("protect injected executable directory");
        let poison_body = format!(
            "#!/bin/sh\nprintf 'poison executed\\n' >> {}\nexit 99\n",
            shell_quote(&poison_marker)
        );
        for name in ["cargo", "rustc", "rustc-wrapper"] {
            write_executable(&poison_bin.join(name), poison_body.as_bytes());
        }
        fs::write(
            &git_alias_config,
            format!(
                "[alias]\nf3-poison-file = \"!touch {}\"\n",
                shell_quote(&git_alias_file_marker)
            ),
        )
        .expect("write injected file-based Git alias");
        Self {
            poison_marker,
            git_alias_file_marker,
            git_alias_count_marker,
            cargo_alias_marker,
            poison_bin,
            git_alias_config,
            live_artifacts,
        }
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
            std::thread::park_timeout(std::time::Duration::from_millis(10));
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
    gate_ready: &Path,
    mode: OwnerFixtureMode,
) {
    let ready = shell_quote(ready);
    let ticks = shell_quote(ticks);
    let holder = bin.join("holder");
    write_executable(
        &holder,
        b"#!/bin/sh\ntrap '' HUP INT TERM\nprintf x >> \"$2\"\n: > \"$1\"\ncount=0\nwhile [ \"$count\" -lt 400 ]; do\n  /bin/sleep 0.05 || :\n  printf x >> \"$2\"\n  count=$((count + 1))\ndone\n",
    );
    let cargo_marker = shell_quote(
        &bin.parent()
            .expect("fixture bin parent")
            .join("cargo-environment"),
    );
    let cargo_alias_marker = shell_quote(
        &bin.parent()
            .expect("fixture bin parent")
            .join("cargo-alias-ran"),
    );
    let rust_tools = format!(
        "#!/bin/sh\nname=${{0##*/}}\ncase \"$name\" in\n  cargo) if [ \"$1\" = f3-poison ]; then if [ \"${{CARGO_ALIAS_F3_POISON+x}}\" = x ]; then printf 'alias ran\\n' > {cargo_alias_marker}; exit 0; else exit 1; fi; fi; if [ \"${{RUSTC_BOOTSTRAP+x}}\" = x ]; then printf 'present\\n' > {cargo_marker}; else printf 'absent\\n' > {cargo_marker}; fi; printf 'cargo 1.99.1 (fixture)\\n' ;;\n  rustc) printf 'rustc 1.99.1 (fixture)\\nbinary: rustc\\ncommit-hash: 0123456789abcdef0123456789abcdef01234567\\nrelease: 1.99.1\\n' ;;\n  rustdoc) printf 'rustdoc 1.99.1 (fixture)\\nbinary: rustdoc\\ncommit-hash: 0123456789abcdef0123456789abcdef01234567\\nrelease: 1.99.1\\n' ;;\n  clippy-driver) printf 'clippy 0.1.99 (0123456789abcdef0123456789abcdef01234567 fixture)\\n' ;;\n  rustfmt) printf 'rustfmt 1.99.1-stable (0123456789abcdef0123456789abcdef01234567 fixture)\\n' ;;\n  *) exit 91 ;;\nesac\n"
    );
    for name in ["cargo", "rustc", "rustdoc", "clippy-driver", "rustfmt"] {
        write_executable(&bin.join(name), rust_tools.as_bytes());
    }

    let gate_marker = shell_quote(
        &bin.parent()
            .expect("fixture bin parent")
            .join("gate-environment"),
    );
    let gate_body = if matches!(mode, OwnerFixtureMode::IgnoredSourceGate) {
        "#!/bin/sh\nprintf 'ignored staged-source fixture\\n' > ignored-extra.txt\nexit 0\n"
            .to_owned()
    } else if matches!(mode, OwnerFixtureMode::IsolationGate) {
        format!(
            "#!/bin/sh\nset -eu\nif git f3-poison-file >/dev/null 2>&1; then exit 81; fi\nif git poison >/dev/null 2>&1; then exit 82; fi\nif cargo f3-poison >/dev/null 2>&1; then exit 83; fi\nif [ \"${{CARGO+x}}\" = x ]; then \"$CARGO\" --version || exit 84; fi\nif [ -n \"${{RUSTC_WRAPPER:-}}\" ]; then \"$RUSTC_WRAPPER\" \"$RUSTC\" --version || exit 85; else \"$RUSTC\" --version >/dev/null || exit 86; fi\ncargo --version >/dev/null || exit 87\nmkdir -p \"$CARGO_TARGET_DIR\" \"$CARGO_BUILD_BUILD_DIR\" || exit 88\nprintf 'private target\\n' > \"$CARGO_TARGET_DIR/artifact\"\nprintf 'private build\\n' > \"$CARGO_BUILD_BUILD_DIR/artifact\"\nprintf '%s\\n' \"$CARGO_TARGET_DIR\" \"$CARGO_BUILD_BUILD_DIR\" \"$RUSTC\" \"$(command -v cargo)\" > \"${{JACKIN_F3_OWNER_DRIVER_PROBE_OUTPUT}}\"\n: > {}\nprintf x >> {}\ncount=0\nwhile [ ! -e \"${{JACKIN_F3_OWNER_DRIVER_GATE_RELEASE}}\" ] && [ \"$count\" -lt 300 ]; do /bin/sleep 0.05; printf x >> {}; count=$((count + 1)); done\n[ -e \"${{JACKIN_F3_OWNER_DRIVER_GATE_RELEASE}}\" ] || exit 89\nexit 0\n",
            shell_quote(gate_ready),
            ticks,
            ticks,
        )
    } else if matches!(
        mode,
        OwnerFixtureMode::BootstrapClean | OwnerFixtureMode::BootstrapInjected
    ) {
        format!(
            "#!/bin/sh\nif [ \"${{RUSTC_BOOTSTRAP+x}}\" = x ]; then printf 'present\\n' > {gate_marker}; else printf 'absent\\n' > {gate_marker}; fi\nexit 0\n"
        )
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
        let injection = if matches!(mode, OwnerFixtureMode::BootstrapInjected) {
            "RUSTC_BOOTSTRAP=1; export RUSTC_BOOTSTRAP;"
        } else {
            ""
        };
        format!(
            "#!/bin/sh\ncase \"$1\" in\n  which)\n    if [ \"$2\" = hk ]; then printf '%s\\n' {}; else printf '%s\\n' /usr/bin/true; fi ;;\n  exec)\n    {}\n    case \"$*\" in\n      *'cargo --version'*) \"$4\" \"$5\" ;;\n      *'command -v cargo'*) printf '%s\\n' {} ;;\n      *'RUSTC_BOOTSTRAP'*) \"$4\" \"$5\" \"$6\" ;;\n      *) exit 92 ;;\n    esac ;;\n  *) exit 90 ;;\nesac\n",
            shell_quote(&bin.join("hk")),
            injection,
            shell_quote(&bin.join("cargo"))
        )
    };
    write_executable(&bin.join("mise"), mise_body.as_bytes());
    let rustup_body = format!(
        "#!/bin/sh\n[ \"$1\" = which ] && [ \"$3\" = 1.99.1 ] || exit 90\ncase \"$4\" in rustc|rustdoc|clippy-driver|rustfmt) printf '%s/%s\\n' {} \"$4\" ;; *) exit 91 ;; esac\n",
        shell_quote(bin)
    );
    write_executable(&bin.join("rustup"), rustup_body.as_bytes());
    let guard = PathBuf::from(env::var_os("HOME").expect("HOME is set"))
        .join(".local/bin/rust-build-guard");
    symlink(guard, bin.join("rust-build-guard")).expect("link trusted build guard");
}

fn write_executable(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).expect("write installed-owner fixture executable");
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .expect("make installed-owner fixture executable");
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
        std::thread::park_timeout(std::time::Duration::from_millis(10));
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

fn assert_ticks_stopped(ticks: &Path) {
    let after_return = fs::read(ticks).expect("read strict-child ticks after owner return");
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
    loop {
        assert_eq!(
            fs::read(ticks).expect("re-read strict-child ticks"),
            after_return,
            "strict supervisor must drain the process group before owner return"
        );
        let now = std::time::Instant::now();
        if now >= deadline {
            return;
        }
        std::thread::park_timeout(deadline.saturating_duration_since(now));
    }
}

fn find_evidence_patch(common_git_dir: &Path) -> PathBuf {
    let root = common_git_dir.join("jackin-hook-evidence");
    for invocation in crate::tests::read_dir_sorted(&root).expect("read retained evidence root") {
        let patch = invocation.path().join("fixes.patch");
        if patch.is_file() {
            return patch;
        }
    }
    panic!("installed owner did not persist a staged-source patch");
}
