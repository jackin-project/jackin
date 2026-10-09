// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::MonitorStore;
use jackin_protocol::control::Money;
use jackin_protocol::usage_monitor::{
    MonitorAction, MonitorConfig, MonitorEvidenceFreshness, MonitorIssueCode, MonitorLifecycle,
    MonitorOperation, MonitorProvider, MonitorReply, MonitorStatus, SpendRecord, SpendRecordInput,
    SpendRecordSource, SpendVerification, StatuslineObservation, StatuslineQuotaWindow,
    StatuslineRateLimits, USAGE_MONITOR_SCHEMA_VERSION,
};

const NOW: i64 = 1_800_000_000;

fn open_store() -> (tempfile::TempDir, MonitorStore) {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let store = MonitorStore::open(directory.path()).expect("open monitor store");
    (directory, store)
}

fn config(
    account_id: &str,
    goal_id: &str,
    expected_model: Option<&str>,
    budget: Option<Money>,
) -> MonitorConfig {
    MonitorConfig {
        provider: MonitorProvider::Claude,
        account_id: account_id.to_owned(),
        goal_id: goal_id.to_owned(),
        session_id: None,
        expected_model: expected_model.map(str::to_owned),
        budget,
    }
}

fn quota(used: Option<i32>, reset: Option<i64>) -> Option<StatuslineQuotaWindow> {
    Some(StatuslineQuotaWindow {
        used_percentage_basis_points: used,
        reset_at_epoch: reset,
    })
}

fn observation(
    session_id: &str,
    model: Option<&str>,
    five_hour: (Option<i32>, Option<i64>),
    seven_day: (Option<i32>, Option<i64>),
) -> StatuslineObservation {
    StatuslineObservation {
        schema_version: USAGE_MONITOR_SCHEMA_VERSION,
        session_id: session_id.to_owned(),
        model: model.map(str::to_owned),
        rate_limits: StatuslineRateLimits {
            five_hour: quota(five_hour.0, five_hour.1),
            seven_day: quota(seven_day.0, seven_day.1),
        },
    }
}

fn ingest(
    store: &MonitorStore,
    account_id: &str,
    observation: StatuslineObservation,
    now_epoch: i64,
) -> u64 {
    match store
        .operate(
            MonitorOperation::Ingest {
                account_id: account_id.to_owned(),
                observation,
            },
            now_epoch,
        )
        .expect("ingest statusline")
    {
        MonitorReply::Ingested {
            evidence_sequence, ..
        } => evidence_sequence,
        other => panic!("expected ingest reply, got {other:?}"),
    }
}

fn status(store: &MonitorStore, monitor_id: &str, now_epoch: i64) -> MonitorStatus {
    match store
        .operate(
            MonitorOperation::Status {
                monitor_id: monitor_id.to_owned(),
            },
            now_epoch,
        )
        .expect("read monitor status")
    {
        MonitorReply::Status { status } => *status,
        other => panic!("expected status reply, got {other:?}"),
    }
}

fn start(store: &MonitorStore, config: MonitorConfig, now_epoch: i64) -> MonitorStatus {
    match store
        .operate(MonitorOperation::Start { config }, now_epoch)
        .expect("start monitor")
    {
        MonitorReply::Started { status } => *status,
        other => panic!("expected started reply, got {other:?}"),
    }
}

fn actions(status: &MonitorStatus) -> &[MonitorAction] {
    &status
        .latest_decision
        .as_ref()
        .expect("monitor has a decision")
        .actions
}

fn has_checkpoint(status: &MonitorStatus) -> bool {
    actions(status)
        .iter()
        .any(|action| matches!(action, MonitorAction::Checkpoint { .. }))
}

fn has_reduce_dispatch(status: &MonitorStatus) -> bool {
    actions(status)
        .iter()
        .any(|action| matches!(action, MonitorAction::ReduceDispatch { .. }))
}

fn has_pause(status: &MonitorStatus, reason: MonitorIssueCode) -> bool {
    actions(status)
        .iter()
        .any(|action| matches!(action, MonitorAction::Pause { reason: found } if *found == reason))
}

fn has_issue(status: &MonitorStatus, code: MonitorIssueCode) -> bool {
    status.issues.iter().any(|issue| issue.code == code)
}

fn spend_input(
    account_id: &str,
    period_start: i64,
    period_end: i64,
    amount_minor: i64,
    evidence_at_epoch: i64,
    currency: &str,
) -> SpendRecordInput {
    SpendRecordInput {
        account_id: account_id.to_owned(),
        billing_period_start_epoch: period_start,
        billing_period_end_epoch: period_end,
        amount: Money::new(amount_minor, currency, 2),
        evidence_at_epoch: Some(evidence_at_epoch),
        verified: true,
        source: SpendRecordSource::OperatorReceipt,
    }
}

fn record_spend(store: &MonitorStore, record: SpendRecordInput, now_epoch: i64) -> SpendRecord {
    match store
        .operate(MonitorOperation::RecordSpend { record }, now_epoch)
        .expect("record spend")
    {
        MonitorReply::SpendRecorded { record } => record,
        other => panic!("expected spend reply, got {other:?}"),
    }
}

fn seed_zero_sgd_spend(store: &MonitorStore, account_id: &str, now_epoch: i64) {
    record_spend(
        store,
        spend_input(account_id, NOW - 100, NOW + 100_000, 0, now_epoch, "SGD"),
        now_epoch,
    );
}

#[test]
fn quota_thresholds_emit_every_crossed_action_and_keep_repeated_actions_idempotent() {
    let (_directory, store) = open_store();
    let reset = NOW + 3_600;
    ingest(
        &store,
        "acct-quota",
        observation(
            "session-a",
            None,
            (Some(8_900), Some(reset)),
            (Some(8_900), Some(reset)),
        ),
        NOW,
    );
    seed_zero_sgd_spend(&store, "acct-quota", NOW);
    let started = start(
        &store,
        config(
            "acct-quota",
            "goal-quota",
            None,
            Some(Money::new(5_000, "SGD", 2)),
        ),
        NOW,
    );
    let monitor_id = started.monitor_id.clone();
    assert!(started.runnable);
    assert!(!has_checkpoint(&started));

    let mut guard_pause_sequence = None;
    let mut exhausted_sequence = None;
    for (offset, used, checkpoint, reduce, pause_reason) in [
        (1, 9_000, true, false, None),
        (2, 9_100, true, true, None),
        (3, 9_400, true, true, None),
        (
            4,
            9_500,
            true,
            true,
            Some(MonitorIssueCode::LimitGuardReached),
        ),
        (
            5,
            10_000,
            true,
            true,
            Some(MonitorIssueCode::LimitExhausted),
        ),
    ] {
        ingest(
            &store,
            "acct-quota",
            observation(
                "session-a",
                None,
                (Some(used), Some(reset)),
                (Some(used), Some(reset)),
            ),
            NOW + offset,
        );
        let current = status(&store, &monitor_id, NOW + offset);
        assert_eq!(has_checkpoint(&current), checkpoint, "usage {used}");
        assert_eq!(has_reduce_dispatch(&current), reduce, "usage {used}");
        assert_eq!(
            pause_reason.is_some_and(|reason| has_pause(&current, reason)),
            pause_reason.is_some(),
            "usage {used}"
        );
        assert_eq!(current.runnable, pause_reason.is_none(), "usage {used}");
        if used == 9_500 {
            guard_pause_sequence = Some(current.latest_decision.as_ref().unwrap().sequence);
        }
        if used == 10_000 {
            assert_eq!(
                current.latest_decision.as_ref().unwrap().sequence,
                guard_pause_sequence.expect("95 percent pause decision sequence") + 1,
                "the 100 percent exhaustion reason is a distinct decision"
            );
            exhausted_sequence = Some(current.latest_decision.as_ref().unwrap().sequence);
        }
    }

    ingest(
        &store,
        "acct-quota",
        observation(
            "session-a",
            None,
            (Some(10_000), Some(reset)),
            (Some(10_000), Some(reset)),
        ),
        NOW + 6,
    );
    let repeated_exhaustion = status(&store, &monitor_id, NOW + 6);
    assert_eq!(
        repeated_exhaustion
            .latest_decision
            .as_ref()
            .unwrap()
            .sequence,
        exhausted_sequence.expect("100 percent exhaustion decision sequence"),
        "repeating an unchanged exhausted state must not create a decision"
    );
}

#[test]
fn quota_threshold_direct_jumps_emit_every_crossed_action() {
    let (_directory, store) = open_store();
    let reset = NOW + 3_600;
    // A direct jump must still include every action crossed by the new value.
    ingest(
        &store,
        "acct-jump",
        observation(
            "session-jump",
            None,
            (Some(8_900), Some(reset)),
            (Some(8_900), Some(reset)),
        ),
        NOW,
    );
    seed_zero_sgd_spend(&store, "acct-jump", NOW);
    let jump_monitor = start(
        &store,
        config(
            "acct-jump",
            "goal-jump",
            None,
            Some(Money::new(5_000, "SGD", 2)),
        ),
        NOW,
    );
    ingest(
        &store,
        "acct-jump",
        observation(
            "session-jump",
            None,
            (Some(9_600), Some(reset)),
            (Some(9_600), Some(reset)),
        ),
        NOW + 1,
    );
    let jumped = status(&store, &jump_monitor.monitor_id, NOW + 1);
    assert!(has_checkpoint(&jumped));
    assert!(has_reduce_dispatch(&jumped));
    assert!(has_pause(&jumped, MonitorIssueCode::LimitGuardReached));

    ingest(
        &store,
        "acct-jump-100",
        observation(
            "session-jump-100",
            None,
            (Some(9_400), Some(reset)),
            (Some(9_400), Some(reset)),
        ),
        NOW,
    );
    seed_zero_sgd_spend(&store, "acct-jump-100", NOW);
    let jump_to_100 = start(
        &store,
        config(
            "acct-jump-100",
            "goal-jump-100",
            None,
            Some(Money::new(5_000, "SGD", 2)),
        ),
        NOW,
    );
    ingest(
        &store,
        "acct-jump-100",
        observation(
            "session-jump-100",
            None,
            (Some(10_000), Some(reset)),
            (Some(10_000), Some(reset)),
        ),
        NOW + 1,
    );
    let jumped_to_100 = status(&store, &jump_to_100.monitor_id, NOW + 1);
    assert!(has_checkpoint(&jumped_to_100));
    assert!(has_reduce_dispatch(&jumped_to_100));
    assert!(has_pause(&jumped_to_100, MonitorIssueCode::LimitExhausted));
}

#[test]
fn identical_statusline_does_not_refresh_evidence_age_or_decision_sequence() {
    let (_directory, store) = open_store();
    let reset = NOW + 3_600;
    let observation = observation(
        "session-a",
        Some("claude-sonnet"),
        (Some(9_100), Some(reset)),
        (Some(2_000), Some(reset)),
    );
    ingest(&store, "acct-age", observation.clone(), NOW);
    let started = start(&store, config("acct-age", "goal-age", None, None), NOW);
    let sequence = started.latest_decision.as_ref().unwrap().sequence;
    let input_sequence = ingest(&store, "acct-age", observation, NOW + 20);
    let current = status(&store, &started.monitor_id, NOW + 20);
    assert_eq!(current.latest_decision.as_ref().unwrap().sequence, sequence);
    assert_eq!(
        current
            .five_hour
            .used_evidence
            .as_ref()
            .unwrap()
            .evidence_received_at_epoch,
        NOW
    );
    assert_eq!(
        current
            .five_hour
            .used_evidence
            .as_ref()
            .unwrap()
            .age_seconds,
        20
    );
    assert_eq!(
        input_sequence, 1,
        "identical input must not allocate new evidence"
    );
}

#[test]
fn missing_and_stale_quota_fields_remain_unknown_independently() {
    let (_directory, store) = open_store();
    let reset = NOW + 3_600;
    ingest(
        &store,
        "acct-fields",
        observation(
            "session-fields",
            None,
            (Some(1_000), Some(reset)),
            (None, None),
        ),
        NOW,
    );
    let started = start(
        &store,
        config("acct-fields", "goal-fields", None, None),
        NOW,
    );
    assert!(!started.runnable);
    assert_eq!(
        started.five_hour.used_evidence.as_ref().unwrap().freshness,
        MonitorEvidenceFreshness::Current
    );
    assert_eq!(started.seven_day.used_evidence, None);
    assert!(has_issue(&started, MonitorIssueCode::QuotaUnknown));
    assert!(has_issue(&started, MonitorIssueCode::MissingReset));

    // A changed utilization field refreshes only its own TTL; reset and seven-day
    // evidence remain stale or missing.
    ingest(
        &store,
        "acct-fields",
        observation("session-fields", None, (Some(1_100), None), (None, None)),
        NOW + 301,
    );
    let current = status(&store, &started.monitor_id, NOW + 301);
    assert_eq!(
        current.five_hour.used_evidence.as_ref().unwrap().freshness,
        MonitorEvidenceFreshness::Current
    );
    assert_eq!(
        current.five_hour.reset_evidence.as_ref().unwrap().freshness,
        MonitorEvidenceFreshness::Stale
    );
    assert_eq!(current.seven_day.used_evidence, None);
    assert_eq!(current.seven_day.reset_evidence, None);
    assert!(!current.runnable);
    assert!(has_issue(&current, MonitorIssueCode::QuotaStale));
    assert!(has_issue(&current, MonitorIssueCode::QuotaUnknown));
}

#[test]
fn older_overlapping_session_cannot_replace_the_account_reset_watermark() {
    let (_directory, store) = open_store();
    let old_five_hour_reset = NOW + 3_600;
    let old_seven_day_reset = NOW + 86_400;
    ingest(
        &store,
        "acct-reset-watermark",
        observation(
            "session-old",
            None,
            (Some(3_000), Some(old_five_hour_reset)),
            (Some(3_000), Some(old_seven_day_reset)),
        ),
        NOW,
    );
    seed_zero_sgd_spend(&store, "acct-reset-watermark", NOW);
    let started = start(
        &store,
        config(
            "acct-reset-watermark",
            "goal-reset-watermark",
            None,
            Some(Money::new(5_000, "SGD", 2)),
        ),
        NOW,
    );

    let current_five_hour_reset = NOW + 7_200;
    let current_seven_day_reset = NOW + 172_800;
    let current_sequence = ingest(
        &store,
        "acct-reset-watermark",
        observation(
            "session-current",
            None,
            (Some(1_000), Some(current_five_hour_reset)),
            (Some(1_000), Some(current_seven_day_reset)),
        ),
        NOW + 1,
    );
    let current = status(&store, &started.monitor_id, NOW + 1);
    assert!(current.runnable);
    assert_eq!(current.five_hour.used_percentage_basis_points, Some(1_000));
    assert_eq!(
        current.five_hour.reset_at_epoch,
        Some(current_five_hour_reset)
    );
    assert_eq!(current.seven_day.used_percentage_basis_points, Some(1_000));
    assert_eq!(
        current.seven_day.reset_at_epoch,
        Some(current_seven_day_reset)
    );
    assert_eq!(
        current
            .five_hour
            .used_evidence
            .as_ref()
            .unwrap()
            .evidence_received_at_epoch,
        NOW + 1
    );

    // A fresh callback from a different session still refers to the old
    // windows. It cannot lower the account watermark or replace current use.
    let replayed_sequence = ingest(
        &store,
        "acct-reset-watermark",
        observation(
            "session-old",
            None,
            (Some(9_900), Some(old_five_hour_reset)),
            (Some(9_900), Some(old_seven_day_reset)),
        ),
        NOW + 2,
    );
    assert_eq!(replayed_sequence, current_sequence);
    let after_old_callback = status(&store, &started.monitor_id, NOW + 2);
    assert!(after_old_callback.runnable);
    assert_eq!(
        after_old_callback.five_hour.used_percentage_basis_points,
        Some(1_000)
    );
    assert_eq!(
        after_old_callback.five_hour.reset_at_epoch,
        Some(current_five_hour_reset)
    );
    assert_eq!(
        after_old_callback.seven_day.used_percentage_basis_points,
        Some(1_000)
    );
    assert_eq!(
        after_old_callback.seven_day.reset_at_epoch,
        Some(current_seven_day_reset)
    );
    assert_eq!(
        after_old_callback
            .five_hour
            .used_evidence
            .as_ref()
            .unwrap()
            .evidence_received_at_epoch,
        NOW + 1,
        "rejected old-window input must not refresh current evidence age"
    );
    assert!(!has_pause(
        &after_old_callback,
        MonitorIssueCode::LimitGuardReached
    ));
}

#[test]
fn a_stale_high_session_does_not_resume_until_lower_usage_confirms_a_new_reset() {
    let (_directory, store) = open_store();
    let reset = NOW + 3_600;
    ingest(
        &store,
        "acct-overlap",
        observation(
            "session-old",
            None,
            (Some(9_600), Some(reset)),
            (Some(9_600), Some(reset)),
        ),
        NOW,
    );
    seed_zero_sgd_spend(&store, "acct-overlap", NOW);
    let started = start(
        &store,
        config(
            "acct-overlap",
            "goal-overlap",
            None,
            Some(Money::new(5_000, "SGD", 2)),
        ),
        NOW,
    );
    assert!(has_pause(&started, MonitorIssueCode::LimitGuardReached));

    ingest(
        &store,
        "acct-overlap",
        observation(
            "session-current",
            None,
            (Some(1_000), Some(reset)),
            (Some(1_000), Some(reset)),
        ),
        NOW + 1,
    );
    let conservative = status(&store, &started.monitor_id, NOW + 1);
    assert_eq!(
        conservative.five_hour.used_percentage_basis_points,
        Some(9_600)
    );
    assert!(has_pause(
        &conservative,
        MonitorIssueCode::LimitGuardReached
    ));

    // At the exact per-field boundary the lower session is still Current,
    // while the older high observation is Stale. Its old-reset value cannot
    // clear the pause after the high session expires.
    let after_old_expires = status(&store, &started.monitor_id, NOW + 301);
    assert_eq!(
        after_old_expires.five_hour.used_percentage_basis_points,
        Some(1_000)
    );
    assert_eq!(
        after_old_expires
            .five_hour
            .used_evidence
            .as_ref()
            .unwrap()
            .freshness,
        MonitorEvidenceFreshness::Current
    );
    assert!(
        actions(&after_old_expires)
            .iter()
            .any(|action| matches!(action, MonitorAction::Pause { .. }))
    );
    assert!(!after_old_expires.runnable);

    let deadline = reset + 60;
    store
        .tick(deadline)
        .expect("tick at old reset grace deadline");
    let due = status(&store, &started.monitor_id, deadline);
    assert!(has_issue(&due, MonitorIssueCode::ResetDueUnverified));

    let advanced_reset = reset + 3_600;
    seed_zero_sgd_spend(&store, "acct-overlap", deadline + 1);
    ingest(
        &store,
        "acct-overlap",
        observation(
            "session-current",
            None,
            (Some(1_000), Some(advanced_reset)),
            (Some(1_000), Some(advanced_reset)),
        ),
        deadline + 1,
    );
    let recovered = status(&store, &started.monitor_id, deadline + 1);
    assert!(recovered.runnable);
    assert!(
        !actions(&recovered)
            .iter()
            .any(|action| matches!(action, MonitorAction::Pause { .. }))
    );
}

#[test]
fn account_scope_expected_model_does_not_resume_while_another_fresh_session_mismatches() {
    let (_directory, store) = open_store();
    let reset = NOW + 3_600;
    ingest(
        &store,
        "acct-model",
        observation(
            "session-a",
            None,
            (Some(1_000), Some(reset)),
            (Some(1_000), Some(reset)),
        ),
        NOW,
    );
    seed_zero_sgd_spend(&store, "acct-model", NOW);
    let started = start(
        &store,
        config(
            "acct-model",
            "goal-model",
            Some("claude-sonnet"),
            Some(Money::new(5_000, "SGD", 2)),
        ),
        NOW,
    );
    ingest(
        &store,
        "acct-model",
        observation(
            "session-b",
            Some("claude-opus"),
            (Some(1_000), Some(reset)),
            (Some(1_000), Some(reset)),
        ),
        NOW + 1,
    );
    let mismatched = status(&store, &started.monitor_id, NOW + 1);
    assert!(!mismatched.runnable);
    assert!(has_issue(&mismatched, MonitorIssueCode::ModelUnknown));
    assert!(has_issue(&mismatched, MonitorIssueCode::ModelMismatch));

    ingest(
        &store,
        "acct-model",
        observation(
            "session-a",
            Some("claude-sonnet"),
            (Some(1_000), Some(reset)),
            (Some(1_000), Some(reset)),
        ),
        NOW + 2,
    );
    let latest_match = status(&store, &started.monitor_id, NOW + 2);
    assert!(
        !latest_match.runnable,
        "fresh mismatch in session B remains relevant"
    );
    assert_eq!(latest_match.lifecycle, MonitorLifecycle::Paused);
    assert!(has_issue(&latest_match, MonitorIssueCode::ModelMismatch));
}

#[test]
fn reset_deadline_is_scheduled_and_barrier_only_clears_on_new_paired_window_evidence() {
    let (_directory, store) = open_store();
    let reset = NOW + 100;
    let prior = observation(
        "session-reset",
        None,
        (Some(8_000), Some(reset)),
        (Some(8_000), Some(reset)),
    );
    ingest(&store, "acct-reset", prior.clone(), NOW);
    seed_zero_sgd_spend(&store, "acct-reset", NOW);
    let started = start(
        &store,
        config(
            "acct-reset",
            "goal-reset",
            None,
            Some(Money::new(5_000, "SGD", 2)),
        ),
        NOW,
    );
    let deadline = reset + 60;
    assert_eq!(store.next_wake(), Some(reset));
    store.tick(reset).expect("tick at reported reset");
    assert_eq!(store.next_wake(), Some(deadline));

    store.tick(deadline).expect("tick at reset deadline");
    let paused = status(&store, &started.monitor_id, deadline);
    assert_eq!(paused.lifecycle, MonitorLifecycle::Paused);
    assert!(!paused.runnable);
    assert!(has_issue(&paused, MonitorIssueCode::ResetDueUnverified));
    assert!(has_pause(&paused, MonitorIssueCode::ResetDueUnverified));

    store.tick(deadline + 400).expect("clock-only tick");
    let still_paused = status(&store, &started.monitor_id, deadline + 400);
    assert!(
        !still_paused.runnable,
        "clock alone cannot clear the barrier"
    );
    assert!(has_issue(
        &still_paused,
        MonitorIssueCode::ResetDueUnverified
    ));

    ingest(&store, "acct-reset", prior, deadline + 401);
    let repeated = status(&store, &started.monitor_id, deadline + 401);
    assert!(
        !repeated.runnable,
        "identical statusline cannot clear the barrier"
    );
    assert!(has_issue(&repeated, MonitorIssueCode::ResetDueUnverified));
    assert_eq!(
        repeated
            .five_hour
            .used_evidence
            .as_ref()
            .unwrap()
            .evidence_received_at_epoch,
        NOW
    );

    seed_zero_sgd_spend(&store, "acct-reset", deadline + 401);
    let advanced_reset = reset + 3_600;
    ingest(
        &store,
        "acct-reset",
        observation(
            "session-reset",
            None,
            (Some(1_000), Some(advanced_reset)),
            (Some(1_000), Some(advanced_reset)),
        ),
        deadline + 402,
    );
    let recovered = status(&store, &started.monitor_id, deadline + 402);
    assert!(!has_issue(&recovered, MonitorIssueCode::ResetDueUnverified));
    assert!(recovered.runnable);
}

#[test]
fn seven_day_reset_barrier_has_its_own_grace_deadline_and_requires_paired_recovery() {
    let (_directory, store) = open_store();
    let five_hour_reset = NOW + 3_600;
    let seven_day_reset = NOW + 7 * 24 * 60 * 60;
    let seven_day_deadline = seven_day_reset + 60;
    ingest(
        &store,
        "acct-seven-day-reset",
        observation(
            "session-seven-day",
            None,
            (Some(1_000), Some(five_hour_reset)),
            (Some(9_600), Some(seven_day_reset)),
        ),
        NOW,
    );
    record_spend(
        &store,
        spend_input(
            "acct-seven-day-reset",
            NOW - 100,
            NOW + 30 * 24 * 60 * 60,
            0,
            NOW,
            "SGD",
        ),
        NOW,
    );
    let started = start(
        &store,
        config(
            "acct-seven-day-reset",
            "goal-seven-day-reset",
            None,
            Some(Money::new(5_000, "SGD", 2)),
        ),
        NOW,
    );
    assert!(!started.runnable);
    assert!(has_pause(&started, MonitorIssueCode::LimitGuardReached));

    // Keep the independent five-hour window current immediately before the
    // seven-day deadline. The old seven-day fields deliberately stay unchanged.
    let before_deadline = seven_day_deadline - 1;
    ingest(
        &store,
        "acct-seven-day-reset",
        observation(
            "session-seven-day",
            None,
            (Some(1_100), Some(before_deadline + 3_600)),
            (Some(9_600), Some(seven_day_reset)),
        ),
        before_deadline,
    );
    let current_five_hour = status(&store, &started.monitor_id, before_deadline);
    assert_eq!(
        current_five_hour
            .five_hour
            .used_evidence
            .as_ref()
            .unwrap()
            .freshness,
        MonitorEvidenceFreshness::Current
    );
    assert_eq!(store.next_wake(), Some(seven_day_deadline));

    store
        .tick(seven_day_deadline)
        .expect("tick at seven-day reset grace deadline");
    let due = status(&store, &started.monitor_id, seven_day_deadline);
    assert!(!due.runnable);
    assert!(has_issue(&due, MonitorIssueCode::ResetDueUnverified));
    assert!(has_pause(&due, MonitorIssueCode::ResetDueUnverified));
    assert_eq!(
        due.five_hour.used_evidence.as_ref().unwrap().freshness,
        MonitorEvidenceFreshness::Current
    );
    assert_eq!(
        due.seven_day.used_evidence.as_ref().unwrap().freshness,
        MonitorEvidenceFreshness::Stale
    );

    let recovery_five_hour_reset = seven_day_deadline + 3_600;
    let recovery_seven_day_reset = seven_day_reset + 7 * 24 * 60 * 60;
    record_spend(
        &store,
        spend_input(
            "acct-seven-day-reset",
            NOW - 100,
            NOW + 30 * 24 * 60 * 60,
            0,
            seven_day_deadline + 1,
            "SGD",
        ),
        seven_day_deadline + 1,
    );
    ingest(
        &store,
        "acct-seven-day-reset",
        observation(
            "session-seven-day",
            None,
            (Some(1_000), Some(recovery_five_hour_reset)),
            (Some(1_000), Some(recovery_seven_day_reset)),
        ),
        seven_day_deadline + 1,
    );
    let recovered = status(&store, &started.monitor_id, seven_day_deadline + 1);
    assert!(recovered.runnable);
    assert!(!has_issue(&recovered, MonitorIssueCode::ResetDueUnverified));
    assert!(
        !actions(&recovered)
            .iter()
            .any(|action| matches!(action, MonitorAction::Pause { .. }))
    );
}

#[test]
fn lower_usage_from_the_old_reset_cannot_pair_with_a_later_reset_only_observation() {
    let (_directory, store) = open_store();
    let reset = NOW + 10;
    ingest(
        &store,
        "acct-reset-pair",
        observation(
            "session-reset",
            None,
            (None, Some(reset)),
            (None, Some(reset)),
        ),
        NOW,
    );
    let started = start(
        &store,
        config("acct-reset-pair", "goal-reset-pair", None, None),
        NOW,
    );
    let deadline = reset + 60;
    store.tick(deadline).expect("tick at reset deadline");
    assert!(has_issue(
        &status(&store, &started.monitor_id, deadline),
        MonitorIssueCode::ResetDueUnverified
    ));

    // With no prior usage, this lower percentage is accepted against the old reset.
    ingest(
        &store,
        "acct-reset-pair",
        observation(
            "session-reset",
            None,
            (Some(1_000), None),
            (Some(1_000), None),
        ),
        deadline + 1,
    );
    // The reset-only event must not retroactively associate that usage with the new window.
    ingest(
        &store,
        "acct-reset-pair",
        observation(
            "session-reset",
            None,
            (None, Some(reset + 3_600)),
            (None, Some(reset + 3_600)),
        ),
        deadline + 2,
    );
    let current = status(&store, &started.monitor_id, deadline + 2);
    assert!(has_issue(&current, MonitorIssueCode::ResetDueUnverified));
    assert!(!current.runnable);
}

#[test]
fn model_and_spend_evidence_expire_after_the_shared_ttl_boundary() {
    let (_directory, store) = open_store();
    let reset = NOW + 3_600;
    ingest(
        &store,
        "acct-model-spend-ttl",
        observation(
            "session-model-spend-ttl",
            Some("claude-sonnet"),
            (Some(1_000), Some(reset)),
            (Some(1_000), Some(reset)),
        ),
        NOW,
    );
    seed_zero_sgd_spend(&store, "acct-model-spend-ttl", NOW);
    let started = start(
        &store,
        config(
            "acct-model-spend-ttl",
            "goal-model-spend-ttl",
            Some("claude-sonnet"),
            Some(Money::new(5_000, "SGD", 2)),
        ),
        NOW,
    );
    assert!(started.runnable);
    assert_eq!(started.model.as_deref(), Some("claude-sonnet"));

    store
        .tick(NOW + 300)
        .expect("tick at inclusive TTL boundary");
    let at_boundary = status(&store, &started.monitor_id, NOW + 300);
    assert!(at_boundary.runnable);
    assert_eq!(at_boundary.model.as_deref(), Some("claude-sonnet"));
    assert_eq!(
        at_boundary.model_evidence.as_ref().unwrap().freshness,
        MonitorEvidenceFreshness::Current
    );
    assert!(!has_issue(&at_boundary, MonitorIssueCode::SpendStale));
    assert_eq!(
        at_boundary.cumulative_goal_spend,
        Some(Money::new(0, "SGD", 2))
    );

    store.tick(NOW + 301).expect("tick one second beyond TTL");
    let expired = status(&store, &started.monitor_id, NOW + 301);
    assert!(!expired.runnable);
    assert_eq!(expired.model, None);
    assert!(expired.model_evidence.is_none());
    assert!(has_issue(&expired, MonitorIssueCode::ModelUnknown));
    assert!(has_issue(&expired, MonitorIssueCode::SpendStale));
    assert!(has_issue(&expired, MonitorIssueCode::BudgetUnverifiable));
    assert_eq!(
        expired.cumulative_goal_spend,
        Some(Money::new(0, "SGD", 2)),
        "expiry makes the current comparison unknown but does not erase known cumulative spend"
    );
    let stale_model = expired
        .evidence
        .iter()
        .find(|evidence| {
            matches!(
                &evidence.value,
                jackin_protocol::usage_monitor::MonitorEvidenceValue::Model { .. }
            )
        })
        .expect("model observation remains available with its age");
    assert_eq!(stale_model.age_seconds, 301);
}

#[test]
fn a_sticky_quota_pause_survives_more_than_ten_idle_minutes_and_store_reopen() {
    let (directory, store) = open_store();
    let reset = NOW + 3_600;
    ingest(
        &store,
        "acct-idle-barrier",
        observation(
            "session-idle-barrier",
            None,
            (Some(9_600), Some(reset)),
            (Some(9_600), Some(reset)),
        ),
        NOW,
    );
    seed_zero_sgd_spend(&store, "acct-idle-barrier", NOW);
    let started = start(
        &store,
        config(
            "acct-idle-barrier",
            "goal-idle-barrier",
            None,
            Some(Money::new(5_000, "SGD", 2)),
        ),
        NOW,
    );
    assert!(has_pause(&started, MonitorIssueCode::LimitGuardReached));

    let idle_time = NOW + 601;
    store
        .tick(idle_time)
        .expect("advance beyond ten minutes idle");
    let after_idle = status(&store, &started.monitor_id, idle_time);
    assert!(!after_idle.runnable);
    assert!(has_issue(&after_idle, MonitorIssueCode::QuotaStale));
    assert!(has_pause(&after_idle, MonitorIssueCode::LimitGuardReached));
    drop(store);

    let reopened = MonitorStore::open(directory.path()).expect("reopen idle monitor store");
    let after_reopen = status(&reopened, &started.monitor_id, idle_time);
    assert!(!after_reopen.runnable);
    assert!(has_issue(&after_reopen, MonitorIssueCode::QuotaStale));
    assert!(has_pause(
        &after_reopen,
        MonitorIssueCode::LimitGuardReached
    ));

    // A lower reading from the same old window after the idle interval does
    // not satisfy the durable barrier or turn the goal runnable.
    seed_zero_sgd_spend(&reopened, "acct-idle-barrier", idle_time + 1);
    ingest(
        &reopened,
        "acct-idle-barrier",
        observation(
            "session-idle-barrier",
            None,
            (Some(1_000), Some(reset)),
            (Some(1_000), Some(reset)),
        ),
        idle_time + 1,
    );
    let old_window = status(&reopened, &started.monitor_id, idle_time + 1);
    assert!(!old_window.runnable);
    assert!(has_pause(&old_window, MonitorIssueCode::LimitGuardReached));
}

#[test]
fn reopening_after_clock_rollback_cannot_revive_stale_statusline_evidence() {
    let (directory, store) = open_store();
    let reset = NOW + 3_600;
    ingest(
        &store,
        "acct-clock",
        observation(
            "session-clock",
            None,
            (Some(1_000), Some(reset)),
            (Some(1_000), Some(reset)),
        ),
        NOW,
    );
    let started = start(&store, config("acct-clock", "goal-clock", None, None), NOW);
    store.tick(NOW + 301).expect("expire evidence");
    let stale = status(&store, &started.monitor_id, NOW + 301);
    assert!(!stale.runnable);
    assert_eq!(
        stale.five_hour.used_evidence.as_ref().unwrap().freshness,
        MonitorEvidenceFreshness::Stale
    );
    drop(store);

    let reopened = MonitorStore::open(directory.path()).expect("reopen persisted monitor store");
    let after_rollback = status(&reopened, &started.monitor_id, NOW + 200);
    assert!(!after_rollback.runnable);
    assert_eq!(
        after_rollback
            .five_hour
            .used_evidence
            .as_ref()
            .unwrap()
            .freshness,
        MonitorEvidenceFreshness::Stale
    );
    assert!(has_issue(&after_rollback, MonitorIssueCode::QuotaStale));
}

#[test]
fn same_goal_recreation_keeps_cumulative_spend_and_rejects_identity_or_budget_relaxation() {
    let (directory, store) = open_store();
    let period_start = NOW - 100;
    let period_end = NOW + 10_000;
    let reset = NOW + 3_600;
    ingest(
        &store,
        "acct-goal",
        observation(
            "session-goal",
            None,
            (Some(1_000), Some(reset)),
            (Some(1_000), Some(reset)),
        ),
        NOW,
    );
    record_spend(
        &store,
        spend_input("acct-goal", period_start, period_end, 1_000, NOW, "SGD"),
        NOW,
    );
    let original_config = config(
        "acct-goal",
        "durable-goal",
        None,
        Some(Money::new(5_000, "SGD", 2)),
    );
    let original = start(&store, original_config.clone(), NOW);
    record_spend(
        &store,
        spend_input("acct-goal", period_start, period_end, 2_200, NOW + 1, "SGD"),
        NOW + 1,
    );
    let updated = status(&store, &original.monitor_id, NOW + 1);
    assert_eq!(
        updated.cumulative_goal_spend,
        Some(Money::new(1_200, "SGD", 2))
    );

    store
        .operate(
            MonitorOperation::Stop {
                monitor_id: original.monitor_id.clone(),
            },
            NOW + 2,
        )
        .expect("stop original monitor");
    drop(store);
    let reopened = MonitorStore::open(directory.path()).expect("reopen persisted goal state");
    let recreated = start(&reopened, original_config, NOW + 3);
    assert_ne!(recreated.monitor_id, original.monitor_id);
    assert_eq!(
        recreated.cumulative_goal_spend,
        Some(Money::new(1_200, "SGD", 2))
    );

    let wrong_account = config(
        "acct-other",
        "durable-goal",
        None,
        Some(Money::new(5_000, "SGD", 2)),
    );
    let error = reopened
        .operate(
            MonitorOperation::Start {
                config: wrong_account,
            },
            NOW + 4,
        )
        .expect_err("same goal cannot change its canonical account");
    assert_eq!(error.code, MonitorIssueCode::AccountMismatch);

    let looser_budget = config(
        "acct-goal",
        "durable-goal",
        None,
        Some(Money::new(5_001, "SGD", 2)),
    );
    let error = reopened
        .operate(
            MonitorOperation::Start {
                config: looser_budget,
            },
            NOW + 4,
        )
        .expect_err("same goal cannot loosen its budget");
    assert_eq!(error.code, MonitorIssueCode::StatuslineInvalid);

    let preserved = status(&reopened, &recreated.monitor_id, NOW + 4);
    assert_eq!(
        preserved.cumulative_goal_spend,
        Some(Money::new(1_200, "SGD", 2))
    );
}

#[test]
fn same_goal_without_a_start_baseline_never_adopts_a_later_receipt_as_zero_spend() {
    let (directory, store) = open_store();
    let reset = NOW + 3_600;
    ingest(
        &store,
        "acct-no-baseline",
        observation(
            "session-no-baseline",
            None,
            (Some(1_000), Some(reset)),
            (Some(1_000), Some(reset)),
        ),
        NOW,
    );
    let goal_config = config(
        "acct-no-baseline",
        "goal-no-baseline",
        None,
        Some(Money::new(5_000, "SGD", 2)),
    );
    let first = start(&store, goal_config.clone(), NOW);
    assert_eq!(first.cumulative_goal_spend, None);
    assert!(has_issue(&first, MonitorIssueCode::BudgetUnverifiable));
    assert!(has_pause(&first, MonitorIssueCode::BudgetUnverifiable));

    // This is a fresh account total, but it arrived after the goal was
    // created without a baseline. Its entire amount cannot be attributed to
    // the goal as a zero-cost starting point.
    record_spend(
        &store,
        spend_input(
            "acct-no-baseline",
            NOW - 100,
            NOW + 10_000,
            1_000,
            NOW + 1,
            "SGD",
        ),
        NOW + 1,
    );
    let after_receipt = status(&store, &first.monitor_id, NOW + 1);
    assert_eq!(after_receipt.cumulative_goal_spend, None);
    assert!(has_issue(
        &after_receipt,
        MonitorIssueCode::BudgetUnverifiable
    ));
    assert!(has_pause(
        &after_receipt,
        MonitorIssueCode::BudgetUnverifiable
    ));

    store
        .operate(
            MonitorOperation::Stop {
                monitor_id: first.monitor_id.clone(),
            },
            NOW + 2,
        )
        .expect("stop goal without a spend baseline");
    drop(store);

    let reopened = MonitorStore::open(directory.path()).expect("reopen goal without baseline");
    let recreated = start(&reopened, goal_config, NOW + 3);
    assert_ne!(recreated.monitor_id, first.monitor_id);
    assert_eq!(recreated.cumulative_goal_spend, None);
    assert!(has_issue(&recreated, MonitorIssueCode::BudgetUnverifiable));
    assert!(has_pause(&recreated, MonitorIssueCode::BudgetUnverifiable));
}

#[test]
fn sgd_spend_thresholds_and_threshold_jumps_emit_all_required_actions() {
    let (_directory, store) = open_store();
    let period_start = NOW - 100;
    let period_end = NOW + 10_000;
    let reset = NOW + 3_600;
    ingest(
        &store,
        "acct-spend-threshold",
        observation(
            "session-spend",
            None,
            (Some(1_000), Some(reset)),
            (Some(1_000), Some(reset)),
        ),
        NOW,
    );
    record_spend(
        &store,
        spend_input(
            "acct-spend-threshold",
            period_start,
            period_end,
            0,
            NOW,
            "SGD",
        ),
        NOW,
    );
    let started = start(
        &store,
        config(
            "acct-spend-threshold",
            "goal-spend-threshold",
            None,
            Some(Money::new(5_000, "SGD", 2)),
        ),
        NOW,
    );

    for (offset, amount, warn, checkpoint, stop, pause) in [
        (1, 3_999, false, false, false, false),
        (2, 4_000, true, false, false, false),
        (3, 4_500, true, true, true, false),
        (4, 4_800, true, true, true, true),
        (5, 5_000, true, true, true, true),
    ] {
        record_spend(
            &store,
            spend_input(
                "acct-spend-threshold",
                period_start,
                period_end,
                amount,
                NOW + offset,
                "SGD",
            ),
            NOW + offset,
        );
        let current = status(&store, &started.monitor_id, NOW + offset);
        assert_eq!(
            actions(&current).iter().any(|action| matches!(
                action,
                MonitorAction::Warn {
                    reason: MonitorIssueCode::BudgetWarn
                }
            )),
            warn,
            "spend {amount} minor units"
        );
        assert_eq!(has_checkpoint(&current), checkpoint, "spend {amount}");
        assert_eq!(
            actions(&current).iter().any(|action| matches!(
                action,
                MonitorAction::ReduceDispatch {
                    max_parallel: Some(0)
                }
            )),
            stop,
            "spend {amount}"
        );
        assert_eq!(
            has_pause(&current, MonitorIssueCode::BudgetPause),
            pause,
            "spend {amount}"
        );
    }

    ingest(
        &store,
        "acct-spend-jump",
        observation(
            "session-jump",
            None,
            (Some(1_000), Some(reset)),
            (Some(1_000), Some(reset)),
        ),
        NOW,
    );
    record_spend(
        &store,
        spend_input("acct-spend-jump", period_start, period_end, 0, NOW, "SGD"),
        NOW,
    );
    let jump_monitor = start(
        &store,
        config(
            "acct-spend-jump",
            "goal-spend-jump",
            None,
            Some(Money::new(5_000, "SGD", 2)),
        ),
        NOW,
    );
    record_spend(
        &store,
        spend_input(
            "acct-spend-jump",
            period_start,
            period_end,
            4_900,
            NOW + 1,
            "SGD",
        ),
        NOW + 1,
    );
    let jumped = status(&store, &jump_monitor.monitor_id, NOW + 1);
    assert!(actions(&jumped).iter().any(|action| matches!(
        action,
        MonitorAction::Warn {
            reason: MonitorIssueCode::BudgetWarn
        }
    )));
    assert!(has_checkpoint(&jumped));
    assert!(actions(&jumped).iter().any(|action| matches!(
        action,
        MonitorAction::ReduceDispatch {
            max_parallel: Some(0)
        }
    )));
    assert!(has_pause(&jumped, MonitorIssueCode::BudgetPause));
}

#[test]
fn non_sgd_spend_receipts_stay_unverifiable_for_sgd_budget() {
    let period_start = NOW - 100;
    let period_end = NOW + 10_000;
    for currency in ["USD", "XYZ"] {
        let (_directory, store) = open_store();
        let account_id = format!("acct-currency-{currency}");
        let goal_id = format!("goal-currency-{currency}");
        ingest(
            &store,
            &account_id,
            observation(
                &format!("session-currency-{currency}"),
                None,
                (Some(1_000), Some(NOW + 3_600)),
                (Some(1_000), Some(NOW + 3_600)),
            ),
            NOW,
        );
        let receipt = record_spend(
            &store,
            spend_input(&account_id, period_start, period_end, 1_000, NOW, currency),
            NOW,
        );
        assert_eq!(receipt.verification, SpendVerification::Unverified);

        let started = start(
            &store,
            config(
                &account_id,
                &goal_id,
                None,
                Some(Money::new(5_000, "SGD", 2)),
            ),
            NOW,
        );
        assert_eq!(started.cumulative_goal_spend, None);
        assert!(has_issue(&started, MonitorIssueCode::BudgetUnverifiable));
        assert!(has_pause(&started, MonitorIssueCode::BudgetUnverifiable));
    }
}

#[test]
fn rollover_waits_for_a_fresh_verified_closing_total_and_contiguous_opening_total() {
    let (_directory, store) = open_store();
    let period_start = NOW - 100;
    let period_end = NOW + 10;
    let next_end = period_end + 10_000;
    let reset = NOW + 3_600;
    ingest(
        &store,
        "acct-rollover",
        observation(
            "session-rollover",
            None,
            (Some(1_000), Some(reset)),
            (Some(1_000), Some(reset)),
        ),
        NOW,
    );
    record_spend(
        &store,
        spend_input("acct-rollover", period_start, period_end, 1_000, NOW, "SGD"),
        NOW,
    );
    let started = start(
        &store,
        config(
            "acct-rollover",
            "goal-rollover",
            None,
            Some(Money::new(50_000, "SGD", 2)),
        ),
        NOW,
    );

    record_spend(
        &store,
        spend_input(
            "acct-rollover",
            period_start,
            period_end,
            1_500,
            period_end - 1,
            "SGD",
        ),
        period_end - 1,
    );
    let before_close = status(&store, &started.monitor_id, period_end - 1);
    assert_eq!(
        before_close.cumulative_goal_spend,
        Some(Money::new(500, "SGD", 2))
    );

    record_spend(
        &store,
        spend_input(
            "acct-rollover",
            period_end,
            next_end,
            200,
            period_end + 1,
            "SGD",
        ),
        period_end + 1,
    );
    let missing_close = status(&store, &started.monitor_id, period_end + 1);
    assert_eq!(
        missing_close.cumulative_goal_spend,
        Some(Money::new(500, "SGD", 2))
    );
    assert!(has_issue(
        &missing_close,
        MonitorIssueCode::SpendRolloverUnverified
    ));
    assert!(has_issue(
        &missing_close,
        MonitorIssueCode::BudgetUnverifiable
    ));
    assert!(has_pause(
        &missing_close,
        MonitorIssueCode::BudgetUnverifiable
    ));

    let closing = record_spend(
        &store,
        spend_input(
            "acct-rollover",
            period_start,
            period_end,
            1_800,
            period_end + 1,
            "SGD",
        ),
        period_end + 2,
    );
    assert_eq!(closing.verification, SpendVerification::Verified);
    let verified_rollover = status(&store, &started.monitor_id, period_end + 2);
    assert_eq!(
        verified_rollover.cumulative_goal_spend,
        Some(Money::new(1_000, "SGD", 2))
    );
    assert!(!has_issue(
        &verified_rollover,
        MonitorIssueCode::SpendRolloverUnverified
    ));
}
