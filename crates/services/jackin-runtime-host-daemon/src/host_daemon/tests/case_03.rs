// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_workflow::{WorkflowBackend, WorkflowService};

fn round_trip(
    layout: &DaemonLayout,
    attention: &mut impl AttentionNotifierAdapter,
    workflow: &mut dyn WorkflowBackend,
    kind: DaemonRequestKind,
) -> DaemonResponseKind {
    let request = DaemonRequest {
        id: "workflow".to_owned(),
        protocol_version: DAEMON_PROTOCOL_VERSION,
        build_id: "test-build".to_owned(),
        ctx: TelemetryContext::v1(),
        kind,
    };
    handle_request_line_with_workflow(
        &serde_json::to_string(&request).unwrap(),
        layout,
        "test-build",
        &CoredumpPolicy::Disabled,
        attention,
        workflow,
    )
    .kind
}

#[test]
fn workflow_run_control_round_trip() {
    let (temp, _paths, layout) = layout();
    let mut attention = AttentionAdapter::new(RecordingNotifier::default());
    let mut workflow = WorkflowService::open(
        &temp.path().join("workflow-runs.db"),
        &temp.path().join("workflow-artifacts"),
    )
    .unwrap();

    let DaemonResponseKind::WorkflowRun { summary } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowStart {
            plan_hash: "ebc1278f".to_owned(),
            max_repair_turns: 2,
            idempotency_key: "key-1".to_owned(),
        },
    ) else {
        panic!("expected a workflow run summary");
    };
    assert_eq!(summary.status, "active");
    assert_eq!(summary.stage, "prepare");
    assert_eq!(summary.round, 0);
    let run_id = summary.run_id.clone();

    let busy = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowStart {
            plan_hash: "ebc1278f".to_owned(),
            max_repair_turns: 2,
            idempotency_key: "key-2".to_owned(),
        },
    );
    assert!(
        matches!(busy, DaemonResponseKind::Error { .. }),
        "second key while active must be busy"
    );

    let DaemonResponseKind::WorkflowRun { summary } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowStatus {
            run_id: Some(run_id.clone()),
        },
    ) else {
        panic!("expected a workflow status summary");
    };
    assert_eq!(summary.stage, "prepare");

    let DaemonResponseKind::WorkflowRun { summary } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowPause {
            run_id: run_id.clone(),
            reason: "hold".to_owned(),
            checkpoint: false,
        },
    ) else {
        panic!("expected a paused summary");
    };
    assert_eq!(summary.status, "paused");

    let DaemonResponseKind::WorkflowRun { summary } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowResume {
            run_id: run_id.clone(),
        },
    ) else {
        panic!("expected a resumed summary");
    };
    assert_eq!(summary.status, "active");

    let DaemonResponseKind::WorkflowRun { summary } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowCancel {
            run_id: run_id.clone(),
            reason: "done".to_owned(),
        },
    ) else {
        panic!("expected a cancelled summary");
    };
    assert_eq!(summary.status, "cancelled");
}

#[test]
fn workflow_list_detail_and_typed_errors() {
    let (temp, _paths, layout) = layout();
    let mut attention = AttentionAdapter::new(RecordingNotifier::default());
    let mut workflow = WorkflowService::open(
        &temp.path().join("workflow-runs.db"),
        &temp.path().join("workflow-artifacts"),
    )
    .unwrap();

    // Empty store: the list is empty, not an error.
    let DaemonResponseKind::WorkflowRuns { summaries } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowList { limit: 50 },
    ) else {
        panic!("expected a workflow run list");
    };
    assert!(summaries.is_empty());

    let DaemonResponseKind::WorkflowRun { summary } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowStart {
            plan_hash: "ebc1278f".to_owned(),
            max_repair_turns: 2,
            idempotency_key: "key-list".to_owned(),
        },
    ) else {
        panic!("expected a workflow run summary");
    };
    let run_id = summary.run_id.clone();

    let DaemonResponseKind::WorkflowRuns { summaries } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowList { limit: 50 },
    ) else {
        panic!("expected a workflow run list");
    };
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].run_id, run_id);

    let DaemonResponseKind::WorkflowRunDetail { detail } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowDetail {
            run_id: run_id.clone(),
            after_seq: -1,
            event_limit: 200,
        },
    ) else {
        panic!("expected a workflow run detail");
    };
    assert_eq!(detail.summary.run_id, run_id);
    assert!(
        detail
            .events
            .iter()
            .any(|event| event.kind == "run_started"),
        "detail carries the durable event history"
    );

    // Unknown run: typed not_found (the CLI maps it to exit 2).
    let DaemonResponseKind::Error { message, code } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowDetail {
            run_id: "run-missing".to_owned(),
            after_seq: -1,
            event_limit: 200,
        },
    ) else {
        panic!("expected a typed workflow error");
    };
    assert_eq!(code, "not_found", "unexpected message: {message}");

    // Second start while active: typed busy (CLI exit 4).
    let DaemonResponseKind::Error { code, .. } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowStart {
            plan_hash: "ebc1278f".to_owned(),
            max_repair_turns: 2,
            idempotency_key: "key-other".to_owned(),
        },
    ) else {
        panic!("expected a busy error");
    };
    assert_eq!(code, "busy");
}

fn git(dir: &Path, args: &[&str]) -> String {
    let child = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn git in record fixture");
    let output = child.wait_with_output().expect("wait for record git");
    assert!(
        output.status.success(),
        "record git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn record_summary(
    layout: &DaemonLayout,
    attention: &mut impl AttentionNotifierAdapter,
    workflow: &mut dyn WorkflowBackend,
    kind: DaemonRequestKind,
) -> RunSummary {
    match round_trip(layout, attention, workflow, kind) {
        DaemonResponseKind::WorkflowRecord { summary }
        | DaemonResponseKind::WorkflowRun { summary } => summary,
        DaemonResponseKind::Error { message, code } => {
            panic!("expected a record summary, got [{code}]: {message}")
        }
        other => panic!("expected a record summary, got {other:?}"),
    }
}

/// Store a minimal digest-verified plan with the given frozen
/// test binding, so the daemon-side binding check has a stored plan
/// to verify against. Returns the plan hash to start the run with.
fn store_test_plan(
    layout: &DaemonLayout,
    check_program: &str,
    bundle_digest: &str,
    timeout_secs: u64,
) -> String {
    use jackin_workflow_spec::model::{
        AgentSelection, BackupPolicy, CapabilityFacts, ExecutionPlan, InputFile, PlanNode,
        RepositoryInput, ResourceLimits, VerificationPolicy,
    };
    let agent = |runtime: &str| AgentSelection {
        runtime: runtime.to_owned(),
        agent_slug: "test-agent".to_owned(),
        role_id: "test-role".to_owned(),
        role_revision: "1".to_owned(),
        account_id: "test-account".to_owned(),
        auth_mode: "test".to_owned(),
        requested_model: "test".to_owned(),
        actual_model: None,
        binary_digest: "0".repeat(64),
        image_digest: "0".repeat(64),
    };
    let mut plan = ExecutionPlan {
        schema_version: 1,
        template: "repair".to_owned(),
        nodes: vec![PlanNode {
            id: "prepare@r0".to_owned(),
            stage: "prepare".to_owned(),
            round: 0,
        }],
        implementer: agent("claude"),
        reviewer: agent("codex"),
        task: InputFile {
            logical_path: "task.md".to_owned(),
            content_digest: "0".repeat(64),
            byte_length: 0,
            mount_path: "/inputs/task.md".to_owned(),
        },
        references: Vec::new(),
        repository: RepositoryInput {
            repository_id: "test-repo".to_owned(),
            base_commit: "0".repeat(40),
            manifest_digest: "0".repeat(64),
        },
        resources: ResourceLimits {
            cpu_millicores: 1_000,
            memory_bytes: 1_024,
            max_pids: 64,
            max_run_seconds: 600,
            disk_monitor_bytes: 1_024,
        },
        verification: VerificationPolicy {
            test_bundle: "fixture-checks".to_owned(),
            test_bundle_digest: bundle_digest.to_owned(),
            check_argv: vec![check_program.to_owned()],
            check_image_digest: "0".repeat(64),
            timeout_seconds: timeout_secs,
            review_schema_version: 1,
        },
        backup: BackupPolicy {
            mode: "off".to_owned(),
            remote_id: None,
            credential_ref: None,
            ref_prefix: None,
        },
        max_repair_turns: 2,
        input_digests: std::collections::BTreeMap::new(),
        capability_facts: CapabilityFacts {
            sandbox_backend: "test".to_owned(),
            sandbox_profile: "test".to_owned(),
            authorized_remote_ids: Vec::new(),
            evaluator: "test".to_owned(),
            schema_version: 1,
        },
        plan_hash: String::new(),
    };
    let hash = jackin_workflow_spec::digest::plan_hash(&plan).expect("hash test plan");
    plan.plan_hash = hash.clone();
    jackin_workflow_spec::store::InputStore::open(&layout.plan_store_dir)
        .expect("open plan store")
        .persist(&plan, &[])
        .expect("persist test plan");
    hash
}

/// K7 record path through the request line: session, candidate,
/// checkpoint, trusted test, review, backup, and advances walk a run
/// prepare -> implement -> capture -> test -> review -> decide, and
/// the acceptance gate accepts only the full green set.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "End-to-end record walk exercises every phase transition in stage order"
)]
fn workflow_record_round_trip_walks_to_accept() {
    let (temp, _paths, layout) = layout();
    let mut attention = AttentionAdapter::new(RecordingNotifier::default());
    let mut workflow = WorkflowService::open(
        &temp.path().join("workflow-runs.db"),
        &temp.path().join("workflow-artifacts"),
    )
    .unwrap();

    let repo = tempfile::tempdir().expect("record repo tempdir");
    git(repo.path(), &["init", "-q"]);
    git(repo.path(), &["config", "user.email", "k7@example.com"]);
    git(repo.path(), &["config", "user.name", "k7"]);
    fs::write(
        repo.path().join("calculator.py"),
        "def add(a, b):\n    return a + b\n",
    )
    .expect("stage candidate source");
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-qm", "k7 candidate"]);
    let commit = git(repo.path(), &["rev-parse", "HEAD"]);
    let tree = git(repo.path(), &["rev-parse", "HEAD^{tree}"]);

    let scratch = tempfile::tempdir().expect("record scratch tempdir");
    let script = scratch.path().join("check_add.py");
    fs::write(&script, "print('PASS')\n").expect("stage check script");
    let script_bytes = fs::read(&script).expect("read check script");
    let bundle_digest = {
        use sha2::{Digest as _, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&script_bytes);
        hex::encode(hasher.finalize())
    };

    let plan_hash = store_test_plan(&layout, "python3", &bundle_digest, 120);
    let DaemonResponseKind::WorkflowRun { summary } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowStart {
            plan_hash,
            max_repair_turns: 2,
            idempotency_key: "key-record".to_owned(),
        },
    ) else {
        panic!("expected a workflow run summary");
    };
    let run_id = summary.run_id.clone();

    // A moved row version refuses the record before touching state.
    let DaemonResponseKind::Error { code, .. } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowRecordSession {
            run_id: run_id.clone(),
            expected_version: summary.version + 1_000,
            worker: "implementer".to_owned(),
            instance_id: None,
            capsule_epoch: None,
            session_id: None,
            process_identity: None,
            agent_profile: None,
            auth_profile: None,
        },
    ) else {
        panic!("expected a stale version error");
    };
    assert_eq!(code, "state_conflict");

    // Unknown observations refuse before touching the run.
    let DaemonResponseKind::Error { code, .. } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowAdvance {
            run_id: run_id.clone(),
            expected_version: summary.version,
            actor: "host".to_owned(),
            observation: "teleport".to_owned(),
            expected_policy_hash: None,
        },
    ) else {
        panic!("expected an invalid observation error");
    };
    assert_eq!(code, "invalid_request");

    let recorded = record_summary(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowRecordSession {
            run_id: run_id.clone(),
            expected_version: summary.version,
            worker: "implementer".to_owned(),
            instance_id: Some("instance-k7".to_owned()),
            capsule_epoch: None,
            session_id: None,
            process_identity: None,
            agent_profile: Some("claude".to_owned()),
            auth_profile: Some("default-claude".to_owned()),
        },
    );
    assert_eq!(recorded.stage, "prepare");

    let recorded = record_summary(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowRecordCandidate {
            run_id: run_id.clone(),
            candidate: "cand-r0".to_owned(),
            expected_version: recorded.version,
        },
    );
    assert_eq!(recorded.candidate.as_deref(), Some("cand-r0"));

    let recorded = record_summary(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowAdvance {
            run_id: run_id.clone(),
            expected_version: recorded.version,
            actor: "host".to_owned(),
            observation: "turn_started".to_owned(),
            expected_policy_hash: None,
        },
    );
    assert_eq!(recorded.stage, "implement");

    let recorded = record_summary(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowAdvance {
            run_id: run_id.clone(),
            expected_version: recorded.version,
            actor: "host".to_owned(),
            observation: "turn_ended".to_owned(),
            expected_policy_hash: None,
        },
    );
    assert_eq!(recorded.stage, "capture");

    let recorded = record_summary(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowRecordCheckpoint {
            run_id: run_id.clone(),
            expected_version: recorded.version,
            candidate: "cand-r0".to_owned(),
            commit: commit.clone(),
            tree: tree.clone(),
            source_manifest: "manifest-k7".to_owned(),
            local_path: repo.path().to_string_lossy().into_owned(),
            backup_state: "pending".to_owned(),
        },
    );
    assert_eq!(recorded.stage, "capture");

    let recorded = record_summary(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowAdvance {
            run_id: run_id.clone(),
            expected_version: recorded.version,
            actor: "host".to_owned(),
            observation: "test_started".to_owned(),
            expected_policy_hash: None,
        },
    );
    assert_eq!(recorded.stage, "test");

    // Direct-RPC binding bypass: a program, bundle digest, or
    // timeout that disagrees with the stored plan refuses before any
    // worker launches, even with a current row version.
    for (program, digest, timeout) in [
        ("sh".to_owned(), bundle_digest.clone(), 120),
        ("python3".to_owned(), "deadbeef".to_owned(), 120),
        ("python3".to_owned(), bundle_digest.clone(), 7),
    ] {
        let DaemonResponseKind::Error { code, message } = round_trip(
            &layout,
            &mut attention,
            &mut workflow,
            DaemonRequestKind::WorkflowRecordTest {
                run_id: run_id.clone(),
                expected_version: recorded.version,
                check_program: program,
                check_script: script.display().to_string(),
                bundle_digest: digest,
                repo: Some(repo.path().to_string_lossy().into_owned()),
                scratch: scratch.path().to_string_lossy().into_owned(),
                timeout_secs: timeout,
            },
        ) else {
            panic!("expected a binding refusal");
        };
        assert_eq!(code, "invalid_request", "unexpected message: {message}");
    }

    let DaemonResponseKind::WorkflowTestRecord { summary, test } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowRecordTest {
            run_id: run_id.clone(),
            expected_version: recorded.version,
            check_program: "python3".to_owned(),
            check_script: script.display().to_string(),
            bundle_digest,
            repo: Some(repo.path().to_string_lossy().into_owned()),
            scratch: scratch.path().to_string_lossy().into_owned(),
            timeout_secs: 120,
        },
    ) else {
        panic!("expected a recorded test verdict");
    };
    assert!(test.passed);
    assert_eq!(test.exit_code, 0);
    assert!(!test.policy_hash.is_empty());

    let recorded = record_summary(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowAdvance {
            run_id: run_id.clone(),
            expected_version: summary.version,
            actor: "host".to_owned(),
            observation: "test_passed".to_owned(),
            expected_policy_hash: None,
        },
    );
    assert_eq!(recorded.stage, "review");

    let review_file = scratch.path().join("review.json");
    fs::write(
        &review_file,
        format!(
            r#"{{"schema_version":1,"candidate_id":"cand-r0","commit":"{commit}","tree":"{tree}","status":"pass","findings":[]}}"#
        ),
    )
    .expect("stage review");
    let DaemonResponseKind::WorkflowReviewRecord { summary, review } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowRecordReview {
            run_id: run_id.clone(),
            expected_version: recorded.version,
            review_file: review_file.display().to_string(),
            policy_hash: test.policy_hash.clone(),
        },
    ) else {
        panic!("expected a recorded review verdict");
    };
    assert!(review.acceptable);

    let recorded = record_summary(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowAdvance {
            run_id: run_id.clone(),
            expected_version: summary.version,
            actor: "host".to_owned(),
            observation: "review_passed".to_owned(),
            expected_policy_hash: None,
        },
    );
    assert_eq!(recorded.stage, "decide");

    // Acceptance without the expected policy hash refuses.
    let DaemonResponseKind::Error { code, .. } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowAdvance {
            run_id: run_id.clone(),
            expected_version: recorded.version,
            actor: "host".to_owned(),
            observation: "verified_current".to_owned(),
            expected_policy_hash: None,
        },
    ) else {
        panic!("expected a policy-hash error");
    };
    assert_eq!(code, "invalid_request");

    // Acceptance without a completed backup refuses.
    let DaemonResponseKind::Error { code, message } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowAdvance {
            run_id: run_id.clone(),
            expected_version: recorded.version,
            actor: "host".to_owned(),
            observation: "verified_current".to_owned(),
            expected_policy_hash: Some(test.policy_hash.clone()),
        },
    ) else {
        panic!("expected a backup refusal");
    };
    assert_eq!(code, "needs_operator", "unexpected message: {message}");

    let bare = tempfile::tempdir().expect("bare remote tempdir");
    let bare_path = bare.path().join("remote.git");
    git(bare.path(), &["init", "--bare", "-q", "remote.git"]);
    git(
        repo.path(),
        &[
            "push",
            &bare_path.to_string_lossy(),
            &format!("{commit}:refs/jackin/k7"),
        ],
    );
    let recorded = record_summary(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowRecordBackup {
            run_id: run_id.clone(),
            expected_version: recorded.version,
            remote: bare_path.to_string_lossy().into_owned(),
            git_ref: "refs/jackin/k7".to_owned(),
        },
    );
    assert_eq!(recorded.stage, "decide");

    let accepted = record_summary(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowAdvance {
            run_id: run_id.clone(),
            expected_version: recorded.version,
            actor: "host".to_owned(),
            observation: "verified_current".to_owned(),
            expected_policy_hash: Some(test.policy_hash.clone()),
        },
    );
    assert_eq!(accepted.status, "accepted");
    assert_eq!(accepted.stage, "decide");
}

/// `record_test` fails closed when the run's plan has no stored
/// copy: with nothing to verify the binding against, the daemon
/// refuses instead of freezing RPC-carried values.
#[test]
fn workflow_record_test_refuses_without_stored_plan() {
    let (temp, _paths, layout) = layout();
    let mut attention = AttentionAdapter::new(RecordingNotifier::default());
    let mut workflow = WorkflowService::open(
        &temp.path().join("workflow-runs.db"),
        &temp.path().join("workflow-artifacts"),
    )
    .unwrap();

    let repo = tempfile::tempdir().expect("record repo tempdir");
    git(repo.path(), &["init", "-q"]);
    git(repo.path(), &["config", "user.email", "k7@example.com"]);
    git(repo.path(), &["config", "user.name", "k7"]);
    fs::write(repo.path().join("calculator.py"), "x = 1\n").expect("stage source");
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-qm", "planless candidate"]);
    let commit = git(repo.path(), &["rev-parse", "HEAD"]);
    let tree = git(repo.path(), &["rev-parse", "HEAD^{tree}"]);

    let scratch = tempfile::tempdir().expect("record scratch tempdir");
    let script = scratch.path().join("check.py");
    fs::write(&script, "print('PASS')\n").expect("stage check script");

    let DaemonResponseKind::WorkflowRun { summary } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowStart {
            plan_hash: "missing-plan".to_owned(),
            max_repair_turns: 2,
            idempotency_key: "key-planless".to_owned(),
        },
    ) else {
        panic!("expected a workflow run summary");
    };
    let run_id = summary.run_id.clone();
    let recorded = record_summary(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowRecordCandidate {
            run_id: run_id.clone(),
            candidate: "cand-r0".to_owned(),
            expected_version: summary.version,
        },
    );
    let mut version = recorded.version;
    for observation in ["turn_started", "turn_ended"] {
        let next = record_summary(
            &layout,
            &mut attention,
            &mut workflow,
            DaemonRequestKind::WorkflowAdvance {
                run_id: run_id.clone(),
                expected_version: version,
                actor: "host".to_owned(),
                observation: observation.to_owned(),
                expected_policy_hash: None,
            },
        );
        version = next.version;
    }
    let recorded = record_summary(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowRecordCheckpoint {
            run_id: run_id.clone(),
            expected_version: version,
            candidate: "cand-r0".to_owned(),
            commit,
            tree,
            source_manifest: "manifest-k7".to_owned(),
            local_path: repo.path().to_string_lossy().into_owned(),
            backup_state: "pending".to_owned(),
        },
    );
    let recorded = record_summary(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowAdvance {
            run_id: run_id.clone(),
            expected_version: recorded.version,
            actor: "host".to_owned(),
            observation: "test_started".to_owned(),
            expected_policy_hash: None,
        },
    );
    let DaemonResponseKind::Error { code, message } = round_trip(
        &layout,
        &mut attention,
        &mut workflow,
        DaemonRequestKind::WorkflowRecordTest {
            run_id: run_id.clone(),
            expected_version: recorded.version,
            check_program: "python3".to_owned(),
            check_script: script.display().to_string(),
            bundle_digest: "digest".to_owned(),
            repo: Some(repo.path().to_string_lossy().into_owned()),
            scratch: scratch.path().to_string_lossy().into_owned(),
            timeout_secs: 120,
        },
    ) else {
        panic!("expected a missing-plan refusal");
    };
    assert_eq!(code, "invalid_request", "unexpected message: {message}");
    assert!(
        message.contains("cannot verify the test binding"),
        "refusal names the unverifiable binding: {message}"
    );
}
