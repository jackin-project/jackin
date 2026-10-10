// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::Money;
use jackin_protocol::usage_monitor::{
    MonitorAccountBinding, MonitorAccountBindingInput, MonitorAction, MonitorBudgetReadiness,
    MonitorConfig, MonitorDispatchReadiness, MonitorEvidenceFreshness, MonitorIssueCode,
    MonitorLifecycle, MonitorModelGuardValidity, MonitorOperation, MonitorPolicy,
    MonitorPolicyApprovalInput, MonitorPolicyOrigin, MonitorPolicyRecord, MonitorProvider,
    MonitorPurpose, MonitorQuotaReadiness, MonitorReply, MonitorResetValidity, MonitorScope,
    MonitorStatus, SpendRecordInput, SpendRecordSource, StatuslineObservation,
    StatuslineQuotaWindow, StatuslineRateLimits, USAGE_MONITOR_SCHEMA_VERSION,
};

const NOW: i64 = 1_800_000_000;
const TEST_OPERATOR: &str = "isolated-v2-test-operator";

fn open_store() -> (tempfile::TempDir, MonitorStore) {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let store = MonitorStore::open(directory.path()).expect("open monitor store");
    (directory, store)
}

fn assert_invalid_state(state: &StoreState) {
    drop(validate_store_state(state).expect_err("corrupted monitor store must be rejected"));
}

fn assert_invalid_v1(fixture: &[u8]) {
    drop(legacy::migrate_v1(fixture).expect_err("malformed V1 store must be rejected"));
}

fn install_store_state(store: &MonitorStore, state: StoreState) {
    validate_store_state(&state).expect("validate test state before installation");
    let mut current = store.lock();
    storage::save(&store.inner.directory, &state).expect("persist test state");
    *current = state;
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
                    provider_account_id: None,
                    experimental_collector_approved: false,
                },
            },
            now_epoch,
        )
        .expect("confirm isolated account binding")
    {
        MonitorReply::AccountBound { binding } => binding,
        other => panic!("expected account-bound reply, got {other:?}"),
    }
}

struct ApprovalOptions {
    acknowledge_no_sgd_cap: bool,
    expected_revision: Option<u64>,
    now_epoch: i64,
}

fn approve_policy(
    store: &MonitorStore,
    binding: &MonitorAccountBinding,
    goal_id: &str,
    new_policy: MonitorPolicy,
    budget: Option<Money>,
    options: ApprovalOptions,
) -> Result<MonitorPolicyRecord, MonitorIssue> {
    match store.operate(
        MonitorOperation::ApprovePolicy {
            approval: MonitorPolicyApprovalInput {
                binding_id: binding.binding_id.clone(),
                binding_revision: binding.revision,
                goal_id: goal_id.to_owned(),
                new_policy,
                budget,
                operator_label: TEST_OPERATOR.to_owned(),
                operator_confirmed: true,
                acknowledge_no_sgd_cap: options.acknowledge_no_sgd_cap,
                expected_revision: options.expected_revision,
            },
        },
        options.now_epoch,
    )? {
        MonitorReply::PolicyApproved { policy } => Ok(policy),
        other => panic!("expected policy-approved reply, got {other:?}"),
    }
}

fn observe_config(session_id: &str) -> MonitorConfig {
    MonitorConfig {
        provider: MonitorProvider::Claude,
        purpose: MonitorPurpose::ObserveOnly,
        scope: MonitorScope::Session {
            session_id: session_id.to_owned(),
        },
        goal_id: None,
        expected_model: None,
        policy_revision: None,
        experimental_collector: false,
    }
}

fn dispatch_config(
    binding: &MonitorAccountBinding,
    goal_id: &str,
    policy_revision: u64,
) -> MonitorConfig {
    MonitorConfig {
        provider: MonitorProvider::Claude,
        purpose: MonitorPurpose::DispatchGuard,
        scope: MonitorScope::BoundAccount {
            binding_id: binding.binding_id.clone(),
            binding_revision: binding.revision,
            session_id: None,
        },
        goal_id: Some(goal_id.to_owned()),
        expected_model: None,
        policy_revision: Some(policy_revision),
        experimental_collector: false,
    }
}

fn start_result(
    store: &MonitorStore,
    config: MonitorConfig,
    idempotency_key: &str,
    now_epoch: i64,
) -> Result<MonitorStatus, MonitorIssue> {
    match store.operate(
        MonitorOperation::Start {
            config,
            idempotency_key: idempotency_key.to_owned(),
        },
        now_epoch,
    )? {
        MonitorReply::Started { status } => Ok(*status),
        other => panic!("expected started reply, got {other:?}"),
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

fn quota(used: i32, reset_at_epoch: i64) -> Option<StatuslineQuotaWindow> {
    Some(StatuslineQuotaWindow {
        used_percentage_basis_points: Some(used),
        reset_at_epoch: Some(reset_at_epoch),
    })
}

fn observation(
    session_id: &str,
    model: Option<&str>,
    claude_code_version: Option<&str>,
    five_hour: Option<StatuslineQuotaWindow>,
    seven_day: Option<StatuslineQuotaWindow>,
) -> StatuslineObservation {
    StatuslineObservation {
        schema_version: USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
        session_id: session_id.to_owned(),
        model: model.map(str::to_owned),
        claude_code_version: claude_code_version.map(str::to_owned),
        rate_limits: StatuslineRateLimits {
            five_hour,
            seven_day,
        },
    }
}

fn ingest(
    store: &MonitorStore,
    scope: MonitorScope,
    observation: StatuslineObservation,
    now_epoch: i64,
) {
    assert!(matches!(
        store
            .operate(MonitorOperation::Ingest { scope, observation }, now_epoch,)
            .expect("ingest statusline observation"),
        MonitorReply::Ingested { .. }
    ));
}

fn record_zero_sgd_spend(store: &MonitorStore, account_id: &str, now_epoch: i64) {
    let reply = store
        .operate(
            MonitorOperation::RecordSpend {
                record: SpendRecordInput {
                    account_id: account_id.to_owned(),
                    billing_period_start_epoch: NOW - 100,
                    billing_period_end_epoch: NOW + 100_000,
                    amount: Money::new(0, "SGD", 2),
                    evidence_at_epoch: Some(now_epoch),
                    verified: true,
                    source: SpendRecordSource::OperatorReceipt,
                },
            },
            now_epoch,
        )
        .expect("record current zero SGD spend");
    assert!(matches!(reply, MonitorReply::SpendRecorded { .. }));
}

fn assert_observer_has_no_dispatch_actions(status: &MonitorStatus) {
    assert_eq!(
        status.readiness.dispatch,
        MonitorDispatchReadiness::NotAuthorized
    );
    assert!(!status.runnable);
    let decision = status
        .latest_decision
        .as_ref()
        .expect("observer has a reconciled decision");
    assert!(!decision.actions.iter().any(|action| {
        matches!(
            action,
            MonitorAction::Checkpoint { .. }
                | MonitorAction::ReduceDispatch { .. }
                | MonitorAction::Pause { .. }
        )
    }));
}

fn assert_version_only_callback_preserves_field_ages(
    store: &MonitorStore,
    session_id: &str,
    monitor_id: &str,
) {
    // A version-only callback is session metadata; it must not refresh any
    // independently received quota or model field.
    ingest(
        store,
        MonitorScope::Session {
            session_id: session_id.to_owned(),
        },
        observation(session_id, None, Some("2.1.81"), None, None),
        NOW + 120,
    );
    let version_only = status(store, monitor_id, NOW + 120);
    assert_eq!(version_only.claude_code_version.as_deref(), Some("2.1.81"));
    assert_eq!(
        version_only
            .five_hour
            .used_evidence
            .as_ref()
            .unwrap()
            .evidence_received_at_epoch,
        NOW
    );
    assert_eq!(
        version_only
            .five_hour
            .used_evidence
            .as_ref()
            .unwrap()
            .age_seconds,
        120
    );
    assert_eq!(
        version_only
            .five_hour
            .reset_evidence
            .as_ref()
            .unwrap()
            .evidence_received_at_epoch,
        NOW
    );
    assert_eq!(
        version_only
            .five_hour
            .reset_evidence
            .as_ref()
            .unwrap()
            .age_seconds,
        120
    );
    assert_eq!(
        version_only
            .seven_day
            .used_evidence
            .as_ref()
            .unwrap()
            .evidence_received_at_epoch,
        NOW
    );
    assert_eq!(
        version_only
            .seven_day
            .used_evidence
            .as_ref()
            .unwrap()
            .age_seconds,
        120
    );
    assert_eq!(
        version_only
            .seven_day
            .reset_evidence
            .as_ref()
            .unwrap()
            .evidence_received_at_epoch,
        NOW
    );
    assert_eq!(
        version_only
            .seven_day
            .reset_evidence
            .as_ref()
            .unwrap()
            .age_seconds,
        120
    );
    let model_evidence = version_only
        .model_evidence
        .as_ref()
        .expect("model evidence remains present");
    assert_eq!(model_evidence.evidence_received_at_epoch, NOW);
    assert_eq!(model_evidence.age_seconds, 120);
    assert_eq!(model_evidence.freshness, MonitorEvidenceFreshness::Current);
    assert_observer_has_no_dispatch_actions(&version_only);
}

#[test]
fn observe_only_uses_an_unbound_partition_and_survives_restart_and_evidence_wake() {
    let (directory, store) = open_store();
    let session_id = "session-v2-unbound-observer";
    let initial = start_result(
        &store,
        observe_config(session_id),
        "v2-observer-restart",
        NOW,
    )
    .expect("start unbound observation-only monitor");
    assert_eq!(initial.account_id, None);
    assert_eq!(initial.goal_id, None);
    assert_eq!(initial.policy, None);
    assert_eq!(initial.cumulative_goal_spend, None);
    assert_eq!(initial.spend_period_baseline, None);
    assert_observer_has_no_dispatch_actions(&initial);

    let reset = NOW + 10_000;
    ingest(
        &store,
        MonitorScope::Session {
            session_id: session_id.to_owned(),
        },
        observation(
            session_id,
            Some("claude-sonnet"),
            Some("2.1.80"),
            quota(1_500, reset),
            quota(2_500, reset),
        ),
        NOW,
    );
    let observed = status(&store, &initial.monitor_id, NOW);
    assert_eq!(observed.account_id, None);
    assert_eq!(observed.goal_id, None);
    assert_eq!(observed.policy, None);
    assert_eq!(observed.model.as_deref(), Some("claude-sonnet"));
    assert_eq!(observed.five_hour.used_percentage_basis_points, Some(1_500));
    assert_eq!(observed.seven_day.used_percentage_basis_points, Some(2_500));
    assert_observer_has_no_dispatch_actions(&observed);

    assert_version_only_callback_preserves_field_ages(&store, session_id, &initial.monitor_id);

    {
        let state = store.lock();
        assert!(
            state.accounts.is_empty(),
            "session-only evidence is not account-bound"
        );
        assert_eq!(state.unbound_sessions.len(), 1);
        assert!(state.goals.is_empty());
        assert!(state.policy_records.is_empty());
    }
    assert_eq!(store.next_wake(), Some(NOW + MONITOR_EVIDENCE_TTL_SECS + 1));
    drop(store);

    let reopened = MonitorStore::open(directory.path()).expect("reopen monitor store");
    assert_eq!(
        reopened.next_wake(),
        Some(NOW + MONITOR_EVIDENCE_TTL_SECS + 1)
    );
    let persisted = status(&reopened, &initial.monitor_id, NOW + 120);
    assert_eq!(persisted.claude_code_version.as_deref(), Some("2.1.81"));
    assert_eq!(
        persisted
            .five_hour
            .used_evidence
            .as_ref()
            .unwrap()
            .evidence_received_at_epoch,
        NOW
    );

    reopened
        .tick(NOW + MONITOR_EVIDENCE_TTL_SECS + 1)
        .expect("reconcile at the persisted evidence wake");
    let expired = status(
        &reopened,
        &initial.monitor_id,
        NOW + MONITOR_EVIDENCE_TTL_SECS + 1,
    );
    assert_eq!(expired.readiness.quota, MonitorQuotaReadiness::Stale);
    assert_observer_has_no_dispatch_actions(&expired);
}

#[test]
fn start_idempotency_reuses_the_same_monitor_after_stop_and_rejects_changed_config() {
    let (_directory, store) = open_store();
    let config = observe_config("session-v2-idempotent");
    let first =
        start_result(&store, config.clone(), "v2-idempotent-start", NOW).expect("start monitor");
    let repeated = start_result(&store, config.clone(), "v2-idempotent-start", NOW + 1)
        .expect("reuse identical start request");
    assert_eq!(repeated.monitor_id, first.monitor_id);

    let stopped = store
        .operate(
            MonitorOperation::Stop {
                monitor_id: first.monitor_id.clone(),
            },
            NOW + 2,
        )
        .expect("stop monitor");
    let MonitorReply::Stopped { status: stopped } = stopped else {
        panic!("expected stopped reply");
    };
    assert_eq!(stopped.lifecycle, MonitorLifecycle::Stopped);
    let state_after_first_stop =
        serde_json::to_value(&*store.lock()).expect("serialize stopped state");
    let repeated_stop = store
        .operate(
            MonitorOperation::Stop {
                monitor_id: first.monitor_id.clone(),
            },
            NOW + 3,
        )
        .expect("stopping an already stopped monitor is idempotent");
    assert!(matches!(repeated_stop, MonitorReply::Stopped { .. }));
    assert_eq!(
        serde_json::to_value(&*store.lock()).expect("serialize state after repeated stop"),
        state_after_first_stop,
        "repeated Stop must preserve stop time, counters, and event history"
    );

    let after_stop = start_result(&store, config, "v2-idempotent-start", NOW + 4)
        .expect("same key and config keep pointing at the stopped record");
    assert_eq!(after_stop.monitor_id, first.monitor_id);
    assert_eq!(after_stop.lifecycle, MonitorLifecycle::Stopped);

    let mut changed = observe_config("session-v2-idempotent");
    changed.expected_model = Some("claude-sonnet".to_owned());
    let conflict = start_result(&store, changed, "v2-idempotent-start", NOW + 5)
        .expect_err("a changed config cannot reuse the existing key");
    assert_eq!(conflict.code, MonitorIssueCode::IdempotencyConflict);
}

#[test]
fn session_filtered_guard_tracks_account_spend_and_emits_below_threshold_changes() {
    let (_directory, store) = open_store();
    let account_id = "acct-v2-session-spend";
    let goal_id = "goal-v2-session-spend";
    let session_id = "session-v2-session-spend";
    let binding = bind_account(&store, account_id, NOW);
    let scope = MonitorScope::BoundAccount {
        binding_id: binding.binding_id.clone(),
        binding_revision: binding.revision,
        session_id: Some(session_id.to_owned()),
    };
    ingest(
        &store,
        scope.clone(),
        observation(
            session_id,
            Some("claude-sonnet"),
            None,
            quota(1_000, NOW + 3_600),
            quota(1_000, NOW + 7 * 24 * 60 * 60),
        ),
        NOW,
    );
    record_zero_sgd_spend(&store, account_id, NOW);
    let policy = approve_policy(
        &store,
        &binding,
        goal_id,
        MonitorPolicy::StrictSgd,
        Some(Money::new(5_000, "SGD", 2)),
        ApprovalOptions {
            acknowledge_no_sgd_cap: false,
            expected_revision: None,
            now_epoch: NOW,
        },
    )
    .expect("approve strict policy");
    let mut config = dispatch_config(&binding, goal_id, policy.revision);
    config.scope = scope;
    let started = start_result(&store, config, "v2-session-filtered-spend", NOW)
        .expect("session-filtered strict guard can persist account spend evidence");
    assert!(started.evidence.iter().any(|evidence| {
        evidence.session_id.is_none()
            && evidence.account_id.as_deref() == Some(account_id)
            && matches!(&evidence.value, MonitorEvidenceValue::Spend { .. })
    }));
    assert!(
        started.runnable,
        "the baseline receipt stays in the guard input"
    );

    let before_sequence = store
        .lock()
        .monitors
        .get(&started.monitor_id)
        .and_then(|monitor| monitor.events.last())
        .map(|event| event.sequence)
        .expect("start event");
    store
        .operate(
            MonitorOperation::RecordSpend {
                record: SpendRecordInput {
                    account_id: account_id.to_owned(),
                    billing_period_start_epoch: NOW - 100,
                    billing_period_end_epoch: NOW + 100_000,
                    amount: Money::new(100, "SGD", 2),
                    evidence_at_epoch: Some(NOW + 1),
                    verified: true,
                    source: SpendRecordSource::OperatorReceipt,
                },
            },
            NOW + 1,
        )
        .expect("record a spend increase below the budget warning threshold");
    let updated = status(&store, &started.monitor_id, NOW + 1);
    assert_eq!(
        updated.cumulative_goal_spend,
        Some(Money::new(100, "SGD", 2))
    );
    let after_sequence = store
        .lock()
        .monitors
        .get(&started.monitor_id)
        .and_then(|monitor| monitor.events.last())
        .map(|event| event.sequence)
        .expect("spend change event");
    assert!(
        after_sequence > before_sequence,
        "Watch receives the spend change"
    );
}

#[test]
fn strict_policy_rejects_zero_budget_without_mutating_policy_history() {
    let (_directory, store) = open_store();
    let binding = bind_account(&store, "acct-v2-zero-budget", NOW);
    let before = serde_json::to_value(&*store.lock()).expect("serialize pre-approval state");
    let error = approve_policy(
        &store,
        &binding,
        "goal-v2-zero-budget",
        MonitorPolicy::StrictSgd,
        Some(Money::new(0, "SGD", 2)),
        ApprovalOptions {
            acknowledge_no_sgd_cap: false,
            expected_revision: None,
            now_epoch: NOW,
        },
    )
    .expect_err("zero SGD budget is not an operational spend cap");
    assert_eq!(error.code, MonitorIssueCode::StatuslineInvalid);
    let after = serde_json::to_value(&*store.lock()).expect("serialize rejected approval state");
    assert_eq!(
        after, before,
        "invalid budget cannot persist a policy revision"
    );
}

#[test]
fn spend_only_status_changes_create_watch_events_below_threshold() {
    let (_directory, store) = open_store();
    let account_id = "acct-v2-spend-watch";
    let goal_id = "goal-v2-spend-watch";
    let binding = bind_account(&store, account_id, NOW);
    ingest(
        &store,
        MonitorScope::BoundAccount {
            binding_id: binding.binding_id.clone(),
            binding_revision: binding.revision,
            session_id: None,
        },
        observation(
            "session-v2-spend-watch",
            None,
            None,
            quota(1_000, NOW + 3_600),
            quota(1_000, NOW + 7 * 24 * 60 * 60),
        ),
        NOW,
    );
    record_zero_sgd_spend(&store, account_id, NOW);
    let policy = approve_policy(
        &store,
        &binding,
        goal_id,
        MonitorPolicy::StrictSgd,
        Some(Money::new(5_000, "SGD", 2)),
        ApprovalOptions {
            acknowledge_no_sgd_cap: false,
            expected_revision: None,
            now_epoch: NOW,
        },
    )
    .expect("approve strict policy");
    let started = start_result(
        &store,
        dispatch_config(&binding, goal_id, policy.revision),
        "v2-spend-watch-start",
        NOW,
    )
    .expect("start strict guard");

    let mut state = store.lock().clone();
    let mut spend = state
        .goals
        .get(goal_id)
        .unwrap()
        .spend_state
        .clone()
        .unwrap();
    spend.cumulative_goal_spend = Some(Money::new(100, "SGD", 2));
    state.goals.get_mut(goal_id).unwrap().spend_state = Some(spend.clone());
    state
        .monitors
        .get_mut(&started.monitor_id)
        .unwrap()
        .spend_state = Some(spend);
    let prior_sequence = state
        .monitors
        .get(&started.monitor_id)
        .unwrap()
        .events
        .last()
        .unwrap()
        .sequence;
    assert!(append_event_if_changed(
        &mut state,
        &started.monitor_id,
        NOW + 1
    ));
    let monitor = state.monitors.get(&started.monitor_id).unwrap();
    assert!(monitor.events.last().unwrap().sequence > prior_sequence);
    assert_eq!(
        monitor.events.last().unwrap().status.cumulative_goal_spend,
        Some(Money::new(100, "SGD", 2))
    );
}

#[test]
fn session_fingerprint_keys_are_pruned_with_inactive_account_sessions() {
    let (_directory, store) = open_store();
    let account_id = "acct-v2-fingerprint-bound";
    let binding = bind_account(&store, account_id, NOW);
    let scope = MonitorScope::BoundAccount {
        binding_id: binding.binding_id.clone(),
        binding_revision: binding.revision,
        session_id: None,
    };
    let config = MonitorConfig {
        provider: MonitorProvider::Claude,
        purpose: MonitorPurpose::ObserveOnly,
        scope: scope.clone(),
        goal_id: None,
        expected_model: None,
        policy_revision: None,
        experimental_collector: false,
    };
    let started =
        start_result(&store, config, "v2-fingerprint-bound", NOW).expect("start account observer");

    for index in 0..(MAX_SESSIONS_PER_ACCOUNT * 2) {
        let now_epoch = NOW + index as i64 * (MONITOR_EVIDENCE_TTL_SECS + 1);
        let session_id = format!("session-v2-churn-{index}");
        ingest(
            &store,
            scope.clone(),
            observation(
                &session_id,
                Some("claude-sonnet"),
                None,
                quota(1_000, now_epoch + 3_600),
                quota(1_000, now_epoch + 7 * 24 * 60 * 60),
            ),
            now_epoch,
        );
        let state = store.lock();
        let monitor = state.monitors.get(&started.monitor_id).unwrap();
        assert!(monitor.evidence_fingerprints.len() <= MAX_EVIDENCE_FINGERPRINTS);
        validate_store_state(&state).expect("pruning preserves valid monitor state");
    }
    let state = store.lock();
    assert_eq!(state.accounts[account_id].sessions.len(), 1);
    assert_eq!(
        state.monitors[&started.monitor_id]
            .evidence_fingerprints
            .len(),
        5,
        "only the current model and four quota field fingerprints remain"
    );
}

#[test]
fn caller_cannot_use_the_reserved_v1_migration_idempotency_namespace() {
    let (_directory, store) = open_store();
    let before = serde_json::to_value(&*store.lock()).expect("serialize initial store");
    let error = start_result(
        &store,
        observe_config("session-v2-reserved-idempotency"),
        "v1-migrated-monitor-00000001",
        NOW,
    )
    .expect_err("caller keys cannot collide with migrated monitor keys");
    assert_eq!(error.code, MonitorIssueCode::StatuslineInvalid);
    let after = serde_json::to_value(&*store.lock()).expect("serialize store after rejection");
    assert_eq!(after, before, "reserved keys cannot mutate monitor history");
}

fn assert_rejected_counter_and_evidence_states(baseline: &StoreState, monitor_id: &str) {
    let mut corrupted = baseline.clone();
    corrupted.next_input_sequence = u64::MAX;
    assert_invalid_state(&corrupted);

    for counter in [0, 1, 2] {
        let mut corrupted = baseline.clone();
        let monitor = corrupted
            .monitors
            .get_mut(monitor_id)
            .expect("started monitor");
        match counter {
            0 => monitor.next_evidence_sequence = u64::MAX,
            1 => monitor.next_decision_sequence = u64::MAX,
            _ => monitor.next_event_sequence = u64::MAX,
        }
        assert_invalid_state(&corrupted);
    }

    let mut duplicate_evidence = baseline.clone();
    let monitor = duplicate_evidence
        .monitors
        .get_mut(monitor_id)
        .expect("started monitor");
    assert!(monitor.evidence.len() >= 2);
    monitor.evidence[1].sequence = monitor.evidence[0].sequence;
    assert_invalid_state(&duplicate_evidence);

    let mut unbounded_fingerprints = baseline.clone();
    let monitor = unbounded_fingerprints
        .monitors
        .get_mut(monitor_id)
        .expect("started monitor");
    for index in 0..=MAX_EVIDENCE_FINGERPRINTS {
        monitor.evidence_fingerprints.insert(
            format!("model:synthetic-session-{index}"),
            "synthetic-fingerprint".to_owned(),
        );
    }
    assert_invalid_state(&unbounded_fingerprints);

    let mut out_of_order_events = baseline.clone();
    let monitor = out_of_order_events
        .monitors
        .get_mut(monitor_id)
        .expect("started monitor");
    let event = monitor.events[0].clone();
    monitor.events = vec![event.clone(), event];
    monitor.events[0].sequence = 2;
    monitor.events[1].sequence = 1;
    monitor.next_event_sequence = 2;
    assert_invalid_state(&out_of_order_events);

    let mut duplicate_event_evidence = baseline.clone();
    let monitor = duplicate_event_evidence
        .monitors
        .get_mut(monitor_id)
        .expect("started monitor");
    let event = monitor.events.last_mut().expect("observation event");
    assert!(event.status.evidence.len() >= 2);
    event.status.evidence[1].sequence = event.status.evidence[0].sequence;
    assert_invalid_state(&duplicate_event_evidence);

    let mut future_event_evidence = baseline.clone();
    let monitor = future_event_evidence
        .monitors
        .get_mut(monitor_id)
        .expect("started monitor");
    let future_sequence = monitor.next_evidence_sequence + 1;
    let event = monitor.events.last_mut().expect("observation event");
    event.status.evidence[0].sequence = future_sequence;
    assert_invalid_state(&future_event_evidence);

    let mut future_event = baseline.clone();
    let monitor = future_event
        .monitors
        .get_mut(monitor_id)
        .expect("started monitor");
    monitor.events.truncate(1);
    monitor.events[0].sequence = monitor.next_event_sequence + 1;
    assert_invalid_state(&future_event);
}

fn assert_rejected_event_and_decision_states(baseline: &StoreState, monitor_id: &str) {
    let mut evicted_reference = baseline.clone();
    let monitor = evicted_reference
        .monitors
        .get_mut(monitor_id)
        .expect("started monitor");
    let referenced_sequence = monitor.evidence[0].sequence;
    let decision_sequence = monitor.next_decision_sequence.max(1);
    monitor.next_decision_sequence = decision_sequence;
    monitor.latest_decision = Some(MonitorDecision {
        sequence: decision_sequence,
        decided_at_epoch: NOW + 1,
        evidence_sequences: vec![referenced_sequence],
        actions: Vec::new(),
    });
    monitor
        .evidence
        .retain(|evidence| evidence.sequence != referenced_sequence);
    validate_store_state(&evicted_reference)
        .expect("a bounded evidence ring may evict a referenced historical item");

    let mut future_decision = baseline.clone();
    let monitor = future_decision
        .monitors
        .get_mut(monitor_id)
        .expect("started monitor");
    let decision_sequence = monitor.next_decision_sequence + 1;
    monitor.latest_decision = Some(MonitorDecision {
        sequence: decision_sequence,
        decided_at_epoch: NOW + 1,
        evidence_sequences: Vec::new(),
        actions: Vec::new(),
    });
    assert_invalid_state(&future_decision);

    let mut future_decision_evidence = baseline.clone();
    let monitor = future_decision_evidence
        .monitors
        .get_mut(monitor_id)
        .expect("started monitor");
    let decision_sequence = monitor.next_decision_sequence.max(1);
    monitor.next_decision_sequence = decision_sequence;
    monitor.latest_decision = Some(MonitorDecision {
        sequence: decision_sequence,
        decided_at_epoch: NOW + 1,
        evidence_sequences: vec![monitor.next_evidence_sequence + 1],
        actions: Vec::new(),
    });
    assert_invalid_state(&future_decision_evidence);

    let mut duplicate_decision_evidence = baseline.clone();
    let monitor = duplicate_decision_evidence
        .monitors
        .get_mut(monitor_id)
        .expect("started monitor");
    let decision_sequence = monitor.next_decision_sequence.max(1);
    monitor.next_decision_sequence = decision_sequence;
    let referenced_sequence = monitor.evidence[0].sequence;
    monitor.latest_decision = Some(MonitorDecision {
        sequence: decision_sequence,
        decided_at_epoch: NOW + 1,
        evidence_sequences: vec![referenced_sequence, referenced_sequence],
        actions: Vec::new(),
    });
    assert_invalid_state(&duplicate_decision_evidence);
}

#[test]
fn saturated_sequence_counters_are_rejected_and_failed_ingest_preserves_history() {
    let (_directory, store) = open_store();
    let session_id = "session-v2-sequence-exhaustion";
    let started = start_result(
        &store,
        observe_config(session_id),
        "v2-sequence-exhaustion-start",
        NOW,
    )
    .expect("start observer");
    let scope = MonitorScope::Session {
        session_id: session_id.to_owned(),
    };
    ingest(
        &store,
        scope.clone(),
        observation(
            session_id,
            Some("model-a"),
            None,
            quota(5_000, NOW + 3_600),
            quota(2_000, NOW + 86_400),
        ),
        NOW + 1,
    );

    let baseline = store.lock().clone();
    assert_rejected_counter_and_evidence_states(&baseline, &started.monitor_id);
    assert_rejected_event_and_decision_states(&baseline, &started.monitor_id);

    let mut near_exhaustion = baseline;
    near_exhaustion.next_input_sequence = u64::MAX - 1;
    install_store_state(&store, near_exhaustion);
    let before =
        serde_json::to_value(&*store.lock()).expect("serialize state before rejected ingest");
    let error = store
        .operate(
            MonitorOperation::Ingest {
                scope,
                observation: observation(
                    session_id,
                    Some("model-b"),
                    None,
                    quota(5_000, NOW + 3_600),
                    quota(2_000, NOW + 86_400),
                ),
            },
            NOW + 2,
        )
        .expect_err("the MAX sequence must fail closed before persistence");
    assert_eq!(error.code, MonitorIssueCode::MonitorStoreUnavailable);
    let after =
        serde_json::to_value(&*store.lock()).expect("serialize state after rejected ingest");
    assert_eq!(
        after, before,
        "failed sequence allocation preserves all histories"
    );
}

#[test]
fn old_policy_snapshots_cannot_erase_canonical_goal_spend_history() {
    let (_directory, store) = open_store();
    let account_id = "acct-v2-monotonic-spend";
    let goal_id = "goal-v2-monotonic-spend";
    let binding = bind_account(&store, account_id, NOW);
    record_zero_sgd_spend(&store, account_id, NOW);
    let first_policy = approve_policy(
        &store,
        &binding,
        goal_id,
        MonitorPolicy::StrictSgd,
        Some(Money::new(5_000, "SGD", 2)),
        ApprovalOptions {
            acknowledge_no_sgd_cap: false,
            expected_revision: None,
            now_epoch: NOW + 1,
        },
    )
    .expect("approve initial strict policy");
    let first_monitor = start_result(
        &store,
        dispatch_config(&binding, goal_id, first_policy.revision),
        "v2-monotonic-spend-first",
        NOW + 2,
    )
    .expect("start first guard");

    let tightened_policy = approve_policy(
        &store,
        &binding,
        goal_id,
        MonitorPolicy::StrictSgd,
        Some(Money::new(4_000, "SGD", 2)),
        ApprovalOptions {
            acknowledge_no_sgd_cap: false,
            expected_revision: Some(first_policy.revision),
            now_epoch: NOW + 3,
        },
    )
    .expect("tighten policy with a new revision");
    let second_monitor = start_result(
        &store,
        dispatch_config(&binding, goal_id, tightened_policy.revision),
        "v2-monotonic-spend-second",
        NOW + 4,
    )
    .expect("start guard under tightened policy");
    store
        .operate(
            MonitorOperation::RecordSpend {
                record: SpendRecordInput {
                    account_id: account_id.to_owned(),
                    billing_period_start_epoch: NOW - 100,
                    billing_period_end_epoch: NOW + 100_000,
                    amount: Money::new(100, "SGD", 2),
                    evidence_at_epoch: Some(NOW + 5),
                    verified: true,
                    source: SpendRecordSource::OperatorReceipt,
                },
            },
            NOW + 5,
        )
        .expect("advance canonical spend after the older monitor snapshot");

    let valid_state = store.lock().clone();
    let goal_spend = valid_state
        .goals
        .get(goal_id)
        .and_then(|goal| goal.spend_state.as_ref())
        .expect("canonical goal spend state");
    let first_snapshot = valid_state
        .monitors
        .get(&first_monitor.monitor_id)
        .and_then(|monitor| monitor.spend_state.as_ref())
        .expect("older monitor snapshot");
    let second_snapshot = valid_state
        .monitors
        .get(&second_monitor.monitor_id)
        .and_then(|monitor| monitor.spend_state.as_ref())
        .expect("current monitor snapshot");
    assert_eq!(first_snapshot.baseline, goal_spend.baseline);
    assert_eq!(
        first_snapshot.cumulative_goal_spend,
        Some(Money::new(0, "SGD", 2))
    );
    assert_eq!(
        goal_spend.cumulative_goal_spend,
        Some(Money::new(100, "SGD", 2))
    );
    assert_eq!(second_snapshot, goal_spend);
    validate_store_state(&valid_state).expect("older immutable snapshot may lag monotonically");

    let mut erased_cumulative = valid_state.clone();
    erased_cumulative
        .goals
        .get_mut(goal_id)
        .and_then(|goal| goal.spend_state.as_mut())
        .expect("canonical goal spend state")
        .cumulative_goal_spend = Some(Money::new(0, "SGD", 2));
    assert_invalid_state(&erased_cumulative);

    let mut upgraded_uncertainty = valid_state.clone();
    upgraded_uncertainty
        .monitors
        .get_mut(&first_monitor.monitor_id)
        .and_then(|monitor| monitor.spend_state.as_mut())
        .expect("older monitor snapshot")
        .cumulative_complete = false;
    assert_invalid_state(&upgraded_uncertainty);

    let mut changed_baseline = valid_state;
    changed_baseline
        .monitors
        .get_mut(&first_monitor.monitor_id)
        .and_then(|monitor| monitor.spend_state.as_mut())
        .and_then(|spend| spend.baseline.as_mut())
        .expect("older baseline")
        .amount
        .amount_minor += 1;
    assert_invalid_state(&changed_baseline);
}

#[test]
fn v1_migration_rejects_saturated_input_sequence() {
    let fixture = br#"{
      "schema_version": 1,
      "next_monitor_id": 1,
      "next_input_sequence": 18446744073709551615,
      "last_now_epoch": 0,
      "accounts": {},
      "monitors": {},
      "goals": {}
    }"#;
    assert_invalid_v1(fixture);
}

#[test]
fn strict_start_without_baseline_is_atomic_and_does_not_consume_its_key_or_id() {
    let (_directory, store) = open_store();
    let account_id = "acct-v2-no-baseline";
    let goal_id = "goal-v2-no-baseline";
    let binding = bind_account(&store, account_id, NOW);
    let policy = approve_policy(
        &store,
        &binding,
        goal_id,
        MonitorPolicy::StrictSgd,
        Some(Money::new(5_000, "SGD", 2)),
        ApprovalOptions {
            acknowledge_no_sgd_cap: false,
            expected_revision: None,
            now_epoch: NOW,
        },
    )
    .expect("approve strict SGD policy");
    let config = dispatch_config(&binding, goal_id, policy.revision);
    let before = serde_json::to_value(&*store.lock()).expect("serialize pre-start state");

    let error = start_result(&store, config.clone(), "v2-strict-needs-baseline", NOW)
        .expect_err("strict activation requires a verified fresh baseline");
    assert_eq!(error.code, MonitorIssueCode::BudgetUnverifiable);
    let after = serde_json::to_value(&*store.lock()).expect("serialize post-failure state");
    assert_eq!(
        after, before,
        "failed activation must not commit staged state"
    );
    {
        let state = store.lock();
        assert!(!state.goals.contains_key(goal_id));
        assert!(state.monitors.is_empty());
        assert_eq!(state.next_monitor_id, 1);
    }

    // The rejected attempt left both the ID counter and idempotency namespace
    // untouched, so the same request succeeds once its prerequisite is supplied.
    record_zero_sgd_spend(&store, account_id, NOW);
    let started = start_result(&store, config, "v2-strict-needs-baseline", NOW)
        .expect("retry with a verified baseline");
    assert_eq!(started.monitor_id, "monitor-00000001");
    assert!(store.lock().goals.contains_key(goal_id));
}

#[test]
fn quota_only_requires_explicit_acknowledgement_and_marks_budget_disabled() {
    let (_directory, store) = open_store();
    let binding = bind_account(&store, "acct-v2-quota-only", NOW);
    let goal_id = "goal-v2-quota-only";

    let missing_ack = approve_policy(
        &store,
        &binding,
        goal_id,
        MonitorPolicy::QuotaOnly,
        None,
        ApprovalOptions {
            acknowledge_no_sgd_cap: false,
            expected_revision: None,
            now_epoch: NOW,
        },
    )
    .expect_err("quota-only policy requires acknowledgement of the absent SGD cap");
    assert_eq!(
        missing_ack.code,
        MonitorIssueCode::SgdCapAcknowledgementRequired
    );
    assert!(!store.lock().policy_records.contains_key(goal_id));

    let policy = approve_policy(
        &store,
        &binding,
        goal_id,
        MonitorPolicy::QuotaOnly,
        None,
        ApprovalOptions {
            acknowledge_no_sgd_cap: true,
            expected_revision: None,
            now_epoch: NOW,
        },
    )
    .expect("approve acknowledged quota-only policy");
    let started = start_result(
        &store,
        dispatch_config(&binding, goal_id, policy.revision),
        "v2-quota-only-start",
        NOW,
    )
    .expect("start quota-only dispatch guard without an SGD baseline");

    assert_eq!(
        started.policy.as_ref().unwrap().new_policy,
        MonitorPolicy::QuotaOnly
    );
    assert!(started.policy.as_ref().unwrap().acknowledge_no_sgd_cap);
    assert_eq!(started.budget, None);
    assert_eq!(started.readiness.budget, MonitorBudgetReadiness::Disabled);
    assert!(!started.runnable);
}

#[test]
fn dispatch_guard_start_without_approved_policy_is_denied() {
    let (_directory, store) = open_store();
    let binding = bind_account(&store, "acct-v2-no-policy", NOW);
    let config = dispatch_config(&binding, "goal-v2-no-policy", 1);

    let error = start_result(&store, config, "v2-no-policy-start", NOW)
        .expect_err("dispatch guard requires an explicit policy approval");
    assert_eq!(error.code, MonitorIssueCode::PolicyRequired);
    let state = store.lock();
    assert!(state.monitors.is_empty());
    assert!(state.goals.is_empty());
}

#[test]
fn activated_policy_cannot_downgrade_and_tightening_blocks_the_old_revision() {
    let (_directory, store) = open_store();
    let account_id = "acct-v2-policy-revision";
    let goal_id = "goal-v2-policy-revision";
    let binding = bind_account(&store, account_id, NOW);
    ingest(
        &store,
        MonitorScope::BoundAccount {
            binding_id: binding.binding_id.clone(),
            binding_revision: binding.revision,
            session_id: None,
        },
        observation(
            "session-v2-policy-revision",
            None,
            Some("2.1.80"),
            quota(1_000, NOW + 3_600),
            quota(1_000, NOW + 7 * 24 * 60 * 60),
        ),
        NOW,
    );
    record_zero_sgd_spend(&store, account_id, NOW);
    let initial_policy = approve_policy(
        &store,
        &binding,
        goal_id,
        MonitorPolicy::StrictSgd,
        Some(Money::new(5_000, "SGD", 2)),
        ApprovalOptions {
            acknowledge_no_sgd_cap: false,
            expected_revision: None,
            now_epoch: NOW,
        },
    )
    .expect("approve active strict policy");
    let config = dispatch_config(&binding, goal_id, initial_policy.revision);
    let started = start_result(&store, config, "v2-policy-revision-start", NOW)
        .expect("activate goal with current strict baseline");
    assert!(
        started.runnable,
        "fixture starts with verified quota and spend evidence"
    );

    let downgrade = approve_policy(
        &store,
        &binding,
        goal_id,
        MonitorPolicy::QuotaOnly,
        None,
        ApprovalOptions {
            acknowledge_no_sgd_cap: true,
            expected_revision: Some(initial_policy.revision),
            now_epoch: NOW + 1,
        },
    )
    .expect_err("an activated strict policy cannot be downgraded");
    assert_eq!(downgrade.code, MonitorIssueCode::PolicyConflict);

    let tightened = approve_policy(
        &store,
        &binding,
        goal_id,
        MonitorPolicy::StrictSgd,
        Some(Money::new(3_000, "SGD", 2)),
        ApprovalOptions {
            acknowledge_no_sgd_cap: false,
            expected_revision: Some(initial_policy.revision),
            now_epoch: NOW + 2,
        },
    )
    .expect("approve a tighter strict budget");
    assert_eq!(tightened.revision, initial_policy.revision + 1);

    let old_revision = status(&store, &started.monitor_id, NOW + 2);
    assert_eq!(
        old_revision.readiness.dispatch,
        MonitorDispatchReadiness::Blocked
    );
    assert!(!old_revision.runnable);
    assert!(
        old_revision
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::PolicyConflict)
    );
}

#[test]
fn reset_and_model_descriptors_report_state_without_relaxing_dispatch_guard() {
    let (_directory, store) = open_store();
    let account_id = "acct-v2-reset-model-descriptors";
    let goal_id = "goal-v2-reset-model-descriptors";
    let binding = bind_account(&store, account_id, NOW);
    let five_hour_reset = NOW + 100;
    let seven_day_reset = NOW + 7 * 24 * 60 * 60;
    ingest(
        &store,
        MonitorScope::BoundAccount {
            binding_id: binding.binding_id.clone(),
            binding_revision: binding.revision,
            session_id: None,
        },
        observation(
            "session-v2-reset-model-descriptors",
            Some("claude-sonnet"),
            Some("2.1.80"),
            quota(1_000, five_hour_reset),
            quota(1_000, seven_day_reset),
        ),
        NOW,
    );
    record_zero_sgd_spend(&store, account_id, NOW);
    let policy = approve_policy(
        &store,
        &binding,
        goal_id,
        MonitorPolicy::StrictSgd,
        Some(Money::new(5_000, "SGD", 2)),
        ApprovalOptions {
            acknowledge_no_sgd_cap: false,
            expected_revision: None,
            now_epoch: NOW,
        },
    )
    .expect("approve strict SGD policy");
    let mut config = dispatch_config(&binding, goal_id, policy.revision);
    config.expected_model = Some("claude-sonnet".to_owned());
    let started = start_result(&store, config, "v2-reset-model-descriptors", NOW)
        .expect("start model-guarded monitor");

    assert_eq!(started.expected_model.as_deref(), Some("claude-sonnet"));
    assert_eq!(
        started.five_hour.reset_validity,
        MonitorResetValidity::Future
    );
    assert_eq!(
        started.seven_day.reset_validity,
        MonitorResetValidity::Future
    );
    assert_eq!(
        started.five_hour.reset_evidence.as_ref().unwrap().freshness,
        MonitorEvidenceFreshness::Current
    );
    assert_eq!(
        started.model_guard_validity,
        MonitorModelGuardValidity::Match
    );
    assert!(
        started.runnable,
        "descriptors preserve the current authorization"
    );

    let reset_deadline = five_hour_reset + MONITOR_RESET_GRACE_SECS;
    store
        .tick(reset_deadline)
        .expect("tick after the reported reset grace period");
    let due = status(&store, &started.monitor_id, reset_deadline);
    assert_eq!(due.five_hour.reset_validity, MonitorResetValidity::Due);
    assert_eq!(due.seven_day.reset_validity, MonitorResetValidity::Future);
    assert_eq!(
        due.five_hour
            .reset_evidence
            .as_ref()
            .unwrap()
            .evidence_received_at_epoch,
        NOW,
        "the due reset remains the old report because no reset evidence arrived"
    );
    assert_eq!(
        due.five_hour.reset_evidence.as_ref().unwrap().freshness,
        MonitorEvidenceFreshness::Current
    );
    assert_eq!(due.model_guard_validity, MonitorModelGuardValidity::Match);
    assert!(
        due.issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::ResetDueUnverified)
    );
    assert_eq!(due.readiness.quota, MonitorQuotaReadiness::Stale);
    assert_eq!(due.readiness.dispatch, MonitorDispatchReadiness::Blocked);
    assert!(
        !due.runnable,
        "reset descriptors must not relax the existing guard"
    );
}

#[test]
fn v1_migration_preserves_missing_baseline_and_existing_spend_history() {
    let fixture = br#"
        {
          "schema_version": 1,
          "next_monitor_id": 1,
          "next_input_sequence": 0,
          "last_now_epoch": 1800000000,
          "accounts": {},
          "monitors": {},
          "goals": {
            "goal-v1-history": {
              "account_id": "acct-v1-history",
              "budget": {"amount_minor": 5000, "currency": "SGD", "exponent": 2},
              "spend_state": {
                "baseline": null,
                "period_anchor": {
                  "account_id": "acct-v1-history",
                  "billing_period_start_epoch": 1799999900,
                  "billing_period_end_epoch": 1800001000,
                  "amount": {"amount_minor": 1500, "currency": "SGD", "exponent": 2},
                  "evidence_at_epoch": 1799999990,
                  "evidence_received_at_epoch": 1800000000,
                  "source": "operator_receipt",
                  "verification": "verified"
                },
                "cumulative_goal_spend": {"amount_minor": 1500, "currency": "SGD", "exponent": 2},
                "rollover_unknown": false,
                "cumulative_complete": true
              }
            }
          }
        }
    "#;
    let migrated = legacy::migrate_v1(fixture).expect("migrate V1 fixture");
    validate_store_state(&migrated).expect("validate migrated V2 state");

    assert_eq!(migrated.schema_version, USAGE_MONITOR_SCHEMA_VERSION);
    let goal = migrated
        .goals
        .get("goal-v1-history")
        .expect("migrated goal");
    let spend = goal
        .spend_state
        .as_ref()
        .expect("migrated goal spend state");
    assert!(
        spend.baseline.is_none(),
        "migration must preserve missing baseline"
    );
    assert_eq!(
        spend.period_anchor.as_ref().unwrap().amount,
        Money::new(1_500, "SGD", 2)
    );
    assert_eq!(
        spend.cumulative_goal_spend,
        Some(Money::new(1_500, "SGD", 2))
    );
    assert!(spend.cumulative_complete);

    let policy = migrated
        .policy_records
        .get("goal-v1-history")
        .and_then(|history| history.last())
        .expect("migrated policy history");
    assert_eq!(policy.origin, MonitorPolicyOrigin::MigratedV1);
    assert_eq!(policy.new_policy, MonitorPolicy::StrictSgd);
    assert!(!policy.operator_confirmed);

    let (_directory, store) = open_store();
    install_store_state(&store, migrated);
    let binding = bind_account(&store, "acct-v1-history", NOW);
    assert_eq!(binding.revision, 2);

    let old_policy_config = dispatch_config(&binding, "goal-v1-history", 1);
    let rejected = start_result(
        &store,
        old_policy_config,
        "v2-migrated-policy-requires-approval",
        NOW + 1,
    )
    .expect_err("migrated policy must not authorize dispatch by itself");
    assert_eq!(rejected.code, MonitorIssueCode::PolicyRequired);
    assert!(store.lock().monitors.is_empty());
    assert_eq!(store.lock().next_monitor_id, 1);

    let approved = approve_policy(
        &store,
        &binding,
        "goal-v1-history",
        MonitorPolicy::StrictSgd,
        Some(Money::new(5_000, "SGD", 2)),
        ApprovalOptions {
            acknowledge_no_sgd_cap: false,
            expected_revision: Some(1),
            now_epoch: NOW + 2,
        },
    )
    .expect("explicit operator approval creates a new policy revision");
    assert_eq!(approved.revision, 2);

    let started = start_result(
        &store,
        dispatch_config(&binding, "goal-v1-history", approved.revision),
        "v2-migrated-policy-after-approval",
        NOW + 3,
    )
    .expect("explicit policy approval permits starting the migrated goal");
    assert_eq!(started.budget, Some(Money::new(5_000, "SGD", 2)));
    assert_eq!(started.readiness.budget, MonitorBudgetReadiness::Unknown);
    assert!(!started.runnable);
    assert!(
        store
            .lock()
            .goals
            .get("goal-v1-history")
            .and_then(|goal| goal.spend_state.as_ref())
            .is_some_and(|spend| spend.baseline.is_none())
    );
}

#[test]
fn migrated_zero_budget_requires_explicit_repair_and_does_not_create_a_baseline() {
    let fixture = br#"
        {
          "schema_version": 1,
          "next_monitor_id": 1,
          "next_input_sequence": 0,
          "last_now_epoch": 1800000000,
          "accounts": {},
          "monitors": {},
          "goals": {
            "goal-v1-zero-budget": {
              "account_id": "acct-v1-zero-budget",
              "budget": {"amount_minor": 0, "currency": "SGD", "exponent": 2},
              "spend_state": {
                "baseline": null,
                "period_anchor": {
                  "account_id": "acct-v1-zero-budget",
                  "billing_period_start_epoch": 1799999900,
                  "billing_period_end_epoch": 1800001000,
                  "amount": {"amount_minor": 1500, "currency": "SGD", "exponent": 2},
                  "evidence_at_epoch": 1799999990,
                  "evidence_received_at_epoch": 1800000000,
                  "source": "operator_receipt",
                  "verification": "verified"
                },
                "cumulative_goal_spend": {"amount_minor": 1500, "currency": "SGD", "exponent": 2},
                "rollover_unknown": false,
                "cumulative_complete": true
              }
            }
          }
        }
    "#;
    let migrated = legacy::migrate_v1(fixture).expect("migrate accepted V1 zero budget");
    validate_store_state(&migrated).expect("validate migrated history");
    let migrated_spend = migrated.goals["goal-v1-zero-budget"]
        .spend_state
        .clone()
        .expect("migrated spend state");
    assert!(migrated_spend.baseline.is_none());
    assert_eq!(
        migrated_spend.cumulative_goal_spend,
        Some(Money::new(1_500, "SGD", 2))
    );

    let (_directory, store) = open_store();
    install_store_state(&store, migrated);
    let binding = bind_account(&store, "acct-v1-zero-budget", NOW);
    let approved = approve_policy(
        &store,
        &binding,
        "goal-v1-zero-budget",
        MonitorPolicy::StrictSgd,
        Some(Money::new(5_000, "SGD", 2)),
        ApprovalOptions {
            acknowledge_no_sgd_cap: false,
            expected_revision: Some(1),
            now_epoch: NOW + 1,
        },
    )
    .expect("operator may repair the migrated zero cap with an explicit positive cap");
    assert_eq!(approved.revision, 2);
    assert_eq!(
        store.lock().goals["goal-v1-zero-budget"]
            .spend_state
            .as_ref()
            .unwrap(),
        &migrated_spend,
        "approval preserves the historical baseline, anchor, and cumulative spend"
    );

    let started = start_result(
        &store,
        dispatch_config(&binding, "goal-v1-zero-budget", approved.revision),
        "v2-repair-v1-zero-budget",
        NOW + 2,
    )
    .expect("an existing migrated goal may be monitored while dispatch remains blocked");
    assert_eq!(started.readiness.budget, MonitorBudgetReadiness::Unknown);
    assert_eq!(
        started.readiness.dispatch,
        MonitorDispatchReadiness::Blocked
    );
    assert!(!started.runnable);
    assert_eq!(
        store.lock().goals["goal-v1-zero-budget"]
            .spend_state
            .as_ref()
            .unwrap(),
        &migrated_spend,
        "monitor creation cannot fabricate the missing historical baseline"
    );

    let mut zero_operator_policy = store.lock().clone();
    let current = zero_operator_policy
        .policy_records
        .get_mut("goal-v1-zero-budget")
        .unwrap()
        .last_mut()
        .unwrap();
    current.budget = Some(Money::new(0, "SGD", 2));
    let operator_zero_budget = current.budget.clone();
    zero_operator_policy
        .goals
        .get_mut("goal-v1-zero-budget")
        .unwrap()
        .budget = operator_zero_budget;
    assert_invalid_state(&zero_operator_policy);
}
