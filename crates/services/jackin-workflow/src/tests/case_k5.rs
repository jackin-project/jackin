// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! K5 table-driven checks: failing round, repair, passing round, the
//! repair limit, malformed review, stale evidence, and the
//! revise-then-decide fault-injection paths (accepted repair and
//! rejected incomplete repair with preserved failed state).

use std::path::{Path, PathBuf};

use super::super::{
    Actor, CandidateEvidence, CandidateRef, Decision, DeliveryGate, ErrorKind, FeedbackInput,
    HostReadiness, InputOwnership, MAX_FEEDBACK_BYTES, Observation, PreparedTestWorker,
    ReviewExpectation, ReviewFinding, ReviewWorkerSpec, SessionIdentity, Severity, TestPolicy,
    TestWorkerSpec, VerificationOutcome, admit_repair_delivery, build_feedback, decide_candidate,
    persist_test_output, prepare_test_worker, read_review_bytes, record_review_evidence,
    record_test_evidence, review_acceptable, run_test_command, stage_repair_feedback,
    summarize_test_failure, validate_review,
};
use super::support::Fixture;

const BUGGY_CALCULATOR: &str = "def add(a: int, b: int) -> int:\n    return a - b\n";
const FIXED_CALCULATOR: &str = "def add(a: int, b: int) -> int:\n    return a + b\n";
const PLAN_HASH: &str = "ebc1278fce36289d775a493f9f314a3b60d2cf4847f6396bab88fb4f03baf6f6";

fn run_git(repo: &Path, args: &[&str]) -> String {
    let child = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn git in k5 fixture");
    let output = child.wait_with_output().expect("wait for k5 git");
    assert!(
        output.status.success(),
        "k5 git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Fixture source repo with the known bug committed, then the repair
/// committed on top. Returns the repo path and both checkpoints.
fn init_fixture_source(root: &Path) -> (PathBuf, Checkpoint, Checkpoint) {
    let repo = root.join("fixture-src");
    std::fs::create_dir_all(&repo).expect("k5 fixture src dir");
    std::fs::write(repo.join("calculator.py"), BUGGY_CALCULATOR).expect("write buggy calculator");
    run_git(&repo, &["init", "-q"]);
    run_git(&repo, &["config", "user.name", "k5-fixture"]);
    run_git(&repo, &["config", "user.email", "k5@local"]);
    run_git(&repo, &["add", "calculator.py"]);
    run_git(&repo, &["commit", "-qm", "seeded defect: add subtracts"]);
    let bug = Checkpoint::read(&repo);
    assert_eq!(
        std::fs::read_to_string(repo.join("calculator.py")).expect("read calculator"),
        BUGGY_CALCULATOR
    );
    std::fs::write(repo.join("calculator.py"), FIXED_CALCULATOR).expect("write fixed calculator");
    run_git(&repo, &["add", "calculator.py"]);
    run_git(&repo, &["commit", "-qm", "repair: add sums"]);
    let fix = Checkpoint::read(&repo);
    assert_ne!(bug.commit, fix.commit, "repair must be a new commit");
    (repo, bug, fix)
}

#[derive(Debug, Clone)]
struct Checkpoint {
    commit: String,
    tree: String,
}

impl Checkpoint {
    fn read(repo: &Path) -> Self {
        Self {
            commit: run_git(repo, &["rev-parse", "HEAD"]),
            tree: run_git(repo, &["rev-parse", "HEAD^{tree}"]),
        }
    }
}

/// Trusted bundle outside the worker source: a copy of the frozen
/// `check_add.py`, never inside any implementation checkout.
fn trusted_bundle(root: &Path) -> PathBuf {
    let dir = root.join("fixture-checks");
    std::fs::create_dir_all(&dir).expect("k5 bundle dir");
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../examples/workflows/checks/check_add.py");
    let bytes = std::fs::read(&source).expect("read frozen check_add.py");
    std::fs::write(dir.join("check_add.py"), &bytes).expect("stage trusted bundle");
    dir
}

fn policy_for(bundle_check: &Path, work_dir: &Path) -> TestPolicy {
    TestPolicy::freeze(
        vec![
            "python3".to_owned(),
            bundle_check.to_string_lossy().into_owned(),
            work_dir.to_string_lossy().into_owned(),
        ],
        120,
    )
    .expect("freeze k5 policy")
}

fn test_round(
    repo: &Path,
    scratch: &Path,
    run_id: &str,
    round: u32,
    candidate: &str,
    point: &Checkpoint,
    bundle_check: &Path,
) -> (
    TestWorkerSpec,
    TestPolicy,
    PreparedTestWorker,
    super::super::TestExecution,
) {
    let work_dir = scratch.join(format!("test-{run_id}-r{round}-{candidate}"));
    let policy = policy_for(bundle_check, &work_dir);
    let spec = TestWorkerSpec::new(
        run_id,
        round,
        candidate,
        &point.commit,
        &point.tree,
        &policy,
    )
    .expect("k5 test worker spec");
    assert_eq!(
        spec.credentials(),
        super::super::NoAgentCredentials::NONE,
        "test workers carry no agent credentials"
    );
    let worker = prepare_test_worker(repo, &spec, scratch).expect("prepare k5 test worker");
    assert_eq!(worker.work_dir(), &work_dir);
    assert_eq!(worker.commit(), point.commit);
    assert_eq!(worker.tree(), point.tree);
    let execution = run_test_command(&policy, &worker).expect("run k5 pinned command");
    (spec, policy, worker, execution)
}

fn passing_review_bytes(candidate: &str, commit: &str, tree: &str) -> Vec<u8> {
    serde_json::json!({
        "schema_version": 1,
        "candidate_id": candidate,
        "commit": commit,
        "tree": tree,
        "status": "pass",
        "findings": [],
    })
    .to_string()
    .into_bytes()
}

fn failing_review_bytes(candidate: &str, commit: &str, tree: &str) -> Vec<u8> {
    serde_json::json!({
        "schema_version": 1,
        "candidate_id": candidate,
        "commit": commit,
        "tree": tree,
        "status": "fail",
        "findings": [{
            "id": "F1",
            "severity": "major",
            "path": "calculator.py",
            "line_start": 1,
            "line_end": 2,
            "explanation": "add subtracts instead of summing",
            "suggestion": "return a + b",
        }],
    })
    .to_string()
    .into_bytes()
}

fn green_evidence<'a>(
    candidate: &'a str,
    commit: &'a str,
    tree: &'a str,
    policy_hash: &'a str,
    test_passed: bool,
    review_ok: bool,
    round: u32,
) -> CandidateEvidence<'a> {
    CandidateEvidence {
        candidate: CandidateRef {
            candidate_id: candidate,
            commit,
            tree,
            generation: 1,
        },
        current: CandidateRef {
            candidate_id: candidate,
            commit,
            tree,
            generation: 1,
        },
        verification: VerificationOutcome {
            test_passed,
            review_acceptable: review_ok,
            test_policy_hash: policy_hash,
            expected_policy_hash: policy_hash,
        },
        readiness: HostReadiness {
            source_current: true,
            backup_satisfied: true,
        },
        ownership: InputOwnership {
            host_owned: true,
            epoch_current: true,
            uncertainty_pending: false,
        },
        round,
        max_repair_turns: 2,
    }
}

fn c4_compare(dir: &Path, manifest: &jackin_isolation::capture::SourceManifest) -> String {
    match jackin_isolation::capture::compare_source(dir, manifest, 10_000_000) {
        jackin_isolation::capture::SourceCompare::Match => "match".to_owned(),
        jackin_isolation::capture::SourceCompare::Changed => "changed".to_owned(),
        jackin_isolation::capture::SourceCompare::Unknown(reason) => {
            panic!("k5 C4 compare unknown: {reason}");
        }
    }
}

#[test]
fn k5_verification_rows() {
    for row in [
        "pass",
        "repair_accepted",
        "repair_rejected",
        "exhausted",
        "malformed_review",
        "stale_candidate",
    ] {
        match row {
            "pass" => row_pass(),
            "repair_accepted" => row_repair_accepted(),
            "repair_rejected" => row_repair_rejected(),
            "exhausted" => row_exhausted(),
            "malformed_review" => row_malformed_review(),
            "stale_candidate" => row_stale_candidate(),
            _ => unreachable!("k5 row {row}"),
        }
    }
}

/// Passing round: the repaired candidate exits zero and an acceptable
/// review decides Accept.
fn row_pass() {
    let temp = tempfile::tempdir().expect("k5 tempdir");
    let (repo, _bug, fix) = init_fixture_source(temp.path());
    let bundle = trusted_bundle(temp.path());
    let bundle_check = bundle.join("check_add.py");
    let scratch = temp.path().join("scratch");
    let (spec, policy, _worker, execution) = test_round(
        &repo,
        &scratch,
        "run-k5pass",
        0,
        "candidate-002",
        &fix,
        &bundle_check,
    );
    assert_eq!(execution.exit_code(), 0, "fixed candidate must exit zero");
    assert!(
        execution.stdout().text().contains("PASS"),
        "trusted output names the pass, got: {}",
        execution.stdout().text()
    );
    let result = super::super::TestResult::from_execution(&spec, &policy, &execution);
    assert!(result.passed());
    let review = validate_review(
        &passing_review_bytes("candidate-002", &fix.commit, &fix.tree),
        ReviewExpectation {
            candidate_id: "candidate-002",
            commit: &fix.commit,
            tree: &fix.tree,
        },
    )
    .expect("valid passing review");
    assert!(review_acceptable(&review));
    let evidence = green_evidence(
        "candidate-002",
        &fix.commit,
        &fix.tree,
        policy.policy_hash(),
        result.passed(),
        review_acceptable(&review),
        0,
    );
    assert_eq!(decide_candidate(&evidence), Decision::Accept);
}

/// Accepted fault-injection path: revise-then-decide with one repair
/// turn. The failing round records test evidence and a validated
/// review BEFORE any repair; the repaired checkpoint verifies through
/// the C4 path; the passing round accepts.
fn row_repair_accepted() {
    let temp = tempfile::tempdir().expect("k5 tempdir");
    let (repo, bug, fix) = init_fixture_source(temp.path());
    let bundle = trusted_bundle(temp.path());
    let bundle_check = bundle.join("check_add.py");
    let scratch = temp.path().join("scratch");
    let fixture = Fixture::new();
    let service = fixture.open();
    let run = service
        .start_run(PLAN_HASH, 2, "key-k5-accepted")
        .expect("start k5 run");

    // Round 0: the seeded defect fails the trusted check.
    let (spec, policy, worker, execution) = test_round(
        &repo,
        &scratch,
        &run.run_id,
        0,
        "candidate-001",
        &bug,
        &bundle_check,
    );
    assert_ne!(execution.exit_code(), 0, "seeded defect must fail");
    let result = super::super::TestResult::from_execution(&spec, &policy, &execution);
    assert!(!result.passed());
    let digest = persist_test_output(service.store(), result.candidate_id(), &execution)
        .expect("persist failing output");
    assert!(
        service
            .store()
            .verify_artifact(&format!("test-output-{}", result.candidate_id()), &digest)
            .expect("verify failing artifact"),
        "failing output artifact must round-trip"
    );
    record_test_evidence(service.store(), &run.run_id, &result, &digest)
        .expect("record failing test");
    let review_bytes = failing_review_bytes("candidate-001", &bug.commit, &bug.tree);
    let review_artifact = service
        .store()
        .write_artifact("review-candidate-001", &review_bytes)
        .expect("persist review");
    let review =
        validate_review(&review_bytes, spec_expectation(&spec)).expect("valid failing review");
    assert!(!review_acceptable(&review));
    // The review is stored before any repair turn exists.
    record_review_evidence(
        service.store(),
        &run.run_id,
        "candidate-001",
        policy.policy_hash(),
        &review,
        &review_artifact.digest,
    )
    .expect("record failing review");
    let failing = summarize_test_failure(&policy, &execution, result.excerpt());
    assert!(
        failing.name.contains("check_add.py"),
        "feedback names the failing check, got: {}",
        failing.name
    );
    let blocking: Vec<&ReviewFinding> = review
        .findings
        .iter()
        .filter(|finding| finding.severity != Severity::Note)
        .collect();
    let feedback = build_feedback(&FeedbackInput {
        candidate_id: "candidate-001",
        commit: &bug.commit,
        round: 0,
        remaining_repairs: 2,
        checks: vec![failing],
        findings: blocking,
        evidence_refs: vec![
            format!("test:{digest}"),
            format!("review:{}", review_artifact.digest),
        ],
    })
    .expect("build repair feedback");
    assert!(feedback.contains("candidate-001"));
    assert!(
        feedback.contains("calculator.py:1-2"),
        "feedback carries file:line, got: {feedback}"
    );
    let decision = decide_candidate(&green_evidence(
        "candidate-001",
        &bug.commit,
        &bug.tree,
        policy.policy_hash(),
        false,
        false,
        0,
    ));
    assert_eq!(decision, Decision::Repair);

    // One repair turn in the same session: the defect is corrected.
    // The repaired checkpoint verifies through the C4 path: the C4
    // manifest changes with the repair and matches the repaired tree.
    let (_, pre_manifest) =
        jackin_isolation::capture::digest_source_tree(worker.work_dir(), &[], 10_000_000)
            .expect("digest worker source before repair");
    std::fs::write(worker.work_dir().join("calculator.py"), FIXED_CALCULATOR)
        .expect("apply repair in worker tree");
    let (_, repaired_manifest) =
        jackin_isolation::capture::digest_source_tree(worker.work_dir(), &[], 10_000_000)
            .expect("digest repaired tree");
    assert_ne!(
        pre_manifest.entries, repaired_manifest.entries,
        "the C4 manifest must change with the repair"
    );
    assert_eq!(
        c4_compare(worker.work_dir(), &repaired_manifest),
        "match",
        "C4 confirms the repaired checkpoint"
    );

    // Round 1: the repaired candidate passes and accepts.
    let (spec2, policy2, _worker2, execution2) = test_round(
        &repo,
        &scratch,
        &run.run_id,
        1,
        "candidate-002",
        &fix,
        &bundle_check,
    );
    let result2 = super::super::TestResult::from_execution(&spec2, &policy2, &execution2);
    assert!(result2.passed(), "repaired candidate must pass");
    let review2 = validate_review(
        &passing_review_bytes("candidate-002", &fix.commit, &fix.tree),
        spec_expectation(&spec2),
    )
    .expect("valid passing review");
    let decision2 = decide_candidate(&green_evidence(
        "candidate-002",
        &fix.commit,
        &fix.tree,
        policy2.policy_hash(),
        result2.passed(),
        review_acceptable(&review2),
        1,
    ));
    assert_eq!(decision2, Decision::Accept);
}

fn spec_expectation(spec: &TestWorkerSpec) -> ReviewExpectation<'_> {
    ReviewExpectation {
        candidate_id: spec.candidate_id(),
        commit: spec.commit(),
        tree: spec.tree(),
    }
}

/// Rejected fault-injection path: two incomplete repairs spend the
/// budget, the run fails, and the failed state stays in the tree with
/// no silent cleanup.
fn row_repair_rejected() {
    let temp = tempfile::tempdir().expect("k5 tempdir");
    let (repo, bug, _fix) = init_fixture_source(temp.path());
    let bundle = trusted_bundle(temp.path());
    let bundle_check = bundle.join("check_add.py");
    let scratch = temp.path().join("scratch");

    let mut round = 0;
    let mut worker_dir = PathBuf::new();
    let mut last_policy_hash = String::new();
    for turn in 0..2 {
        let (_spec, policy, worker, execution) = test_round(
            &repo,
            &scratch,
            "run-k5reject",
            round,
            "candidate-001",
            &bug,
            &bundle_check,
        );
        last_policy_hash = policy.policy_hash().to_owned();
        assert_ne!(
            execution.exit_code(),
            0,
            "incomplete repair must still fail"
        );
        let decision = decide_candidate(&green_evidence(
            "candidate-001",
            &bug.commit,
            &bug.tree,
            policy.policy_hash(),
            false,
            false,
            round,
        ));
        if turn == 0 {
            assert_eq!(decision, Decision::Repair);
        }
        round += 1;
        worker_dir = worker.work_dir().to_owned();
        // The incomplete repair touches nothing: the failed state stays.
        let content = std::fs::read_to_string(worker_dir.join("calculator.py"))
            .expect("read preserved worker source");
        assert_eq!(
            content, BUGGY_CALCULATOR,
            "failed state is preserved, never cleaned"
        );
    }
    let decision = decide_candidate(&green_evidence(
        "candidate-001",
        &bug.commit,
        &bug.tree,
        &last_policy_hash,
        false,
        false,
        round,
    ));
    assert!(
        matches!(decision, Decision::Fail(_)),
        "exhausted budget with failing verification must fail, got: {decision:?}"
    );
    assert_eq!(
        std::fs::read_to_string(worker_dir.join("calculator.py")).expect("read final source"),
        BUGGY_CALCULATOR,
        "rejected failure stays in the tree"
    );
}

#[test]
fn k5_policy_and_identity_rows() {
    let policy = TestPolicy::freeze(vec!["python3".to_owned(), "check.py".to_owned()], 120)
        .expect("freeze valid policy");
    assert_eq!(policy.program(), "python3");
    assert_eq!(policy.args(), &["check.py".to_owned()]);
    let again = TestPolicy::freeze(vec!["python3".to_owned(), "check.py".to_owned()], 120)
        .expect("freeze same policy");
    assert_eq!(
        policy.policy_hash(),
        again.policy_hash(),
        "identical policies share a hash"
    );
    let other = TestPolicy::freeze(vec!["python3".to_owned(), "other.py".to_owned()], 120)
        .expect("freeze other policy");
    assert_ne!(
        policy.policy_hash(),
        other.policy_hash(),
        "changed argv changes the hash"
    );
    for argv in [
        Vec::<String>::new(),
        vec![String::new()],
        vec!["python3".to_owned(), "  ".to_owned()],
    ] {
        assert!(
            TestPolicy::freeze(argv, 120).is_err(),
            "empty argv parts are rejected"
        );
    }
    let _rejected =
        TestPolicy::freeze(vec!["python3".to_owned()], 0).expect_err("zero timeout is rejected");
    let _rejected = TestPolicy::freeze(vec!["python3".to_owned()], 3_601)
        .expect_err("overlong timeout is rejected");

    let spec = TestWorkerSpec::new(
        "run-k5id",
        0,
        "candidate-001",
        "commit-1",
        "tree-1",
        &policy,
    )
    .expect("valid test worker spec");
    assert_eq!(spec.worker_id(), "test-run-k5id-r0-candidate-001");
    assert_eq!(spec.policy_hash(), policy.policy_hash());
    for (label, run, candidate) in [
        ("empty-run", "", "candidate-001"),
        ("slash-candidate", "run-k5id", "a/b"),
        ("space-commit", "run-k5id", "a b"),
    ] {
        assert!(
            TestWorkerSpec::new(run, 0, candidate, "commit-1", "tree-1", &policy).is_err(),
            "bad worker identity {label} is rejected"
        );
    }
}

#[test]
fn k5_review_schema_rows() {
    let commit = "commit-1";
    let tree = "tree-1";
    let expected = ReviewExpectation {
        candidate_id: "candidate-001",
        commit,
        tree,
    };
    // Note-only findings never block acceptance.
    let noted = validate_review(
        &serde_json::json!({
            "schema_version": 1, "candidate_id": "candidate-001",
            "commit": commit, "tree": tree, "status": "pass",
            "findings": [{
                "id": "N1", "severity": "note", "path": "calculator.py",
                "line_start": 1,
                "explanation": "style nit", "suggestion": "reformat",
            }],
        })
        .to_string()
        .into_bytes(),
        expected,
    )
    .expect("note-only pass validates");
    assert!(review_acceptable(&noted));
    // Each blocking severity prevents acceptance on its own.
    for severity in ["blocker", "major", "minor"] {
        let reviewed = validate_review(
            &serde_json::json!({
                "schema_version": 1, "candidate_id": "candidate-001",
                "commit": commit, "tree": tree, "status": "fail",
                "findings": [{
                    "id": "F1", "severity": severity, "path": "sub/calculator.py",
                    "explanation": "wrong", "suggestion": "fix it",
                }],
            })
            .to_string()
            .into_bytes(),
            expected,
        )
        .expect("failing review validates");
        assert!(
            !review_acceptable(&reviewed),
            "{severity} must prevent acceptance"
        );
    }
    // Unknown severity strings never parse into the closed schema.
    let unknown = serde_json::json!({
        "schema_version": 1, "candidate_id": "candidate-001",
        "commit": commit, "tree": tree, "status": "fail",
        "findings": [{
            "id": "F1", "severity": "critical", "path": "calculator.py",
            "explanation": "wrong", "suggestion": "fix it",
        }],
    })
    .to_string()
    .into_bytes();
    let _rejected = validate_review(&unknown, expected).expect_err("unknown severity is rejected");
}

#[test]
fn k5_adapter_rows() {
    let temp = tempfile::tempdir().expect("k5 tempdir");
    let missing = temp.path().join("no-review.json");
    assert_eq!(
        read_review_bytes(&missing)
            .expect_err("missing review must error")
            .kind(),
        ErrorKind::NotFound
    );
    // A transcript with an embedded JSON fragment is not a result.
    let transcript = temp.path().join("transcript.txt");
    std::fs::write(
        &transcript,
        "some long log output\n{\"schema_version\": 1, \"candidate_id\": \"x\"}\nmore logs\n",
    )
    .expect("write transcript");
    let bytes = read_review_bytes(&transcript).expect("adapter reads the exact file");
    let expected = ReviewExpectation {
        candidate_id: "candidate-001",
        commit: "commit-1",
        tree: "tree-1",
    };
    assert!(
        validate_review(&bytes, expected).is_err(),
        "transcript fragments never validate"
    );
    let oversize = temp.path().join("big.json");
    std::fs::write(
        &oversize,
        vec![b'x'; super::super::MAX_REVIEW_BYTES as usize + 2],
    )
    .expect("write oversize review");
    assert!(
        read_review_bytes(&oversize).is_err(),
        "oversize review refused"
    );
}

#[test]
fn k5_feedback_rows() {
    // Secrets are assembled at runtime so no credential-shaped
    // literal is committed to the tree.
    let fake_token = ["sk", "-", "test", "-", "0123456789abcdef"].concat();
    let hostile = format!(
        "Traceback (most recent call last):\n  File \"/work/repo/calculator.py\", line 2, in add\
         \n    return a - b\nAssertionError: add(2, 3) returned -1; expected 5 (api_key={fake_token} \
         session_id=77 agent_id=claude-1)\n",
    );
    let policy = TestPolicy::freeze(vec!["python3".to_owned(), "check_add.py".to_owned()], 120)
        .expect("freeze policy");
    let execution = super::super::TestExecution::from_test_parts(1, &hostile);
    let failing = summarize_test_failure(&policy, &execution, &hostile);
    assert!(
        failing.name.contains("check_add.py") && failing.name.contains("exit 1"),
        "feedback names the failing check, got: {}",
        failing.name
    );
    assert_eq!(
        failing.location.as_deref(),
        Some("calculator.py:2"),
        "feedback carries file:line without the host path"
    );
    let finding = ReviewFinding {
        id: "F1".to_owned(),
        severity: Severity::Major,
        path: "calculator.py".to_owned(),
        line_start: Some(1),
        line_end: Some(2),
        explanation: "add subtracts".to_owned(),
        suggestion: "return a + b".to_owned(),
    };
    let feedback = build_feedback(&FeedbackInput {
        candidate_id: "candidate-001",
        commit: "abcdef0123456789",
        round: 0,
        remaining_repairs: 2,
        checks: vec![failing],
        findings: vec![&finding],
        evidence_refs: vec!["test:digest-1".to_owned()],
    })
    .expect("build hostile feedback");
    assert!(feedback.contains("candidate-001"));
    assert!(feedback.contains("calculator.py:1-2"));
    assert!(
        !feedback.contains(&fake_token),
        "feedback must not leak the token"
    );
    for leaked in ["session_id=77", "agent_id=claude-1", "/work/repo"] {
        assert!(
            !feedback.contains(leaked),
            "feedback must not leak identities or host paths"
        );
    }
    assert!(
        feedback.contains("[redacted]"),
        "secrets are visibly redacted"
    );
    assert!(
        feedback.len() <= MAX_FEEDBACK_BYTES,
        "feedback stays bounded"
    );
    // An ocean of failures still fits the bound with a marker.
    let ocean: Vec<super::super::FailingCheck> = (0..300)
        .map(|index| super::super::FailingCheck {
            name: format!("check-{index} (exit 1)"),
            excerpt: hostile.clone(),
            location: None,
        })
        .collect();
    let capped = build_feedback(&FeedbackInput {
        candidate_id: "candidate-001",
        commit: "abcdef0123456789",
        round: 0,
        remaining_repairs: 0,
        checks: ocean,
        findings: Vec::new(),
        evidence_refs: Vec::new(),
    })
    .expect("build ocean feedback");
    assert!(capped.len() <= MAX_FEEDBACK_BYTES + 16);
    assert!(capped.contains("[truncated]"));
    // Empty feedback is refused: no failure, no document.
    let _rejected = build_feedback(&FeedbackInput {
        candidate_id: "candidate-001",
        commit: "c",
        round: 0,
        remaining_repairs: 1,
        checks: Vec::new(),
        findings: Vec::new(),
        evidence_refs: Vec::new(),
    })
    .expect_err("empty feedback is refused");
}

#[test]
fn k5_ownership_gate_rows() {
    let identity = SessionIdentity {
        instance_id: "instance-1".to_owned(),
        session_id: 7,
        capsule_epoch: Some(3),
        process_identity: Some("claude:pid-99".to_owned()),
    };
    let open = DeliveryGate {
        recorded: identity.clone(),
        observed: identity.clone(),
        owner_epoch_current: true,
        operator_owned: false,
        turn_ended: true,
    };
    assert!(
        admit_repair_delivery(&open).is_ok(),
        "same healthy session with a finished turn admits delivery"
    );
    let mut_cases: Vec<(&str, DeliveryGate)> = vec![
        (
            "moved-instance",
            DeliveryGate {
                observed: SessionIdentity {
                    instance_id: "instance-2".to_owned(),
                    ..identity.clone()
                },
                ..open.clone()
            },
        ),
        (
            "moved-session",
            DeliveryGate {
                observed: SessionIdentity {
                    session_id: 8,
                    ..identity.clone()
                },
                ..open.clone()
            },
        ),
        (
            "moved-epoch",
            DeliveryGate {
                observed: SessionIdentity {
                    capsule_epoch: Some(4),
                    ..identity.clone()
                },
                ..open.clone()
            },
        ),
        (
            "stale-owner-epoch",
            DeliveryGate {
                owner_epoch_current: false,
                ..open.clone()
            },
        ),
        (
            "operator-owned",
            DeliveryGate {
                operator_owned: true,
                ..open.clone()
            },
        ),
        (
            "turn-running",
            DeliveryGate {
                turn_ended: false,
                ..open.clone()
            },
        ),
    ];
    for (label, gate) in mut_cases {
        assert_eq!(
            admit_repair_delivery(&gate)
                .expect_err("uncertain delivery must refuse")
                .kind(),
            ErrorKind::Blocked,
            "gate row {label} refuses delivery"
        );
    }
}

#[test]
fn k5_repair_staging_rows() {
    let fixture = Fixture::new();
    let service = fixture.open();
    let run = service
        .start_run(PLAN_HASH, 2, "key-k5-stage")
        .expect("start k5 run");
    let identity = SessionIdentity {
        instance_id: "instance-1".to_owned(),
        session_id: 7,
        capsule_epoch: Some(3),
        process_identity: Some("claude:pid-99".to_owned()),
    };
    let open = DeliveryGate {
        recorded: identity.clone(),
        observed: identity.clone(),
        owner_epoch_current: true,
        operator_owned: false,
        turn_ended: true,
    };
    // The frozen prompt is saved before delivery: the command row and
    // its artifact exist right after staging, with no receipt yet.
    let staged = stage_repair_feedback(
        &service,
        &run.run_id,
        "session:7",
        "repair feedback for candidate-001 (abcdef01234567) round 0: 2 repair turn(s) left\n",
        0,
        0,
        &open,
    )
    .expect("stage admitted repair");
    let command = service
        .store()
        .load_command(&staged.command_id)
        .expect("reload staged command")
        .expect("staged command exists");
    assert_eq!(
        command.state,
        super::super::CommandState::Dispatching.as_str(),
        "staging commits dispatch; only a receipt moves it to written"
    );
    assert!(
        service
            .store()
            .verify_artifact(&staged.command_id, &staged.prompt_digest)
            .expect("verify staged prompt"),
        "frozen prompt bytes survive content addressing"
    );
    // A refused gate stages nothing: no command row appears.
    let refused = DeliveryGate {
        operator_owned: true,
        ..open.clone()
    };
    let before = service
        .store()
        .commands_in_state(&run.run_id, super::super::CommandState::Dispatching)
        .expect("list dispatching")
        .len();
    let _rejected = stage_repair_feedback(
        &service,
        &run.run_id,
        "session:7",
        "repair feedback",
        0,
        0,
        &refused,
    )
    .expect_err("refused delivery stages nothing");
    let after = service
        .store()
        .commands_in_state(&run.run_id, super::super::CommandState::Dispatching)
        .expect("list dispatching")
        .len();
    assert_eq!(before, after, "refused delivery stages no command");
}

#[test]
fn k5_evidence_binding_rows() {
    let temp = tempfile::tempdir().expect("k5 tempdir");
    let (repo, bug, _fix) = init_fixture_source(temp.path());
    let bundle = trusted_bundle(temp.path());
    let bundle_check = bundle.join("check_add.py");
    let scratch = temp.path().join("scratch");
    let fixture = Fixture::new();
    let service = fixture.open();
    let run = service
        .start_run(PLAN_HASH, 2, "key-k5-bind")
        .expect("start k5 run");
    let (spec, policy, _worker, execution) = test_round(
        &repo,
        &scratch,
        &run.run_id,
        0,
        "candidate-001",
        &bug,
        &bundle_check,
    );
    let result = super::super::TestResult::from_execution(&spec, &policy, &execution);
    let digest = persist_test_output(service.store(), result.candidate_id(), &execution)
        .expect("persist output");
    record_test_evidence(service.store(), &run.run_id, &result, &digest)
        .expect("record test evidence");
    let review_bytes = failing_review_bytes("candidate-001", &bug.commit, &bug.tree);
    let review_artifact = service
        .store()
        .write_artifact("review-candidate-001", &review_bytes)
        .expect("persist review");
    let review = validate_review(&review_bytes, spec_expectation(&spec)).expect("valid review");
    record_review_evidence(
        service.store(),
        &run.run_id,
        "candidate-001",
        policy.policy_hash(),
        &review,
        &review_artifact.digest,
    )
    .expect("record review evidence");
    // Completed verdicts are never partial: recovery discards nothing,
    // so the bound rows survive for the decision.
    assert_eq!(
        service
            .store()
            .discard_partial_evidence(&run.run_id, "candidate-001")
            .expect("discard partial evidence"),
        0,
        "pass/fail verdicts are preserved, never discarded as partial"
    );
    assert!(
        service
            .store()
            .verify_artifact("review-candidate-001", &review_artifact.digest)
            .expect("verify review artifact"),
        "review bytes survive content addressing"
    );
}

#[test]
fn k5_reducer_repair_rows() {
    use super::super::{RunView, Stage, advance};
    let view = RunView {
        status: super::super::RunStatus::Active,
        stage: Stage::Test,
        round: 0,
        candidate_generation: 1,
    };
    let next = advance(&view, 2, &Observation::TestFailed, Actor::Host).expect("repair turn");
    assert_eq!(next.round, 1);
    assert_eq!(next.stage, Stage::Implement);
    let spent = RunView {
        round: 2,
        ..view.clone()
    };
    let failed = advance(&spent, 2, &Observation::TestFailed, Actor::Host).expect("budget spent");
    assert_eq!(failed.status, super::super::RunStatus::Failed);
    // Workers can report findings but never accept a run.
    let deciding = RunView {
        stage: Stage::Decide,
        ..view.clone()
    };
    assert!(
        advance(&deciding, 2, &Observation::VerifiedCurrent, Actor::Worker).is_err(),
        "only the host can accept"
    );
    assert!(
        advance(&deciding, 2, &Observation::VerifiedCurrent, Actor::Host).is_ok(),
        "host acceptance at decide succeeds"
    );
}

/// Live Codex review proof: BLOCKED on the missing role prerequisite.
/// A codex-capable role with a working preflight does not exist
/// (agent-smith is claude-only; the-architect lacks the caveman
/// skill), so no Codex session is launched and no retry is attempted.
/// This row is ignored locally and tracked as BLOCKED in the report.
#[test]
#[ignore = "BLOCKED: needs a codex-capable role with a working preflight"]
fn k5_live_codex_review_blocked() {
    panic!("live Codex proof is BLOCKED: no codex-capable role with a working preflight exists");
}

/// Exhausted budget at the decision layer: failures at the final
/// round fail instead of repairing.
fn row_exhausted() {
    let decision = decide_candidate(&green_evidence(
        "candidate-009",
        "commit-9",
        "tree-9",
        "policy-9",
        false,
        false,
        2,
    ));
    assert!(
        matches!(decision, Decision::Fail(_)),
        "round == max with failures must fail, got: {decision:?}"
    );
}

/// Malformed review rows: every malformed document is rejected, and
/// the correction budget allows exactly one retry per review session.
fn row_malformed_review() {
    let expected = ReviewExpectation {
        candidate_id: "candidate-001",
        commit: "commit-1",
        tree: "tree-1",
    };
    let bad_documents: Vec<(&str, Vec<u8>)> = vec![
        ("non-json", b"this is a transcript, not {json".to_vec()),
        (
            "wrong-version",
            serde_json::json!({
                "schema_version": 2, "candidate_id": "candidate-001",
                "commit": "commit-1", "tree": "tree-1",
                "status": "pass", "findings": [],
            })
            .to_string()
            .into_bytes(),
        ),
        (
            "symbolic-commit",
            serde_json::json!({
                "schema_version": 1, "candidate_id": "candidate-001",
                "commit": "EXACT_COMMIT_FROM_REVIEW_INPUT", "tree": "tree-1",
                "status": "pass", "findings": [],
            })
            .to_string()
            .into_bytes(),
        ),
        (
            "mismatched-tree",
            serde_json::json!({
                "schema_version": 1, "candidate_id": "candidate-001",
                "commit": "commit-1", "tree": "other-tree",
                "status": "pass", "findings": [],
            })
            .to_string()
            .into_bytes(),
        ),
        (
            "pass-with-blocker",
            serde_json::json!({
                "schema_version": 1, "candidate_id": "candidate-001",
                "commit": "commit-1", "tree": "tree-1", "status": "pass",
                "findings": [{
                    "id": "B1", "severity": "blocker", "path": "calculator.py",
                    "explanation": "wrong", "suggestion": "fix it",
                }],
            })
            .to_string()
            .into_bytes(),
        ),
        (
            "fail-without-findings",
            serde_json::json!({
                "schema_version": 1, "candidate_id": "candidate-001",
                "commit": "commit-1", "tree": "tree-1",
                "status": "fail", "findings": [],
            })
            .to_string()
            .into_bytes(),
        ),
        (
            "absolute-path",
            serde_json::json!({
                "schema_version": 1, "candidate_id": "candidate-001",
                "commit": "commit-1", "tree": "tree-1", "status": "fail",
                "findings": [{
                    "id": "B1", "severity": "major", "path": "/etc/shadow",
                    "explanation": "wrong", "suggestion": "fix it",
                }],
            })
            .to_string()
            .into_bytes(),
        ),
        (
            "dotdot-path",
            serde_json::json!({
                "schema_version": 1, "candidate_id": "candidate-001",
                "commit": "commit-1", "tree": "tree-1", "status": "fail",
                "findings": [{
                    "id": "B1", "severity": "major", "path": "../plan.md",
                    "explanation": "wrong", "suggestion": "fix it",
                }],
            })
            .to_string()
            .into_bytes(),
        ),
        (
            "reversed-lines",
            serde_json::json!({
                "schema_version": 1, "candidate_id": "candidate-001",
                "commit": "commit-1", "tree": "tree-1", "status": "fail",
                "findings": [{
                    "id": "B1", "severity": "major", "path": "calculator.py",
                    "line_start": 9, "line_end": 2,
                    "explanation": "wrong", "suggestion": "fix it",
                }],
            })
            .to_string()
            .into_bytes(),
        ),
    ];
    for (label, bytes) in bad_documents {
        assert!(
            validate_review(&bytes, expected).is_err(),
            "malformed review {label} must be rejected"
        );
    }
    let oversize = vec![b'x'; super::super::MAX_REVIEW_BYTES as usize + 1];
    assert!(
        validate_review(&oversize, expected).is_err(),
        "oversize review rejected"
    );

    let temp = tempfile::tempdir().expect("k5 tempdir");
    let mut reviewer = ReviewWorkerSpec::new(
        "run-k5malformed",
        0,
        "candidate-001",
        "commit-1",
        "tree-1",
        temp.path(),
    )
    .expect("review worker spec");
    reviewer
        .take_correction()
        .expect("first correction fits the budget");
    assert_eq!(reviewer.corrections_used(), 1);
    assert!(
        reviewer.take_correction().is_err(),
        "second correction requires operator action"
    );
}

/// Stale evidence rows: superseded candidates, moved generations, and
/// changed policies never accept.
fn row_stale_candidate() {
    let fresh = green_evidence(
        "candidate-002",
        "commit-2",
        "tree-2",
        "policy-a",
        true,
        true,
        1,
    );
    assert_eq!(decide_candidate(&fresh), Decision::Accept);
    for (label, mut stale) in [
        (
            "moved-commit",
            green_evidence(
                "candidate-001",
                "commit-1",
                "tree-1",
                "policy-a",
                true,
                true,
                1,
            ),
        ),
        (
            "moved-generation",
            green_evidence(
                "candidate-002",
                "commit-2",
                "tree-2",
                "policy-a",
                true,
                true,
                1,
            ),
        ),
        (
            "changed-policy",
            green_evidence(
                "candidate-002",
                "commit-2",
                "tree-2",
                "policy-a",
                true,
                true,
                1,
            ),
        ),
    ] {
        match label {
            "moved-commit" => {
                stale.current.commit = "commit-2";
                stale.current.tree = "tree-2";
            }
            "moved-generation" => {
                stale.current.generation = 2;
            }
            "changed-policy" => {
                stale.verification.expected_policy_hash = "policy-b";
            }
            _ => unreachable!(),
        }
        let decision = decide_candidate(&stale);
        assert!(
            matches!(decision, Decision::NeedsOperator(_)),
            "stale evidence {label} must hold for the operator, got: {decision:?}"
        );
        assert_ne!(
            decision,
            Decision::Accept,
            "stale evidence {label} never accepts"
        );
    }
}

/// Runner hygiene: the pinned test command never emits `__pycache__`
/// into the worker source, even when the trusted check imports a
/// candidate module. The K7 review FAIL came from exactly this: our
/// own trusted-check run left an untracked `__pycache__/` outside the
/// candidate diff. Without `PYTHONDONTWRITEBYTECODE` the import below
/// byte-compiles the helper and this test fails on the residue walk.
#[test]
fn k5_runner_never_emits_bytecode() {
    let temp = tempfile::tempdir().expect("k5 tempdir");
    let repo = temp.path().join("bytecode-src");
    std::fs::create_dir_all(&repo).expect("bytecode src dir");
    std::fs::write(repo.join("helpermod.py"), "VALUE = 41\n").expect("write helper module");
    std::fs::write(
        repo.join("check_import.py"),
        "import sys\nsys.path.insert(0, sys.argv[1])\nimport helpermod\nassert helpermod.VALUE == 41, helpermod.VALUE\nprint('PASS: import without bytecode')\n",
    )
    .expect("write import check");
    run_git(&repo, &["init", "-q"]);
    run_git(&repo, &["config", "user.name", "k5-fixture"]);
    run_git(&repo, &["config", "user.email", "k5@local"]);
    run_git(&repo, &["add", "."]);
    run_git(&repo, &["commit", "-qm", "import check fixture"]);
    let point = Checkpoint::read(&repo);
    let scratch = temp.path().join("scratch");
    let worker_id = "test-run-k5bytecode-r0-candidate-001";
    let work_dir = scratch.join(worker_id);
    let policy = TestPolicy::freeze(
        vec![
            "python3".to_owned(),
            work_dir
                .join("check_import.py")
                .to_string_lossy()
                .into_owned(),
            work_dir.to_string_lossy().into_owned(),
        ],
        120,
    )
    .expect("freeze bytecode policy");
    let spec = TestWorkerSpec::new(
        "run-k5bytecode",
        0,
        "candidate-001",
        &point.commit,
        &point.tree,
        &policy,
    )
    .expect("bytecode worker spec");
    assert_eq!(spec.worker_id(), worker_id);
    let worker = prepare_test_worker(&repo, &spec, &scratch).expect("prepare bytecode worker");
    let execution = run_test_command(&policy, &worker).expect("run bytecode pinned command");
    assert_eq!(execution.exit_code(), 0, "import check must exit zero");
    assert!(
        execution.stdout().text().contains("PASS"),
        "import check names the pass, got: {}",
        execution.stdout().text()
    );
    let mut residue = Vec::new();
    let mut dirs = vec![worker.work_dir().to_owned()];
    while let Some(dir) = dirs.pop() {
        let entries = std::fs::read_dir(&dir).expect("read worker dir");
        for entry in entries {
            let entry = entry.expect("worker dir entry");
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name == "__pycache__") {
                    residue.push(path);
                } else {
                    dirs.push(path);
                }
            } else if path.extension().is_some_and(|ext| ext == "pyc") {
                residue.push(path);
            }
        }
    }
    assert!(
        residue.is_empty(),
        "pinned test command emitted bytecode residue: {residue:?}"
    );
}
