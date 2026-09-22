use super::*;
use std::{fs, path::Path, process::Command};

fn commit_from_source(sha: &str, source: DenominatorSource) -> ExpectedCommit {
    ExpectedCommit {
        sha: sha.to_owned(),
        base_sha: Some("base".to_owned()),
        tree_sha: "tree".to_owned(),
        committed_at: Some("2026-09-22T00:00:00Z".to_owned()),
        source,
    }
}

fn obligation(sha: &str, cohort: Cohort) -> ExpectedObligation {
    obligation_from_source(sha, cohort, DenominatorSource::Fixture)
}

fn expected_for_sha(sha: &str) -> Vec<ExpectedObligation> {
    Cohort::ALL
        .into_iter()
        .map(|cohort| obligation(sha, cohort))
        .collect()
}

fn obligation_from_source(
    sha: &str,
    cohort: Cohort,
    source: DenominatorSource,
) -> ExpectedObligation {
    ExpectedObligation {
        commit: commit_from_source(sha, source),
        cohort,
        provenance: match source {
            DenominatorSource::PushHeadLedger => ObligationProvenance::PushHeadLedger,
            DenominatorSource::Fixture => ObligationProvenance::Fixture,
        },
    }
}

fn test_runtime() -> RuntimeIdentity {
    RuntimeIdentity {
        runtime_revision: Some("0123456789abcdef0123456789abcdef01234567".to_owned()),
        contract_digest: Some(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_owned(),
        ),
    }
}

fn test_provenance() -> CollectionProvenance {
    CollectionProvenance {
        repository: "example/repo".to_owned(),
        branch: "main".to_owned(),
        event: "test".to_owned(),
        workflow_path: "test".to_owned(),
        run_id: None,
    }
}

fn attempt(
    run_id: u64,
    attempt_number: u32,
    cohort: Cohort,
    sha: &str,
    classification: OutcomeClass,
    created_at: &str,
) -> AttemptEvidence {
    AttemptEvidence {
        run_id,
        attempt: attempt_number,
        is_first_attempt: attempt_number == 1,
        cohort,
        workflow_id: Some(42),
        workflow_name: Some("renamed workflow".to_owned()),
        workflow_path: Some(".github/workflows/renamed.yml".to_owned()),
        event: Some("push".to_owned()),
        head_sha: sha.to_owned(),
        denominator_source: DenominatorSource::Fixture,
        base_sha: Some("base".to_owned()),
        tree_sha: "tree".to_owned(),
        created_at: created_at.to_owned(),
        started_at: Some(created_at.to_owned()),
        completed_at: Some(created_at.to_owned()),
        duration_seconds: Some(60),
        within_120_seconds: Some(true),
        status: "completed".to_owned(),
        conclusion: Some(
            match classification {
                OutcomeClass::Success => "success",
                OutcomeClass::Cancellation => "cancelled",
                OutcomeClass::Inapplicable => "skipped",
                OutcomeClass::Infrastructure => "timed_out",
                _ => "failure",
            }
            .to_owned(),
        ),
        expected_work: cohort
            .expected_work()
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
        observed_work: cohort
            .expected_work()
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
        jobs: completed_jobs(cohort),
        classification,
        data_quality_reason: None,
        conflicting_observations: Vec::new(),
        raw_observations: vec![RawAttemptObservation {
            status: "completed".to_owned(),
            conclusion: Some(
                match classification {
                    OutcomeClass::Success => "success",
                    OutcomeClass::Cancellation => "cancelled",
                    OutcomeClass::Inapplicable => "skipped",
                    OutcomeClass::Infrastructure => "timed_out",
                    _ => "failure",
                }
                .to_owned(),
            ),
            jobs: completed_jobs(cohort),
        }],
        runtime: test_runtime(),
        evidence_urls: vec![format!("https://example.test/runs/{run_id}")],
        first_observed_at: "2026-09-22T00:01:00Z".to_owned(),
    }
}

fn evidence(expected: Vec<ExpectedObligation>, attempts: Vec<AttemptEvidence>) -> EvidenceFile {
    let history = expected
        .iter()
        .map(|obligation| {
            let history = HistoryCommitObservation {
                sha: obligation.commit.sha.clone(),
                base_sha: obligation.commit.base_sha.clone(),
                tree_sha: obligation.commit.tree_sha.clone(),
                committed_at: obligation
                    .commit
                    .committed_at
                    .clone()
                    .unwrap_or_else(|| "2026-09-22T00:00:00Z".to_owned()),
            };
            (history.sha.clone(), history)
        })
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .collect::<Vec<_>>();
    EvidenceFile {
        schema: SCHEMA,
        repository: "example/repo".to_owned(),
        window: TimeWindow {
            since: "2026-09-21T00:00:00Z".to_owned(),
            until: "2026-09-23T00:00:00Z".to_owned(),
        },
        generated_at: "2026-09-22T00:00:00Z".to_owned(),
        runtime: test_runtime(),
        provenance: CollectionProvenance {
            repository: "example/repo".to_owned(),
            branch: "main".to_owned(),
            event: "test".to_owned(),
            workflow_path: "test".to_owned(),
            run_id: None,
        },
        denominator: DenominatorProof {
            source: DenominatorSource::Fixture,
            branch: "main".to_owned(),
            window: TimeWindow {
                since: "2026-09-21T00:00:00Z".to_owned(),
                until: "2026-09-23T00:00:00Z".to_owned(),
            },
            fetch_succeeded: false,
            commit_count: history.len(),
            source_workflow: None,
            source_run_count: 0,
            boundary: DenominatorBoundary::Fixture,
        },
        history,
        push_heads: Vec::new(),
        expected,
        attempts,
        unclassified_runs: Vec::new(),
    }
}

fn update_denominator(
    expected: &[ExpectedObligation],
) -> (DenominatorProof, Vec<HistoryCommitObservation>) {
    let history = expected
        .iter()
        .map(|obligation| {
            let history = HistoryCommitObservation {
                sha: obligation.commit.sha.clone(),
                base_sha: obligation.commit.base_sha.clone(),
                tree_sha: obligation.commit.tree_sha.clone(),
                committed_at: obligation.commit.committed_at.clone().unwrap(),
            };
            (history.sha.clone(), history)
        })
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .collect::<Vec<_>>();
    (
        DenominatorProof {
            source: DenominatorSource::Fixture,
            branch: "main".to_owned(),
            window: TimeWindow {
                since: "2026-09-21T00:00:00Z".to_owned(),
                until: "2026-09-23T00:00:00Z".to_owned(),
            },
            fetch_succeeded: false,
            commit_count: history.len(),
            source_workflow: None,
            source_run_count: 0,
            boundary: DenominatorBoundary::Fixture,
        },
        history,
    )
}

fn push_head_observation(head_sha: &str, before_sha: &str, run_id: u64) -> PushHeadObservation {
    let head_sha = fixture_sha(head_sha);
    let before_sha = fixture_sha(before_sha);
    let tree_sha = fixture_sha(&format!("tree-{head_sha}"));
    let mut observation = PushHeadObservation {
        repository: "example/repo".to_owned(),
        branch: "main".to_owned(),
        event: "push".to_owned(),
        workflow_id: 7,
        workflow_path: DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW.to_owned(),
        run_id,
        head_sha,
        before_sha,
        tree_sha,
        committed_at: "2026-09-22T00:00:00Z".to_owned(),
        created_at: "2026-09-22T00:01:00Z".to_owned(),
        pushed_commits: Vec::new(),
        raw_event_sha256: String::new(),
        artifact: PushHeadArtifactProof {
            manifest: String::new(),
            event: String::new(),
            manifest_sha256: String::new(),
        },
    };
    observation.pushed_commits = vec![observation.head_sha.clone()];
    refresh_push_head_artifact(&mut observation);
    observation
}

fn refresh_push_head_artifact(observation: &mut PushHeadObservation) {
    let raw_event = serde_json::json!({
        "repository": {"full_name": observation.repository},
        "ref": format!("refs/heads/{}", observation.branch),
        "before": observation.before_sha,
        "after": observation.head_sha,
        "commits": observation
            .pushed_commits
            .iter()
            .map(|id| serde_json::json!({"id": id}))
            .collect::<Vec<_>>(),
    });
    let raw_event_bytes = serde_json::to_vec(&raw_event).unwrap();
    observation.raw_event_sha256 = sha256_hex(&raw_event_bytes);
    let manifest = serde_json::json!({
        "schema": PUSH_HEAD_LEDGER_SCHEMA,
        "repository": observation.repository,
        "branch": observation.branch,
        "event": observation.event,
        "workflow_path": observation.workflow_path,
        "run_id": observation.run_id,
        "head_sha": observation.head_sha,
        "before_sha": observation.before_sha,
        "tree_sha": observation.tree_sha,
        "committed_at": observation.committed_at,
        "pushed_commits": observation.pushed_commits,
        "raw_event_sha256": observation.raw_event_sha256,
    });
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    observation.artifact = PushHeadArtifactProof {
        manifest: String::from_utf8(manifest_bytes.clone()).unwrap(),
        event: String::from_utf8(raw_event_bytes).unwrap(),
        manifest_sha256: sha256_hex(&manifest_bytes),
    };
}

fn fixture_sha(label: &str) -> String {
    sha256_hex(label.as_bytes())[..40].to_owned()
}

fn run_git(root: &Path, args: &[&str]) -> String {
    let output = cmd::output(Command::new("git").current_dir(root).args(args))
        .unwrap_or_else(|error| panic!("running git {args:?}: {error}"));
    String::from_utf8(output)
        .unwrap_or_else(|error| panic!("git {args:?} returned non-UTF-8: {error}"))
        .trim()
        .to_owned()
}

fn commit_git(root: &Path, message: &str, date: &str) {
    cmd::run(
        Command::new("git")
            .current_dir(root)
            .args(["commit", "--quiet", "-m", message])
            .env("GIT_AUTHOR_NAME", "CI evidence test")
            .env("GIT_AUTHOR_EMAIL", "ci-evidence@example.test")
            .env("GIT_COMMITTER_NAME", "CI evidence test")
            .env("GIT_COMMITTER_EMAIL", "ci-evidence@example.test")
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date),
    )
    .unwrap_or_else(|error| panic!("committing {message}: {error}"));
}

fn push_head_artifact_fixture() -> (tempfile::TempDir, ApiRun, Vec<u8>, Vec<u8>) {
    let repository = tempfile::tempdir().unwrap();
    run_git(repository.path(), &["init", "--quiet"]);
    run_git(
        repository.path(),
        &["config", "user.name", "CI evidence test"],
    );
    run_git(
        repository.path(),
        &["config", "user.email", "ci-evidence@example.test"],
    );
    fs::write(repository.path().join("base.txt"), "base\n").unwrap();
    run_git(repository.path(), &["add", "base.txt"]);
    commit_git(repository.path(), "base", "2026-09-22T00:00:00Z");
    let before_sha = run_git(repository.path(), &["rev-parse", "HEAD"]);

    fs::write(repository.path().join("head.txt"), "head\n").unwrap();
    run_git(repository.path(), &["add", "head.txt"]);
    commit_git(repository.path(), "head", "2026-09-22T01:00:00Z");
    let head_sha = run_git(repository.path(), &["rev-parse", "HEAD"]);
    let tree_sha = run_git(repository.path(), &["rev-parse", "HEAD^{tree}"]);
    let committed_at = run_git(repository.path(), &["show", "-s", "--format=%cI", "HEAD"]);
    let raw_event = serde_json::json!({
        "repository": {"full_name": "example/repo"},
        "ref": "refs/heads/main",
        "before": before_sha.clone(),
        "after": head_sha.clone(),
        "commits": [{"id": head_sha.clone()}],
    });
    let raw_event_bytes = serde_json::to_vec(&raw_event).unwrap();
    let manifest = serde_json::json!({
        "schema": PUSH_HEAD_LEDGER_SCHEMA,
        "repository": "example/repo",
        "branch": "main",
        "event": "push",
        "workflow_path": DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW,
        "run_id": 42,
        "head_sha": head_sha.clone(),
        "before_sha": before_sha,
        "tree_sha": tree_sha,
        "committed_at": committed_at,
        "pushed_commits": [head_sha.clone()],
        "raw_event_sha256": sha256_hex(&raw_event_bytes),
    });
    let run = ApiRun {
        id: 42,
        workflow_id: Some(7),
        workflow_name: Some("push-head ledger".to_owned()),
        path: Some(format!(
            ".github/workflows/{DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW}"
        )),
        event: Some("push".to_owned()),
        head_branch: Some("main".to_owned()),
        head_sha,
        status: "completed".to_owned(),
        conclusion: Some("success".to_owned()),
        run_attempt: 1,
        created_at: "2026-09-22T01:01:00Z".to_owned(),
        html_url: None,
    };
    (
        repository,
        run,
        serde_json::to_vec(&manifest).unwrap(),
        raw_event_bytes,
    )
}

#[test]
fn push_head_artifact_binds_manifest_raw_event_and_git_head() {
    let (repository, run, manifest, raw_event) = push_head_artifact_fixture();
    let observation = validate_push_head_artifact(
        repository.path(),
        "example/repo",
        "main",
        &run,
        7,
        (manifest.clone(), raw_event.clone()),
    )
    .unwrap();
    assert_eq!(observation.head_sha, run.head_sha);
    assert_eq!(observation.run_id, run.id);

    let mut forged_manifest: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    forged_manifest["run_id"] = serde_json::json!(43);
    let error = validate_push_head_artifact(
        repository.path(),
        "example/repo",
        "main",
        &run,
        7,
        (
            serde_json::to_vec(&forged_manifest).unwrap(),
            raw_event.clone(),
        ),
    )
    .unwrap_err();
    assert!(error.to_string().contains("mismatched provenance"));

    let mut forged_event: serde_json::Value = serde_json::from_slice(&raw_event).unwrap();
    forged_event["after"] = serde_json::json!(fixture_sha("different-head"));
    let forged_event = serde_json::to_vec(&forged_event).unwrap();
    forged_manifest["run_id"] = serde_json::json!(run.id);
    forged_manifest["raw_event_sha256"] = serde_json::json!(sha256_hex(&forged_event));
    let error = validate_push_head_artifact(
        repository.path(),
        "example/repo",
        "main",
        &run,
        7,
        (serde_json::to_vec(&forged_manifest).unwrap(), forged_event),
    )
    .unwrap_err();
    assert!(error.to_string().contains("raw push event"));
}

#[test]
fn push_head_artifact_requires_exact_file_set() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("push-head.json"), b"{}").unwrap();
    let error = read_push_head_artifact(directory.path(), 42).unwrap_err();
    assert!(error.to_string().contains("unexpected files"));

    fs::write(directory.path().join("event.json"), b"{}").unwrap();
    fs::write(directory.path().join("unexpected.txt"), b"").unwrap();
    let error = read_push_head_artifact(directory.path(), 42).unwrap_err();
    assert!(error.to_string().contains("unexpected files"));
}

fn push_head_denominator(
    history: Vec<HistoryCommitObservation>,
    push_heads: Vec<PushHeadObservation>,
) -> DenominatorProof {
    DenominatorProof {
        source: DenominatorSource::PushHeadLedger,
        branch: "main".to_owned(),
        window: TimeWindow {
            since: "2026-09-21T00:00:00Z".to_owned(),
            until: "2026-09-23T00:00:00Z".to_owned(),
        },
        fetch_succeeded: true,
        commit_count: history.len(),
        source_workflow: Some(DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW.to_owned()),
        source_run_count: push_heads.len(),
        boundary: push_heads
            .first()
            .map_or(DenominatorBoundary::Fixture, |first| {
                let mut predecessor = push_head_observation("boundary", "boundary-before", 99);
                predecessor.head_sha = first.before_sha.clone();
                predecessor.created_at = "2026-09-21T23:59:00Z".to_owned();
                predecessor.pushed_commits = vec![predecessor.head_sha.clone()];
                refresh_push_head_artifact(&mut predecessor);
                DenominatorBoundary::PushHead {
                    predecessor: Box::new(predecessor),
                }
            }),
    }
}

fn completed_jobs(cohort: Cohort) -> Vec<JobEvidence> {
    cohort
        .expected_work()
        .iter()
        .enumerate()
        .map(|(index, name)| JobEvidence {
            id: index as u64 + 1,
            name: (*name).to_owned(),
            status: "completed".to_owned(),
            conclusion: Some("success".to_owned()),
            started_at: Some("2026-09-22T00:00:00Z".to_owned()),
            completed_at: Some("2026-09-22T00:01:00Z".to_owned()),
            evidence_url: None,
        })
        .collect()
}

#[test]
fn paginated_object_pages_are_all_decoded() {
    let pages = vec![
        serde_json::json!({"jobs": [{"id": 1, "name": "a"}]}),
        serde_json::json!({"jobs": [{"id": 2, "name": "b"}]}),
    ];
    let jobs: Vec<ApiJob> = decode_pages(&pages, "jobs").unwrap();
    assert_eq!(jobs.iter().map(|job| job.id).collect::<Vec<_>>(), [1, 2]);
}

#[test]
fn array_pages_are_all_decoded() {
    let pages = vec![serde_json::json!([{"id": 1}, {"id": 2}])];
    let attempts: Vec<ApiAttempt> = decode_pages(&pages, "workflow_runs").unwrap();
    assert_eq!(attempts.len(), 2);
}

#[test]
fn retired_workflow_alias_is_unclassified_without_stable_id() {
    let run = ApiRun {
        id: 1,
        workflow_id: Some(99),
        workflow_name: Some("CI / Main".to_owned()),
        path: Some(".github/workflows/ci-main-v2.yml".to_owned()),
        event: Some("push".to_owned()),
        head_branch: Some("main".to_owned()),
        head_sha: "sha".to_owned(),
        status: "completed".to_owned(),
        conclusion: Some("success".to_owned()),
        run_attempt: 1,
        created_at: "2026-09-22T00:00:00Z".to_owned(),
        html_url: None,
    };
    assert_eq!(
        classify_workflow(&run, &BTreeSet::new(), &BTreeSet::new()),
        None
    );
}

#[test]
fn stable_workflow_id_survives_path_and_name_rename() {
    let run = ApiRun {
        id: 1,
        workflow_id: Some(99),
        workflow_name: Some("renamed".to_owned()),
        path: Some(".github/workflows/renamed.yml".to_owned()),
        event: Some("push".to_owned()),
        head_branch: Some("main".to_owned()),
        head_sha: "sha".to_owned(),
        status: "completed".to_owned(),
        conclusion: Some("success".to_owned()),
        run_attempt: 1,
        created_at: "2026-09-22T00:00:00Z".to_owned(),
        html_url: None,
    };
    assert_eq!(
        classify_workflow(&run, &BTreeSet::from([99]), &BTreeSet::new()),
        Some(Cohort::CiMain)
    );
}

#[test]
fn zero_run_attempt_is_rejected() {
    let error = checked_attempt_count(7, 0).unwrap_err();
    assert!(error.to_string().contains("no run attempt number"));
}

#[test]
fn required_skipped_cohort_is_non_green() {
    let expected = expected_for_sha("skipped");
    let row = attempt(
        7,
        1,
        Cohort::CiMain,
        "skipped",
        OutcomeClass::Inapplicable,
        "2026-09-22T00:02:00Z",
    );
    let rollup = build_rollup(&evidence(expected, vec![row]));
    assert_eq!(rollup.cohorts[0].inapplicable, 1);
    assert_eq!(rollup.total_first_attempt_failures, 2);
    assert!(!rollup.green_claim_qualified);
    assert!(require_qualified(&rollup).is_err());
}

#[test]
fn conflict_marker_requires_raw_conflicting_observations() {
    let expected = expected_for_sha("conflict");
    let existing = evidence(
        expected.clone(),
        vec![attempt(
            10,
            1,
            Cohort::CiMain,
            "conflict",
            OutcomeClass::Product,
            "2026-09-22T00:02:00Z",
        )],
    );
    let (denominator, history) = update_denominator(&expected);
    let merged = merge_evidence(
        existing,
        EvidenceUpdate {
            expected,
            attempts: vec![attempt(
                10,
                1,
                Cohort::CiMain,
                "conflict",
                OutcomeClass::Success,
                "2026-09-22T00:03:00Z",
            )],
            unclassified_runs: Vec::new(),
            denominator,
            history,
            push_heads: Vec::new(),
            repository: "example/repo".to_owned(),
            window: TimeWindow {
                since: "2026-09-21T00:00:00Z".to_owned(),
                until: "2026-09-23T00:00:00Z".to_owned(),
            },
            runtime: test_runtime(),
            provenance: test_provenance(),
        },
    )
    .unwrap();
    assert_eq!(
        merged.attempts[0].data_quality_reason,
        Some(DataQualityReason::ConflictingTerminalObservation)
    );
    assert_eq!(merged.attempts[0].classification, OutcomeClass::DataQuality);
    assert_eq!(merged.attempts[0].conflicting_observations.len(), 2);
    assert!(merged.attempts[0].raw_observations.len() >= 2);
    validate_evidence(&merged).unwrap();
    let mut raw_forged = merged.clone();
    raw_forged.attempts[0].raw_observations.clear();
    let error = validate_evidence(&raw_forged).unwrap_err();
    assert!(error.to_string().contains("retained row"));
    let mut forged = merged;
    forged.attempts[0].conflicting_observations.clear();
    let error = validate_evidence(&forged).unwrap_err();
    assert!(error.to_string().contains("unproven data-quality conflict"));
}

#[test]
fn same_class_terminal_change_remains_sticky_conflict() {
    let expected = expected_for_sha("same-class");
    let existing = evidence(
        expected.clone(),
        vec![attempt(
            10,
            1,
            Cohort::CiMain,
            "same-class",
            OutcomeClass::Product,
            "2026-09-22T00:02:00Z",
        )],
    );
    let (denominator, history) = update_denominator(&expected);
    let mut incoming = attempt(
        10,
        1,
        Cohort::CiMain,
        "same-class",
        OutcomeClass::Product,
        "2026-09-22T00:03:00Z",
    );
    incoming.jobs[0].id = 99;
    let merged = merge_evidence(
        existing,
        EvidenceUpdate {
            expected,
            attempts: vec![incoming],
            unclassified_runs: Vec::new(),
            denominator,
            history,
            push_heads: Vec::new(),
            repository: "example/repo".to_owned(),
            window: TimeWindow {
                since: "2026-09-21T00:00:00Z".to_owned(),
                until: "2026-09-23T00:00:00Z".to_owned(),
            },
            runtime: test_runtime(),
            provenance: test_provenance(),
        },
    )
    .unwrap();
    assert_eq!(merged.attempts[0].classification, OutcomeClass::DataQuality);
    assert_eq!(merged.attempts[0].conflicting_observations.len(), 2);
    validate_evidence(&merged).unwrap();
}

#[test]
fn active_attempt_has_no_terminal_timing_verdict() {
    let expected = expected_for_sha("active");
    let run = ApiRun {
        id: 9,
        workflow_id: Some(42),
        workflow_name: Some("CI/Main".to_owned()),
        path: Some(".github/workflows/ci-main.yml".to_owned()),
        event: Some("push".to_owned()),
        head_branch: Some("main".to_owned()),
        head_sha: "active".to_owned(),
        status: "in_progress".to_owned(),
        conclusion: None,
        run_attempt: 1,
        created_at: "2026-09-22T00:00:00Z".to_owned(),
        html_url: None,
    };
    let api_attempt = ApiAttempt {
        run_attempt: 1,
        status: "in_progress".to_owned(),
        conclusion: None,
        created_at: "2026-09-22T00:00:00Z".to_owned(),
        run_started_at: Some("2026-09-22T00:00:05Z".to_owned()),
        html_url: None,
    };
    let jobs = completed_jobs(Cohort::CiMain)
        .into_iter()
        .map(|job| ApiJob {
            id: job.id,
            name: job.name,
            status: job.status,
            conclusion: job.conclusion,
            started_at: job.started_at,
            completed_at: job.completed_at,
            html_url: job.evidence_url,
        })
        .collect();
    let normalized = normalize_attempt(
        &run,
        &api_attempt,
        Cohort::CiMain,
        jobs,
        &expected,
        RuntimeIdentity::default(),
    )
    .unwrap();
    assert_eq!(normalized.classification, OutcomeClass::DataQuality);
    assert!(normalized.completed_at.is_none());
    assert!(normalized.duration_seconds.is_none());
    assert!(normalized.within_120_seconds.is_none());
}

#[test]
fn merge_deduplicates_delivery_and_keeps_rerun_attempt() {
    let expected = expected_for_sha("sha");
    let (denominator, history) = update_denominator(&expected);
    let existing = evidence(
        expected.clone(),
        vec![attempt(
            10,
            1,
            Cohort::CiMain,
            "sha",
            OutcomeClass::Product,
            "2026-09-22T00:02:00Z",
        )],
    );
    let merged = merge_evidence(
        existing,
        EvidenceUpdate {
            expected,
            attempts: vec![
                attempt(
                    10,
                    1,
                    Cohort::CiMain,
                    "sha",
                    OutcomeClass::Product,
                    "2026-09-22T00:02:00Z",
                ),
                attempt(
                    10,
                    2,
                    Cohort::CiMain,
                    "sha",
                    OutcomeClass::Success,
                    "2026-09-22T00:03:00Z",
                ),
            ],
            unclassified_runs: Vec::new(),
            denominator,
            history,
            push_heads: Vec::new(),
            repository: "example/repo".to_owned(),
            window: TimeWindow {
                since: "2026-09-21T00:00:00Z".to_owned(),
                until: "2026-09-23T00:00:00Z".to_owned(),
            },
            runtime: test_runtime(),
            provenance: test_provenance(),
        },
    )
    .unwrap();
    assert_eq!(merged.attempts.len(), 2);
    assert!(merged.attempts.iter().any(|row| row.attempt == 1));
    assert!(merged.attempts.iter().any(|row| row.attempt == 2));
}

#[test]
fn merge_does_not_replace_terminal_observation_with_stale_in_progress_row() {
    let expected = expected_for_sha("sha");
    let (denominator, history) = update_denominator(&expected);
    let existing = evidence(
        expected.clone(),
        vec![attempt(
            10,
            1,
            Cohort::CiMain,
            "sha",
            OutcomeClass::Product,
            "2026-09-22T00:02:00Z",
        )],
    );
    let mut stale = attempt(
        10,
        1,
        Cohort::CiMain,
        "sha",
        OutcomeClass::DataQuality,
        "2026-09-22T00:03:00Z",
    );
    stale.status = "in_progress".to_owned();
    stale.conclusion = None;
    let merged = merge_evidence(
        existing,
        EvidenceUpdate {
            expected,
            attempts: vec![stale],
            unclassified_runs: Vec::new(),
            denominator,
            history,
            push_heads: Vec::new(),
            repository: "example/repo".to_owned(),
            window: TimeWindow {
                since: "2026-09-21T00:00:00Z".to_owned(),
                until: "2026-09-23T00:00:00Z".to_owned(),
            },
            runtime: test_runtime(),
            provenance: test_provenance(),
        },
    )
    .unwrap();
    assert_eq!(merged.attempts[0].classification, OutcomeClass::Product);
    assert_eq!(merged.attempts[0].status, "completed");
    assert!(
        merged.attempts[0]
            .raw_observations
            .iter()
            .any(|observation| observation.status == "in_progress")
    );
    validate_evidence(&merged).unwrap();
}

#[test]
fn rollup_counts_missing_and_does_not_recode_cancelled_rerun() {
    let evidence = evidence(
        vec![
            obligation("sha-a", Cohort::CiMain),
            obligation("sha-a", Cohort::Desktop),
            obligation("sha-b", Cohort::CiMain),
            obligation("sha-b", Cohort::Desktop),
        ],
        vec![
            attempt(
                1,
                1,
                Cohort::CiMain,
                "sha-a",
                OutcomeClass::Cancellation,
                "2026-09-22T00:01:00Z",
            ),
            attempt(
                1,
                2,
                Cohort::CiMain,
                "sha-a",
                OutcomeClass::Success,
                "2026-09-22T00:02:00Z",
            ),
            attempt(
                2,
                1,
                Cohort::Desktop,
                "sha-a",
                OutcomeClass::Success,
                "2026-09-22T00:01:30Z",
            ),
        ],
    );
    let rollup = build_rollup(&evidence);
    let ci = &rollup.cohorts[0];
    assert_eq!(ci.cancellation, 1);
    assert_eq!(ci.missing, 1);
    assert_eq!(rollup.commits[0].end_to_end, OutcomeClass::Cancellation);
    assert!(!rollup.six_nines_claimed);
}

#[test]
fn expected_duplicates_fail_closed() {
    let error = validate_expected(&[
        obligation("sha", Cohort::CiMain),
        obligation("sha", Cohort::CiMain),
    ])
    .unwrap_err();
    assert!(error.to_string().contains("duplicate expected"));
}

#[test]
fn outcome_classes_keep_platform_failures_separate() {
    assert_eq!(
        classify_outcome("in_progress", None, &[], &[]),
        OutcomeClass::DataQuality
    );
    let jobs = completed_jobs(Cohort::CiMain);
    let expected_work = Cohort::CiMain
        .expected_work()
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        classify_outcome("in_progress", Some("success"), &jobs, &expected_work),
        OutcomeClass::DataQuality
    );
    assert_eq!(
        classify_outcome("completed", Some("timed_out"), &[], &[]),
        OutcomeClass::Infrastructure
    );
    assert_eq!(
        classify_outcome("completed", Some("success"), &[], &[]),
        OutcomeClass::Infrastructure
    );
    assert_eq!(
        classify_outcome("completed", Some("cancelled"), &[], &[]),
        OutcomeClass::Cancellation
    );
    assert_eq!(
        classify_outcome("completed", Some("skipped"), &[], &[]),
        OutcomeClass::Inapplicable
    );
    assert_eq!(
        classify_outcome("completed", Some("failure"), &[JobEvidence::default()], &[]),
        OutcomeClass::Product
    );
    assert_eq!(
        classify_outcome(
            "completed",
            Some("success"),
            &jobs[..jobs.len() - 1],
            &Cohort::CiMain
                .expected_work()
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>(),
        ),
        OutcomeClass::DataQuality
    );
    assert_eq!(
        classify_outcome(
            "completed",
            Some("success"),
            &jobs,
            &Cohort::CiMain
                .expected_work()
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>(),
        ),
        OutcomeClass::Success
    );
}

#[test]
fn forged_success_classification_is_rejected() {
    let expected = expected_for_sha("sha");
    let mut row = attempt(
        10,
        1,
        Cohort::CiMain,
        "sha",
        OutcomeClass::Success,
        "2026-09-22T00:02:00Z",
    );
    row.jobs = vec![JobEvidence::default()];
    row.observed_work = vec!["wrong".to_owned()];
    row.raw_observations = vec![raw_attempt_observation(&row)];
    let error = validate_evidence(&evidence(expected, vec![row])).unwrap_err();
    assert!(error.to_string().contains("classification"));
}

#[test]
fn duplicate_first_attempts_are_data_quality() {
    let expected = vec![obligation("sha", Cohort::CiMain)];
    let mut first = attempt(
        10,
        1,
        Cohort::CiMain,
        "sha",
        OutcomeClass::Product,
        "2026-09-22T00:02:00Z",
    );
    first.jobs = completed_jobs(Cohort::CiMain);
    first.observed_work = first.jobs.iter().map(|job| job.name.clone()).collect();
    first.classification = OutcomeClass::Success;
    first.conclusion = Some("success".to_owned());
    let mut duplicate = first.clone();
    duplicate.run_id = 11;
    let rollup = build_rollup(&evidence(expected, vec![first, duplicate]));
    assert_eq!(rollup.cohorts[0].data_quality, 1);
}

#[test]
fn denominator_history_derives_both_contract_obligations() {
    let history = vec![HistoryCommitObservation {
        sha: "main-head".to_owned(),
        base_sha: Some("base".to_owned()),
        tree_sha: "tree".to_owned(),
        committed_at: "2026-09-22T00:00:00Z".to_owned(),
    }];
    let expected = expected_from_history(&history, DenominatorSource::Fixture).unwrap();

    assert_eq!(expected.len(), Cohort::ALL.len());
    assert!(expected.iter().all(|obligation| {
        obligation.commit.source == DenominatorSource::Fixture
            && obligation.provenance == ObligationProvenance::Fixture
    }));
}

#[test]
fn push_head_ledger_counts_one_obligation_unit_per_push_head() {
    let history = vec![HistoryCommitObservation {
        sha: "push-head".to_owned(),
        base_sha: Some("before".to_owned()),
        tree_sha: "tree-push-head".to_owned(),
        committed_at: "2026-09-22T00:00:00Z".to_owned(),
    }];
    let expected = expected_from_history(&history, DenominatorSource::PushHeadLedger).unwrap();

    assert_eq!(expected.len(), Cohort::ALL.len());
    assert!(expected.iter().all(|obligation| {
        obligation.commit.sha == "push-head"
            && obligation.commit.source == DenominatorSource::PushHeadLedger
            && obligation.provenance == ObligationProvenance::PushHeadLedger
    }));
}

#[test]
fn missing_push_head_ledger_proof_is_rejected() {
    let window = TimeWindow {
        since: "2026-09-21T00:00:00Z".to_owned(),
        until: "2026-09-23T00:00:00Z".to_owned(),
    };
    let proof = DenominatorProof {
        source: DenominatorSource::PushHeadLedger,
        branch: "main".to_owned(),
        window: window.clone(),
        fetch_succeeded: true,
        commit_count: 0,
        source_workflow: Some(DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW.to_owned()),
        source_run_count: 0,
        boundary: DenominatorBoundary::Fixture,
    };
    let error = validate_denominator("example/repo", &proof, &[], &[], &window).unwrap_err();
    assert!(error.to_string().contains("durable source proof"));
}

#[test]
fn push_head_chain_gap_is_rejected_without_observed_head_fallback() {
    let first = push_head_observation("head-a", "base", 1);
    let second = push_head_observation("head-b", "unrelated", 2);
    let history = vec![
        HistoryCommitObservation {
            sha: first.head_sha.clone(),
            base_sha: Some(first.before_sha.clone()),
            tree_sha: first.tree_sha.clone(),
            committed_at: first.committed_at.clone(),
        },
        HistoryCommitObservation {
            sha: second.head_sha.clone(),
            base_sha: Some(second.before_sha.clone()),
            tree_sha: second.tree_sha.clone(),
            committed_at: second.committed_at.clone(),
        },
    ];
    let push_heads = vec![first, second];
    let proof = push_head_denominator(history.clone(), push_heads.clone());
    let error = validate_denominator("example/repo", &proof, &history, &push_heads, &proof.window)
        .unwrap_err();
    assert!(error.to_string().contains("coverage gap"));
}

#[test]
fn push_head_window_requires_a_verified_boundary_predecessor() {
    let first = push_head_observation("head-a", "missing-predecessor", 1);
    let history = vec![HistoryCommitObservation {
        sha: first.head_sha.clone(),
        base_sha: Some(first.before_sha.clone()),
        tree_sha: first.tree_sha.clone(),
        committed_at: first.committed_at.clone(),
    }];
    let push_heads = vec![first];
    let mut proof = push_head_denominator(history.clone(), push_heads.clone());
    proof.boundary = DenominatorBoundary::Fixture;
    let error = validate_denominator("example/repo", &proof, &history, &push_heads, &proof.window)
        .unwrap_err();
    assert!(error.to_string().contains("boundary predecessor"));
}

#[test]
fn missing_tree_identity_is_rejected() {
    let mut expected = expected_for_sha("tree-missing");
    expected[0].commit.tree_sha.clear();
    let error = validate_expected(&expected).unwrap_err();
    assert!(error.to_string().contains("no tree identity"));
}

#[test]
fn runtime_and_collection_provenance_are_required() {
    let mut evidence = evidence(expected_for_sha("runtime"), Vec::new());
    evidence.runtime = RuntimeIdentity::default();
    let error = validate_evidence(&evidence).unwrap_err();
    assert!(error.to_string().contains("runtime revision proof"));

    let mut provenance = test_provenance();
    provenance.event = "workflow_dispatch".to_owned();
    let error = validate_collection_provenance(&provenance, "example/repo").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unsupported CI evidence collection event")
    );
}

#[test]
fn runtime_markers_bind_to_present_workflow_contract() {
    let repository = tempfile::tempdir().unwrap();
    let config = repository.path().join(".github-gen/velnor-workflow.toml");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    let revision = "0123456789abcdef0123456789abcdef01234567";
    let contents = format!("revision = \"{revision}\"\n");
    fs::write(&config, &contents).unwrap();
    let digest = sha256_hex(contents.as_bytes());

    let identity = runtime_identity(repository.path(), RuntimeIdentity::default()).unwrap();
    assert_eq!(identity.runtime_revision.as_deref(), Some(revision));
    assert_eq!(identity.contract_digest.as_deref(), Some(digest.as_str()));

    let error = runtime_identity(
        repository.path(),
        RuntimeIdentity {
            runtime_revision: Some("f".repeat(40)),
            contract_digest: Some(digest.clone()),
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("runtime revision marker"));

    let missing = tempfile::tempdir().unwrap();
    let error = runtime_identity(missing.path(), RuntimeIdentity::default()).unwrap_err();
    assert!(error.to_string().contains("reading workflow contract"));
}

#[test]
fn remote_identity_parser_rejects_unbound_hosts() {
    assert_eq!(
        remote_repository_identity("git@github.com:example/repo.git").unwrap(),
        "example/repo"
    );
    assert!(
        remote_repository_identity("git@gitlab.com:example/repo.git")
            .err()
            .is_some()
    );
}

#[test]
fn fixture_denominator_cannot_qualify_green() {
    let expected = Cohort::ALL
        .into_iter()
        .map(|cohort| obligation_from_source("fixture", cohort, DenominatorSource::Fixture))
        .collect::<Vec<_>>();
    let mut evidence = evidence(expected, Vec::new());
    evidence.denominator.source = DenominatorSource::Fixture;
    evidence.denominator.fetch_succeeded = false;
    let rollup = build_rollup(&evidence);
    assert!(!rollup.green_claim_qualified);
    assert!(require_qualified(&rollup).is_err());
}

#[test]
fn local_collection_cannot_qualify_green() {
    let expected = Cohort::ALL
        .into_iter()
        .map(|cohort| {
            obligation_from_source("local-green", cohort, DenominatorSource::PushHeadLedger)
        })
        .collect::<Vec<_>>();
    let mut evidence = evidence(expected, Vec::new());
    evidence.denominator.source = DenominatorSource::PushHeadLedger;
    evidence.denominator.fetch_succeeded = true;
    evidence.denominator.source_workflow = Some(DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW.to_owned());
    evidence.denominator.source_run_count = 1;
    let rollup = build_rollup(&evidence);
    assert!(!rollup.green_claim_qualified);
    assert!(require_qualified(&rollup).is_err());
}

#[test]
fn scheduled_provenance_without_current_workflow_contract_cannot_qualify_green() {
    let expected = Cohort::ALL
        .into_iter()
        .map(|cohort| {
            obligation_from_source(
                "fake-scheduled-head",
                cohort,
                DenominatorSource::PushHeadLedger,
            )
        })
        .collect::<Vec<_>>();
    let attempts = Cohort::ALL
        .into_iter()
        .enumerate()
        .map(|(index, cohort)| {
            let mut row = attempt(
                index as u64 + 1,
                1,
                cohort,
                "fake-scheduled-head",
                OutcomeClass::Success,
                "2026-09-22T00:02:00Z",
            );
            row.denominator_source = DenominatorSource::PushHeadLedger;
            row
        })
        .collect();
    let mut evidence = evidence(expected, attempts);
    evidence.provenance.event = "schedule".to_owned();
    evidence.provenance.workflow_path = DEFAULT_CI_EVIDENCE_WORKFLOW.to_owned();
    evidence.provenance.run_id = Some(7);
    evidence.denominator.source = DenominatorSource::PushHeadLedger;
    evidence.denominator.fetch_succeeded = true;
    evidence.denominator.source_workflow = Some(DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW.to_owned());
    evidence.denominator.source_run_count = 1;

    let rollup = build_rollup(&evidence);
    assert!(!rollup.green_claim_qualified);
    assert!(require_qualified(&rollup).is_err());
}

#[test]
fn retained_push_head_artifact_bytes_are_digest_bound() {
    let mut observation = push_head_observation("artifact-head", "artifact-before", 1);
    observation.artifact.event.push(' ');
    let error = validate_push_head_observation("example/repo", "main", &observation).unwrap_err();
    assert!(error.to_string().contains("event artifact digest"));
}

#[test]
fn fake_git_commit_identity_is_rejected() {
    let root = docs::repo_root().unwrap();
    let error = validate_git_commit_identity(
        &root,
        &fixture_sha("fake-commit"),
        &fixture_sha("fake-tree"),
        "2026-09-22T00:00:00Z",
    )
    .unwrap_err();
    assert!(error.to_string().contains("Git commit object"));
}

#[test]
fn absent_evidence_workflows_fail_closed() {
    let root = docs::repo_root().unwrap();
    let error = validate_workflow_contract(&root).unwrap_err();
    assert!(error.to_string().contains("workflow contract"));
}

#[test]
fn derived_history_rejects_edited_expected_source() {
    let mut evidence = evidence(
        vec![
            obligation("head", Cohort::CiMain),
            obligation("head", Cohort::Desktop),
        ],
        Vec::new(),
    );
    evidence.expected[0].commit.source = DenominatorSource::PushHeadLedger;
    evidence.expected[0].provenance = ObligationProvenance::PushHeadLedger;
    let error = validate_evidence(&evidence).unwrap_err();
    assert!(error.to_string().contains("not derived"));
}

#[test]
fn rolling_window_merge_prunes_attempts_outside_new_denominator() {
    let old = evidence(
        expected_for_sha("old-head"),
        vec![attempt(
            1,
            1,
            Cohort::CiMain,
            "old-head",
            OutcomeClass::Product,
            "2026-08-01T00:00:00Z",
        )],
    );
    let expected = expected_for_sha("new-head");
    let (denominator, history) = update_denominator(&expected);
    let merged = merge_evidence(
        old,
        EvidenceUpdate {
            expected,
            attempts: Vec::new(),
            unclassified_runs: Vec::new(),
            denominator,
            history,
            push_heads: Vec::new(),
            repository: "example/repo".to_owned(),
            window: TimeWindow {
                since: "2026-09-21T00:00:00Z".to_owned(),
                until: "2026-09-23T00:00:00Z".to_owned(),
            },
            runtime: test_runtime(),
            provenance: test_provenance(),
        },
    )
    .unwrap();

    assert!(merged.attempts.is_empty());
    validate_evidence(&merged).unwrap();
}

#[test]
fn merge_rejects_immutable_runtime_replacement() {
    let expected = expected_for_sha("immutable-runtime");
    let (denominator, history) = update_denominator(&expected);
    let mut runtime = test_runtime();
    runtime.contract_digest = Some("f".repeat(64));
    let error = merge_evidence(
        evidence(expected.clone(), Vec::new()),
        EvidenceUpdate {
            expected,
            attempts: Vec::new(),
            unclassified_runs: Vec::new(),
            denominator,
            history,
            push_heads: Vec::new(),
            repository: "example/repo".to_owned(),
            window: TimeWindow {
                since: "2026-09-21T00:00:00Z".to_owned(),
                until: "2026-09-23T00:00:00Z".to_owned(),
            },
            runtime,
            provenance: test_provenance(),
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("immutable identity differs"));
}

#[test]
fn api_timestamps_are_canonical_and_plus_safe() {
    assert_eq!(
        api_timestamp("2026-09-22T00:00:00+07:00"),
        "2026-09-21T17:00:00Z"
    );
    assert_eq!(api_timestamp("not-a-timestamp"), "not-a-timestamp");
}
