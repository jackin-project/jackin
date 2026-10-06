// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const CI_EVIDENCE_WORKFLOW_FIXTURE: &str = include_str!("../fixtures/ci-evidence.yml");

pub(super) const CI_PUSH_HEAD_LEDGER_WORKFLOW_FIXTURE: &str =
    include_str!("../fixtures/ci-push-head-ledger.yml");

pub(super) fn commit_from_source(sha: &str, source: DenominatorSource) -> ExpectedCommit {
    ExpectedCommit {
        sha: sha.to_owned(),
        base_sha: Some("base".to_owned()),
        tree_sha: "tree".to_owned(),
        committed_at: Some("2026-09-22T00:00:00Z".to_owned()),
        source,
    }
}

pub(super) fn obligation(sha: &str, cohort: Cohort) -> ExpectedObligation {
    obligation_from_source(sha, cohort, DenominatorSource::Fixture)
}

pub(super) fn expected_for_sha(sha: &str) -> Vec<ExpectedObligation> {
    Cohort::ALL
        .into_iter()
        .map(|cohort| obligation(sha, cohort))
        .collect()
}

pub(super) fn obligation_from_source(
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

pub(super) fn test_runtime() -> RuntimeIdentity {
    RuntimeIdentity {
        runtime_revision: Some("0123456789abcdef0123456789abcdef01234567".to_owned()),
        contract_digest: Some(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_owned(),
        ),
    }
}

pub(super) fn test_provenance() -> CollectionProvenance {
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

pub(super) fn test_api_repository() -> ApiRunRepository {
    ApiRunRepository {
        id: TARGET_REPOSITORY_ID,
        full_name: TARGET_REPOSITORY.to_owned(),
    }
}

pub(super) fn attempt(
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

pub(super) fn evidence(
    expected: Vec<ExpectedObligation>,
    attempts: Vec<AttemptEvidence>,
) -> EvidenceFile {
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

pub(super) fn update_denominator(
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

pub(super) fn push_head_observation(
    head_sha: &str,
    before_sha: &str,
    run_id: u64,
) -> PushHeadObservation {
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

pub(super) fn refresh_push_head_artifact(observation: &mut PushHeadObservation) {
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

pub(super) fn fixture_sha(label: &str) -> String {
    sha256_hex(label.as_bytes())[..40].to_owned()
}

pub(super) fn run_git(root: &Path, args: &[&str]) -> String {
    let output = cmd::output(Command::new("git").current_dir(root).args(args))
        .unwrap_or_else(|error| panic!("running git {args:?}: {error}"));
    String::from_utf8(output)
        .unwrap_or_else(|error| panic!("git {args:?} returned non-UTF-8: {error}"))
        .trim()
        .to_owned()
}

pub(super) fn commit_git(root: &Path, message: &str, date: &str) {
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
