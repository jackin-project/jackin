// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
