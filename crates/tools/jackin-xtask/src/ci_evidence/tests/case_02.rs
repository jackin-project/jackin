// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
