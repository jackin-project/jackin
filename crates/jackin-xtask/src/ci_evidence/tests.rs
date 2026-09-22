use super::*;

fn commit(sha: &str) -> ExpectedCommit {
    commit_from_source(sha, DenominatorSource::PushEvent)
}

fn commit_from_source(sha: &str, source: DenominatorSource) -> ExpectedCommit {
    ExpectedCommit {
        sha: sha.to_owned(),
        base_sha: (source == DenominatorSource::PushEvent).then(|| "base".to_owned()),
        tree_sha: (source == DenominatorSource::PushEvent).then(|| "tree".to_owned()),
        committed_at: (source == DenominatorSource::PushEvent)
            .then(|| "2026-09-22T00:00:00Z".to_owned()),
        source,
    }
}

fn obligation(sha: &str, cohort: Cohort) -> ExpectedObligation {
    obligation_from_source(sha, cohort, DenominatorSource::PushEvent)
}

fn obligation_from_source(
    sha: &str,
    cohort: Cohort,
    source: DenominatorSource,
) -> ExpectedObligation {
    ExpectedObligation {
        commit: commit_from_source(sha, source),
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
        denominator_source: DenominatorSource::PushEvent,
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
        unclassified_runs: Vec::new(),
        event_source_gaps: Vec::new(),
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
fn workflow_name_survives_path_rename() {
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
        classify_workflow(
            &run,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &["ci-main.yml".to_owned()],
            &[],
        ),
        Some(Cohort::CiMain)
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
        classify_workflow(&run, &BTreeSet::from([99]), &BTreeSet::new(), &[], &[],),
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
        EvidenceUpdate {
            expected: vec![obligation("sha", Cohort::CiMain)],
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
            event_source_gaps: Vec::new(),
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
        EvidenceUpdate {
            expected: vec![obligation("sha", Cohort::CiMain)],
            attempts: vec![stale],
            unclassified_runs: Vec::new(),
            event_source_gaps: Vec::new(),
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
    let expected = vec![obligation("sha", Cohort::CiMain)];
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
fn missing_push_event_adds_observed_head_with_explicit_gap() {
    let commits = BTreeMap::from([("event-head".to_owned(), commit("event-head"))]);
    let observed = BTreeMap::from([(
        "run-head".to_owned(),
        ObservedHead {
            run_ids: BTreeSet::from([35_721_133_867]),
            cohorts: BTreeSet::from([Cohort::Desktop]),
        },
    )]);

    let (commits, gaps) = merge_observed_heads(commits, observed);

    assert_eq!(
        commits["run-head"].source,
        DenominatorSource::ObservedRunFallback
    );
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].head_sha, "run-head");
    assert_eq!(gaps[0].observed_run_ids, [35_721_133_867]);
    assert_eq!(gaps[0].observed_cohorts, [Cohort::Desktop]);
}

#[test]
fn observed_fallback_reconstructs_both_cohort_obligations() {
    let observed = BTreeMap::from([(
        "run-head".to_owned(),
        ObservedHead {
            run_ids: BTreeSet::from([101, 202]),
            cohorts: BTreeSet::from([Cohort::CiMain, Cohort::Desktop]),
        },
    )]);
    let (commits, gaps) = merge_observed_heads(BTreeMap::new(), observed);
    let expected = Cohort::ALL
        .into_iter()
        .map(|cohort| ExpectedObligation {
            commit: commits["run-head"].clone(),
            cohort,
        })
        .collect::<Vec<_>>();

    assert_eq!(expected.len(), 2);
    assert!(
        expected.iter().all(|obligation| {
            obligation.commit.source == DenominatorSource::ObservedRunFallback
        })
    );
    assert_eq!(gaps[0].observed_cohorts, [Cohort::CiMain, Cohort::Desktop]);
}

#[test]
fn fallback_warning_provenance_blocks_qualified_green() {
    let expected = vec![
        obligation_from_source(
            "fallback",
            Cohort::CiMain,
            DenominatorSource::ObservedRunFallback,
        ),
        obligation_from_source(
            "fallback",
            Cohort::Desktop,
            DenominatorSource::ObservedRunFallback,
        ),
    ];
    let mut ci = attempt(
        101,
        1,
        Cohort::CiMain,
        "fallback",
        OutcomeClass::Success,
        "2026-09-22T00:02:00Z",
    );
    let mut desktop = attempt(
        202,
        1,
        Cohort::Desktop,
        "fallback",
        OutcomeClass::Success,
        "2026-09-22T00:02:00Z",
    );
    for row in [&mut ci, &mut desktop] {
        row.denominator_source = DenominatorSource::ObservedRunFallback;
        row.jobs = completed_jobs(row.cohort);
        row.observed_work = row.jobs.iter().map(|job| job.name.clone()).collect();
        row.conclusion = Some("success".to_owned());
        row.classification = OutcomeClass::Success;
    }
    let mut evidence = evidence(expected, vec![ci, desktop]);
    evidence.event_source_gaps = vec![EventSourceGap {
        head_sha: "fallback".to_owned(),
        observed_run_ids: vec![101, 202],
        observed_cohorts: vec![Cohort::CiMain, Cohort::Desktop],
        reason: "push event missing".to_owned(),
    }];

    validate_evidence(&evidence).unwrap();
    let rollup = build_rollup(&evidence);
    assert!(!rollup.green_claim_qualified);
    assert!(!rollup.six_nines_claimed);
    assert_eq!(rollup.event_source_gaps.len(), 1);

    evidence.event_source_gaps.clear();
    let error = validate_evidence(&evidence).unwrap_err();
    assert!(error.to_string().contains("fallback denominator head"));
}

#[test]
fn rolling_window_merge_prunes_attempts_outside_new_denominator() {
    let mut old = evidence(
        vec![obligation("old-head", Cohort::CiMain)],
        vec![attempt(
            1,
            1,
            Cohort::CiMain,
            "old-head",
            OutcomeClass::Product,
            "2026-08-01T00:00:00Z",
        )],
    );
    old.event_source_gaps = vec![EventSourceGap {
        head_sha: "old-head".to_owned(),
        observed_run_ids: vec![1],
        observed_cohorts: vec![Cohort::CiMain],
        reason: "old rolling-window warning".to_owned(),
    }];
    let merged = merge_evidence(
        old,
        EvidenceUpdate {
            expected: vec![obligation("new-head", Cohort::CiMain)],
            attempts: Vec::new(),
            unclassified_runs: Vec::new(),
            event_source_gaps: Vec::new(),
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
    assert!(merged.event_source_gaps.is_empty());
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
