// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! K7 record-path checks: the reducer reaches every fixed stage in
//! order through recorded observations, candidates are version-gated,
//! checkpoints verify the local source, and the acceptance gate
//! refuses anything short of passing tests, an acceptable review, a
//! current source, and a completed backup. Every record path is
//! version-gated, reviews bind to the recorded test policy, and
//! singular evidence is unique at the store level.

use std::path::Path;

use super::super::{
    Actor, CheckpointRecord, EffectKind, EffectStatus, ErrorKind, EvidenceRecord,
    InstanceInventory, Observation, RecordTestParams, RunStatus, RunView, SessionAttachment, Stage,
    WorkflowService, advance,
};
use super::support::Fixture;
use jackin_workflow_spec::digest::bundle_manifest_digest;
use jackin_workflow_spec::model::TestBundleFile;

fn view(status: RunStatus, stage: Stage) -> RunView {
    RunView {
        status,
        stage,
        round: 0,
        candidate_generation: 0,
    }
}

#[test]
fn k7_reducer_reaches_every_fixed_stage_in_order() {
    // prepare -> implement needs an explicit turn start.
    let started = advance(
        &view(RunStatus::Active, Stage::Prepare),
        2,
        &Observation::TurnStarted,
        Actor::Host,
    )
    .expect("turn start enters implement");
    assert_eq!(started.stage, Stage::Implement);
    assert_eq!(started.event_kind, "turn_started");

    // implement -> capture on turn end (existing edge).
    let captured = advance(
        &view(RunStatus::Active, Stage::Implement),
        2,
        &Observation::TurnEnded,
        Actor::Host,
    )
    .expect("turn end enters capture");
    assert_eq!(captured.stage, Stage::Capture);

    // capture -> test needs an explicit test start.
    let testing = advance(
        &view(RunStatus::Active, Stage::Capture),
        2,
        &Observation::TestStarted,
        Actor::Host,
    )
    .expect("test start enters test");
    assert_eq!(testing.stage, Stage::Test);
    assert_eq!(testing.event_kind, "test_started");

    // Wrong-stage starts refuse instead of skipping stages.
    for (stage, observation) in [
        (Stage::Implement, Observation::TurnStarted),
        (Stage::Prepare, Observation::TestStarted),
        (Stage::Test, Observation::TestStarted),
        (Stage::Review, Observation::TurnStarted),
    ] {
        advance(
            &view(RunStatus::Active, stage),
            2,
            &observation,
            Actor::Host,
        )
        .expect_err("out-of-stage starts must refuse");
    }

    // Starts never apply while paused, blocked, or terminal.
    for status in [RunStatus::Paused, RunStatus::NeedsOperator] {
        for observation in [Observation::TurnStarted, Observation::TestStarted] {
            advance(&view(status, Stage::Prepare), 2, &observation, Actor::Host)
                .expect_err("starts must refuse outside active runs");
        }
    }
    advance(
        &view(RunStatus::Accepted, Stage::Decide),
        2,
        &Observation::TurnStarted,
        Actor::Host,
    )
    .expect_err("starts must refuse terminal runs");
}

/// `turn_ended` leaves the implement stage only: every other stage
/// refuses instead of jumping to capture.
#[test]
fn k7_turn_end_refuses_outside_implement() {
    for stage in [
        Stage::Prepare,
        Stage::Capture,
        Stage::Test,
        Stage::Review,
        Stage::Decide,
    ] {
        let refusal = advance(
            &view(RunStatus::Active, stage),
            2,
            &Observation::TurnEnded,
            Actor::Host,
        )
        .expect_err("turn end outside implement must refuse");
        assert_eq!(refusal.reason, "turn end outside the implement stage");
    }
}

fn init_git_repo(dir: &Path) -> (String, String) {
    let run = |args: &[&str]| {
        let child = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn git in k7 fixture");
        let output = child.wait_with_output().expect("wait for k7 git");
        assert!(
            output.status.success(),
            "k7 git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "k7@example.com"]);
    run(&["config", "user.name", "k7"]);
    std::fs::write(
        dir.join("calculator.py"),
        "def add(a, b):\n    return a + b\n",
    )
    .expect("stage candidate source");
    run(&["add", "."]);
    run(&["commit", "-qm", "k7 candidate"]);
    (
        run(&["rev-parse", "HEAD"]),
        run(&["rev-parse", "HEAD^{tree}"]),
    )
}

/// The full record walk: session, candidate, turn, checkpoint, and
/// staged advances move a run prepare -> implement -> capture with
/// the candidate pinned and the source verified.
#[test]
fn k7_record_walk_reaches_capture_with_verified_checkpoint() {
    let fixture = Fixture::new();
    let service = fixture.open();
    let repo = tempfile::tempdir().expect("k7 repo tempdir");
    let (commit, tree) = init_git_repo(repo.path());

    let started = service
        .start_run("557632d5", 2, "key-k7-record")
        .expect("start k7 run");
    assert_eq!(started.stage, "prepare");

    let recorded = service
        .record_session(
            &SessionAttachment {
                run_id: &started.run_id,
                worker_id: "implementer",
                instance_id: Some("instance-k7"),
                capsule_epoch: None,
                session_id: None,
                process_identity: None,
                agent_profile: Some("claude"),
                auth_profile: Some("default-claude"),
            },
            started.version,
            "operator",
        )
        .expect("record implementer session");
    assert_eq!(recorded.stage, "prepare");
    let sessions = service
        .store()
        .load_sessions(&started.run_id)
        .expect("load sessions");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].worker_id, "implementer");
    assert_eq!(sessions[0].instance_id.as_deref(), Some("instance-k7"));

    let candidate = service
        .record_candidate(&started.run_id, "cand-r0", recorded.version, "operator")
        .expect("record candidate");
    assert_eq!(candidate.candidate.as_deref(), Some("cand-r0"));
    assert_eq!(candidate.candidate_generation, 1);

    // A moved version refuses the record.
    let stale = service.record_candidate(&started.run_id, "cand-r0b", recorded.version, "operator");
    assert_eq!(
        stale.expect_err("stale version must refuse").kind(),
        ErrorKind::StaleVersion
    );

    let implementing = service
        .advance_run(
            &started.run_id,
            candidate.version,
            "host",
            "turn_started",
            None,
        )
        .expect("advance turn_started");
    assert_eq!(implementing.stage, "implement");

    let captured = service
        .advance_run(
            &started.run_id,
            implementing.version,
            "host",
            "turn_ended",
            None,
        )
        .expect("advance turn_ended");
    assert_eq!(captured.stage, "capture");

    let checkpointed = service
        .record_checkpoint(
            &CheckpointRecord {
                run_id: &started.run_id,
                candidate: "cand-r0",
                commit: &commit,
                tree: &tree,
                source_manifest: "manifest-k7",
                local_path: &repo.path().to_string_lossy(),
                backup_state: "pending",
                preservation_hold: None,
            },
            captured.version,
        )
        .expect("record checkpoint");
    assert_eq!(checkpointed.stage, "capture");
    let checkpoints = service
        .store()
        .load_checkpoints(&started.run_id)
        .expect("load checkpoints");
    assert_eq!(checkpoints.len(), 1);
    assert_eq!(checkpoints[0].commit_hash, commit);
    assert_eq!(checkpoints[0].tree_hash, tree);

    // A checkpoint for another candidate refuses; a checkpoint that
    // moves the commit after verification refuses with a new
    // candidate required.
    let other = service.record_checkpoint(
        &CheckpointRecord {
            run_id: &started.run_id,
            candidate: "cand-r9",
            commit: &commit,
            tree: &tree,
            source_manifest: "manifest-k7",
            local_path: &repo.path().to_string_lossy(),
            backup_state: "pending",
            preservation_hold: None,
        },
        captured.version,
    );
    assert_eq!(
        other.expect_err("foreign candidate must refuse").kind(),
        ErrorKind::Invalid
    );
}

/// Checkpoints verify the local source: a drifted HEAD, a wrong tree,
/// and a dirty worktree all refuse the record.
#[test]
fn k7_checkpoint_refuses_drift_and_dirt() {
    let fixture = Fixture::new();
    let service = fixture.open();
    let repo = tempfile::tempdir().expect("k7 repo tempdir");
    let (commit, tree) = init_git_repo(repo.path());

    let started = service
        .start_run("557632d5", 2, "key-k7-drift")
        .expect("start drift run");
    let candidate = service
        .record_candidate(&started.run_id, "cand-r0", started.version, "operator")
        .expect("record candidate");

    let local = repo.path().to_string_lossy().into_owned();
    let record = |commit: &str, tree: &str| {
        service.record_checkpoint(
            &CheckpointRecord {
                run_id: &started.run_id,
                candidate: "cand-r0",
                commit,
                tree,
                source_manifest: "manifest-k7",
                local_path: &local,
                backup_state: "pending",
                preservation_hold: None,
            },
            candidate.version,
        )
    };

    // Wrong commit and wrong tree refuse.
    record("deadbeef", &tree).expect_err("drifted HEAD must refuse");
    record(&commit, "deadbeef").expect_err("wrong tree must refuse");

    // A dirty worktree refuses even with the right identity.
    std::fs::write(repo.path().join("calculator.py"), "dirty\n").expect("dirty the source");
    let dirty = record(&commit, &tree);
    assert_eq!(
        dirty.expect_err("dirty source must refuse").kind(),
        ErrorKind::Blocked
    );
}

/// `record_test` runs the frozen check in a pinned worker and binds
/// the host-observed verdict; a failing check records fail and the
/// advance moves back to implement for the repair turn.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "Single record walk covers early, tampered, passing, and double records in stage order"
)]
fn k7_record_test_binds_host_observed_verdict() {
    let fixture = Fixture::new();
    let service = fixture.open();
    let repo = tempfile::tempdir().expect("k7 repo tempdir");
    let (commit, tree) = init_git_repo(repo.path());
    let scratch = tempfile::tempdir().expect("k7 scratch tempdir");

    // Trusted check OUTSIDE the candidate repo: fail on the seeded
    // bug shape, pass on the repaired source.
    let bundle = scratch.path().join("bundle");
    std::fs::create_dir_all(&bundle).expect("bundle dir");
    let script = bundle.join("check_add.py");
    std::fs::write(
        &script,
        "import importlib.util, pathlib, sys\n\
         root = pathlib.Path(sys.argv[1])\n\
         spec = importlib.util.spec_from_file_location('candidate', root / 'calculator.py')\n\
         module = importlib.util.module_from_spec(spec)\n\
         spec.loader.exec_module(module)\n\
         assert module.add(2, 3) == 5, 'add is broken'\n\
         print('PASS')\n",
    )
    .expect("stage check script");
    let (bundle_files, bundle_digest) = bundle_binding(&script);

    let started = service
        .start_run("557632d5", 2, "key-k7-test")
        .expect("start test run");
    let candidate = service
        .record_candidate(&started.run_id, "cand-r0", started.version, "operator")
        .expect("record candidate");
    let implementing = service
        .advance_run(
            &started.run_id,
            candidate.version,
            "host",
            "turn_started",
            None,
        )
        .expect("turn started");
    let captured = service
        .advance_run(
            &started.run_id,
            implementing.version,
            "host",
            "turn_ended",
            None,
        )
        .expect("turn ended");
    // Tests belong to the test stage: recording at capture refuses.
    let early = service.record_test(
        &started.run_id,
        captured.version,
        &RecordTestParams {
            check_program: "python3",
            check_script: &script,
            bundle_digest: &bundle_digest,
            bundle_files: &bundle_files,
            repo: Some(repo.path()),
            scratch: scratch.path(),
            timeout_secs: 120,
        },
    );
    assert_eq!(
        early.expect_err("early test record must refuse").kind(),
        ErrorKind::Invalid
    );
    service
        .record_checkpoint(
            &CheckpointRecord {
                run_id: &started.run_id,
                candidate: "cand-r0",
                commit: &commit,
                tree: &tree,
                source_manifest: "manifest-k7",
                local_path: &repo.path().to_string_lossy(),
                backup_state: "pending",
                preservation_hold: None,
            },
            captured.version,
        )
        .expect("record checkpoint");
    let testing = service
        .advance_run(
            &started.run_id,
            captured.version,
            "host",
            "test_started",
            None,
        )
        .expect("test started");
    assert_eq!(testing.stage, "test");

    // A tampered script refuses before any worker launches.
    let tampered = service.record_test(
        &started.run_id,
        testing.version,
        &RecordTestParams {
            check_program: "python3",
            check_script: &script,
            bundle_digest: "deadbeef",
            bundle_files: &bundle_files,
            repo: Some(repo.path()),
            scratch: scratch.path(),
            timeout_secs: 120,
        },
    );
    assert_eq!(
        tampered.expect_err("tampered script must refuse").kind(),
        ErrorKind::Invalid
    );

    let (summary, recorded) = service
        .record_test(
            &started.run_id,
            testing.version,
            &RecordTestParams {
                check_program: "python3",
                check_script: &script,
                bundle_digest: &bundle_digest,
                bundle_files: &bundle_files,
                repo: Some(repo.path()),
                scratch: scratch.path(),
                timeout_secs: 120,
            },
        )
        .expect("record passing test");
    assert!(recorded.passed);
    assert_eq!(recorded.exit_code, 0);
    assert!(!recorded.policy_hash.is_empty());
    assert!(!recorded.artifact_digest.is_empty());
    assert!(recorded.worker_id.contains("cand-r0"));
    assert_eq!(summary.stage, "test");

    // One verdict per candidate: a second record refuses.
    let again = service.record_test(
        &started.run_id,
        testing.version,
        &RecordTestParams {
            check_program: "python3",
            check_script: &script,
            bundle_digest: &bundle_digest,
            bundle_files: &bundle_files,
            repo: Some(repo.path()),
            scratch: scratch.path(),
            timeout_secs: 120,
        },
    );
    assert_eq!(
        again.expect_err("double record must refuse").kind(),
        ErrorKind::Invalid
    );

    // A passing verdict advances the pipeline into review.
    let reviewed = service
        .advance_run(
            &started.run_id,
            summary.version,
            "host",
            "test_passed",
            None,
        )
        .expect("advance test_passed");
    assert_eq!(reviewed.stage, "review");
    assert_eq!(reviewed.round, 0);
}

/// A run staged to the test stage with one trusted check script:
/// the shared setup for the bundle-binding regression tests.
struct TestStage {
    _fixture: Fixture,
    _repo: tempfile::TempDir,
    scratch: tempfile::TempDir,
    service: WorkflowService,
    run_id: String,
    version: i64,
    script: std::path::PathBuf,
    bundle_files: Vec<TestBundleFile>,
    bundle_digest: String,
}

fn stage_test_run(key: &str, script_text: &str) -> TestStage {
    let fixture = Fixture::new();
    let service = fixture.open();
    let repo = tempfile::tempdir().expect("binding repo tempdir");
    let (commit, tree) = init_git_repo(repo.path());
    let scratch = tempfile::tempdir().expect("binding scratch tempdir");
    let script = scratch.path().join("check_add.py");
    std::fs::write(&script, script_text).expect("stage check script");
    let (bundle_files, bundle_digest) = bundle_binding(&script);
    let started = service
        .start_run("557632d5", 2, key)
        .expect("start binding run");
    let run_id = started.run_id.clone();
    let candidate = service
        .record_candidate(&run_id, "cand-r0", started.version, "operator")
        .expect("record candidate");
    let implementing = service
        .advance_run(&run_id, candidate.version, "host", "turn_started", None)
        .expect("turn started");
    let captured = service
        .advance_run(&run_id, implementing.version, "host", "turn_ended", None)
        .expect("turn ended");
    let local = repo.path().to_string_lossy().into_owned();
    service
        .record_checkpoint(
            &CheckpointRecord {
                run_id: &run_id,
                candidate: "cand-r0",
                commit: &commit,
                tree: &tree,
                source_manifest: "manifest-k7",
                local_path: &local,
                backup_state: "pending",
                preservation_hold: None,
            },
            captured.version,
        )
        .expect("record checkpoint");
    let testing = service
        .advance_run(&run_id, captured.version, "host", "test_started", None)
        .expect("test started");
    TestStage {
        _fixture: fixture,
        _repo: repo,
        scratch,
        service,
        run_id,
        version: testing.version,
        script,
        bundle_files,
        bundle_digest,
    }
}

/// `record_test` refuses a foreign script: the claimed manifest
/// digest is correct, but the executed script matches no manifest
/// member, so the record refuses before any worker launches.
#[test]
fn k7_record_test_refuses_foreign_script() {
    let stage = stage_test_run("key-k7-foreign-script", "print('PASS')\n");
    let foreign = stage.script.with_file_name("foreign.py");
    std::fs::write(&foreign, "print('FOREIGN')\n").expect("stage foreign script");
    let refused = stage.service.record_test(
        &stage.run_id,
        stage.version,
        &RecordTestParams {
            check_program: "python3",
            check_script: &foreign,
            bundle_digest: &stage.bundle_digest,
            bundle_files: &stage.bundle_files,
            repo: None,
            scratch: stage.scratch.path(),
            timeout_secs: 120,
        },
    );
    assert_eq!(
        refused.expect_err("foreign script must refuse").kind(),
        ErrorKind::Invalid
    );
}

/// `record_test` refuses a tampered manifest: the executed script
/// is a member of the claimed files, but the files do not recompute
/// to the claimed manifest digest.
#[test]
fn k7_record_test_refuses_tampered_manifest() {
    let stage = stage_test_run("key-k7-tampered-manifest", "print('PASS')\n");
    let mut tampered = stage.bundle_files.clone();
    tampered.push(TestBundleFile {
        logical_path: "ghost.py".to_owned(),
        digest: "0".repeat(64),
    });
    let recomputed = bundle_manifest_digest(
        tampered
            .iter()
            .map(|file| (file.logical_path.as_str(), file.digest.as_str())),
    );
    assert_ne!(
        recomputed, stage.bundle_digest,
        "tampered files must recompute elsewhere"
    );
    let refused = stage.service.record_test(
        &stage.run_id,
        stage.version,
        &RecordTestParams {
            check_program: "python3",
            check_script: &stage.script,
            bundle_digest: &stage.bundle_digest,
            bundle_files: &tampered,
            repo: None,
            scratch: stage.scratch.path(),
            timeout_secs: 120,
        },
    );
    assert_eq!(
        refused.expect_err("tampered manifest must refuse").kind(),
        ErrorKind::Invalid
    );
}

/// `record_test` accepts any manifest member of a multi-file
/// bundle: the executed script matches one entry while the manifest
/// covers two files.
#[test]
fn k7_record_test_accepts_multi_file_bundle_member() {
    let stage = stage_test_run("key-k7-multi-file", "print('PASS')\n");
    std::fs::write(stage.scratch.path().join("helper.py"), b"helper")
        .expect("stage helper bundle file");
    let mut files = stage.bundle_files.clone();
    files.push(TestBundleFile {
        logical_path: "helper.py".to_owned(),
        digest: sha256_hex(b"helper"),
    });
    let manifest = bundle_manifest_digest(
        files
            .iter()
            .map(|file| (file.logical_path.as_str(), file.digest.as_str())),
    );
    let (summary, recorded) = stage
        .service
        .record_test(
            &stage.run_id,
            stage.version,
            &RecordTestParams {
                check_program: "python3",
                check_script: &stage.script,
                bundle_digest: &manifest,
                bundle_files: &files,
                repo: None,
                scratch: stage.scratch.path(),
                timeout_secs: 120,
            },
        )
        .expect("multi-file member must record");
    assert!(recorded.passed);
    assert_eq!(summary.stage, "test");
}

#[test]
fn k7_record_test_timeout_removes_sandbox_container() {
    let stage = stage_test_run("key-k7-test-timeout", "import time\ntime.sleep(30)\n");
    let result = stage.service.record_test(
        &stage.run_id,
        stage.version,
        &RecordTestParams {
            check_program: "python3",
            check_script: &stage.script,
            bundle_digest: &stage.bundle_digest,
            bundle_files: &stage.bundle_files,
            repo: None,
            scratch: stage.scratch.path(),
            timeout_secs: 2,
        },
    );
    assert_eq!(
        result
            .expect_err("timeout must be recorded as infrastructure failure")
            .kind(),
        ErrorKind::Blocked
    );
    let worker = format!("test-{}-r0-cand-r0", stage.run_id);
    let filter = format!("label=jackin.workflow.worker={worker}");
    let containers = super::super::verify::run_host_command_output_until(
        std::process::Command::new("docker").args(["ps", "-aq", "--filter", &filter]),
        std::time::Instant::now() + std::time::Duration::from_secs(2),
    )
    .expect("list timed out test containers")
    .expect("Docker container listing timed out");
    assert!(
        containers.status.success(),
        "Docker container listing failed"
    );
    assert!(
        containers.stdout.is_empty(),
        "timed-out sandbox container remains: {}",
        String::from_utf8_lossy(&containers.stdout)
    );
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Manifest binding for one staged check script: the per-file
/// manifest plus the manifest digest `record_test` must be told.
/// Mirrors the plan's frozen bundle for a single-file bundle.
fn bundle_binding(script: &Path) -> (Vec<TestBundleFile>, String) {
    let digest = sha256_hex(&std::fs::read(script).expect("read check script"));
    let logical_path = script
        .file_name()
        .expect("check script has a file name")
        .to_string_lossy()
        .into_owned();
    let files = vec![TestBundleFile {
        logical_path,
        digest,
    }];
    let manifest = bundle_manifest_digest(
        files
            .iter()
            .map(|file| (file.logical_path.as_str(), file.digest.as_str())),
    );
    (files, manifest)
}

/// A run driven to decide with passing tests and an acceptable
/// review, backup still open: the shared setup for the acceptance
/// gate tests.
struct DecideSetup {
    _fixture: Fixture,
    _repo: tempfile::TempDir,
    _scratch: tempfile::TempDir,
    service: WorkflowService,
    run_id: String,
    policy_hash: String,
    decide_version: i64,
}

fn drive_to_decide(key: &str) -> DecideSetup {
    let fixture = Fixture::new();
    let service = fixture.open();
    let repo = tempfile::tempdir().expect("k7 repo tempdir");
    let (commit, tree) = init_git_repo(repo.path());
    let scratch = tempfile::tempdir().expect("k7 scratch tempdir");
    let bundle = scratch.path().join("bundle");
    std::fs::create_dir_all(&bundle).expect("bundle dir");
    let script = bundle.join("check_add.py");
    std::fs::write(&script, "print('PASS')\n").expect("stage check script");
    let (bundle_files, bundle_digest) = bundle_binding(&script);

    let started = service
        .start_run("557632d5", 2, key)
        .expect("start accept run");
    let run_id = started.run_id.clone();
    let candidate = service
        .record_candidate(&run_id, "cand-r0", started.version, "operator")
        .expect("record candidate");
    let implementing = service
        .advance_run(&run_id, candidate.version, "host", "turn_started", None)
        .expect("turn started");
    let captured = service
        .advance_run(&run_id, implementing.version, "host", "turn_ended", None)
        .expect("turn ended");
    let local = repo.path().to_string_lossy().into_owned();
    service
        .record_checkpoint(
            &CheckpointRecord {
                run_id: &run_id,
                candidate: "cand-r0",
                commit: &commit,
                tree: &tree,
                source_manifest: "manifest-k7",
                local_path: &local,
                backup_state: "pending",
                preservation_hold: None,
            },
            captured.version,
        )
        .expect("record checkpoint");
    let testing = service
        .advance_run(&run_id, captured.version, "host", "test_started", None)
        .expect("test started");
    let (summary, recorded) = service
        .record_test(
            &run_id,
            testing.version,
            &RecordTestParams {
                check_program: "python3",
                check_script: &script,
                bundle_digest: &bundle_digest,
                bundle_files: &bundle_files,
                repo: Some(repo.path()),
                scratch: scratch.path(),
                timeout_secs: 120,
            },
        )
        .expect("record test");
    assert!(recorded.passed);
    // Recording a verdict moves no run position: the row version is
    // unchanged by the record itself.
    assert_eq!(summary.version, testing.version);
    let reviewed = service
        .advance_run(&run_id, summary.version, "host", "test_passed", None)
        .expect("test passed");
    assert_eq!(reviewed.stage, "review");

    // A review naming another candidate refuses validation.
    let foreign = scratch.path().join("foreign.json");
    std::fs::write(
        &foreign,
        format!(
            r#"{{"schema_version":1,"candidate_id":"cand-r9","commit":"{commit}","tree":"{tree}","status":"pass","findings":[]}}"#
        ),
    )
    .expect("stage foreign review");
    service
        .record_review(&run_id, reviewed.version, &foreign, &recorded.policy_hash)
        .expect_err("foreign review must refuse");

    // An acceptable review records; a second verdict refuses.
    let review_file = scratch.path().join("review.json");
    std::fs::write(
        &review_file,
        format!(
            r#"{{"schema_version":1,"candidate_id":"cand-r0","commit":"{commit}","tree":"{tree}","status":"pass","findings":[]}}"#
        ),
    )
    .expect("stage review");
    let (summary, review) = service
        .record_review(
            &run_id,
            reviewed.version,
            &review_file,
            &recorded.policy_hash,
        )
        .expect("record review");
    assert!(review.acceptable);
    assert!(!review.artifact_digest.is_empty());
    service
        .record_review(
            &run_id,
            reviewed.version,
            &review_file,
            &recorded.policy_hash,
        )
        .expect_err("double review must refuse");

    let decided = service
        .advance_run(&run_id, summary.version, "host", "review_passed", None)
        .expect("review passed");
    assert_eq!(decided.stage, "decide");
    let policy_hash = recorded.policy_hash.clone();
    let decide_version = decided.version;
    DecideSetup {
        _fixture: fixture,
        _repo: repo,
        _scratch: scratch,
        service,
        run_id,
        policy_hash,
        decide_version,
    }
}

fn run_git(dir: &Path, args: &[&str]) {
    let child = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn git in k7 fixture");
    let output = child.wait_with_output().expect("wait for k7 git");
    assert!(
        output.status.success(),
        "k7 git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Confirm a bare remote at its ref after pushing the candidate, for
/// the green acceptance path.
fn confirm_backup(setup: &DecideSetup) -> i64 {
    let detail = setup
        .service
        .run_detail(&setup.run_id, -1, 0)
        .expect("load run detail");
    let checkpoint = detail
        .checkpoints
        .iter()
        .find(|row| Some(row.candidate.as_str()) == detail.summary.candidate.as_deref())
        .expect("checkpoint exists");
    let bare = tempfile::tempdir().expect("bare remote tempdir");
    let bare_path = bare.path().join("remote.git");
    run_git(bare.path(), &["init", "--bare", "-q", "remote.git"]);
    run_git(
        Path::new(&checkpoint.local_path),
        &[
            "push",
            &bare_path.to_string_lossy(),
            &format!("{}:refs/jackin/k7", checkpoint.commit_hash),
        ],
    );
    let backed = setup
        .service
        .record_backup(
            &setup.run_id,
            setup.decide_version,
            &bare_path.to_string_lossy(),
            "refs/jackin/k7",
            "operator",
        )
        .expect("record backup");
    assert_eq!(backed.stage, "decide");
    backed.version
}

/// The acceptance gate refuses a missing backup, an unpushed ref,
/// and a changed policy before anything can mark a run accepted.
#[test]
fn k7_acceptance_gate_refuses_everything_short_of_green() {
    let setup = drive_to_decide("key-k7-refusals");
    let service = &setup.service;

    // No backup yet: acceptance refuses with the backup reason.
    let no_backup = service.accept_run(&setup.run_id, setup.decide_version, &setup.policy_hash);
    assert!(
        format!("{}", no_backup.expect_err("backup-less accept must refuse")).contains("backup"),
        "backup refusal names the backup"
    );

    // A backup ref that does not resolve to the candidate refuses.
    let bare = tempfile::tempdir().expect("bare remote tempdir");
    run_git(bare.path(), &["init", "--bare", "-q", "remote.git"]);
    let bare_path = bare.path().join("remote.git");
    service
        .record_backup(
            &setup.run_id,
            setup.decide_version,
            &bare_path.to_string_lossy(),
            "refs/jackin/k7",
            "operator",
        )
        .expect_err("unpushed backup ref must refuse");

    // A changed policy hash refuses as stale evidence.
    let stale = service.accept_run(&setup.run_id, setup.decide_version, "changed-policy");
    assert!(
        format!("{}", stale.expect_err("changed policy must refuse")).contains("policy"),
        "stale policy refusal names the policy"
    );
}

/// Only the full green set accepts: passing tests, an acceptable
/// review, a current source, and a completed backup. Terminal runs
/// refuse new records and advances afterwards.
#[test]
fn k7_acceptance_gate_accepts_the_full_green_set() {
    let setup = drive_to_decide("key-k7-green");
    let service = &setup.service;
    let backed_version = confirm_backup(&setup);
    let accepted = service
        .accept_run(&setup.run_id, backed_version, &setup.policy_hash)
        .expect("accept the green run");
    assert_eq!(accepted.status, "accepted");
    assert_eq!(accepted.stage, "decide");

    // Terminal runs refuse new records and advances.
    service
        .record_candidate(&setup.run_id, "cand-r1", accepted.version, "operator")
        .expect_err("terminal record must refuse");
    service
        .advance_run(&setup.run_id, accepted.version, "host", "test_failed", None)
        .expect_err("terminal advance must refuse");
}

/// Unresolved delivery uncertainty blocks acceptance even when
/// every other leg is green: the operator continued from the current
/// session, but the original command stays marked uncertain.
#[test]
fn k7_acceptance_gate_refuses_unresolved_uncertainty() {
    let fixture = Fixture::new();
    let service = fixture.open();
    let repo = tempfile::tempdir().expect("k7 repo tempdir");
    let (commit, tree) = init_git_repo(repo.path());
    let scratch = tempfile::tempdir().expect("k7 scratch tempdir");
    let bundle = scratch.path().join("bundle");
    std::fs::create_dir_all(&bundle).expect("bundle dir");
    let script = bundle.join("check_add.py");
    std::fs::write(&script, "print('PASS')\n").expect("stage check script");
    let (bundle_files, bundle_digest) = bundle_binding(&script);

    let started = service
        .start_run("557632d5", 2, "key-k7-uncertain-accept")
        .expect("start uncertain run");
    let run_id = started.run_id.clone();
    let candidate = service
        .record_candidate(&run_id, "cand-r0", started.version, "operator")
        .expect("record candidate");
    let implementing = service
        .advance_run(&run_id, candidate.version, "host", "turn_started", None)
        .expect("turn started");
    assert_eq!(implementing.stage, "implement");
    // The implementer prompt is dispatched but its receipt is lost.
    let (command_id, _) = service
        .begin_dispatch(&run_id, "session:7", "implement the fix", 0, 0)
        .expect("begin dispatch");
    drop(command_id);
    let recovered = service
        .recover(&InstanceInventory::default())
        .expect("recover uncertain run");
    assert!(
        recovered
            .actions
            .iter()
            .any(|action| action.contains("delivery_unknown")),
        "recovery marks the interrupted dispatch"
    );
    // The operator inspects the original session and continues from
    // its current state; the original command stays uncertain.
    let resumed = service
        .resolve_input(
            &run_id,
            &service
                .store()
                .commands_in_state(&run_id, super::super::CommandState::DeliveryUnknown)
                .expect("list uncertain")
                .iter()
                .map(|command| command.command_id.clone())
                .collect::<Vec<_>>()[0],
            super::super::OperatorDecision::ContinueFromCurrent,
            "operator",
            "session 7 shows the repaired source on screen",
        )
        .expect("resolve inspected input");
    assert_eq!(resumed.status, "active");
    let captured = service
        .advance_run(&run_id, resumed.version, "host", "turn_ended", None)
        .expect("turn ended");
    let local = repo.path().to_string_lossy().into_owned();
    service
        .record_checkpoint(
            &CheckpointRecord {
                run_id: &run_id,
                candidate: "cand-r0",
                commit: &commit,
                tree: &tree,
                source_manifest: "manifest-k7",
                local_path: &local,
                backup_state: "pending",
                preservation_hold: None,
            },
            captured.version,
        )
        .expect("record checkpoint");
    let testing = service
        .advance_run(&run_id, captured.version, "host", "test_started", None)
        .expect("test started");
    let (summary, recorded) = service
        .record_test(
            &run_id,
            testing.version,
            &RecordTestParams {
                check_program: "python3",
                check_script: &script,
                bundle_digest: &bundle_digest,
                bundle_files: &bundle_files,
                repo: Some(repo.path()),
                scratch: scratch.path(),
                timeout_secs: 120,
            },
        )
        .expect("record test");
    assert_eq!(testing.stage, "test");
    let reviewed = service
        .advance_run(&run_id, summary.version, "host", "test_passed", None)
        .expect("test passed");
    assert_eq!(reviewed.stage, "review");
    let review_file = bundle.join("review.json");
    std::fs::write(
        &review_file,
        format!(
            r#"{{"schema_version":1,"candidate_id":"cand-r0","commit":"{commit}","tree":"{tree}","status":"pass","findings":[]}}"#
        ),
    )
    .expect("stage review");
    let (summary, _) = service
        .record_review(
            &run_id,
            reviewed.version,
            &review_file,
            &recorded.policy_hash,
        )
        .expect("record review");
    let decided = service
        .advance_run(&run_id, summary.version, "host", "review_passed", None)
        .expect("review passed");
    let effect = service
        .plan_effect(&run_id, EffectKind::Backup, "uncertain:backup")
        .expect("plan backup effect");
    service
        .store()
        .set_effect(
            &run_id,
            &effect.operation_id,
            EffectStatus::Done,
            Some("remote uncertain/k7"),
            None,
        )
        .expect("mark backup done");
    let refused = service.accept_run(&run_id, decided.version, &recorded.policy_hash);
    assert!(
        format!("{}", refused.expect_err("uncertainty must block accept")).contains("uncertainty"),
        "uncertainty refusal names the uncertainty"
    );
}

/// A failing check with no repair budget left fails the run, and the
/// terminal run refuses acceptance.
#[test]
fn k7_acceptance_gate_refuses_failed_verification() {
    let fixture = Fixture::new();
    let service = fixture.open();
    let repo = tempfile::tempdir().expect("k7 repo tempdir");
    let (commit, tree) = init_git_repo(repo.path());
    let scratch = tempfile::tempdir().expect("k7 scratch tempdir");
    let bundle = scratch.path().join("bundle");
    std::fs::create_dir_all(&bundle).expect("bundle dir");
    let script = bundle.join("check_fail.py");
    std::fs::write(&script, "import sys\nsys.exit(3)\n").expect("stage failing check");
    let (bundle_files, bundle_digest) = bundle_binding(&script);

    let started = service
        .start_run("557632d5", 0, "key-k7-failed-accept")
        .expect("start failing run");
    let run_id = started.run_id.clone();
    let candidate = service
        .record_candidate(&run_id, "cand-r0", started.version, "operator")
        .expect("record candidate");
    let implementing = service
        .advance_run(&run_id, candidate.version, "host", "turn_started", None)
        .expect("turn started");
    let captured = service
        .advance_run(&run_id, implementing.version, "host", "turn_ended", None)
        .expect("turn ended");
    let local = repo.path().to_string_lossy().into_owned();
    service
        .record_checkpoint(
            &CheckpointRecord {
                run_id: &run_id,
                candidate: "cand-r0",
                commit: &commit,
                tree: &tree,
                source_manifest: "manifest-k7",
                local_path: &local,
                backup_state: "pending",
                preservation_hold: None,
            },
            captured.version,
        )
        .expect("record checkpoint");
    let testing = service
        .advance_run(&run_id, captured.version, "host", "test_started", None)
        .expect("test started");
    let (summary, recorded) = service
        .record_test(
            &run_id,
            testing.version,
            &RecordTestParams {
                check_program: "python3",
                check_script: &script,
                bundle_digest: &bundle_digest,
                bundle_files: &bundle_files,
                repo: Some(repo.path()),
                scratch: scratch.path(),
                timeout_secs: 120,
            },
        )
        .expect("record failing test");
    assert!(!recorded.passed);
    // No repair budget: the failure is terminal, never accepted.
    assert_eq!(testing.stage, "test");
    let failed = service
        .advance_run(&run_id, summary.version, "host", "test_failed", None)
        .expect("advance test_failed");
    assert_eq!(failed.status, "failed");
    service
        .accept_run(&run_id, failed.version, &recorded.policy_hash)
        .expect_err("terminal failed run must refuse accept");
}

/// Acceptance admits only the host actor, a current row version, and
/// a fresh candidate identity: worker-actor acceptance, a moved
/// version, and a re-recorded verified identity all refuse.
#[test]
fn k7_acceptance_gate_refuses_wrong_actor_stale_version_and_rerecord() {
    let setup = drive_to_decide("key-k7-actor-version");
    let service = &setup.service;

    service
        .advance_run(
            &setup.run_id,
            setup.decide_version,
            "worker",
            "verified_current",
            Some(&setup.policy_hash),
        )
        .expect_err("worker acceptance must refuse");

    service
        .accept_run(&setup.run_id, setup.decide_version - 1, &setup.policy_hash)
        .expect_err("stale version must refuse");

    service
        .record_candidate(&setup.run_id, "cand-r0", setup.decide_version, "operator")
        .expect_err("re-recording a verified identity must refuse");
}

/// A drifted source refuses acceptance at the current-source check.
#[test]
fn k7_acceptance_gate_refuses_drifted_source() {
    let fixture = Fixture::new();
    let service = fixture.open();
    let repo = tempfile::tempdir().expect("k7 repo tempdir");
    let (commit, tree) = init_git_repo(repo.path());
    let scratch = tempfile::tempdir().expect("k7 scratch tempdir");
    let drifted = drift_copy(&service, repo.path(), &commit, &tree, scratch.path());
    assert_eq!(drifted.kind(), ErrorKind::Blocked);
}

/// A second run on a repo whose source drifted after its checkpoint
/// refuses acceptance at the current-source check.
fn drift_copy(
    service: &WorkflowService,
    repo: &Path,
    commit: &str,
    tree: &str,
    scratch: &Path,
) -> super::super::WorkflowError {
    let bundle = scratch.join("bundle2");
    std::fs::create_dir_all(&bundle).expect("bundle dir");
    let script = bundle.join("check_add.py");
    std::fs::write(&script, "print('PASS')\n").expect("stage check script");
    let (bundle_files, bundle_digest) = bundle_binding(&script);

    let started = service
        .start_run("557632d5", 2, "key-k7-drifted-accept")
        .expect("start drifted run");
    let candidate = service
        .record_candidate(&started.run_id, "cand-r0", started.version, "operator")
        .expect("record candidate");
    let implementing = service
        .advance_run(
            &started.run_id,
            candidate.version,
            "host",
            "turn_started",
            None,
        )
        .expect("turn started");
    let captured = service
        .advance_run(
            &started.run_id,
            implementing.version,
            "host",
            "turn_ended",
            None,
        )
        .expect("turn ended");
    service
        .record_checkpoint(
            &CheckpointRecord {
                run_id: &started.run_id,
                candidate: "cand-r0",
                commit,
                tree,
                source_manifest: "manifest-k7",
                local_path: &repo.to_string_lossy(),
                backup_state: "pending",
                preservation_hold: None,
            },
            captured.version,
        )
        .expect("record checkpoint");
    let testing = service
        .advance_run(
            &started.run_id,
            captured.version,
            "host",
            "test_started",
            None,
        )
        .expect("test started");
    let (_, recorded) = service
        .record_test(
            &started.run_id,
            testing.version,
            &RecordTestParams {
                check_program: "python3",
                check_script: &script,
                bundle_digest: &bundle_digest,
                bundle_files: &bundle_files,
                repo: Some(repo),
                scratch,
                timeout_secs: 120,
            },
        )
        .expect("record test");
    let passed = service
        .advance_run(
            &started.run_id,
            testing.version,
            "host",
            "test_passed",
            None,
        )
        .expect("test passed");
    let review_file = bundle.join("review.json");
    std::fs::write(
        &review_file,
        format!(
            r#"{{"schema_version":1,"candidate_id":"cand-r0","commit":"{commit}","tree":"{tree}","status":"pass","findings":[]}}"#
        ),
    )
    .expect("stage review");
    let (summary, _) = service
        .record_review(
            &started.run_id,
            passed.version,
            &review_file,
            &recorded.policy_hash,
        )
        .expect("record review");
    let decided = service
        .advance_run(
            &started.run_id,
            summary.version,
            "host",
            "review_passed",
            None,
        )
        .expect("review passed");
    // A backup effect marked done by hand satisfies the backup leg so
    // the drift leg decides alone.
    let effect = service
        .plan_effect(&started.run_id, EffectKind::Backup, "drift:backup")
        .expect("plan backup effect");
    service
        .adopt_effect(&started.run_id, &effect.operation_id, "remote drift/k7")
        .expect("adopt backup effect");
    service
        .store()
        .set_effect(
            &started.run_id,
            &effect.operation_id,
            EffectStatus::Done,
            Some("remote drift/k7"),
            None,
        )
        .expect("mark backup done");
    // Drift the source after verification: an extra commit moves HEAD.
    std::fs::write(
        repo.join("calculator.py"),
        "def add(a, b):\n    return a - b\n",
    )
    .expect("drift the source");
    run_git(repo, &["commit", "-qam", "drift"]);
    service
        .accept_run(&started.run_id, decided.version, &recorded.policy_hash)
        .expect_err("drifted source must refuse")
}

/// Every record path names the run row version it acted on: a moved
/// row refuses with `StaleVersion` before any side row is written,
/// whatever the stage or payload.
#[test]
fn k7_record_paths_refuse_moved_versions() {
    let fixture = Fixture::new();
    let service = fixture.open();
    let repo = tempfile::tempdir().expect("k7 repo tempdir");
    let (commit, tree) = init_git_repo(repo.path());
    let scratch = tempfile::tempdir().expect("k7 scratch tempdir");
    let script = scratch.path().join("check_add.py");
    std::fs::write(&script, "print('PASS')\n").expect("stage check script");
    let (bundle_files, bundle_digest) = bundle_binding(&script);

    let started = service
        .start_run("557632d5", 2, "key-k7-versions")
        .expect("start version run");
    let run_id = started.run_id.clone();
    let candidate = service
        .record_candidate(&run_id, "cand-r0", started.version, "operator")
        .expect("record candidate");
    let implementing = service
        .advance_run(&run_id, candidate.version, "host", "turn_started", None)
        .expect("turn started");
    let captured = service
        .advance_run(&run_id, implementing.version, "host", "turn_ended", None)
        .expect("turn ended");
    let local = repo.path().to_string_lossy().into_owned();
    service
        .record_checkpoint(
            &CheckpointRecord {
                run_id: &run_id,
                candidate: "cand-r0",
                commit: &commit,
                tree: &tree,
                source_manifest: "manifest-k7",
                local_path: &local,
                backup_state: "pending",
                preservation_hold: None,
            },
            captured.version,
        )
        .expect("record checkpoint");
    let testing = service
        .advance_run(&run_id, captured.version, "host", "test_started", None)
        .expect("test started");
    // The test stage is current; any other version is moved.
    let stale = testing.version - 1;

    let session = service.record_session(
        &SessionAttachment {
            run_id: &run_id,
            worker_id: "implementer",
            instance_id: None,
            capsule_epoch: None,
            session_id: None,
            process_identity: None,
            agent_profile: None,
            auth_profile: None,
        },
        stale,
        "operator",
    );
    assert_eq!(
        session.expect_err("stale session must refuse").kind(),
        ErrorKind::StaleVersion
    );
    let checkpoint = service.record_checkpoint(
        &CheckpointRecord {
            run_id: &run_id,
            candidate: "cand-r0",
            commit: &commit,
            tree: &tree,
            source_manifest: "manifest-k7",
            local_path: &local,
            backup_state: "pending",
            preservation_hold: None,
        },
        stale,
    );
    assert_eq!(
        checkpoint.expect_err("stale checkpoint must refuse").kind(),
        ErrorKind::StaleVersion
    );
    let params = RecordTestParams {
        check_program: "python3",
        check_script: &script,
        bundle_digest: &bundle_digest,
        bundle_files: &bundle_files,
        repo: Some(repo.path()),
        scratch: scratch.path(),
        timeout_secs: 120,
    };
    let test = service.record_test(&run_id, stale, &params);
    assert_eq!(
        test.expect_err("stale test must refuse").kind(),
        ErrorKind::StaleVersion
    );
    // A stale test record launches no worker: the version check runs
    // before any worker directory is prepared under scratch.
    let workers = std::fs::read_dir(scratch.path())
        .expect("list scratch")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .collect::<Vec<_>>();
    assert!(
        workers.is_empty(),
        "stale record must not prepare a worker: {workers:?}"
    );
    let review_file = scratch.path().join("review.json");
    std::fs::write(&review_file, "{}").expect("stage review");
    let review = service.record_review(&run_id, stale, &review_file, "policy");
    assert_eq!(
        review.expect_err("stale review must refuse").kind(),
        ErrorKind::StaleVersion
    );
    let backup = service.record_backup(&run_id, stale, "remote", "ref", "operator");
    assert_eq!(
        backup.expect_err("stale backup must refuse").kind(),
        ErrorKind::StaleVersion
    );

    // The current version still records on every path.
    service
        .record_session(
            &SessionAttachment {
                run_id: &run_id,
                worker_id: "implementer",
                instance_id: None,
                capsule_epoch: None,
                session_id: None,
                process_identity: None,
                agent_profile: None,
                auth_profile: None,
            },
            testing.version,
            "operator",
        )
        .expect("current session records");
}

/// A review binds to the recorded test policy at record time: a
/// review for a candidate with no recorded test verdict refuses, and
/// a policy hash that disagrees with the recorded verdict refuses.
#[test]
fn k7_record_review_binds_recorded_test_policy() {
    let fixture = Fixture::new();
    let service = fixture.open();
    let repo = tempfile::tempdir().expect("k7 repo tempdir");
    let (commit, tree) = init_git_repo(repo.path());
    let scratch = tempfile::tempdir().expect("k7 scratch tempdir");
    let script = scratch.path().join("check_add.py");
    std::fs::write(&script, "print('PASS')\n").expect("stage check script");
    let (bundle_files, bundle_digest) = bundle_binding(&script);

    // Run A reaches review with a recorded test verdict: a foreign
    // policy hash refuses, the recorded one records.
    let started = service
        .start_run("557632d5", 2, "key-k7-review-policy")
        .expect("start policy run");
    let run_id = started.run_id.clone();
    let candidate = service
        .record_candidate(&run_id, "cand-r0", started.version, "operator")
        .expect("record candidate");
    let implementing = service
        .advance_run(&run_id, candidate.version, "host", "turn_started", None)
        .expect("turn started");
    let captured = service
        .advance_run(&run_id, implementing.version, "host", "turn_ended", None)
        .expect("turn ended");
    let local = repo.path().to_string_lossy().into_owned();
    service
        .record_checkpoint(
            &CheckpointRecord {
                run_id: &run_id,
                candidate: "cand-r0",
                commit: &commit,
                tree: &tree,
                source_manifest: "manifest-k7",
                local_path: &local,
                backup_state: "pending",
                preservation_hold: None,
            },
            captured.version,
        )
        .expect("record checkpoint");
    let testing = service
        .advance_run(&run_id, captured.version, "host", "test_started", None)
        .expect("test started");
    let (_, recorded) = service
        .record_test(
            &run_id,
            testing.version,
            &RecordTestParams {
                check_program: "python3",
                check_script: &script,
                bundle_digest: &bundle_digest,
                bundle_files: &bundle_files,
                repo: Some(repo.path()),
                scratch: scratch.path(),
                timeout_secs: 120,
            },
        )
        .expect("record test");
    let reviewed = service
        .advance_run(&run_id, testing.version, "host", "test_passed", None)
        .expect("test passed");
    let review_file = scratch.path().join("review.json");
    std::fs::write(
        &review_file,
        format!(
            r#"{{"schema_version":1,"candidate_id":"cand-r0","commit":"{commit}","tree":"{tree}","status":"pass","findings":[]}}"#
        ),
    )
    .expect("stage review");
    let foreign = service.record_review(&run_id, reviewed.version, &review_file, "foreign-policy");
    let foreign_error = foreign.expect_err("foreign policy must refuse");
    assert_eq!(foreign_error.kind(), ErrorKind::Invalid);
    assert!(
        format!("{foreign_error}").contains("does not match the recorded test policy"),
        "policy refusal names the mismatch"
    );
    service
        .record_review(
            &run_id,
            reviewed.version,
            &review_file,
            &recorded.policy_hash,
        )
        .expect("recorded policy records");
    service
        .cancel_run(&run_id, "operator", "policy run done")
        .expect("cancel policy run");

    // Run B reaches review with no recorded test verdict: the review
    // refuses until a test verdict exists to bind to.
    let restarted = service
        .start_run("557632d5", 2, "key-k7-review-untested")
        .expect("start untested run");
    let run_id = restarted.run_id.clone();
    let candidate = service
        .record_candidate(&run_id, "cand-r0", restarted.version, "operator")
        .expect("record candidate");
    let implementing = service
        .advance_run(&run_id, candidate.version, "host", "turn_started", None)
        .expect("turn started");
    let captured = service
        .advance_run(&run_id, implementing.version, "host", "turn_ended", None)
        .expect("turn ended");
    service
        .record_checkpoint(
            &CheckpointRecord {
                run_id: &run_id,
                candidate: "cand-r0",
                commit: &commit,
                tree: &tree,
                source_manifest: "manifest-k7",
                local_path: &local,
                backup_state: "pending",
                preservation_hold: None,
            },
            captured.version,
        )
        .expect("record checkpoint");
    let testing = service
        .advance_run(&run_id, captured.version, "host", "test_started", None)
        .expect("test started");
    let untested = service
        .advance_run(&run_id, testing.version, "host", "test_passed", None)
        .expect("test passed");
    let missing = service.record_review(&run_id, untested.version, &review_file, "policy");
    assert_eq!(
        missing
            .expect_err("review without a test must refuse")
            .kind(),
        ErrorKind::Blocked
    );
}

/// Singular evidence is unique at the store level: a second test or
/// review row for the same run and candidate fails the insert even
/// when it bypasses the service pre-check.
#[test]
fn k7_evidence_uniqueness_is_store_enforced() {
    let fixture = Fixture::new();
    let service = fixture.open();
    let started = service
        .start_run("557632d5", 2, "key-k7-evidence-unique")
        .expect("start evidence run");
    let row = EvidenceRecord {
        run_id: &started.run_id,
        candidate: "cand-r0",
        kind: "test",
        source: "trusted-test-worker",
        quality: "host-observed",
        result: "pass",
        policy_hash: Some("policy"),
        artifact_digest: Some("artifact"),
    };
    service
        .store()
        .record_evidence(&row)
        .expect("first evidence row inserts");
    let duplicate = service.store().record_evidence(&row);
    assert_eq!(
        duplicate.expect_err("duplicate evidence must fail").kind(),
        ErrorKind::Storage
    );
    // The sibling kind still inserts: uniqueness is per kind.
    service
        .store()
        .record_evidence(&EvidenceRecord {
            kind: "review",
            ..row
        })
        .expect("review row inserts beside the test row");
}
