// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
