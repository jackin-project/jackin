// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn push_head_artifact_fixture() -> (tempfile::TempDir, ApiRun, Vec<u8>, Vec<u8>) {
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

pub(super) fn zip_fixture(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in files {
        writer.start_file(*name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

pub(super) fn push_head_denominator(
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

pub(super) fn completed_jobs(cohort: Cohort) -> Vec<JobEvidence> {
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
