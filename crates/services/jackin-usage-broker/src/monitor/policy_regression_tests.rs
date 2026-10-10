// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use jackin_protocol::control::Money;
use jackin_protocol::usage_monitor::{
    MonitorAccountBinding, MonitorAccountBindingInput, MonitorAction, MonitorConfig,
    MonitorIssueCode, MonitorOperation, MonitorPolicy, MonitorPolicyApprovalInput,
    MonitorPolicyRecord, MonitorProvider, MonitorPurpose, MonitorReply, MonitorScope,
    MonitorStatus, SpendRecordInput, SpendRecordSource, StatuslineObservation,
    StatuslineQuotaWindow, StatuslineRateLimits, USAGE_MONITOR_SCHEMA_VERSION,
};

use crate::projection::empty_projection;

use super::MonitorStore;

const NOW: i64 = 1_800_000_000;
const ACCOUNT: &str = "acct-policy-regressions";
const BILLING_PERIOD_END: i64 = NOW + 100_000;
const TEST_OPERATOR: &str = "isolated-test-operator";

fn quota_window(used: Option<i32>, reset: Option<i64>) -> Option<StatuslineQuotaWindow> {
    Some(StatuslineQuotaWindow {
        used_percentage_basis_points: used,
        reset_at_epoch: reset,
    })
}

fn observation(
    session_id: &str,
    used: Option<i32>,
    reset: Option<i64>,
    model: Option<&str>,
) -> StatuslineObservation {
    StatuslineObservation {
        schema_version: USAGE_MONITOR_SCHEMA_VERSION,
        session_id: session_id.to_owned(),
        model: model.map(str::to_owned),
        claude_code_version: Some("2.1.80".to_owned()),
        rate_limits: StatuslineRateLimits {
            five_hour: quota_window(used, reset),
            seven_day: quota_window(used, reset),
        },
    }
}

fn observation_with_windows(
    session_id: &str,
    five_hour_used: Option<i32>,
    five_hour_reset: Option<i64>,
    seven_day_used: Option<i32>,
    seven_day_reset: Option<i64>,
) -> StatuslineObservation {
    StatuslineObservation {
        schema_version: USAGE_MONITOR_SCHEMA_VERSION,
        session_id: session_id.to_owned(),
        model: None,
        claude_code_version: Some("2.1.80".to_owned()),
        rate_limits: StatuslineRateLimits {
            five_hour: quota_window(five_hour_used, five_hour_reset),
            seven_day: quota_window(seven_day_used, seven_day_reset),
        },
    }
}

fn spend_input(account_id: &str, evidence_at_epoch: i64) -> SpendRecordInput {
    SpendRecordInput {
        account_id: account_id.to_owned(),
        billing_period_start_epoch: NOW - 100,
        billing_period_end_epoch: BILLING_PERIOD_END,
        amount: Money::new(0, "SGD", 2),
        evidence_at_epoch: Some(evidence_at_epoch),
        verified: true,
        source: SpendRecordSource::OperatorReceipt,
    }
}

fn ingest(
    store: &MonitorStore,
    account_id: &str,
    observation: StatuslineObservation,
    now_epoch: i64,
) -> u64 {
    let binding = bind_account(store, account_id, now_epoch);
    let reply = store
        .operate(
            MonitorOperation::Ingest {
                scope: MonitorScope::BoundAccount {
                    binding_id: binding.binding_id,
                    binding_revision: binding.revision,
                    session_id: None,
                },
                observation,
            },
            now_epoch,
        )
        .expect("ingest statusline evidence");
    let MonitorReply::Ingested {
        evidence_sequence, ..
    } = reply
    else {
        panic!("expected statusline ingest result");
    };
    evidence_sequence
}

fn bind_account(store: &MonitorStore, account_id: &str, now_epoch: i64) -> MonitorAccountBinding {
    match store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: account_id.to_owned(),
                    operator_label: TEST_OPERATOR.to_owned(),
                    operator_confirmed: true,
                },
            },
            now_epoch,
        )
        .expect("confirm account binding in isolated fixture")
    {
        MonitorReply::AccountBound { binding } => binding,
        other => panic!("expected account-bound result, got {other:?}"),
    }
}

#[derive(Debug, Clone)]
struct PreparedStart {
    config: MonitorConfig,
    idempotency_key: String,
}

fn approve_strict_policy(
    store: &MonitorStore,
    binding: &MonitorAccountBinding,
    goal_id: &str,
    now_epoch: i64,
) -> MonitorPolicyRecord {
    match store
        .operate(
            MonitorOperation::ApprovePolicy {
                approval: MonitorPolicyApprovalInput {
                    binding_id: binding.binding_id.clone(),
                    binding_revision: binding.revision,
                    goal_id: goal_id.to_owned(),
                    new_policy: MonitorPolicy::StrictSgd,
                    budget: Some(Money::new(5_000, "SGD", 2)),
                    operator_label: TEST_OPERATOR.to_owned(),
                    operator_confirmed: true,
                    acknowledge_no_sgd_cap: false,
                    expected_revision: None,
                },
            },
            now_epoch,
        )
        .expect("approve strict SGD policy in isolated fixture")
    {
        MonitorReply::PolicyApproved { policy } => policy,
        other => panic!("expected policy-approved result, got {other:?}"),
    }
}

fn prepare_start(
    store: &MonitorStore,
    goal_id: &str,
    expected_model: Option<&str>,
    now_epoch: i64,
) -> PreparedStart {
    let binding = bind_account(store, ACCOUNT, now_epoch);
    let policy = approve_strict_policy(store, &binding, goal_id, now_epoch);
    PreparedStart {
        config: MonitorConfig {
            provider: MonitorProvider::Claude,
            purpose: MonitorPurpose::DispatchGuard,
            scope: MonitorScope::BoundAccount {
                binding_id: binding.binding_id,
                binding_revision: binding.revision,
                session_id: None,
            },
            goal_id: Some(goal_id.to_owned()),
            expected_model: expected_model.map(str::to_owned),
            policy_revision: Some(policy.revision),
        },
        idempotency_key: format!("fixture-start:{goal_id}:{now_epoch}"),
    }
}

fn start_prepared(
    store: &MonitorStore,
    prepared: &PreparedStart,
    now_epoch: i64,
) -> Result<MonitorStatus, jackin_protocol::usage_monitor::MonitorIssue> {
    match store.operate(
        MonitorOperation::Start {
            config: prepared.config.clone(),
            idempotency_key: prepared.idempotency_key.clone(),
        },
        now_epoch,
    )? {
        MonitorReply::Started { status } => Ok(*status),
        other => panic!("expected started monitor result, got {other:?}"),
    }
}

fn record_spend(store: &MonitorStore, record: SpendRecordInput, now_epoch: i64) {
    let reply = store
        .operate(MonitorOperation::RecordSpend { record }, now_epoch)
        .expect("record verified SGD spend");
    assert!(matches!(reply, MonitorReply::SpendRecorded { .. }));
}

fn start_monitor(store: &MonitorStore, goal_id: &str, now_epoch: i64) -> MonitorStatus {
    let prepared = prepare_start(store, goal_id, None, now_epoch);
    start_prepared(store, &prepared, now_epoch).expect("start monitor")
}

fn start_monitor_with_expected_model(
    store: &MonitorStore,
    goal_id: &str,
    expected_model: &str,
    now_epoch: i64,
) -> MonitorStatus {
    let prepared = prepare_start(store, goal_id, Some(expected_model), now_epoch);
    start_prepared(store, &prepared, now_epoch).expect("start model-guarded monitor")
}

fn status(store: &MonitorStore, monitor_id: &str, now_epoch: i64) -> MonitorStatus {
    let reply = store
        .operate(
            MonitorOperation::Status {
                monitor_id: monitor_id.to_owned(),
            },
            now_epoch,
        )
        .expect("read monitor status");
    let MonitorReply::Status { status } = reply else {
        panic!("expected monitor status result");
    };
    *status
}

fn fresh_watch_cursor(store: &MonitorStore, monitor_id: &str) -> u64 {
    let reply = store
        .operate(
            MonitorOperation::Watch {
                monitor_id: monitor_id.to_owned(),
                after_sequence: 0,
                timeout_ms: 0,
            },
            NOW,
        )
        .expect("attach to the current monitor event");
    let MonitorReply::Watch {
        events,
        next_sequence,
        timed_out,
    } = reply
    else {
        panic!("expected initial watch result");
    };
    assert!(!timed_out);
    assert_eq!(events.len(), 1);
    assert!(events[0].status.runnable);
    next_sequence
}

fn wait_for_cursor(
    store: &MonitorStore,
    monitor_id: &str,
    after_sequence: u64,
    ready: mpsc::Sender<()>,
) -> MonitorReply {
    store.tick(NOW).expect("reconcile before waiting on cursor");
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut state = store.lock();
    let (events, next_sequence) = super::watch_snapshot(&state, monitor_id, after_sequence)
        .expect("read initial cursor snapshot");
    if !events.is_empty() {
        return MonitorReply::Watch {
            events,
            next_sequence,
            timed_out: false,
        };
    }

    // Signal while holding the store mutex. The caller's operation cannot
    // acquire it until this cursor enters the same condition-variable wait
    // used by MonitorStore::watch.
    ready.send(()).expect("signal cursor is ready to wait");
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return MonitorReply::Watch {
                events: Vec::new(),
                next_sequence,
                timed_out: true,
            };
        }
        let (next_state, wait) = store
            .inner
            .changed
            .wait_timeout(state, remaining)
            .expect("wait on monitor event condition variable");
        state = next_state;
        let (events, next_sequence) = super::watch_snapshot(&state, monitor_id, after_sequence)
            .expect("read cursor snapshot after wake");
        if !events.is_empty() {
            return MonitorReply::Watch {
                events,
                next_sequence,
                timed_out: false,
            };
        }
        if wait.timed_out() {
            return MonitorReply::Watch {
                events,
                next_sequence,
                timed_out: true,
            };
        }
    }
}

fn spawn_waiting_cursor(
    store: &MonitorStore,
    monitor_id: String,
    after_sequence: u64,
) -> (mpsc::Receiver<MonitorReply>, thread::JoinHandle<()>) {
    let watch_store = store.clone();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (reply_tx, reply_rx) = mpsc::channel();
    let watcher = thread::spawn(move || {
        let reply = wait_for_cursor(&watch_store, &monitor_id, after_sequence, ready_tx);
        reply_tx.send(reply).expect("return cursor result");
    });
    ready_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("cursor entered the condition-variable wait");
    (reply_rx, watcher)
}

fn assert_expiry_is_reconciled_and_notified(trigger: impl FnOnce(&MonitorStore, i64, u64)) {
    let directory = tempfile::tempdir().expect("temporary monitor directory");
    let store = MonitorStore::open(directory.path()).expect("open monitor store");
    let original = observation("session-expiry", Some(1_000), Some(NOW + 3_600), None);
    let original_sequence = ingest(&store, ACCOUNT, original.clone(), NOW);
    let original_spend = spend_input(ACCOUNT, NOW);
    record_spend(&store, original_spend, NOW);
    let started = start_monitor(&store, "goal-expiry", NOW);
    assert!(started.runnable, "fixture must begin runnable");
    let cursor = fresh_watch_cursor(&store, &started.monitor_id);

    let (reply_rx, watcher) = spawn_waiting_cursor(&store, started.monitor_id.clone(), cursor);

    trigger(&store, NOW + 301, original_sequence);

    let reply = reply_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("clock-advancing no-op must wake the waiting cursor");
    watcher.join().expect("join watch thread");
    let MonitorReply::Watch {
        events, timed_out, ..
    } = reply
    else {
        panic!("expected watch response");
    };
    assert!(!timed_out, "the stale transition should publish an event");
    let event = events.last().expect("watch receives the new event");
    assert_eq!(event.occurred_at_epoch, NOW + 301);
    assert!(!event.status.runnable);
    assert!(event.status.issues.iter().any(|issue| {
        matches!(
            issue.code,
            MonitorIssueCode::QuotaStale | MonitorIssueCode::SpendStale
        )
    }));
    assert_eq!(
        event.status.five_hour.used_percentage_basis_points,
        Some(1_000)
    );
    let used = event
        .status
        .five_hour
        .used_evidence
        .as_ref()
        .expect("used evidence remains visible after expiry");
    assert_eq!(used.evidence_sequence, original_sequence);
    assert_eq!(used.evidence_received_at_epoch, NOW);
    assert_eq!(used.age_seconds, 301);
    assert_eq!(
        used.freshness,
        jackin_protocol::usage_monitor::MonitorEvidenceFreshness::Stale
    );
}

#[test]
fn identical_statusline_after_ttl_reconciles_and_notifies_without_refreshing_age() {
    let original = observation("session-expiry", Some(1_000), Some(NOW + 3_600), None);
    assert_expiry_is_reconciled_and_notified(move |store, now_epoch, original_sequence| {
        let evidence_sequence = ingest(store, ACCOUNT, original, now_epoch);
        assert_eq!(
            evidence_sequence, original_sequence,
            "identical input must not refresh evidence"
        );
    });
}

#[test]
fn duplicate_spend_after_ttl_reconciles_and_notifies_without_refreshing_receipt() {
    let duplicate = spend_input(ACCOUNT, NOW);
    assert_expiry_is_reconciled_and_notified(move |store, now_epoch, _original_sequence| {
        let reply = store
            .operate(
                MonitorOperation::RecordSpend { record: duplicate },
                now_epoch,
            )
            .expect("repeat identical spend input");
        let MonitorReply::SpendRecorded { record } = reply else {
            panic!("expected spend record result");
        };
        assert_eq!(record.evidence_received_at_epoch, NOW);
    });
}

#[test]
fn unchanged_projection_clock_advance_reconciles_and_notifies() {
    let directory = tempfile::tempdir().expect("temporary monitor directory");
    let store = MonitorStore::open(directory.path()).expect("open monitor store");
    ingest(
        &store,
        ACCOUNT,
        observation(
            "session-projection-expiry",
            Some(1_000),
            Some(NOW + 3_600),
            None,
        ),
        NOW,
    );
    record_spend(&store, spend_input(ACCOUNT, NOW), NOW);
    let projection = empty_projection("policy-regression");
    store
        .observe_projection(&projection, NOW)
        .expect("record initial empty projection");
    let started = start_monitor(&store, "goal-projection-expiry", NOW);
    assert!(started.runnable);
    let cursor = fresh_watch_cursor(&store, &started.monitor_id);

    let (reply_rx, watcher) = spawn_waiting_cursor(&store, started.monitor_id.clone(), cursor);
    store
        .observe_projection(&projection, NOW + 301)
        .expect("repeat unchanged projection at advanced clock");

    let reply = reply_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("unchanged projection clock advance must wake the cursor");
    watcher.join().expect("join watch thread");
    let MonitorReply::Watch {
        events, timed_out, ..
    } = reply
    else {
        panic!("expected watch response");
    };
    assert!(!timed_out);
    let event = events.last().expect("watch receives stale transition");
    assert_eq!(event.occurred_at_epoch, NOW + 301);
    assert!(!event.status.runnable);
    let used = event
        .status
        .five_hour
        .used_evidence
        .as_ref()
        .expect("used evidence remains visible after expiry");
    assert_eq!(used.evidence_received_at_epoch, NOW);
    assert_eq!(used.age_seconds, 301);
}

#[test]
fn account_95_barrier_survives_stop_reopen_new_goals_and_requires_a_fresh_paired_reset() {
    let directory = tempfile::tempdir().expect("temporary monitor directory");
    let old_reset = NOW + 3_600;
    let triggered = observation("session-trigger", Some(9_500), Some(old_reset), None);
    let store = MonitorStore::open(directory.path()).expect("open monitor store");

    // The account threshold arrives before any monitor exists and must still
    // latch an account-wide barrier.
    ingest(&store, ACCOUNT, triggered, NOW);
    record_spend(&store, spend_input(ACCOUNT, NOW), NOW);
    let first_setup = prepare_start(&store, "goal-recreated", None, NOW);
    let first = start_prepared(&store, &first_setup, NOW).expect("start first monitor");
    assert!(!first.runnable);
    assert!(
        first
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::LimitGuardReached)
    );
    let stopped = store
        .operate(
            MonitorOperation::Stop {
                monitor_id: first.monitor_id,
            },
            NOW,
        )
        .expect("stop the first monitor");
    assert!(matches!(stopped, MonitorReply::Stopped { .. }));
    drop(store);

    // A fresh low value from another session in the old reset window, after
    // the triggering statusline is stale, cannot erase the persisted barrier.
    let reopened = MonitorStore::open(directory.path()).expect("reopen monitor store");
    ingest(
        &reopened,
        ACCOUNT,
        observation("session-low-old-reset", Some(1_000), Some(old_reset), None),
        NOW + 302,
    );
    record_spend(&reopened, spend_input(ACCOUNT, NOW + 302), NOW + 302);
    let mut recreated_setup = first_setup.clone();
    recreated_setup.idempotency_key = "fixture-start:goal-recreated:second-run".to_owned();
    let recreated = start_prepared(&reopened, &recreated_setup, NOW + 302)
        .expect("start new run for the persisted goal and policy");
    let different_goal = start_monitor(&reopened, "goal-new", NOW + 302);
    for current in [&recreated, &different_goal] {
        assert!(
            !current.runnable,
            "monitor recreation cannot clear the account barrier"
        );
        assert!(
            current
                .issues
                .iter()
                .any(|issue| issue.code == MonitorIssueCode::LimitGuardReached)
        );
    }

    // Crossing reset+60 only changes the barrier to its unverified-reset
    // state. A reset-only update and a model-only update are not a quota pair.
    let deadline = old_reset + 60;
    reopened
        .tick(deadline)
        .expect("advance to reset grace deadline");
    for current in [&recreated, &different_goal] {
        let paused = status(&reopened, &current.monitor_id, deadline);
        assert!(!paused.runnable);
        assert!(
            paused
                .issues
                .iter()
                .any(|issue| issue.code == MonitorIssueCode::ResetDueUnverified)
        );
    }

    let recovery_session = "session-recovery";
    let advanced_reset = old_reset + 3_600;
    ingest(
        &reopened,
        ACCOUNT,
        observation(recovery_session, None, Some(advanced_reset), None),
        deadline + 1,
    );
    let paused_after_reset_only = status(&reopened, &recreated.monitor_id, deadline + 1);
    assert!(!paused_after_reset_only.runnable);
    assert!(
        paused_after_reset_only
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::ResetDueUnverified)
    );

    ingest(
        &reopened,
        ACCOUNT,
        observation(recovery_session, None, None, Some("claude-sonnet-4")),
        deadline + 2,
    );
    let paused_after_model_only = status(&reopened, &different_goal.monitor_id, deadline + 2);
    assert!(!paused_after_model_only.runnable);
    assert!(
        paused_after_model_only
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::ResetDueUnverified)
    );

    // A full paired observation from a different session clears the account
    // barrier. The lower utilization equals the old-window reading above.
    let recovery_at = deadline + 3;
    record_spend(&reopened, spend_input(ACCOUNT, recovery_at), recovery_at);
    for current in [&recreated, &different_goal] {
        let after_spend_only = status(&reopened, &current.monitor_id, recovery_at);
        assert!(!after_spend_only.runnable);
        assert!(
            after_spend_only
                .issues
                .iter()
                .any(|issue| issue.code == MonitorIssueCode::ResetDueUnverified)
        );
    }
    ingest(
        &reopened,
        ACCOUNT,
        observation(recovery_session, Some(1_000), Some(advanced_reset), None),
        recovery_at,
    );
    for current in [&recreated, &different_goal] {
        let resumed = status(&reopened, &current.monitor_id, recovery_at);
        assert!(
            resumed.runnable,
            "fresh paired reset should release the barrier"
        );
        assert!(
            !resumed
                .issues
                .iter()
                .any(|issue| issue.code == MonitorIssueCode::ResetDueUnverified)
        );
    }
}

#[test]
fn weekly_exhaustion_survives_a_confirmed_five_hour_reset() {
    let directory = tempfile::tempdir().expect("temporary monitor directory");
    let five_hour_reset = NOW + 10;
    let seven_day_reset = NOW + 7 * 24 * 60 * 60;
    let store = MonitorStore::open(directory.path()).expect("open monitor store");

    ingest(
        &store,
        ACCOUNT,
        observation_with_windows(
            "session-weekly-exhausted",
            Some(9_600),
            Some(five_hour_reset),
            Some(10_000),
            Some(seven_day_reset),
        ),
        NOW,
    );
    record_spend(&store, spend_input(ACCOUNT, NOW), NOW);
    let started = start_monitor(&store, "goal-weekly-exhausted", NOW);
    assert!(!started.runnable);
    assert_eq!(started.five_hour.used_percentage_basis_points, Some(9_600));
    assert_eq!(started.seven_day.used_percentage_basis_points, Some(10_000));
    let actions = &started.latest_decision.as_ref().unwrap().actions;
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, MonitorAction::Checkpoint { .. }))
    );
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, MonitorAction::ReduceDispatch { .. }))
    );
    assert!(actions.iter().any(|action| matches!(
        action,
        MonitorAction::Pause {
            reason: MonitorIssueCode::LimitExhausted
        }
    )));

    let five_hour_deadline = five_hour_reset + 60;
    store
        .tick(five_hour_deadline)
        .expect("tick at the five-hour reset grace deadline");
    let five_hour_due = status(&store, &started.monitor_id, five_hour_deadline);
    assert!(!five_hour_due.runnable);
    assert!(
        five_hour_due
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::ResetDueUnverified)
    );

    let recovery_at = five_hour_deadline + 1;
    record_spend(&store, spend_input(ACCOUNT, recovery_at), recovery_at);
    ingest(
        &store,
        ACCOUNT,
        observation_with_windows(
            "session-weekly-exhausted",
            Some(1_000),
            Some(five_hour_reset + 3_600),
            Some(10_000),
            Some(seven_day_reset),
        ),
        recovery_at,
    );
    let after_five_hour_reset = status(&store, &started.monitor_id, recovery_at);
    assert_eq!(
        after_five_hour_reset.five_hour.used_percentage_basis_points,
        Some(1_000)
    );
    assert_eq!(
        after_five_hour_reset.seven_day.used_percentage_basis_points,
        Some(10_000)
    );
    assert!(!after_five_hour_reset.runnable);
    assert!(
        after_five_hour_reset
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::LimitExhausted)
    );
    assert!(
        !after_five_hour_reset
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::ResetDueUnverified)
    );
}

#[test]
fn decision_sequence_survives_store_reopen_and_duplicate_input() {
    let directory = tempfile::tempdir().expect("temporary monitor directory");
    let reset = NOW + 3_600;
    let store = MonitorStore::open(directory.path()).expect("open monitor store");
    ingest(
        &store,
        ACCOUNT,
        observation("session-sequence", Some(8_900), Some(reset), None),
        NOW,
    );
    record_spend(&store, spend_input(ACCOUNT, NOW), NOW);
    let started = start_monitor(&store, "goal-sequence", NOW);
    let initial_sequence = started.latest_decision.as_ref().unwrap().sequence;
    drop(store);

    let reopened = MonitorStore::open(directory.path()).expect("reopen monitor store");
    let after_reopen = status(&reopened, &started.monitor_id, NOW);
    assert_eq!(
        after_reopen.latest_decision.as_ref().unwrap().sequence,
        initial_sequence,
        "unchanged persisted state must keep its decision sequence"
    );

    let threshold_sequence_input = ingest(
        &reopened,
        ACCOUNT,
        observation("session-sequence", Some(9_000), Some(reset), None),
        NOW + 1,
    );
    let threshold = status(&reopened, &started.monitor_id, NOW + 1);
    let threshold_sequence = threshold.latest_decision.as_ref().unwrap().sequence;
    assert!(threshold_sequence > initial_sequence);
    assert!(
        threshold
            .latest_decision
            .as_ref()
            .unwrap()
            .actions
            .iter()
            .any(|action| matches!(action, MonitorAction::Checkpoint { .. }))
    );
    drop(reopened);

    let reopened_again = MonitorStore::open(directory.path()).expect("reopen threshold state");
    let after_threshold_reopen = status(&reopened_again, &started.monitor_id, NOW + 1);
    assert_eq!(
        after_threshold_reopen
            .latest_decision
            .as_ref()
            .unwrap()
            .sequence,
        threshold_sequence,
        "the same monitor ID must retain its current decision across reopen"
    );
    let duplicate_sequence = ingest(
        &reopened_again,
        ACCOUNT,
        observation("session-sequence", Some(9_000), Some(reset), None),
        NOW + 2,
    );
    assert_eq!(duplicate_sequence, threshold_sequence_input);
    let after_duplicate = status(&reopened_again, &started.monitor_id, NOW + 2);
    assert_eq!(
        after_duplicate.latest_decision.as_ref().unwrap().sequence,
        threshold_sequence,
        "a duplicate callback after restart must not replay the action"
    );
}

#[test]
fn confirming_quota_reset_does_not_clear_expected_model_mismatch() {
    let directory = tempfile::tempdir().expect("temporary monitor directory");
    let reset = NOW + 10;
    let store = MonitorStore::open(directory.path()).expect("open monitor store");
    ingest(
        &store,
        ACCOUNT,
        observation(
            "session-model-reset",
            Some(9_600),
            Some(reset),
            Some("claude-opus-4"),
        ),
        NOW,
    );
    record_spend(&store, spend_input(ACCOUNT, NOW), NOW);
    let started =
        start_monitor_with_expected_model(&store, "goal-model-reset", "claude-sonnet-4", NOW);
    assert!(!started.runnable);
    assert!(
        started
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::LimitGuardReached)
    );
    assert!(
        started
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::ModelMismatch)
    );

    let deadline = reset + 60;
    store.tick(deadline).expect("tick at reset grace deadline");
    let due = status(&store, &started.monitor_id, deadline);
    assert!(
        due.issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::ResetDueUnverified)
    );

    let recovery_at = deadline + 1;
    ingest(
        &store,
        ACCOUNT,
        observation(
            "session-model-recovery",
            Some(1_000),
            Some(reset + 3_600),
            Some("claude-opus-4"),
        ),
        recovery_at,
    );
    let recovered_quota = status(&store, &started.monitor_id, recovery_at);
    assert_eq!(
        recovered_quota.five_hour.used_percentage_basis_points,
        Some(1_000)
    );
    assert!(
        !recovered_quota
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::ResetDueUnverified)
    );
    assert!(
        !recovered_quota
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::LimitGuardReached)
    );
    assert!(
        recovered_quota
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::ModelMismatch)
    );
    assert!(!recovered_quota.runnable);
}

#[test]
fn confirming_quota_reset_does_not_clear_spend_pause() {
    let directory = tempfile::tempdir().expect("temporary monitor directory");
    let reset = NOW + 10;
    let store = MonitorStore::open(directory.path()).expect("open monitor store");
    ingest(
        &store,
        ACCOUNT,
        observation("session-spend-reset", Some(9_600), Some(reset), None),
        NOW,
    );
    record_spend(&store, spend_input(ACCOUNT, NOW), NOW);
    let started = start_monitor(&store, "goal-spend-reset", NOW);
    record_spend(
        &store,
        SpendRecordInput {
            amount: Money::new(4_800, "SGD", 2),
            ..spend_input(ACCOUNT, NOW + 1)
        },
        NOW + 1,
    );
    let spend_paused = status(&store, &started.monitor_id, NOW + 1);
    assert!(
        spend_paused
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::BudgetPause)
    );
    assert!(
        spend_paused
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::LimitGuardReached)
    );

    let deadline = reset + 60;
    store.tick(deadline).expect("tick at reset grace deadline");
    let recovery_at = deadline + 1;
    record_spend(
        &store,
        SpendRecordInput {
            amount: Money::new(4_800, "SGD", 2),
            ..spend_input(ACCOUNT, recovery_at)
        },
        recovery_at,
    );
    ingest(
        &store,
        ACCOUNT,
        observation(
            "session-spend-recovery",
            Some(1_000),
            Some(reset + 3_600),
            None,
        ),
        recovery_at,
    );
    let after_quota_reset = status(&store, &started.monitor_id, recovery_at);
    assert!(
        !after_quota_reset
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::ResetDueUnverified)
    );
    assert!(
        !after_quota_reset
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::LimitGuardReached)
    );
    assert!(
        after_quota_reset
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::BudgetPause)
    );
    assert!(!after_quota_reset.runnable);
}

#[test]
fn confirming_quota_reset_does_not_clear_stale_spend_evidence() {
    let directory = tempfile::tempdir().expect("temporary monitor directory");
    let reset = NOW + 10;
    let store = MonitorStore::open(directory.path()).expect("open monitor store");
    ingest(
        &store,
        ACCOUNT,
        observation(
            "session-unknown-spend-reset",
            Some(9_600),
            Some(reset),
            None,
        ),
        NOW,
    );
    record_spend(&store, spend_input(ACCOUNT, NOW), NOW);
    let started = start_monitor(&store, "goal-unknown-spend-reset", NOW);
    assert!(
        !started
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::SpendStale)
    );
    assert!(
        !started
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::BudgetUnverifiable)
    );

    let deadline = reset + 60;
    store.tick(deadline).expect("tick at reset grace deadline");
    let recovery_at = NOW + 301;
    assert!(recovery_at > deadline);
    ingest(
        &store,
        ACCOUNT,
        observation(
            "session-unknown-spend-recovery",
            Some(1_000),
            Some(reset + 3_600),
            None,
        ),
        recovery_at,
    );
    let after_quota_reset = status(&store, &started.monitor_id, recovery_at);
    assert!(
        !after_quota_reset
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::ResetDueUnverified)
    );
    assert!(
        !after_quota_reset
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::LimitGuardReached)
    );
    assert!(
        after_quota_reset
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::SpendStale)
    );
    assert!(
        after_quota_reset
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::BudgetUnverifiable)
    );
    assert!(!after_quota_reset.runnable);
}
