use super::*;

fn commit_from_source(sha: &str, source: DenominatorSource) -> ExpectedCommit {
    ExpectedCommit {
        sha: sha.to_owned(),
        base_sha: Some("base".to_owned()),
        tree_sha: Some("tree".to_owned()),
        committed_at: Some("2026-09-22T00:00:00Z".to_owned()),
        source,
    }
}

fn obligation(sha: &str, cohort: Cohort) -> ExpectedObligation {
    obligation_from_source(sha, cohort, DenominatorSource::FirstParentHistory)
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
            DenominatorSource::FirstParentHistory => ObligationProvenance::FirstParentHistory,
            DenominatorSource::Fixture => ObligationProvenance::Fixture,
        },
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
        denominator_source: DenominatorSource::FirstParentHistory,
        base_sha: Some("base".to_owned()),
        tree_sha: Some("tree".to_owned()),
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
        runtime: RuntimeIdentity {
            runtime_revision: Some("runtime".to_owned()),
            contract_digest: Some("contract".to_owned()),
        },
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
        runtime: RuntimeIdentity {
            runtime_revision: Some("runtime".to_owned()),
            contract_digest: Some("contract".to_owned()),
        },
        denominator: DenominatorProof {
            source: DenominatorSource::FirstParentHistory,
            branch: "main".to_owned(),
            window: TimeWindow {
                since: "2026-09-21T00:00:00Z".to_owned(),
                until: "2026-09-23T00:00:00Z".to_owned(),
            },
            fetch_succeeded: true,
            commit_count: history.len(),
        },
        history,
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
            source: DenominatorSource::FirstParentHistory,
            branch: "main".to_owned(),
            window: TimeWindow {
                since: "2026-09-21T00:00:00Z".to_owned(),
                until: "2026-09-23T00:00:00Z".to_owned(),
            },
            fetch_succeeded: true,
            commit_count: history.len(),
        },
        history,
    )
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
        head_sha: "sha".to_owned(),
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
        head_sha: "sha".to_owned(),
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
            repository: "example/repo".to_owned(),
            window: TimeWindow {
                since: "2026-09-21T00:00:00Z".to_owned(),
                until: "2026-09-23T00:00:00Z".to_owned(),
            },
            runtime: RuntimeIdentity::default(),
        },
    )
    .unwrap();
    let mut forged = merged;
    forged.attempts[0].conflicting_observations.clear();
    let error = validate_evidence(&forged).unwrap_err();
    assert!(error.to_string().contains("unproven data-quality conflict"));
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
        head_sha: "active".to_owned(),
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
            repository: "example/repo".to_owned(),
            window: TimeWindow {
                since: "2026-09-21T00:00:00Z".to_owned(),
                until: "2026-09-23T00:00:00Z".to_owned(),
            },
            runtime: RuntimeIdentity::default(),
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
            repository: "example/repo".to_owned(),
            window: TimeWindow {
                since: "2026-09-21T00:00:00Z".to_owned(),
                until: "2026-09-23T00:00:00Z".to_owned(),
            },
            runtime: RuntimeIdentity::default(),
        },
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
        classify_outcome("in_progress", None, &[], &[]),
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
    let jobs = completed_jobs(Cohort::CiMain);
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
fn first_parent_history_derives_both_contract_obligations() {
    let history = vec![HistoryCommitObservation {
        sha: "main-head".to_owned(),
        base_sha: Some("base".to_owned()),
        tree_sha: Some("tree".to_owned()),
        committed_at: "2026-09-22T00:00:00Z".to_owned(),
    }];
    let expected = expected_from_history(&history, DenominatorSource::FirstParentHistory).unwrap();

    assert_eq!(expected.len(), Cohort::ALL.len());
    assert!(expected.iter().all(|obligation| {
        obligation.commit.source == DenominatorSource::FirstParentHistory
            && obligation.provenance == ObligationProvenance::FirstParentHistory
    }));
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
fn derived_history_rejects_edited_expected_source() {
    let mut evidence = evidence(
        vec![
            obligation("head", Cohort::CiMain),
            obligation("head", Cohort::Desktop),
        ],
        Vec::new(),
    );
    evidence.expected[0].commit.source = DenominatorSource::Fixture;
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
            repository: "example/repo".to_owned(),
            window: TimeWindow {
                since: "2026-09-21T00:00:00Z".to_owned(),
                until: "2026-09-23T00:00:00Z".to_owned(),
            },
            runtime: RuntimeIdentity::default(),
        },
    )
    .unwrap();

    assert!(merged.attempts.is_empty());
    validate_evidence(&merged).unwrap();
}

#[test]
fn api_timestamps_are_canonical_and_plus_safe() {
    assert_eq!(
        api_timestamp("2026-09-22T00:00:00+07:00"),
        "2026-09-21T17:00:00Z"
    );
    assert_eq!(api_timestamp("not-a-timestamp"), "not-a-timestamp");
}
