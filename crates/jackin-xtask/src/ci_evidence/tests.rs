use super::*;
use std::{fs, io::Write, path::Path, process::Command};

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
        run_attempt: None,
        workflow_ref: None,
        head_sha: None,
        workflow_sha: None,
        artifact_name: None,
    }
}

fn test_api_repository() -> ApiRunRepository {
    ApiRunRepository {
        id: TARGET_REPOSITORY_ID,
        full_name: TARGET_REPOSITORY.to_owned(),
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
        workflow_path: Some(format!(
            ".github/workflows/{}",
            match cohort {
                Cohort::CiMain => DEFAULT_CI_WORKFLOW,
                Cohort::Desktop => DEFAULT_DESKTOP_WORKFLOW,
            }
        )),
        workflow_file_sha: None,
        event: Some("push".to_owned()),
        head_branch: Some("main".to_owned()),
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
        generated_at: "2026-09-22T00:10:00Z".to_owned(),
        runtime: test_runtime(),
        provenance: CollectionProvenance {
            repository: "example/repo".to_owned(),
            branch: "main".to_owned(),
            event: "test".to_owned(),
            workflow_path: "test".to_owned(),
            run_id: None,
            run_attempt: None,
            workflow_ref: None,
            head_sha: None,
            workflow_sha: None,
            artifact_name: None,
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
        workflow_sha: head_sha.clone(),
        run_id,
        run_attempt: 1,
        head_sha,
        before_sha,
        tree_sha,
        committed_at: "2026-09-22T00:00:00Z".to_owned(),
        created_at: "2026-09-22T00:01:00Z".to_owned(),
        pushed_commits: Vec::new(),
        event_sha256: String::new(),
        artifact: PushHeadArtifactProof {
            manifest: String::new(),
            event: String::new(),
            manifest_sha256: String::new(),
            artifact_id: 0,
            artifact_digest: String::new(),
        },
    };
    observation.pushed_commits = vec![observation.head_sha.clone()];
    refresh_push_head_artifact(&mut observation);
    observation
}

fn refresh_push_head_artifact(observation: &mut PushHeadObservation) {
    let event_proof = serde_json::json!({
        "repository": observation.repository,
        "repository_id": TARGET_REPOSITORY_ID,
        "ref": format!("refs/heads/{}", observation.branch),
        "before": observation.before_sha,
        "after": observation.head_sha,
    });
    let event_bytes = serde_json::to_vec(&event_proof).unwrap();
    observation.event_sha256 = sha256_hex(&event_bytes);
    let manifest = serde_json::json!({
        "schema": PUSH_HEAD_LEDGER_SCHEMA,
        "repository": observation.repository,
        "repository_id": TARGET_REPOSITORY_ID,
        "branch": observation.branch,
        "event": observation.event,
        "workflow_path": observation.workflow_path,
        "workflow_sha": observation.workflow_sha,
        "workflow_id": observation.workflow_id,
        "run_id": observation.run_id,
        "run_attempt": observation.run_attempt,
        "head_sha": observation.head_sha,
        "before_sha": observation.before_sha,
        "tree_sha": observation.tree_sha,
        "committed_at": observation.committed_at,
        "pushed_commits": observation.pushed_commits,
        "event_sha256": observation.event_sha256,
    });
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    observation.artifact = PushHeadArtifactProof {
        manifest: String::from_utf8(manifest_bytes.clone()).unwrap(),
        event: String::from_utf8(event_bytes).unwrap(),
        manifest_sha256: sha256_hex(&manifest_bytes),
        artifact_id: observation.run_id + 10_000,
        artifact_digest: format!("sha256:{}", "a".repeat(64)),
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
    let event_proof = serde_json::json!({
        "repository": "example/repo",
        "repository_id": TARGET_REPOSITORY_ID,
        "ref": "refs/heads/main",
        "before": before_sha.clone(),
        "after": head_sha.clone(),
    });
    let event_bytes = serde_json::to_vec(&event_proof).unwrap();
    let pushed_commits = run_git(
        repository.path(),
        &[
            "rev-list",
            "--first-parent",
            "--reverse",
            &format!("{before_sha}..{head_sha}"),
        ],
    );
    let manifest = serde_json::json!({
        "schema": PUSH_HEAD_LEDGER_SCHEMA,
        "repository": "example/repo",
        "repository_id": TARGET_REPOSITORY_ID,
        "branch": "main",
        "event": "push",
        "workflow_path": DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW,
        "workflow_sha": head_sha.clone(),
        "workflow_id": 7,
        "run_id": 42,
        "run_attempt": 1,
        "head_sha": head_sha.clone(),
        "before_sha": before_sha,
        "tree_sha": tree_sha,
        "committed_at": committed_at,
        "pushed_commits": pushed_commits.lines().collect::<Vec<_>>(),
        "event_sha256": sha256_hex(&event_bytes),
    });
    let run = ApiRun {
        id: 42,
        repository: Some(test_api_repository()),
        head_repository: Some(test_api_repository()),
        workflow_id: Some(7),
        name: Some("push-head ledger".to_owned()),
        display_title: None,
        path: Some(format!(
            ".github/workflows/{DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW}@refs/heads/main"
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
        event_bytes,
    )
}

#[test]
fn push_head_artifact_binds_manifest_sanitized_event_and_git_head() {
    let (repository, run, manifest, event) = push_head_artifact_fixture();
    let observation = validate_push_head_artifact(
        repository.path(),
        "example/repo",
        "main",
        &run,
        7,
        VerifiedPushArtifact {
            manifest: manifest.clone(),
            event: event.clone(),
            id: 1001,
            digest: format!("sha256:{}", "a".repeat(64)),
        },
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
        VerifiedPushArtifact {
            manifest: serde_json::to_vec(&forged_manifest).unwrap(),
            event: event.clone(),
            id: 1001,
            digest: format!("sha256:{}", "a".repeat(64)),
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("mismatched provenance"));

    let mut forged_event: serde_json::Value = serde_json::from_slice(&event).unwrap();
    forged_event["after"] = serde_json::json!(fixture_sha("different-head"));
    let forged_event = serde_json::to_vec(&forged_event).unwrap();
    forged_manifest["run_id"] = serde_json::json!(run.id);
    forged_manifest["event_sha256"] = serde_json::json!(sha256_hex(&forged_event));
    let error = validate_push_head_artifact(
        repository.path(),
        "example/repo",
        "main",
        &run,
        7,
        VerifiedPushArtifact {
            manifest: serde_json::to_vec(&forged_manifest).unwrap(),
            event: forged_event,
            id: 1001,
            digest: format!("sha256:{}", "a".repeat(64)),
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("sanitized push event"));
}

#[test]
fn push_head_zip_requires_exact_safe_file_set() {
    let valid = zip_fixture(&[("push-head.json", b"{}"), ("event.json", b"{}")]);
    let files = read_push_head_zip(&valid, 42).unwrap();
    assert_eq!(files, (b"{}".to_vec(), b"{}".to_vec()));

    let extra = zip_fixture(&[
        ("push-head.json", b"{}"),
        ("event.json", b"{}"),
        ("extra.txt", b"private"),
    ]);
    read_push_head_zip(&extra, 42).unwrap_err();

    let traversal = zip_fixture(&[("../event.json", b"{}"), ("push-head.json", b"{}")]);
    read_push_head_zip(&traversal, 42).unwrap_err();
}

#[test]
fn workflow_identity_requires_exact_path_and_branch_ref() {
    assert!(workflow_ref_matches(
        ".github/workflows/ci-evidence.yml",
        DEFAULT_CI_EVIDENCE_WORKFLOW,
        "main"
    ));
    assert!(workflow_ref_matches(
        ".github/workflows/ci-evidence.yml@refs/heads/main",
        DEFAULT_CI_EVIDENCE_WORKFLOW,
        "main"
    ));
    assert!(!workflow_ref_matches(
        ".github/workflows/ci-evidence.yml.backup@refs/heads/main",
        DEFAULT_CI_EVIDENCE_WORKFLOW,
        "main"
    ));
    assert!(!workflow_ref_matches(
        ".github/workflows/ci-evidence.yml@refs/tags/main",
        DEFAULT_CI_EVIDENCE_WORKFLOW,
        "main"
    ));
    assert!(!workflow_ref_matches(
        ".github/workflows/ci-evidence.yml@refs/heads/evil",
        DEFAULT_CI_EVIDENCE_WORKFLOW,
        "main"
    ));
    assert!(!workflow_ref_matches(
        ".github/workflows/ci-evidence.yml@",
        DEFAULT_CI_EVIDENCE_WORKFLOW,
        "main"
    ));
}

#[test]
fn api_run_and_attempt_paths_accept_equivalent_branch_suffixes() {
    assert!(workflow_paths_equivalent(
        Some(".github/workflows/ci-main.yml"),
        Some(".github/workflows/ci-main.yml@refs/heads/main"),
        "main"
    ));
    assert!(workflow_paths_equivalent(
        Some(".github/workflows/ci-main.yml@main"),
        Some(".github/workflows/ci-main.yml@refs/heads/main"),
        "main"
    ));
    assert!(!workflow_paths_equivalent(
        Some(".github/workflows/ci-main.yml@refs/tags/main"),
        Some(".github/workflows/ci-main.yml"),
        "main"
    ));
    assert!(!workflow_paths_equivalent(
        Some(".github/workflows/ci-main.yml@"),
        Some(".github/workflows/ci-main.yml"),
        "main"
    ));
}

#[test]
fn github_run_and_jobs_decode_real_rest_identity_fields() {
    let head_sha = cmd::output_string(
        Command::new("git")
            .current_dir(docs::repo_root().unwrap())
            .args(["rev-parse", "HEAD"]),
    )
    .unwrap()
    .trim()
    .to_owned();
    // Post-#1110 the main CI workflow is velnor-actions 0.1.0 `ci.yml` (named
    // "CI", no run-name template); ci-main.yml was deleted by main commit
    // 6c389d38e.
    let run_payload = serde_json::json!({
        "id": 35475235267_u64,
        "repository": {"id": TARGET_REPOSITORY_ID, "full_name": TARGET_REPOSITORY},
        "head_repository": {"id": TARGET_REPOSITORY_ID, "full_name": TARGET_REPOSITORY},
        "workflow_id": 357877882_u64,
        "name": "CI",
        "display_title": "Update the CI evidence observer",
        "path": ".github/workflows/ci.yml",
        "event": "push",
        "head_branch": "main",
        "head_sha": head_sha,
        "status": "completed",
        "conclusion": "success",
        "run_attempt": 1,
        "run_started_at": "2026-09-19T23:07:36Z",
        "created_at": "2026-09-19T23:07:36Z"
    });
    assert_ne!(run_payload["name"], run_payload["display_title"]);
    let run: ApiRun = serde_json::from_value(run_payload).unwrap();
    assert_eq!(run.name.as_deref(), Some("CI"));
    assert_eq!(
        run.display_title.as_deref(),
        Some("Update the CI evidence observer")
    );
    validate_api_run_contract(
        &run,
        run.id,
        "main",
        "push",
        "ci.yml",
        Some(357877882),
        Some(&run.head_sha),
    )
    .unwrap();

    let api_attempt: ApiAttempt = serde_json::from_value(serde_json::json!({
        "id": run.id,
        "repository": {"id": TARGET_REPOSITORY_ID, "full_name": TARGET_REPOSITORY},
        "head_repository": {"id": TARGET_REPOSITORY_ID, "full_name": TARGET_REPOSITORY},
        "workflow_id": 357877882_u64,
        "name": "CI",
        "display_title": "CI",
        "path": ".github/workflows/ci.yml@refs/heads/main",
        "event": "push",
        "head_branch": "main",
        "head_sha": run.head_sha,
        "run_attempt": 1,
        "status": "completed",
        "conclusion": "success",
        "created_at": "2026-09-19T23:07:36Z",
        "run_started_at": "2026-09-19T23:07:36Z"
    }))
    .unwrap();
    validate_api_attempt_binding(&run, &api_attempt).unwrap();
    let mut changed_attempt = api_attempt.clone();
    changed_attempt.event = Some("workflow_dispatch".to_owned());
    assert!(validate_api_attempt_binding(&run, &changed_attempt).is_err());

    let job: ApiJob = serde_json::from_value(serde_json::json!({
        "id": 888_u64,
        "run_id": run.id,
        "run_attempt": 1,
        "head_sha": run.head_sha,
        "head_branch": "main",
        "workflow_name": "CI",
        "name": "Control / Planning",
        "status": "completed",
        "conclusion": "success"
    }))
    .unwrap();
    validate_api_jobs(&run, 1, std::slice::from_ref(&job)).unwrap();
    let mut wrong_attempt = job.clone();
    wrong_attempt.run_attempt = Some(2);
    assert!(validate_api_jobs(&run, 1, &[wrong_attempt]).is_err());
    let mut missing_attempt = job;
    missing_attempt.run_attempt = None;
    assert!(validate_api_jobs(&run, 1, &[missing_attempt]).is_err());
}

#[test]
fn attempt_start_time_binding_normalizes_equivalent_timestamps() {
    assert!(same_api_timestamp(
        Some("2026-09-22T00:00:05Z"),
        Some("2026-09-22T00:00:05+00:00")
    ));
    assert!(!same_api_timestamp(
        Some("2026-09-22T00:00:05Z"),
        Some("2026-09-22T00:00:06Z")
    ));
    assert!(!same_api_timestamp(Some("2026-09-22T00:00:05Z"), None));
}

#[test]
fn workflow_blob_sha_uses_the_requested_historical_commit() {
    let repository = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        String::from_utf8_lossy(
            &cmd::output(
                Command::new("git")
                    .current_dir(repository.path())
                    .args(args)
                    .env("GIT_AUTHOR_NAME", "CI evidence test")
                    .env("GIT_AUTHOR_EMAIL", "ci-evidence@example.invalid")
                    .env("GIT_COMMITTER_NAME", "CI evidence test")
                    .env("GIT_COMMITTER_EMAIL", "ci-evidence@example.invalid"),
            )
            .unwrap(),
        )
        .trim()
        .to_owned()
    };
    git(&["init", "--quiet"]);
    let workflow_dir = repository.path().join(".github/workflows");
    fs::create_dir_all(&workflow_dir).unwrap();
    let workflow_path = workflow_dir.join(DEFAULT_CI_WORKFLOW);
    fs::write(&workflow_path, "name: CI / Main\nrun-name: CI / main · ${{ github.event_name }} · ${{ github.ref_name }}\n").unwrap();
    git(&["add", ".github/workflows/ci-main.yml"]);
    git(&["commit", "--quiet", "-m", "first workflow"]);
    let first_commit = git(&["rev-parse", "HEAD"]);
    let first_blob =
        workflow_blob_sha(repository.path(), &first_commit, DEFAULT_CI_WORKFLOW).unwrap();

    fs::write(&workflow_path, "name: CI / Main\nrun-name: new identity\n").unwrap();
    git(&["add", ".github/workflows/ci-main.yml"]);
    git(&["commit", "--quiet", "-m", "change workflow identity"]);
    let second_commit = git(&["rev-parse", "HEAD"]);
    let second_blob =
        workflow_blob_sha(repository.path(), &second_commit, DEFAULT_CI_WORKFLOW).unwrap();

    assert_ne!(first_blob, second_blob);
    assert_eq!(
        workflow_blob_sha(repository.path(), &first_commit, DEFAULT_CI_WORKFLOW).unwrap(),
        first_blob
    );
    assert_eq!(
        workflow_run_name_at(
            repository.path(),
            &first_commit,
            DEFAULT_CI_WORKFLOW,
            "push",
            "main"
        )
        .unwrap(),
        "CI / main · push · main"
    );

    fs::write(
        &workflow_path,
        "name: CI / Main\nrun-name: 'CI / ${{ github.actor }}'\n",
    )
    .unwrap();
    git(&["add", ".github/workflows/ci-main.yml"]);
    git(&[
        "commit",
        "--quiet",
        "-m",
        "use unsupported run-name expression",
    ]);
    let unsupported_commit = git(&["rev-parse", "HEAD"]);
    let _error = workflow_run_name_at(
        repository.path(),
        &unsupported_commit,
        DEFAULT_CI_WORKFLOW,
        "push",
        "main",
    )
    .unwrap_err();

    fs::write(&workflow_path, "name: CI / Main\n").unwrap();
    git(&["add", ".github/workflows/ci-main.yml"]);
    git(&["commit", "--quiet", "-m", "use standard workflow name"]);
    let standard_commit = git(&["rev-parse", "HEAD"]);
    assert_eq!(
        workflow_run_name_at(
            repository.path(),
            &standard_commit,
            DEFAULT_CI_WORKFLOW,
            "push",
            "main"
        )
        .unwrap(),
        "CI / Main"
    );
}

#[test]
fn branch_ref_validation_rejects_github_api_injection() {
    validate_branch_name("main").unwrap();
    validate_branch_name("release/v2").unwrap();
    for branch in [
        "",
        "-main",
        "main?per_page=1000",
        "main@{0}",
        "../main",
        "main.lock",
    ] {
        assert!(validate_branch_name(branch).is_err(), "accepted {branch:?}");
    }
}

#[test]
fn duplicate_required_job_names_are_data_quality() {
    let mut jobs = completed_jobs(Cohort::CiMain);
    jobs.push(jobs[0].clone());
    let expected_work = Cohort::CiMain
        .expected_work()
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        classify_outcome("completed", Some("success"), &jobs, &expected_work),
        OutcomeClass::DataQuality
    );
}

#[test]
fn bootstrap_boundary_starts_window_at_first_verified_push() {
    let first = push_head_observation("bootstrap-head", "bootstrap-before", 51);
    let history = vec![HistoryCommitObservation {
        sha: first.head_sha.clone(),
        base_sha: Some(first.before_sha.clone()),
        tree_sha: first.tree_sha.clone(),
        committed_at: first.committed_at.clone(),
    }];
    let push_heads = vec![first.clone()];
    let mut proof = push_head_denominator(history.clone(), push_heads.clone());
    proof.window.since = first.created_at.clone();
    proof.boundary = DenominatorBoundary::Bootstrap {
        first_run_id: first.run_id,
        before_sha: first.before_sha.clone(),
    };
    validate_denominator("example/repo", &proof, &history, &push_heads, &proof.window).unwrap();

    proof.boundary = DenominatorBoundary::Bootstrap {
        first_run_id: first.run_id + 1,
        before_sha: first.before_sha,
    };
    assert!(
        validate_denominator("example/repo", &proof, &history, &push_heads, &proof.window).is_err()
    );
}

#[test]
fn evidence_writes_replace_atomically_without_staging_artifacts() {
    let output_dir = tempfile::tempdir().unwrap();
    let output_dir_path = fs::canonicalize(output_dir.path()).unwrap();
    let output = output_dir_path
        .join("target")
        .join("ci-evidence")
        .join("attempts.json");
    write_json(&output, &serde_json::json!({"generation": 1})).unwrap();
    write_json(&output, &serde_json::json!({"generation": 2})).unwrap();
    let stored: serde_json::Value = read_json(&output).unwrap();
    assert_eq!(stored["generation"], 2);
    let entries = crate::fs_util::read_dir_sorted(&output_dir_path.join("target/ci-evidence"))
        .unwrap()
        .into_iter()
        .map(|entry| entry.file_name())
        .collect::<BTreeSet<_>>();
    assert_eq!(entries, BTreeSet::from(["attempts.json".into()]));
}

#[cfg(unix)]
#[test]
fn evidence_output_rejects_symlinked_parent_and_preserves_outside_directory() {
    use std::os::unix::fs::symlink;

    let repository = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let repository = fs::canonicalize(repository.path()).unwrap();
    let outside = fs::canonicalize(outside.path()).unwrap();
    symlink(&outside, repository.join("target")).unwrap();

    let output = repository.join("target/ci-evidence/attempts.json");
    write_json(&output, &serde_json::json!({"unsafe": true})).unwrap_err();
    assert!(!outside.join("ci-evidence").exists());
}

#[cfg(unix)]
#[test]
fn evidence_output_rejects_nested_symlinked_parent_and_preserves_canary() {
    use std::os::unix::fs::symlink;

    let repository = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let repository = fs::canonicalize(repository.path()).unwrap();
    let outside = fs::canonicalize(outside.path()).unwrap();
    fs::create_dir_all(repository.join("target")).unwrap();
    symlink(&outside, repository.join("target/ci-evidence")).unwrap();
    fs::write(outside.join("canary"), b"untouched").unwrap();

    let output = repository.join("target/ci-evidence/attempts.json");
    assert!(write_json(&output, &serde_json::json!({"unsafe": true})).is_err());
    assert_eq!(fs::read(outside.join("canary")).unwrap(), b"untouched");
    assert!(!outside.join("attempts.json").exists());
}

#[cfg(unix)]
#[test]
fn evidence_output_rejects_symlink_file_for_reads_and_writes() {
    use std::os::unix::fs::symlink;

    let repository = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let repository = fs::canonicalize(repository.path()).unwrap();
    let outside = fs::canonicalize(outside.path()).unwrap();
    let canary = outside.join("canary.json");
    fs::write(&canary, br#"{"generation":1}"#).unwrap();
    let output = repository.join("attempts.json");
    symlink(&canary, &output).unwrap();

    read_json::<serde_json::Value>(&output).unwrap_err();
    assert_eq!(
        OutputLocation::open(&output, false)
            .unwrap()
            .parent
            .entry_kind(OsStr::new("attempts.json"))
            .unwrap(),
        EntryKind::Symlink
    );
    write_json(&output, &serde_json::json!({"generation": 2})).unwrap_err();
    assert_eq!(fs::read(canary).unwrap(), br#"{"generation":1}"#);
}

#[test]
fn repository_output_paths_reject_absolute_and_parent_traversal() {
    let root = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(root.path()).unwrap();
    repository_output_path(&root, Path::new("/tmp/out.json")).unwrap_err();
    repository_output_path(&root, Path::new("target/../out.json")).unwrap_err();
    assert_eq!(
        repository_output_path(&root, Path::new("target/ci-evidence/out.json")).unwrap(),
        root.join("target/ci-evidence/out.json")
    );
}

fn zip_fixture(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in files {
        writer.start_file(*name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
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
                predecessor.created_at = "2026-09-20T23:59:00Z".to_owned();
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
fn github_api_page_requests_force_get_and_keep_query_parameters() {
    let arguments = api_page_arguments(
        "repos/jackin-project/jackin/actions/runs",
        &[
            ("branch", "main".to_owned()),
            (
                "created",
                "2026-09-01T00:00:00Z..2026-09-29T00:00:00Z".to_owned(),
            ),
        ],
        3,
        API_PAGE_SIZE,
    );
    assert_eq!(
        &arguments[..4],
        [
            "api",
            "--method",
            "GET",
            "repos/jackin-project/jackin/actions/runs"
        ]
    );
    assert!(
        arguments
            .windows(2)
            .any(|pair| pair == ["-f", "branch=main"])
    );
    assert!(
        arguments
            .windows(2)
            .any(|pair| pair == ["-F", "per_page=100"])
    );
    assert!(arguments.windows(2).any(|pair| pair == ["-F", "page=3"]));
    assert!(
        arguments
            .iter()
            .any(|argument| argument == "created=2026-09-01T00:00:00Z..2026-09-29T00:00:00Z")
    );
}

#[test]
fn github_api_paging_counts_the_expected_array_and_total() {
    let response = serde_json::json!({
        "total_count": 2,
        "jobs": [{"id": 1}, {"id": 2}],
        "unrelated": [{"id": 3}]
    });
    assert_eq!(api_page_count(&response, "jobs").unwrap(), (2, Some(2)));
    let wrong_key = api_page_count(&response, "workflow_runs").unwrap_err();
    assert!(wrong_key.to_string().contains("workflow_runs"));
    let missing_total = api_page_count(&serde_json::json!({"jobs": []}), "jobs").unwrap_err();
    assert!(missing_total.to_string().contains("total_count"));
}

#[test]
fn github_api_paging_stops_only_at_total_and_fails_on_short_incomplete_pages() {
    for total in [1_usize, 100, 101, 201, 1000] {
        let mut collected = 0;
        let mut complete = false;
        while !complete {
            let remaining = total - collected;
            let page_count = remaining.min(API_PAGE_SIZE);
            collected += page_count;
            complete = page_is_complete(page_count, collected, Some(total), "runs").unwrap();
        }
        assert_eq!(collected, total);
    }
    page_is_complete(0, 0, Some(0), "runs").unwrap();
    page_is_complete(20, 20, None, "runs").unwrap();
    let error = page_is_complete(20, 20, Some(101), "runs").unwrap_err();
    assert!(error.to_string().contains("short"));
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
        repository: Some(test_api_repository()),
        head_repository: Some(test_api_repository()),
        workflow_id: Some(99),
        name: Some("CI / Main".to_owned()),
        display_title: None,
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
        repository: Some(test_api_repository()),
        head_repository: Some(test_api_repository()),
        workflow_id: Some(99),
        name: Some("renamed".to_owned()),
        display_title: None,
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
    assert!(error.to_string().contains("invalid attempt count"));
}

#[test]
fn skipped_cohort_is_reported_by_the_advisory_observer() {
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
    assert_eq!(rollup.status, "advisory");
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
        repository: Some(test_api_repository()),
        head_repository: Some(test_api_repository()),
        workflow_id: Some(42),
        name: Some("CI/Main".to_owned()),
        display_title: None,
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
        id: run.id,
        workflow_id: run.workflow_id,
        head_sha: run.head_sha.clone(),
        head_branch: run.head_branch.clone(),
        repository: Some(test_api_repository()),
        head_repository: Some(test_api_repository()),
        name: run.name.clone(),
        path: run.path.clone(),
        event: run.event.clone(),
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
            run_id: Some(run.id),
            run_attempt: Some(api_attempt.run_attempt),
            head_sha: Some(run.head_sha.clone()),
            head_branch: run.head_branch.clone(),
            workflow_name: run.name.clone(),
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
    assert!(
        error.to_string().contains("classification"),
        "error: {error:#}"
    );
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
    assert!(
        error.to_string().contains("coverage gap"),
        "error: {error:#}"
    );
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
    assert!(error.to_string().contains("missing its boundary proof"));
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
    provenance.workflow_path = DEFAULT_CI_EVIDENCE_WORKFLOW.to_owned();
    provenance.run_id = Some(1);
    provenance.run_attempt = Some(1);
    provenance.workflow_ref = Some(format!(
        "example/repo/.github/workflows/{DEFAULT_CI_EVIDENCE_WORKFLOW}@refs/heads/main"
    ));
    provenance.head_sha = Some(fixture_sha("collector-head"));
    provenance.workflow_sha = Some(fixture_sha("workflow-sha"));
    provenance.artifact_name = Some(CI_EVIDENCE_ARTIFACT_NAME.to_owned());
    let error = validate_collection_provenance(&provenance, "example/repo").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unsupported CI evidence collection event")
    );
    provenance.event = "push".to_owned();
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
    let contents = format!("[generator]\nrevision = \"{revision}\"\n");
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
fn fixture_denominator_remains_advisory() {
    let expected = Cohort::ALL
        .into_iter()
        .map(|cohort| obligation_from_source("fixture", cohort, DenominatorSource::Fixture))
        .collect::<Vec<_>>();
    let mut evidence = evidence(expected, Vec::new());
    evidence.denominator.source = DenominatorSource::Fixture;
    evidence.denominator.fetch_succeeded = false;
    let rollup = build_rollup(&evidence);
    assert_eq!(rollup.status, "advisory");
}

#[test]
fn local_collection_remains_advisory() {
    let expected = Cohort::ALL
        .into_iter()
        .map(|cohort| {
            obligation_from_source("local-observer", cohort, DenominatorSource::PushHeadLedger)
        })
        .collect::<Vec<_>>();
    let mut evidence = evidence(expected, Vec::new());
    evidence.denominator.source = DenominatorSource::PushHeadLedger;
    evidence.denominator.fetch_succeeded = true;
    evidence.denominator.source_workflow = Some(DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW.to_owned());
    evidence.denominator.source_run_count = 1;
    let rollup = build_rollup(&evidence);
    assert_eq!(rollup.status, "advisory");
}

#[test]
fn scheduled_provenance_is_reported_without_a_semantic_claim() {
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
    assert_eq!(rollup.status, "advisory");
}

#[test]
fn retained_push_head_artifact_bytes_are_digest_bound() {
    let mut observation = push_head_observation("artifact-head", "artifact-before", 1);
    observation.artifact.event.push(' ');
    let error = validate_push_head_observation("example/repo", "main", &observation).unwrap_err();
    assert!(error.to_string().contains("sanitized push proof digest"));
}

#[test]
fn retained_push_head_artifact_rechecks_repository_and_workflow_ids() {
    let observation = push_head_observation("id-check-head", "id-check-before", 17);
    for (key, value) in [
        ("repository_id", serde_json::json!(TARGET_REPOSITORY_ID + 1)),
        (
            "workflow_id",
            serde_json::json!(observation.workflow_id + 1),
        ),
    ] {
        let mut changed = observation.clone();
        let mut manifest: serde_json::Value =
            serde_json::from_str(&changed.artifact.manifest).unwrap();
        manifest[key] = value;
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        changed.artifact.manifest = String::from_utf8(manifest_bytes.clone()).unwrap();
        changed.artifact.manifest_sha256 = sha256_hex(&manifest_bytes);
        let error = validate_push_head_observation("example/repo", "main", &changed)
            .expect_err("retained manifest must bind repository and workflow IDs");
        assert!(
            error.to_string().contains("manifest does not match"),
            "{error:#}"
        );
    }

    let mut changed = observation;
    let mut event: serde_json::Value = serde_json::from_str(&changed.artifact.event).unwrap();
    event["repository_id"] = serde_json::json!(TARGET_REPOSITORY_ID + 1);
    let event_bytes = serde_json::to_vec(&event).unwrap();
    changed.artifact.event = String::from_utf8(event_bytes.clone()).unwrap();
    changed.event_sha256 = sha256_hex(&event_bytes);
    let mut manifest: serde_json::Value = serde_json::from_str(&changed.artifact.manifest).unwrap();
    manifest["event_sha256"] = serde_json::json!(changed.event_sha256);
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    changed.artifact.manifest = String::from_utf8(manifest_bytes.clone()).unwrap();
    changed.artifact.manifest_sha256 = sha256_hex(&manifest_bytes);
    let error = validate_push_head_observation("example/repo", "main", &changed)
        .expect_err("retained event must bind repository ID");
    assert!(
        error
            .to_string()
            .contains("sanitized push proof does not match"),
        "{error:#}"
    );
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
fn generated_evidence_workflows_are_complete_and_tamper_bound() {
    // Post-#1110 the repo no longer carries the velnor-workflow contract or
    // generator state (main commit 6c389d38e deleted .github-gen/ and
    // .github/ci/ as "legacy velnor-workflow generator files"), so the
    // tamper-binding assertions below run against a fixture: the REAL
    // surviving evidence workflows and task contract copied from this repo,
    // plus a synthesized workflow contract and ownership state.
    let root = docs::repo_root().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let copy = directory.path();
    for relative in [
        ".github/workflows/ci-evidence.yml",
        ".github/workflows/ci-push-head-ledger.yml",
        MISE_PATH,
    ] {
        let destination = copy.join(relative);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(root.join(relative), destination).unwrap();
    }
    let contract = "schema = 2\n\
         \n\
         [generator]\n\
         repository = \"jackin-project/jackin\"\n\
         revision = \"0123456789abcdef0123456789abcdef01234567\"\n\
         \n\
         [workflow]\n\
         default_branch = \"main\"\n\
         providers = [\"github-hosted\"]\n\
         automatic_providers = [\"github-hosted\"]\n\
         \n\
         [[check_profile]]\n\
         id = \"ci-evidence\"\n\
         tasks = [\"ci-evidence\"]\n\
         artifacts = [\"target/ci-evidence/\"]\n\
         runner = \"github\"\n\
         status = \"advisory\"\n\
         env = { GH_TOKEN = \"${{ github.token }}\" }\n\
         \n\
         [[check_profile]]\n\
         id = \"ci-push-head-ledger\"\n\
         tasks = [\"ci-push-head-ledger\"]\n\
         artifacts = [\"target/ci-push-head-ledger/\"]\n\
         runner = \"github\"\n\
         status = \"advisory\"\n\
         env = { GH_TOKEN = \"${{ github.token }}\" }\n\
         \n\
         [[declare]]\n\
         primitive = \"scheduled-checks\"\n\
         file = \"ci-evidence.yml\"\n\
         args = { profiles = [\"ci-evidence\"] }\n\
         \n\
         [[declare]]\n\
         primitive = \"scheduled-checks\"\n\
         file = \"ci-push-head-ledger.yml\"\n\
         args = { profiles = [\"ci-push-head-ledger\"], events = [\"push\"], branches = [\"main\"] }\n";
    let contract_path = copy.join(WORKFLOW_CONTRACT_PATH);
    fs::create_dir_all(contract_path.parent().unwrap()).unwrap();
    fs::write(&contract_path, contract).unwrap();
    let mut state = format!(
        "# Generated ownership state; do not edit.\n\
         schema = {GENERATED_STATE_SCHEMA}\n\
         [inputs]\n\
         config\t{}\n\
         scan\t{}\n\
         generator\t{GENERATED_STATE_GENERATOR}\n\
         [outputs]\n",
        fnv1a_digest(contract.as_bytes()),
        fnv1a_digest(b"fixture-scan"),
    );
    for relative in [
        ".github/workflows/ci-evidence.yml",
        ".github/workflows/ci-push-head-ledger.yml",
    ] {
        let bytes = fs::read(copy.join(relative)).unwrap();
        state.push_str(&format!("{relative}\t{}\n", fnv1a_digest(&bytes)));
    }
    let state_path = copy.join(WORKFLOW_STATE_PATH);
    fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    fs::write(&state_path, &state).unwrap();
    validate_workflow_contract(copy).unwrap();
    let pristine_state = fs::read(&state_path).unwrap();

    let state_path = copy.join(WORKFLOW_STATE_PATH);
    let tampered_state = fs::read_to_string(&state_path).unwrap().replace(
        &format!("generator\t{GENERATED_STATE_GENERATOR}"),
        "generator\tinvalid",
    );
    fs::write(&state_path, tampered_state).unwrap();
    let error = validate_workflow_contract(copy).unwrap_err();
    assert!(
        error.to_string().contains("generator identity"),
        "error: {error:#}"
    );

    fs::write(&state_path, &pristine_state).unwrap();
    let workflow_path = copy.join(".github/workflows/ci-evidence.yml");
    let workflow_bytes = fs::read(&workflow_path).unwrap();
    let workflow_text = String::from_utf8(workflow_bytes.clone()).unwrap();
    let tampered_workflow = workflow_text.replace(
        "  workflow_dispatch:\n",
        "  workflow_dispatch:\n  pull_request:\n",
    );
    assert_ne!(tampered_workflow, workflow_text);
    fs::write(&workflow_path, tampered_workflow.as_bytes()).unwrap();
    let relative = ".github/workflows/ci-evidence.yml";
    let original_hash = fnv1a_digest(&workflow_bytes);
    let tampered_hash = fnv1a_digest(tampered_workflow.as_bytes());
    let expected_row = format!("{relative}\t{original_hash}");
    let replacement_row = format!("{relative}\t{tampered_hash}");
    let state_text = fs::read_to_string(&state_path).unwrap();
    assert!(state_text.contains(&expected_row));
    fs::write(
        &state_path,
        state_text.replacen(&expected_row, &replacement_row, 1),
    )
    .unwrap();
    let error = validate_workflow_contract(copy).unwrap_err();
    assert!(
        error.to_string().contains("unexpected event triggers"),
        "error: {error:#}"
    );

    fs::write(&state_path, &pristine_state).unwrap();
    fs::copy(
        root.join(".github/workflows/ci-evidence.yml"),
        &workflow_path,
    )
    .unwrap();
    #[expect(
        clippy::disallowed_methods,
        reason = "the synchronous xtask test edits a generated workflow fixture"
    )]
    fs::OpenOptions::new()
        .append(true)
        .open(copy.join(".github/workflows/ci-evidence.yml"))
        .unwrap()
        .write_all(b"# tampered\n")
        .unwrap();
    let error = validate_workflow_contract(copy).unwrap_err();
    assert!(
        error.to_string().contains("output hash mismatch"),
        "error: {error:#}"
    );
}

#[test]
fn generated_evidence_workflows_reject_skip_controls() {
    let root = docs::repo_root().unwrap();
    let bytes = fs::read(root.join(".github/workflows/ci-evidence.yml")).unwrap();
    let workflow: serde_json::Value = serde_yaml_ng::from_slice(&bytes).unwrap();
    let contract = EvidenceWorkflowContract {
        file: DEFAULT_CI_EVIDENCE_WORKFLOW,
        display_name: "CI first-attempt evidence",
        artifact: CI_EVIDENCE_ARTIFACT_PATH,
        task: "ci-evidence",
        command: "cargo xtask ci-evidence run",
        timeout: 30,
        schedule: Some("47 4 * * *"),
        push_main: false,
        advisory: true,
        full_history: false,
    };
    validate_generated_workflow_shape(&workflow, &contract).unwrap();

    let mut skipped_job = workflow.clone();
    skipped_job["jobs"]["ci-evidence"]
        .as_object_mut()
        .unwrap()
        .insert("if".to_owned(), serde_json::json!("false"));
    let error = validate_generated_workflow_shape(&skipped_job, &contract).unwrap_err();
    assert!(error.to_string().contains("unexpected fields"), "{error:#}");

    let mut optional_command = workflow;
    optional_command["jobs"]["ci-evidence"]["steps"][2]
        .as_object_mut()
        .unwrap()
        .insert("continue-on-error".to_owned(), serde_json::json!(true));
    let error = validate_generated_workflow_shape(&optional_command, &contract).unwrap_err();
    assert!(error.to_string().contains("unexpected fields"), "{error:#}");
}

#[test]
fn push_head_ledger_requires_full_history_checkout() {
    let root = docs::repo_root().unwrap();
    let bytes = fs::read(root.join(".github/workflows/ci-push-head-ledger.yml")).unwrap();
    let workflow: serde_json::Value = serde_yaml_ng::from_slice(&bytes).unwrap();
    let contract = EvidenceWorkflowContract {
        file: DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW,
        display_name: "CI push-head ledger",
        artifact: CI_PUSH_HEAD_LEDGER_ARTIFACT_PATH,
        task: "ci-push-head-ledger",
        command: "cargo xtask ci-evidence record-push",
        timeout: 10,
        schedule: None,
        push_main: true,
        advisory: true,
        full_history: true,
    };
    validate_generated_workflow_shape(&workflow, &contract).unwrap();

    let mut shallow = workflow;
    shallow["jobs"]["ci-push-head-ledger"]["steps"][0]["with"]
        .as_object_mut()
        .unwrap()
        .remove("fetch-depth");
    let error = validate_generated_workflow_shape(&shallow, &contract).unwrap_err();
    assert!(error.to_string().contains("checkout inputs"), "{error:#}");
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
