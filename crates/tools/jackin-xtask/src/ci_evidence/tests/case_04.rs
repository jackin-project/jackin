// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
