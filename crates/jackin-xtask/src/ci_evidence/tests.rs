use super::*;

fn commit(sha: &str) -> ExpectedCommit {
    ExpectedCommit {
        sha: sha.to_owned(),
        base_sha: Some("base".to_owned()),
        tree_sha: Some("tree".to_owned()),
        committed_at: Some("2026-09-22T00:00:00Z".to_owned()),
    }
}

fn obligation(sha: &str, cohort: Cohort) -> ExpectedObligation {
    ExpectedObligation {
        commit: commit(sha),
        cohort,
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
        base_sha: Some("base".to_owned()),
        tree_sha: Some("tree".to_owned()),
        created_at: created_at.to_owned(),
        started_at: Some(created_at.to_owned()),
        completed_at: Some(created_at.to_owned()),
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
        observed_work: vec!["job".to_owned()],
        jobs: vec![JobEvidence {
            id: run_id + 1000,
            name: "job".to_owned(),
            status: "completed".to_owned(),
            conclusion: Some("success".to_owned()),
            started_at: Some(created_at.to_owned()),
            completed_at: Some(created_at.to_owned()),
            evidence_url: Some(format!("https://example.test/jobs/{run_id}")),
        }],
        classification,
        runtime: RuntimeIdentity {
            runtime_revision: Some("runtime".to_owned()),
            contract_digest: Some("contract".to_owned()),
        },
        evidence_urls: vec![format!("https://example.test/runs/{run_id}")],
        first_observed_at: "2026-09-22T00:01:00Z".to_owned(),
    }
}

fn evidence(expected: Vec<ExpectedObligation>, attempts: Vec<AttemptEvidence>) -> EvidenceFile {
    EvidenceFile {
        schema: SCHEMA,
        repository: "example/repo".to_owned(),
        window: TimeWindow {
            since: "2026-09-21T00:00:00Z".to_owned(),
            until: "2026-09-23T00:00:00Z".to_owned(),
        },
        generated_at: "2026-09-22T00:00:00Z".to_owned(),
        runtime: RuntimeIdentity {
            runtime_revision: Some("runtime".to_owned()),
            contract_digest: Some("contract".to_owned()),
        },
        expected,
        attempts,
    }
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
fn workflow_name_survives_path_rename() {
    let run = ApiRun {
        id: 1,
        workflow_id: Some(99),
        workflow_name: Some("CI / Main".to_owned()),
        path: Some(".github/workflows/ci-main-v2.yml".to_owned()),
        event: Some("push".to_owned()),
        head_sha: "sha".to_owned(),
        status: "completed".to_owned(),
        conclusion: Some("success".to_owned()),
        run_attempt: 1,
        created_at: "2026-09-22T00:00:00Z".to_owned(),
        run_started_at: None,
        updated_at: None,
        html_url: None,
    };
    assert_eq!(
        classify_workflow(&run, &["ci-main.yml".to_owned()], &[]),
        Some(Cohort::CiMain)
    );
}

#[test]
fn merge_deduplicates_delivery_and_keeps_rerun_attempt() {
    let existing = evidence(
        vec![obligation("sha", Cohort::CiMain)],
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
        vec![obligation("sha", Cohort::CiMain)],
        vec![
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
        "example/repo".to_owned(),
        TimeWindow {
            since: "2026-09-21T00:00:00Z".to_owned(),
            until: "2026-09-23T00:00:00Z".to_owned(),
        },
        RuntimeIdentity::default(),
    )
    .unwrap();
    assert_eq!(merged.attempts.len(), 2);
    assert!(merged.attempts.iter().any(|row| row.attempt == 1));
    assert!(merged.attempts.iter().any(|row| row.attempt == 2));
}

#[test]
fn merge_does_not_replace_terminal_observation_with_stale_in_progress_row() {
    let existing = evidence(
        vec![obligation("sha", Cohort::CiMain)],
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
        vec![obligation("sha", Cohort::CiMain)],
        vec![stale],
        "example/repo".to_owned(),
        TimeWindow {
            since: "2026-09-21T00:00:00Z".to_owned(),
            until: "2026-09-23T00:00:00Z".to_owned(),
        },
        RuntimeIdentity::default(),
    )
    .unwrap();
    assert_eq!(merged.attempts[0].classification, OutcomeClass::Product);
    assert_eq!(merged.attempts[0].status, "completed");
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
        classify_outcome("in_progress", None, &[]),
        OutcomeClass::DataQuality
    );
    assert_eq!(
        classify_outcome("completed", Some("timed_out"), &[]),
        OutcomeClass::Infrastructure
    );
    assert_eq!(
        classify_outcome("completed", Some("success"), &[]),
        OutcomeClass::Infrastructure
    );
    assert_eq!(
        classify_outcome("completed", Some("cancelled"), &[]),
        OutcomeClass::Cancellation
    );
    assert_eq!(
        classify_outcome("completed", Some("skipped"), &[]),
        OutcomeClass::Inapplicable
    );
    assert_eq!(
        classify_outcome("completed", Some("failure"), &[JobEvidence::default()]),
        OutcomeClass::Product
    );
}

#[test]
fn api_timestamps_are_canonical_and_plus_safe() {
    assert_eq!(
        api_timestamp("2026-09-22T00:00:00+07:00"),
        "2026-09-21T17:00:00Z"
    );
    assert_eq!(api_timestamp("not-a-timestamp"), "not-a-timestamp");
}
