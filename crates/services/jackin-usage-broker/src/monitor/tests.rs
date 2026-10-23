// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    MonitorStore, SpendState, monitor_basis_points, monitor_source_used_points,
    monitor_window_used_points, spend_snapshot_preserves_history,
};
use jackin_protocol::control::{
    MeterPercentPoints, Money, PercentObservationOrigin, ProviderQuotaState, RawPercentPoints,
    UsageMeterSide,
};
use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageCalendarPeriodV1, UsageFreshnessPhaseV1, UsageFreshnessV1,
    UsageIdentityKindV1, UsageLifecycleV1, UsageLimitWindowV1, UsageMembershipStateV1,
    UsageMetricGroupKindV1, UsageMetricGroupV1, UsageMetricPeriodV1, UsageMetricScopeV1,
    UsageMetricValueV1, UsageProjectionRefreshStateV1, UsageProjectionSchemaV2, UsageProjectionV1,
    UsageProviderV1, UsageQuotaStateV1, UsageWindowCategoryV1,
};
use jackin_protocol::usage_monitor::{
    MonitorAccountBinding, MonitorAccountBindingInput, MonitorAction, MonitorConfig,
    MonitorDispatchReadiness, MonitorEvidenceFreshness, MonitorIssueCode, MonitorLifecycle,
    MonitorModelGuardValidity, MonitorOperation, MonitorPolicy, MonitorPolicyApprovalInput,
    MonitorPolicyRecord, MonitorProvider, MonitorPurpose, MonitorQuotaReadiness, MonitorReply,
    MonitorScope, MonitorStatus, SpendRecord, SpendRecordInput, SpendRecordSource,
    SpendVerification, StatuslineObservation, StatuslineQuotaWindow, StatuslineRateLimits,
};

const NOW: i64 = 1_800_000_000;

#[test]
fn generic_points_convert_to_monitor_basis_points_only_at_boundary() {
    let raw = |points| RawPercentPoints::new(points).expect("finite percentage points");
    assert_eq!(monitor_basis_points(raw(12.34)), Some(1_234));
    assert_eq!(monitor_basis_points(raw(12.345)), Some(1_235));
    assert_eq!(monitor_basis_points(raw(-10.0)), None);
    assert_eq!(monitor_basis_points(raw(150.0)), Some(10_000));
}

#[test]
fn exhausted_state_geometry_does_not_create_monitor_basis_point_evidence() {
    let window = UsageLimitWindowV1 {
        window_id: "exhausted-state-only".to_owned(),
        rank: 0,
        category: UsageWindowCategoryV1::Session,
        label: "session".to_owned(),
        value_label: "Exhausted".to_owned(),
        reset_label: String::new(),
        remaining_percent: None,
        remaining_percent_origin: None,
        used_percent: None,
        used_percent_origin: None,
        meter_percent: Some(MeterPercentPoints::new(0.0).expect("exhausted geometry")),
        meter_side: Some(UsageMeterSide::Remaining),
        count_quota: None,
        provider_quota_state: Some(ProviderQuotaState::Exhausted),
        money_used_percent: None,
        reset_at_epoch: None,
        quota_state: UsageQuotaStateV1::Exhausted,
        pace_label: None,
        runs_out_label: None,
    };

    assert_eq!(monitor_window_used_points(&window), None);
}

#[test]
fn monitor_source_precedence_respects_origins_and_preserves_reported_remaining() {
    let raw = |points| RawPercentPoints::new(points).expect("finite percentage points");

    let money_one = raw(1.0);
    assert_eq!(
        monitor_source_used_points(
            None,
            None,
            Some(raw(0.0)),
            Some(PercentObservationOrigin::HistoricalUnknown),
            Some(money_one),
        ),
        Some(money_one),
        "historical remaining=0 must not hide exact Money usage",
    );

    assert_eq!(
        monitor_source_used_points(
            None,
            None,
            Some(raw(0.0)),
            Some(PercentObservationOrigin::ProviderReported),
            Some(money_one),
        ),
        Some(raw(100.0)),
        "a provider-reported remaining side keeps the monitor's legacy complement",
    );

    assert_eq!(
        monitor_source_used_points(
            Some(raw(43.375)),
            Some(PercentObservationOrigin::ProviderReported),
            Some(raw(12.625)),
            Some(PercentObservationOrigin::ProviderReported),
            Some(raw(1.0)),
        ),
        Some(raw(43.375)),
        "explicit provider used value retains precedence even when sides disagree",
    );

    assert_eq!(
        monitor_source_used_points(
            None,
            None,
            Some(raw(0.0)),
            Some(PercentObservationOrigin::ProviderReported),
            Some(raw(120.0)),
        ),
        Some(raw(120.0)),
        "Money overage keeps the established monitor full-usage behavior",
    );
}

fn open_store() -> (tempfile::TempDir, MonitorStore) {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let store = MonitorStore::open(directory.path()).expect("open monitor store");
    (directory, store)
}

#[derive(Debug, Clone)]
struct StartSpec {
    account_id: String,
    goal_id: String,
    expected_model: Option<String>,
    budget: Option<Money>,
}

#[derive(Debug, Clone)]
struct PreparedStart {
    config: MonitorConfig,
    idempotency_key: String,
}

fn config(
    account_id: &str,
    goal_id: &str,
    expected_model: Option<&str>,
    budget: Option<Money>,
) -> StartSpec {
    StartSpec {
        account_id: account_id.to_owned(),
        goal_id: goal_id.to_owned(),
        expected_model: expected_model.map(str::to_owned),
        budget,
    }
}

fn bind_account(store: &MonitorStore, account_id: &str, now_epoch: i64) -> MonitorAccountBinding {
    match store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: account_id.to_owned(),
                    operator_label: "isolated-test-operator".to_owned(),
                    operator_confirmed: true,
                    provider_account_id: None,
                    experimental_collector_approved: false,
                },
            },
            now_epoch,
        )
        .expect("confirm isolated test account binding")
    {
        MonitorReply::AccountBound { binding } => binding,
        other => panic!("expected account-bound reply, got {other:?}"),
    }
}

#[test]
fn collector_consent_selects_exact_foreground_source_id() {
    let (_directory, store) = open_store();
    let source_id = "a".repeat(64);
    let foreign_source_id = "b".repeat(64);
    store.set_experimental_collector_source(Some(source_id.clone()));

    let binding = match store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: "work-account".to_owned(),
                    operator_label: "isolated-test-operator".to_owned(),
                    operator_confirmed: true,
                    provider_account_id: Some(source_id.clone()),
                    experimental_collector_approved: true,
                },
            },
            NOW,
        )
        .expect("confirm local account and exact source mapping")
    {
        MonitorReply::AccountBound { binding } => binding,
        other => panic!("expected account-bound reply, got {other:?}"),
    };

    let config = MonitorConfig {
        provider: MonitorProvider::Claude,
        purpose: MonitorPurpose::ObserveOnly,
        scope: MonitorScope::BoundAccount {
            binding_id: binding.binding_id,
            binding_revision: binding.revision,
            session_id: None,
        },
        goal_id: None,
        expected_model: None,
        policy_revision: None,
        experimental_collector: true,
    };
    assert!(matches!(
        store
            .operate(
                MonitorOperation::Start {
                    config,
                    idempotency_key: "collector-source-selection".to_owned(),
                },
                NOW,
            )
            .expect("start opted-in account observer"),
        MonitorReply::Started { .. }
    ));

    assert_eq!(store.collection_accounts(), vec![source_id.clone()]);
    assert_ne!(store.collection_accounts()[0], "work-account");

    store.set_experimental_collector_source(Some(foreign_source_id));
    assert!(store.collection_accounts().is_empty());
}

#[test]
fn account_binding_rejects_noncanonical_source_capability_ids() {
    let (_directory, store) = open_store();
    for source_capability_id in ["not-a-source-id".to_owned(), "A".repeat(64)] {
        let error = store
            .operate(
                MonitorOperation::BindAccount {
                    binding: MonitorAccountBindingInput {
                        provider: MonitorProvider::Claude,
                        account_id: "work-account".to_owned(),
                        operator_label: "isolated-test-operator".to_owned(),
                        operator_confirmed: true,
                        provider_account_id: Some(source_capability_id),
                        experimental_collector_approved: true,
                    },
                },
                NOW,
            )
            .expect_err("source mappings must use the opaque hash wire format");
        assert_eq!(error.code, MonitorIssueCode::StatuslineInvalid);
    }
    assert!(store.lock().bindings.is_empty());
}

fn approve_strict_policy(
    store: &MonitorStore,
    binding: &MonitorAccountBinding,
    goal_id: &str,
    budget: &Money,
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
                    budget: Some(budget.clone()),
                    operator_label: "isolated-test-operator".to_owned(),
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
        other => panic!("expected policy-approved reply, got {other:?}"),
    }
}

fn prepare_start(store: &MonitorStore, spec: &StartSpec, now_epoch: i64) -> PreparedStart {
    let binding = bind_account(store, &spec.account_id, now_epoch);
    let (purpose, goal_id, policy_revision) = if let Some(budget) = spec.budget.as_ref() {
        let policy = approve_strict_policy(store, &binding, &spec.goal_id, budget, now_epoch);
        (
            MonitorPurpose::DispatchGuard,
            Some(spec.goal_id.clone()),
            Some(policy.revision),
        )
    } else {
        (MonitorPurpose::ObserveOnly, None, None)
    };
    PreparedStart {
        config: MonitorConfig {
            provider: MonitorProvider::Claude,
            purpose,
            scope: MonitorScope::BoundAccount {
                binding_id: binding.binding_id,
                binding_revision: binding.revision,
                session_id: None,
            },
            goal_id,
            expected_model: spec.expected_model.clone(),
            policy_revision,
            experimental_collector: false,
        },
        idempotency_key: format!("fixture-start:{}:{}", spec.goal_id, now_epoch),
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
        other => panic!("expected started reply, got {other:?}"),
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
        schema_version: jackin_protocol::usage_monitor::USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
        session_id: session_id.to_owned(),
        model: model.map(str::to_owned),
        claude_code_version: Some("2.1.80".to_owned()),
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
                scope: {
                    let binding = bind_account(store, account_id, now_epoch);
                    MonitorScope::BoundAccount {
                        binding_id: binding.binding_id,
                        binding_revision: binding.revision,
                        session_id: None,
                    }
                },
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

fn start(store: &MonitorStore, spec: StartSpec, now_epoch: i64) -> MonitorStatus {
    let prepared = prepare_start(store, &spec, now_epoch);
    start_prepared(store, &prepared, now_epoch).expect("start monitor")
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
    seed_zero_sgd_spend(&store, "acct-age", NOW);
    let started = start(
        &store,
        config(
            "acct-age",
            "goal-age",
            None,
            Some(Money::new(5_000, "SGD", 2)),
        ),
        NOW,
    );
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
    seed_zero_sgd_spend(&store, "acct-fields", NOW);
    let started = start(
        &store,
        config(
            "acct-fields",
            "goal-fields",
            None,
            Some(Money::new(5_000, "SGD", 2)),
        ),
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
    seed_zero_sgd_spend(&store, "acct-reset-pair", NOW);
    let started = start(
        &store,
        config(
            "acct-reset-pair",
            "goal-reset-pair",
            None,
            Some(Money::new(5_000, "SGD", 2)),
        ),
        NOW,
    );
    let deadline = reset + 60;
    store.tick(deadline).expect("tick at reset deadline");
    assert!(has_issue(
        &status(&store, &started.monitor_id, deadline),
        MonitorIssueCode::ResetDueUnverified
    ));

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
    assert_eq!(expired.model.as_deref(), Some("claude-sonnet"));
    assert_eq!(
        expired.model_evidence.as_ref().unwrap().freshness,
        MonitorEvidenceFreshness::Stale
    );
    assert_eq!(
        expired.model_guard_validity,
        MonitorModelGuardValidity::Unknown
    );
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
    seed_zero_sgd_spend(&store, "acct-clock", NOW);
    let started = start(
        &store,
        config(
            "acct-clock",
            "goal-clock",
            None,
            Some(Money::new(5_000, "SGD", 2)),
        ),
        NOW,
    );
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
fn same_goal_recreation_keeps_cumulative_spend_and_rejects_identity_or_unapproved_policy() {
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
    let original_spec = config(
        "acct-goal",
        "durable-goal",
        None,
        Some(Money::new(5_000, "SGD", 2)),
    );
    let original_setup = prepare_start(&store, &original_spec, NOW);
    let original = start_prepared(&store, &original_setup, NOW)
        .expect("activate strict goal with fresh verified spend");
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
    let recreated_setup = PreparedStart {
        config: original_setup.config.clone(),
        idempotency_key: "fixture-start-durable-goal-run-2".to_owned(),
    };
    let recreated = start_prepared(&reopened, &recreated_setup, NOW + 3)
        .expect("new run key recreates stopped monitor without resetting spend");
    assert_ne!(recreated.monitor_id, original.monitor_id);
    assert_eq!(
        recreated.cumulative_goal_spend,
        Some(Money::new(1_200, "SGD", 2))
    );

    let wrong_binding = bind_account(&reopened, "acct-other", NOW + 4);
    let error = reopened
        .operate(
            MonitorOperation::ApprovePolicy {
                approval: MonitorPolicyApprovalInput {
                    binding_id: wrong_binding.binding_id,
                    binding_revision: wrong_binding.revision,
                    goal_id: "durable-goal".to_owned(),
                    new_policy: MonitorPolicy::StrictSgd,
                    budget: Some(Money::new(5_000, "SGD", 2)),
                    operator_label: "isolated-test-operator".to_owned(),
                    operator_confirmed: true,
                    acknowledge_no_sgd_cap: false,
                    expected_revision: None,
                },
            },
            NOW + 4,
        )
        .expect_err("same goal policy cannot move to a different bound account");
    assert_eq!(error.code, MonitorIssueCode::AccountMismatch);

    let mut unapproved_policy_revision = recreated_setup.clone();
    unapproved_policy_revision.config.policy_revision = Some(
        unapproved_policy_revision
            .config
            .policy_revision
            .expect("strict run has an approved policy revision")
            .saturating_add(1),
    );
    unapproved_policy_revision.idempotency_key =
        "fixture-start-durable-goal-unapproved-revision".to_owned();
    let error = start_prepared(&reopened, &unapproved_policy_revision, NOW + 4)
        .expect_err("a start cannot silently select a different policy revision");
    assert_eq!(error.code, MonitorIssueCode::PolicyRequired);

    let preserved = status(&reopened, &recreated.monitor_id, NOW + 4);
    assert_eq!(
        preserved.cumulative_goal_spend,
        Some(Money::new(1_200, "SGD", 2))
    );
}

#[test]
fn strict_activation_without_a_start_baseline_is_atomic_and_does_not_reserve_its_key() {
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
    let goal_spec = config(
        "acct-no-baseline",
        "goal-no-baseline",
        None,
        Some(Money::new(5_000, "SGD", 2)),
    );
    let prepared = prepare_start(&store, &goal_spec, NOW);
    let state_path = std::path::Path::new(directory.path())
        .join(crate::BROKER_DIR)
        .join("monitor")
        .join("state.json");
    let before_failed_start = std::fs::read(&state_path).expect("read state after setup");
    let error = start_prepared(&store, &prepared, NOW + 1)
        .expect_err("strict activation requires a fresh verified SGD baseline");
    assert_eq!(error.code, MonitorIssueCode::BudgetUnverifiable);
    let after_failed_start = std::fs::read(&state_path).expect("read state after rejected start");
    assert_eq!(after_failed_start, before_failed_start);

    record_spend(
        &store,
        spend_input(
            "acct-no-baseline",
            NOW - 100,
            NOW + 10_000,
            1_000,
            NOW + 2,
            "SGD",
        ),
        NOW + 2,
    );
    let started = start_prepared(&store, &prepared, NOW + 2)
        .expect("same retry key succeeds after current spend evidence arrives");
    assert_eq!(started.goal_id.as_deref(), Some("goal-no-baseline"));
    assert_eq!(started.cumulative_goal_spend, Some(Money::new(0, "SGD", 2)));

    let replayed = start_prepared(&store, &prepared, NOW + 3)
        .expect("successful retry remains idempotent for the same key and config");
    assert_eq!(replayed.monitor_id, started.monitor_id);
}

#[test]
fn sgd_spend_thresholds_emit_actions_and_align_runnable_readiness() {
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
        assert_spend_threshold_status(&current, amount, (warn, checkpoint, stop, pause));
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

fn assert_spend_threshold_status(
    status: &MonitorStatus,
    amount: i64,
    expectations: (bool, bool, bool, bool),
) {
    let (warn, checkpoint, stop, pause) = expectations;
    assert_eq!(
        status.runnable,
        !stop && !pause,
        "spend {amount} minor units"
    );
    assert_eq!(
        status.readiness.dispatch,
        if stop || pause {
            MonitorDispatchReadiness::Blocked
        } else {
            MonitorDispatchReadiness::Ready
        },
        "spend {amount} minor units"
    );
    assert_eq!(
        actions(status).iter().any(|action| matches!(
            action,
            MonitorAction::Warn {
                reason: MonitorIssueCode::BudgetWarn
            }
        )),
        warn,
        "spend {amount} minor units"
    );
    assert_eq!(has_checkpoint(status), checkpoint, "spend {amount}");
    assert_eq!(
        actions(status).iter().any(|action| matches!(
            action,
            MonitorAction::ReduceDispatch {
                max_parallel: Some(0)
            }
        )),
        stop,
        "spend {amount}"
    );
    assert_eq!(
        has_pause(status, MonitorIssueCode::BudgetPause),
        pause,
        "spend {amount}"
    );
}

#[test]
fn spend_snapshots_preserve_closed_period_markers_without_blocking_rollover() {
    let verified_record = |period_start, period_end, amount_minor| SpendRecord {
        account_id: "acct-snapshot".to_owned(),
        billing_period_start_epoch: period_start,
        billing_period_end_epoch: period_end,
        amount: Money::new(amount_minor, "SGD", 2),
        evidence_at_epoch: Some(period_end),
        evidence_received_at_epoch: period_end + 1,
        source: SpendRecordSource::OperatorReceipt,
        verification: SpendVerification::Verified,
    };
    let baseline = verified_record(100, 200, 1_000);
    let old_closed_period = verified_record(100, 200, 1_200);
    let first_period_anchor = verified_record(200, 300, 300);
    let snapshot = SpendState {
        baseline: Some(baseline.clone()),
        period_anchor: Some(first_period_anchor.clone()),
        cumulative_goal_spend: Some(Money::new(500, "SGD", 2)),
        rollover_unknown: false,
        cumulative_complete: true,
        closed_period_anchor: Some(old_closed_period.clone()),
    };

    let mut dropped_marker = snapshot.clone();
    dropped_marker.closed_period_anchor = None;
    assert!(!spend_snapshot_preserves_history(
        &snapshot,
        &dropped_marker
    ));

    let mut corrected_marker = snapshot.clone();
    corrected_marker.closed_period_anchor = Some(verified_record(100, 200, 1_300));
    corrected_marker.cumulative_goal_spend = Some(Money::new(600, "SGD", 2));
    assert!(spend_snapshot_preserves_history(
        &snapshot,
        &corrected_marker
    ));

    let mut regressed_marker = snapshot.clone();
    regressed_marker.closed_period_anchor = Some(verified_record(100, 200, 1_100));
    assert!(!spend_snapshot_preserves_history(
        &snapshot,
        &regressed_marker
    ));

    let mut wrong_account_marker = snapshot.clone();
    wrong_account_marker
        .closed_period_anchor
        .as_mut()
        .expect("snapshot has a closed-period marker")
        .account_id = "acct-other".to_owned();
    assert!(!super::spend_state_matches_account(
        &wrong_account_marker,
        "acct-snapshot"
    ));

    let mut later_rollover = snapshot.clone();
    later_rollover.closed_period_anchor = Some(verified_record(200, 300, 600));
    later_rollover.period_anchor = Some(verified_record(300, 400, 400));
    later_rollover.cumulative_goal_spend = Some(Money::new(1_200, "SGD", 2));
    assert!(spend_snapshot_preserves_history(&snapshot, &later_rollover));
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

        let prepared = prepare_start(
            &store,
            &config(
                &account_id,
                &goal_id,
                None,
                Some(Money::new(5_000, "SGD", 2)),
            ),
            NOW,
        );
        let error = start_prepared(&store, &prepared, NOW)
            .expect_err("strict SGD activation rejects unsupported currency evidence");
        assert_eq!(error.code, MonitorIssueCode::BudgetUnverifiable);
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

#[test]
fn used_only_without_a_principal_group_is_ignored_without_erasing_history() {
    use jackin_protocol::usage_broker::{
        UsageAccountV1, UsageFreshnessPhaseV1, UsageFreshnessV1, UsageIdentityKindV1,
        UsageLifecycleV1, UsageLimitWindowV1, UsageQuotaStateV1, UsageWindowCategoryV1,
    };

    let account = UsageAccountV1 {
        canonical_account_id: "acct-used-only".to_owned(),
        identity_kind: UsageIdentityKindV1::ProviderAccountId,
        rank: 0,
        display_label: "used-only account".to_owned(),
        plan_label: None,
        status_label: None,
        lifecycle: UsageLifecycleV1::Available,
        freshness: UsageFreshnessV1 {
            generation: 1,
            phase: UsageFreshnessPhaseV1::Current,
            last_good_at_epoch: Some(NOW),
            retry_at_epoch: None,
            is_stale: false,
        },
        provenance_count: 1,
        windows: vec![
            UsageLimitWindowV1 {
                window_id: "session".to_owned(),
                rank: 0,
                category: UsageWindowCategoryV1::Session,
                label: "session".to_owned(),
                value_label: "session value".to_owned(),
                reset_label: "session reset".to_owned(),
                remaining_percent: Some(RawPercentPoints::new(100.0).expect("raw remainder")),
                remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
                used_percent: None,
                used_percent_origin: None,
                meter_percent: Some(MeterPercentPoints::new(100.0).expect("meter geometry")),
                meter_side: Some(UsageMeterSide::Remaining),
                count_quota: None,
                provider_quota_state: None,
                money_used_percent: None,
                reset_at_epoch: None,
                quota_state: UsageQuotaStateV1::Available,
                pace_label: None,
                runs_out_label: None,
            },
            UsageLimitWindowV1 {
                window_id: "long-range".to_owned(),
                rank: 1,
                category: UsageWindowCategoryV1::LongRange,
                label: "long-range".to_owned(),
                value_label: "long-range value".to_owned(),
                reset_label: "long-range reset".to_owned(),
                remaining_percent: None,
                remaining_percent_origin: None,
                used_percent: Some(RawPercentPoints::new(42.0).expect("raw used")),
                used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
                meter_percent: Some(MeterPercentPoints::new(42.0).expect("meter geometry")),
                meter_side: Some(UsageMeterSide::Used),
                count_quota: None,
                provider_quota_state: None,
                money_used_percent: None,
                reset_at_epoch: None,
                quota_state: UsageQuotaStateV1::Available,
                pace_label: None,
                runs_out_label: None,
            },
        ],
        metric_groups: Vec::new(),
        credential_expires_at_epoch: None,
        issues: Vec::new(),
    };

    let windows = super::projection_windows(&account, NOW, 1);
    let session = windows[0].as_ref().expect("session projection");
    let session_used = session.used.as_ref().expect("session used");
    assert_eq!(session_used.value, 0);
    assert!(session_used.reset_at_epoch.is_none());
    assert!(session.reset.is_none());

    let long_range = windows[1].as_ref().expect("long-range projection");
    let long_range_used = long_range.used.as_ref().expect("long-range used");
    assert_eq!(long_range_used.value, 4_200);
    assert!(long_range_used.reset_at_epoch.is_none());

    let mut stored = super::AccountObservations::default();
    let prior_reset = NOW + 3_600;
    for index in 0..2 {
        stored.broker_windows[index].reset = Some(super::Observed {
            value: prior_reset,
            evidence_at_epoch: Some(NOW - 10),
            received_at_epoch: NOW - 10,
            input_sequence: 1,
            claude_code_version: None,
        });
        stored.broker_windows[index].paired = Some(super::ObservedQuotaPair {
            used_percentage_basis_points: 2_500,
            reset_at_epoch: prior_reset,
            evidence_at_epoch: Some(NOW - 10),
            source_times: [Some(NOW - 10), Some(NOW - 10)],
            received_at_epoch: NOW - 10,
            input_sequence: 1,
            claude_code_version: None,
        });
        stored.latest_reset_epochs[index] = Some(prior_reset);
    }

    let candidates = super::resetless::strict_resetless_candidates(&account, NOW, NOW);
    assert!(candidates.iter().all(Option::is_none));
    assert!(!super::update_broker_windows(
        &mut stored,
        windows,
        candidates,
        2,
        NOW,
    ));

    let stored_session = &stored.broker_windows[0];
    assert!(stored_session.used.is_none());
    assert_eq!(
        stored_session.reset.as_ref().map(|reset| reset.value),
        Some(prior_reset)
    );
    assert_eq!(
        stored_session
            .paired
            .as_ref()
            .map(|pair| pair.reset_at_epoch),
        Some(prior_reset)
    );
    assert_eq!(stored.latest_reset_epochs[0], Some(prior_reset));

    let stored_long_range = &stored.broker_windows[1];
    assert!(stored_long_range.used.is_none());
    assert_eq!(
        stored_long_range.reset.as_ref().map(|reset| reset.value),
        Some(prior_reset)
    );
    assert_eq!(
        stored_long_range
            .paired
            .as_ref()
            .map(|pair| pair.reset_at_epoch),
        Some(prior_reset)
    );
    assert_eq!(stored.latest_reset_epochs[1], Some(prior_reset));
}

#[derive(Clone, Copy)]
enum PrincipalScopeAxis {
    Service,
    Model,
    Pool,
    KeyId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WeeklyWindowEvidence {
    used_basis_points: Option<i32>,
    used_at: Option<i64>,
    used_received_at: Option<i64>,
    reset_at: Option<i64>,
    reset_evidence_at: Option<i64>,
    reset_received_at: Option<i64>,
}

fn projected_weekly_evidence_at(
    projection: &UsageProjectionV1,
    received_at: i64,
) -> (Option<i64>, Option<i64>) {
    let windows = super::projection_windows(&projection.providers[0].accounts[0], received_at, 1);
    let window = windows[1].as_ref();
    (
        window
            .and_then(|window| window.used.as_ref())
            .and_then(|used| used.evidence_at_epoch),
        window
            .and_then(|window| window.reset.as_ref())
            .and_then(|reset| reset.evidence_at_epoch),
    )
}

fn weekly_window_evidence(store: &MonitorStore, account_id: &str) -> WeeklyWindowEvidence {
    let state = store.lock();
    let window = &state.accounts[account_id].broker_windows[1];
    WeeklyWindowEvidence {
        used_basis_points: window.used.as_ref().map(|used| used.value),
        used_at: window.used.as_ref().and_then(|used| used.evidence_at_epoch),
        used_received_at: window.used.as_ref().map(|used| used.received_at_epoch),
        reset_at: window.reset.as_ref().map(|reset| reset.value),
        reset_evidence_at: window
            .reset
            .as_ref()
            .and_then(|reset| reset.evidence_at_epoch),
        reset_received_at: window.reset.as_ref().map(|reset| reset.received_at_epoch),
    }
}

fn assert_weekly_status_evidence(
    status: &MonitorStatus,
    evidence: &WeeklyWindowEvidence,
    received_at: i64,
) {
    assert_eq!(
        status.seven_day.used_percentage_basis_points,
        evidence.used_basis_points
    );
    assert_eq!(status.seven_day.reset_at_epoch, evidence.reset_at);
    let used = status
        .seven_day
        .used_evidence
        .as_ref()
        .expect("weekly used evidence");
    assert_eq!(used.evidence_at_epoch, evidence.used_at);
    assert_eq!(used.evidence_received_at_epoch, received_at);
    let reset = status
        .seven_day
        .reset_evidence
        .as_ref()
        .expect("weekly reset evidence");
    assert_eq!(reset.evidence_at_epoch, evidence.reset_evidence_at);
    assert_eq!(reset.evidence_received_at_epoch, received_at);
}

fn scoped_metric_scope(axis: PrincipalScopeAxis) -> UsageMetricScopeV1 {
    let mut scope = UsageMetricScopeV1::default();
    match axis {
        PrincipalScopeAxis::Service => scope.service = Some("anthropic".to_owned()),
        PrincipalScopeAxis::Model => scope.model = Some("fable".to_owned()),
        PrincipalScopeAxis::Pool => scope.pool = Some("fable-pool".to_owned()),
        PrincipalScopeAxis::KeyId => scope.key_id = Some("fable-key".to_owned()),
    }
    scope
}

struct PrincipalWindowGroupSpec<'a> {
    group_id: &'a str,
    rank: u32,
    label: &'a str,
    used_raw_percent: i32,
    reset_at_epoch: Option<i64>,
    observed_at_epoch: Option<i64>,
    last_success_at_epoch: Option<i64>,
    scope: UsageMetricScopeV1,
    current: bool,
}

fn principal_window_group(spec: PrincipalWindowGroupSpec<'_>) -> UsageMetricGroupV1 {
    let PrincipalWindowGroupSpec {
        group_id,
        rank,
        label,
        used_raw_percent,
        reset_at_epoch,
        observed_at_epoch,
        last_success_at_epoch,
        scope,
        current,
    } = spec;
    let fetched_at_epoch = observed_at_epoch.or(last_success_at_epoch).unwrap_or(NOW);
    UsageMetricGroupV1 {
        group_id: group_id.to_owned(),
        rank,
        kind: UsageMetricGroupKindV1::Window,
        label: label.to_owned(),
        scope,
        observed_at_epoch,
        fetched_at_epoch,
        last_success_at_epoch,
        phase: if current {
            UsageFreshnessPhaseV1::Current
        } else {
            UsageFreshnessPhaseV1::Failed
        },
        is_stale: !current,
        quota_state: UsageQuotaStateV1::Available,
        value: UsageMetricValueV1::Window {
            remaining_percent: None,
            remaining_percent_origin: None,
            used_percent: Some(
                RawPercentPoints::new(f64::from(used_raw_percent)).expect("raw used percent"),
            ),
            used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
            meter_percent: Some(
                RawPercentPoints::new(f64::from(used_raw_percent))
                    .expect("raw used percent")
                    .meter_geometry(),
            ),
            meter_side: Some(UsageMeterSide::Used),
            count_quota: None,
            provider_quota_state: None,
            money_used_percent: None,
            period: UsageMetricPeriodV1::Calendar {
                granularity: UsageCalendarPeriodV1::Weekly,
            },
            unit: None,
        },
        reset_at_epoch,
        renews_at_epoch: None,
        issues: Vec::new(),
    }
}

fn long_range_projection(
    account_id: &str,
    generation: u64,
    window_label: &str,
    used_raw_percent: i32,
    reset_at_epoch: Option<i64>,
    last_good_at_epoch: i64,
    mut groups: Vec<UsageMetricGroupV1>,
) -> UsageProjectionV1 {
    for (rank, group) in groups.iter_mut().enumerate() {
        group.rank = u32::try_from(rank).expect("group rank");
    }
    let freshness = UsageFreshnessV1 {
        generation,
        phase: UsageFreshnessPhaseV1::Current,
        last_good_at_epoch: Some(last_good_at_epoch),
        retry_at_epoch: None,
        is_stale: false,
    };
    UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV2,
        projection_id: format!("principal-window-{generation}"),
        generated_at_epoch: last_good_at_epoch,
        discovery_revision: "fixture-discovery".to_owned(),
        broker_instance_id: "fixture-broker".to_owned(),
        broker_generation: generation,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: vec![UsageProviderV1 {
            provider_id: "claude".to_owned(),
            display_name: "Claude".to_owned(),
            rank: 0,
            membership_state: UsageMembershipStateV1::Current,
            freshness: freshness.clone(),
            accounts: vec![UsageAccountV1 {
                canonical_account_id: account_id.to_owned(),
                identity_kind: UsageIdentityKindV1::ProviderAccountId,
                rank: 0,
                display_label: "principal account".to_owned(),
                plan_label: None,
                status_label: Some("available".to_owned()),
                lifecycle: UsageLifecycleV1::Available,
                freshness,
                provenance_count: 1,
                windows: vec![UsageLimitWindowV1 {
                    window_id: "weekly".to_owned(),
                    rank: 0,
                    category: UsageWindowCategoryV1::LongRange,
                    label: window_label.to_owned(),
                    value_label: format!("{used_raw_percent}% used"),
                    reset_label: "reset time".to_owned(),
                    remaining_percent: None,
                    remaining_percent_origin: None,
                    used_percent: Some(
                        RawPercentPoints::new(f64::from(used_raw_percent))
                            .expect("raw used percent"),
                    ),
                    used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
                    meter_percent: Some(
                        RawPercentPoints::new(f64::from(used_raw_percent))
                            .expect("raw used percent")
                            .meter_geometry(),
                    ),
                    meter_side: Some(UsageMeterSide::Used),
                    count_quota: None,
                    provider_quota_state: None,
                    money_used_percent: None,
                    reset_at_epoch,
                    quota_state: UsageQuotaStateV1::Available,
                    pace_label: None,
                    runs_out_label: None,
                }],
                metric_groups: groups,
                credential_expires_at_epoch: None,
                issues: Vec::new(),
            }],
            issues: Vec::new(),
        }],
        unresolved: Vec::new(),
        issues: Vec::new(),
    }
}

struct QuotaProjectionSpec<'a> {
    account_id: &'a str,
    generation: u64,
    category: UsageWindowCategoryV1,
    label: &'a str,
    used_points: Option<f64>,
    reset_at_epoch: Option<i64>,
    source_at_epoch: i64,
    last_good_at_epoch: i64,
}

fn quota_projection(spec: QuotaProjectionSpec<'_>) -> UsageProjectionV1 {
    let QuotaProjectionSpec {
        account_id,
        generation,
        category,
        label,
        used_points,
        reset_at_epoch,
        source_at_epoch,
        last_good_at_epoch,
    } = spec;
    let seed = used_points.unwrap_or(0.0).trunc() as i32;
    let group = principal_window_group(PrincipalWindowGroupSpec {
        group_id: "principal-quota",
        rank: 0,
        label,
        used_raw_percent: seed,
        reset_at_epoch,
        observed_at_epoch: Some(source_at_epoch),
        last_success_at_epoch: Some(source_at_epoch),
        scope: UsageMetricScopeV1::default(),
        current: true,
    });
    let mut projection = long_range_projection(
        account_id,
        generation,
        label,
        seed,
        reset_at_epoch,
        last_good_at_epoch,
        vec![group],
    );
    projection.generated_at_epoch = source_at_epoch;
    let account = &mut projection.providers[0].accounts[0];
    account.freshness.last_good_at_epoch = Some(last_good_at_epoch);
    let window = &mut account.windows[0];
    window.category = category;
    window.label = label.to_owned();
    window.value_label = used_points.map_or_else(String::new, |value| format!("{value}% used"));
    window.used_percent =
        used_points.map(|value| RawPercentPoints::new(value).expect("finite raw usage percentage"));
    window.used_percent_origin = used_points.map(|_| PercentObservationOrigin::ProviderReported);
    window.meter_percent = used_points.map(|value| {
        RawPercentPoints::new(value)
            .expect("finite raw usage percentage")
            .meter_geometry()
    });
    window.meter_side = used_points.map(|_| UsageMeterSide::Used);
    window.reset_at_epoch = reset_at_epoch;
    let group = &mut account.metric_groups[0];
    group.label = label.to_owned();
    group.observed_at_epoch = Some(source_at_epoch);
    group.last_success_at_epoch = Some(source_at_epoch);
    group.fetched_at_epoch = source_at_epoch;
    group.reset_at_epoch = reset_at_epoch;
    if let UsageMetricValueV1::Window {
        used_percent,
        used_percent_origin,
        remaining_percent,
        remaining_percent_origin,
        money_used_percent,
        period,
        ..
    } = &mut group.value
    {
        *used_percent = used_points
            .map(|value| RawPercentPoints::new(value).expect("finite raw usage percentage"));
        *used_percent_origin = used_points.map(|_| PercentObservationOrigin::ProviderReported);
        *remaining_percent = None;
        *remaining_percent_origin = None;
        *money_used_percent = None;
        *period = match category {
            UsageWindowCategoryV1::Session => UsageMetricPeriodV1::Calendar {
                granularity: UsageCalendarPeriodV1::Daily,
            },
            UsageWindowCategoryV1::LongRange => UsageMetricPeriodV1::Calendar {
                granularity: UsageCalendarPeriodV1::Weekly,
            },
            _ => UsageMetricPeriodV1::Unknown,
        };
    }
    projection
}

type BrokerUsedIdentity = (i32, Option<i64>, i64, u64);
type BrokerResetIdentity = (i64, Option<i64>, i64, u64);
type BrokerPairIdentity = (i32, i64, u64);
type BrokerWindowIdentity = (
    Option<BrokerUsedIdentity>,
    Option<BrokerResetIdentity>,
    Option<BrokerPairIdentity>,
);

fn broker_window_identity(window: &super::ObservedWindow) -> BrokerWindowIdentity {
    (
        window.used.as_ref().map(|used| {
            (
                used.value,
                used.evidence_at_epoch,
                used.received_at_epoch,
                used.input_sequence,
            )
        }),
        window.reset.as_ref().map(|reset| {
            (
                reset.value,
                reset.evidence_at_epoch,
                reset.received_at_epoch,
                reset.input_sequence,
            )
        }),
        window.paired.as_ref().map(|pair| {
            (
                pair.used_percentage_basis_points,
                pair.reset_at_epoch,
                pair.input_sequence,
            )
        }),
    )
}

struct ResetlessProviderSnapshot<'a> {
    account_id: &'a str,
    monitor_id: &'a str,
    index: usize,
    reset_at_epoch: i64,
    reset_identity: Option<BrokerResetIdentity>,
    reset_admission_fence: super::ResetAdmissionFence,
    sibling_identity: BrokerWindowIdentity,
}

fn assert_initial_resetless_orphan(
    store: &MonitorStore,
    snapshot: &ResetlessProviderSnapshot<'_>,
    orphan_at: i64,
) -> u64 {
    let first_identity = {
        let state = store.lock();
        let account = &state.accounts[snapshot.account_id];
        let used = account.broker_windows[snapshot.index]
            .used
            .as_ref()
            .expect("stored resetless usage");
        assert_eq!(used.value, 0);
        assert_eq!(used.reset_at_epoch, None);
        assert_eq!(used.evidence_at_epoch, Some(orphan_at));
        assert_eq!(used.received_at_epoch, orphan_at);
        assert_eq!(
            account.used_only_admission_sequences[snapshot.index],
            Some(used.input_sequence)
        );
        assert!(account.broker_windows[snapshot.index].paired.is_none());
        assert_eq!(
            broker_window_identity(&account.broker_windows[snapshot.index]).1,
            snapshot.reset_identity
        );
        assert_eq!(
            account.latest_reset_epochs[snapshot.index],
            Some(snapshot.reset_at_epoch)
        );
        assert_eq!(
            account.reset_admission_fences[snapshot.index],
            snapshot.reset_admission_fence
        );
        assert_eq!(
            broker_window_identity(&account.broker_windows[1 - snapshot.index]),
            snapshot.sibling_identity
        );
        used.input_sequence
    };
    let first_status = status(store, snapshot.monitor_id, orphan_at);
    let first_window = if snapshot.index == 0 {
        &first_status.five_hour
    } else {
        &first_status.seven_day
    };
    assert_eq!(first_window.used_percentage_basis_points, Some(0));
    assert_eq!(first_window.reset_at_epoch, Some(snapshot.reset_at_epoch));
    assert_eq!(first_status.readiness.quota, MonitorQuotaReadiness::Unknown);
    assert!(!first_status.runnable);
    assert!(has_issue(&first_status, MonitorIssueCode::MissingReset));
    assert_eq!(
        first_window
            .used_evidence
            .as_ref()
            .and_then(|evidence| evidence.evidence_at_epoch),
        Some(orphan_at)
    );
    first_identity
}

fn assert_newer_resetless_orphan(
    store: &MonitorStore,
    snapshot: &ResetlessProviderSnapshot<'_>,
    first_identity: u64,
    newer_at: i64,
) {
    let state = store.lock();
    let account = &state.accounts[snapshot.account_id];
    let used = account.broker_windows[snapshot.index]
        .used
        .as_ref()
        .expect("updated resetless usage");
    assert_eq!(used.value, 0);
    assert_eq!(used.evidence_at_epoch, Some(newer_at));
    assert_eq!(used.received_at_epoch, newer_at);
    assert!(used.input_sequence > first_identity);
    assert_eq!(
        broker_window_identity(&account.broker_windows[snapshot.index]).1,
        snapshot.reset_identity
    );
    assert_eq!(
        account.reset_admission_fences[snapshot.index],
        snapshot.reset_admission_fence
    );
    assert_eq!(
        broker_window_identity(&account.broker_windows[1 - snapshot.index]),
        snapshot.sibling_identity
    );
    assert!(account.broker_windows[snapshot.index].paired.is_none());
}

fn assert_stale_resetless_orphan(
    store: &MonitorStore,
    snapshot: &ResetlessProviderSnapshot<'_>,
    stale_at: i64,
) {
    let stale = status(store, snapshot.monitor_id, stale_at);
    let stale_window = if snapshot.index == 0 {
        &stale.five_hour
    } else {
        &stale.seven_day
    };
    assert_eq!(stale.readiness.quota, MonitorQuotaReadiness::Unknown);
    assert!(!stale.runnable);
    assert!(has_issue(&stale, MonitorIssueCode::QuotaStale));
    assert!(has_issue(&stale, MonitorIssueCode::MissingReset));
    assert_eq!(
        stale_window
            .used_evidence
            .as_ref()
            .map(|evidence| evidence.freshness),
        Some(MonitorEvidenceFreshness::Stale)
    );
    let state = store.lock();
    let account = &state.accounts[snapshot.account_id];
    assert!(super::resetless::broker_orphan_is_admissible(
        account,
        snapshot.index,
        state.last_now_epoch,
    ));
    assert_eq!(
        broker_window_identity(&account.broker_windows[snapshot.index]).1,
        snapshot.reset_identity
    );
    assert_eq!(
        account.reset_admission_fences[snapshot.index],
        snapshot.reset_admission_fence
    );
    assert_eq!(
        broker_window_identity(&account.broker_windows[1 - snapshot.index]),
        snapshot.sibling_identity
    );
}

#[test]
fn fresh_resetless_provider_usage_is_visible_but_stays_unpaired_for_both_windows() {
    for (category, label, index) in [
        (UsageWindowCategoryV1::Session, "session", 0),
        (UsageWindowCategoryV1::LongRange, "weekly", 1),
    ] {
        let account_id = format!("acct-resetless-{label}");
        let (_directory, store) = open_store();
        let started = start(
            &store,
            config(&account_id, "goal-resetless-provider", None, None),
            NOW,
        );
        let reset_at_epoch = NOW + 3_600;
        let baseline = quota_projection(QuotaProjectionSpec {
            account_id: &account_id,
            generation: 1,
            category,
            label,
            used_points: Some(54.0),
            reset_at_epoch: Some(reset_at_epoch),
            source_at_epoch: NOW,
            last_good_at_epoch: NOW,
        });
        store
            .observe_projection(&baseline, NOW)
            .expect("store the paired baseline");

        let refresh_at = NOW + 250;
        let reset_refresh = quota_projection(QuotaProjectionSpec {
            account_id: &account_id,
            generation: 2,
            category,
            label,
            used_points: None,
            reset_at_epoch: Some(reset_at_epoch),
            source_at_epoch: refresh_at,
            last_good_at_epoch: refresh_at,
        });
        store
            .observe_projection(&reset_refresh, refresh_at)
            .expect("refresh the independent reset source and receipt");
        let (reset_identity, reset_admission_fence, sibling_identity) = {
            let state = store.lock();
            let account = &state.accounts[&account_id];
            (
                broker_window_identity(&account.broker_windows[index]).1,
                account.reset_admission_fences[index].clone(),
                broker_window_identity(&account.broker_windows[1 - index]),
            )
        };
        let snapshot = ResetlessProviderSnapshot {
            account_id: &account_id,
            monitor_id: &started.monitor_id,
            index,
            reset_at_epoch,
            reset_identity,
            reset_admission_fence,
            sibling_identity,
        };

        let orphan_at = NOW + 301;
        let orphan = quota_projection(QuotaProjectionSpec {
            account_id: &account_id,
            generation: 3,
            category,
            label,
            used_points: Some(0.0),
            reset_at_epoch: None,
            source_at_epoch: orphan_at,
            last_good_at_epoch: orphan_at,
        });
        store
            .observe_projection(&orphan, orphan_at)
            .expect("admit a strictly newer resetless sample");
        let first_identity = assert_initial_resetless_orphan(&store, &snapshot, orphan_at);

        let newer_at = NOW + 302;
        let newer_same_value = quota_projection(QuotaProjectionSpec {
            account_id: &account_id,
            generation: 4,
            category,
            label,
            used_points: Some(0.0),
            reset_at_epoch: None,
            source_at_epoch: newer_at,
            last_good_at_epoch: newer_at,
        });
        store
            .observe_projection(&newer_same_value, newer_at)
            .expect("accept a genuinely newer same-value sample");
        assert_newer_resetless_orphan(&store, &snapshot, first_identity, newer_at);

        let stale_at = newer_at + super::MONITOR_EVIDENCE_TTL_SECS + 1;
        store
            .tick(stale_at)
            .expect("age resetless evidence past its TTL");
        assert_stale_resetless_orphan(&store, &snapshot, stale_at);
    }
}

#[test]
fn known_none_resetless_orphans_require_source_and_keep_statusline_receipt_only_pairs() {
    for (category, label, index) in [
        (UsageWindowCategoryV1::Session, "session", 0),
        (UsageWindowCategoryV1::LongRange, "weekly", 1),
    ] {
        let account_id = format!("acct-resetless-known-none-{label}");
        let (directory, store) = open_store();
        let _started = start(
            &store,
            config(&account_id, "goal-resetless-known-none", None, None),
            NOW,
        );
        let projection = quota_projection(QuotaProjectionSpec {
            account_id: &account_id,
            generation: 1,
            category,
            label,
            used_points: Some(54.0),
            reset_at_epoch: None,
            source_at_epoch: NOW,
            last_good_at_epoch: NOW,
        });
        store
            .observe_projection(&projection, NOW)
            .expect("admit a strict resetless projection with no reset history");

        let admitted = store.lock().clone();
        let account = &admitted.accounts[&account_id];
        assert_eq!(account.latest_reset_epochs[index], None);
        assert_eq!(
            account.reset_admission_fences[index],
            super::ResetAdmissionFence::KnownNone
        );
        assert!(account.broker_windows[index].reset.is_none());
        assert!(account.broker_windows[index].paired.is_none());
        let used = account.broker_windows[index]
            .used
            .as_ref()
            .expect("strict resetless used evidence");
        assert_eq!(used.evidence_at_epoch, Some(NOW));
        assert_eq!(
            account.used_only_admission_sequences[index],
            Some(used.input_sequence)
        );
        assert!(super::resetless::broker_orphan_is_admissible(
            account,
            index,
            admitted.last_now_epoch
        ));
        super::validate_store_state(&admitted)
            .expect("strictly admitted source-bearing resetless state is valid");

        super::storage::save(&store.inner.directory, &admitted)
            .expect("save the strictly admitted resetless state");
        let reopened = super::storage::load(&store.inner.directory)
            .expect("load the saved resetless state")
            .expect("saved resetless state exists");
        assert!(super::resetless::broker_orphan_is_admissible(
            &reopened.accounts[&account_id],
            index,
            reopened.last_now_epoch
        ));

        let stale_at = NOW + super::MONITOR_EVIDENCE_TTL_SECS + 1;
        store
            .tick(stale_at)
            .expect("age resetless evidence past its TTL");
        let historical = store.lock().clone();
        let historical_account = &historical.accounts[&account_id];
        assert!(super::resetless::broker_orphan_is_admissible(
            historical_account,
            index,
            historical.last_now_epoch
        ));
        super::validate_store_state(&historical)
            .expect("an admitted orphan remains structurally valid after aging out");

        let mut source_removed = historical.clone();
        let used = source_removed
            .accounts
            .get_mut(&account_id)
            .expect("cloned account")
            .broker_windows[index]
            .used
            .as_mut()
            .expect("stored resetless usage");
        assert!(used.evidence_at_epoch.is_some());
        used.evidence_at_epoch = None;
        let malformed_account = &source_removed.accounts[&account_id];
        assert!(super::resetless::valid_observed_percentage(
            malformed_account.broker_windows[index]
                .used
                .as_ref()
                .expect("source-removed resetless usage"),
            malformed_account.input_sequence,
            source_removed.last_now_epoch
        ));
        assert!(!super::resetless::broker_orphan_is_admissible(
            malformed_account,
            index,
            source_removed.last_now_epoch
        ));
        assert!(super::validate_store_state(&source_removed).is_err());

        super::storage::save(&store.inner.directory, &source_removed)
            .expect("persist the one-field malformed source proof for load testing");
        let state_path = std::path::Path::new(directory.path())
            .join(crate::BROKER_DIR)
            .join("monitor")
            .join("state.json");
        let source_bytes =
            std::fs::read(&state_path).expect("read malformed persisted source bytes");
        super::storage::load(&store.inner.directory)
            .expect_err("malformed source-less resetless orphan must fail load");
        assert_eq!(
            std::fs::read(&state_path).expect("read rejected persisted source bytes"),
            source_bytes,
            "rejecting the malformed v5 orphan must not rewrite its source bytes"
        );
    }

    let (_directory, statusline_store) = open_store();
    let statusline_account_id = "acct-resetless-receipt-only-statusline";
    ingest(
        &statusline_store,
        statusline_account_id,
        observation(
            "receipt-only-session",
            None,
            (Some(5_400), Some(NOW + 3_600)),
            (Some(7_200), Some(NOW + 7 * 24 * 60 * 60)),
        ),
        NOW,
    );
    let statusline_state = statusline_store.lock().clone();
    let statusline_account = &statusline_state.accounts[statusline_account_id];
    let session = &statusline_account.sessions["receipt-only-session"];
    for window in &session.windows {
        let used = window.used.as_ref().expect("statusline used evidence");
        assert_eq!(used.evidence_at_epoch, None);
        let pair = window
            .paired
            .as_ref()
            .expect("receipt-only statusline pair");
        assert_eq!(pair.source_times, [None, None]);
        assert!(super::resetless::valid_statusline_observed_window(
            window,
            statusline_account.input_sequence,
            statusline_state.last_now_epoch
        ));
    }
    super::validate_store_state(&statusline_state)
        .expect("receipt-only statusline pairs remain valid");
}

fn paired_statusline_account_fixture(
    account_id: &str,
    session_id: &str,
) -> (tempfile::TempDir, MonitorStore, super::AccountObservations) {
    let (directory, store) = open_store();
    let _started = start(
        &store,
        config(account_id, "goal-resetless-statusline-fixture", None, None),
        NOW,
    );
    ingest(
        &store,
        account_id,
        observation(
            session_id,
            None,
            (Some(5_400), Some(NOW + 3_600)),
            (Some(7_200), Some(NOW + 7 * 24 * 60 * 60)),
        ),
        NOW,
    );
    let account = store.lock().accounts[account_id].clone();
    (directory, store, account)
}

fn assert_receipt_only_statusline_window(
    account: &super::AccountObservations,
    session_id: &str,
    index: usize,
    now_epoch: i64,
) -> serde_json::Value {
    let window = &account.sessions[session_id].windows[index];
    assert_eq!(
        window
            .used
            .as_ref()
            .expect("statusline used field")
            .evidence_at_epoch,
        None
    );
    assert_eq!(
        window
            .reset
            .as_ref()
            .expect("statusline reset field")
            .evidence_at_epoch,
        None
    );
    assert_eq!(
        window
            .paired
            .as_ref()
            .expect("receipt-only statusline pair")
            .source_times,
        [None, None]
    );
    assert!(super::resetless::valid_statusline_observed_window(
        window,
        account.input_sequence,
        now_epoch
    ));
    assert!(!super::resetless::valid_broker_observed_window(
        window,
        account.input_sequence,
        now_epoch
    ));
    serde_json::to_value(window).expect("serialize receipt-only statusline window")
}

fn resetless_candidate(
    source_at_epoch: i64,
    received_at_epoch: i64,
    input_sequence: u64,
) -> super::ObservedPercentage {
    super::ObservedPercentage {
        value: 5_400,
        reset_at_epoch: None,
        evidence_at_epoch: Some(source_at_epoch),
        received_at_epoch,
        input_sequence,
        claude_code_version: None,
    }
}

fn assert_used_only_rejection_preserves_state(
    account: &super::AccountObservations,
    candidate: &super::ObservedPercentage,
    now_epoch: i64,
    reason: &str,
) {
    let before = serde_json::to_value(account).expect("serialize admission state preimage");
    assert!(
        !super::resetless::used_only_is_admissible(
            account,
            0,
            candidate,
            candidate.input_sequence,
            now_epoch
        ),
        "{reason}"
    );
    let after = serde_json::to_value(account).expect("serialize admission state postimage");
    assert_eq!(after, before, "rejected admission preserves saved state");
}

fn assert_invalid_statusline_pair_shapes_rejected(
    account: &super::AccountObservations,
    session_id: &str,
    candidate: &super::ObservedPercentage,
    now_epoch: i64,
) {
    let mut incoherent_pair = account.clone();
    incoherent_pair
        .sessions
        .get_mut(session_id)
        .expect("cloned statusline session")
        .windows[0]
        .paired
        .as_mut()
        .expect("cloned statusline pair")
        .used_percentage_basis_points += 1;
    let incoherent_window = &incoherent_pair.sessions[session_id].windows[0];
    assert!(!super::resetless::valid_statusline_observed_window(
        incoherent_window,
        incoherent_pair.input_sequence,
        now_epoch
    ));

    let mut wrong_source_domain = account.clone();
    let wrong_domain_window = &mut wrong_source_domain
        .sessions
        .get_mut(session_id)
        .expect("cloned statusline session")
        .windows[0];
    wrong_domain_window
        .used
        .as_mut()
        .expect("statusline used field")
        .evidence_at_epoch = Some(NOW);
    wrong_domain_window
        .reset
        .as_mut()
        .expect("statusline reset field")
        .evidence_at_epoch = Some(NOW);
    let wrong_domain_pair = wrong_domain_window
        .paired
        .as_mut()
        .expect("statusline pair");
    wrong_domain_pair.evidence_at_epoch = Some(NOW);
    wrong_domain_pair.source_times = [Some(NOW), Some(NOW)];
    assert!(super::resetless::valid_broker_observed_window(
        wrong_domain_window,
        wrong_source_domain.input_sequence,
        now_epoch
    ));

    for (reason, variant) in [
        ("incoherent statusline pair is rejected", incoherent_pair),
        (
            "source-bearing broker pair is rejected in the statusline domain",
            wrong_source_domain,
        ),
    ] {
        let window = &variant.sessions[session_id].windows[0];
        assert!(!super::resetless::valid_statusline_observed_window(
            window,
            variant.input_sequence,
            now_epoch
        ));
        assert_used_only_rejection_preserves_state(&variant, candidate, now_epoch, reason);
    }
}

#[test]
fn paired_statusline_session_allows_newer_resetless_projection_without_pair_promotion() {
    for (category, label, index) in [
        (UsageWindowCategoryV1::Session, "session", 0),
        (UsageWindowCategoryV1::LongRange, "weekly", 1),
    ] {
        let account_id = format!("acct-resetless-statusline-first-{label}");
        let (_directory, store, before_account) =
            paired_statusline_account_fixture(&account_id, "statusline-before-resetless");
        let statusline_window_before = assert_receipt_only_statusline_window(
            &before_account,
            "statusline-before-resetless",
            index,
            NOW,
        );
        let reset_high_water_before = before_account.latest_reset_epochs;
        let reset_fences_before = before_account.reset_admission_fences.clone();
        let sibling_reset_before =
            broker_window_identity(&before_account.broker_windows[1 - index]).1;
        let broker_reset_before = broker_window_identity(&before_account.broker_windows[index]).1;
        let sibling_marker_before = before_account.used_only_admission_sequences[1 - index];
        let input_sequence_before = before_account.input_sequence;

        let projection_at = NOW + 10;
        let resetless_projection = quota_projection(QuotaProjectionSpec {
            account_id: &account_id,
            generation: 1,
            category,
            label,
            used_points: Some(54.0),
            reset_at_epoch: None,
            source_at_epoch: projection_at,
            last_good_at_epoch: projection_at,
        });
        store
            .observe_projection(&resetless_projection, projection_at)
            .expect("admit a newer resetless projection after statusline session evidence");

        let after = store.lock().clone();
        let account = &after.accounts[&account_id];
        let used = account.broker_windows[index]
            .used
            .as_ref()
            .expect("source-bearing resetless broker usage");
        assert_eq!(used.value, 5_400);
        assert_eq!(used.evidence_at_epoch, Some(projection_at));
        assert_eq!(used.received_at_epoch, projection_at);
        assert!(used.input_sequence > input_sequence_before);
        assert_eq!(used.input_sequence, account.input_sequence);
        assert_eq!(
            account.used_only_admission_sequences[index],
            Some(used.input_sequence)
        );
        assert_eq!(account.latest_reset_epochs, reset_high_water_before);
        assert_eq!(account.reset_admission_fences, reset_fences_before);
        assert_eq!(
            broker_window_identity(&account.broker_windows[index]).1,
            broker_reset_before
        );
        assert_eq!(
            broker_window_identity(&account.broker_windows[1 - index]).1,
            sibling_reset_before
        );
        assert_eq!(
            account.used_only_admission_sequences[1 - index],
            sibling_marker_before
        );
        assert!(account.broker_windows[index].paired.is_none());
        assert_eq!(
            serde_json::to_value(&account.sessions["statusline-before-resetless"].windows[index])
                .expect("serialize retained statusline window"),
            statusline_window_before
        );
        super::validate_store_state(&after)
            .expect("statusline-first resetless state remains valid");
    }
}

#[test]
fn invalid_statusline_session_blocks_resetless_projection_without_state_change() {
    let account_id = "acct-resetless-invalid-statusline-session";
    let (_directory, store, paired_account) =
        paired_statusline_account_fixture(account_id, "statusline-paired-session");
    let newer_candidate =
        resetless_candidate(NOW + 12, NOW + 12, paired_account.input_sequence + 1);
    assert_receipt_only_statusline_window(
        &paired_account,
        "statusline-paired-session",
        0,
        NOW + 12,
    );
    assert!(super::resetless::used_only_is_admissible(
        &paired_account,
        0,
        &newer_candidate,
        newer_candidate.input_sequence,
        NOW + 12
    ));
    assert_invalid_statusline_pair_shapes_rejected(
        &paired_account,
        "statusline-paired-session",
        &newer_candidate,
        NOW + 12,
    );

    let fallback_session = "statusline-paired-session";
    ingest(
        &store,
        account_id,
        observation(fallback_session, None, (Some(6_000), None), (None, None)),
        NOW + 10,
    );
    let stale_account = store.lock().accounts[account_id].clone();
    let stale_window = &stale_account.sessions[fallback_session].windows[0];
    let retained_used = stale_window
        .used
        .as_ref()
        .expect("retained statusline used sample");
    assert_eq!(retained_used.value, 6_000);
    assert_eq!(retained_used.evidence_at_epoch, None);
    assert_eq!(retained_used.reset_at_epoch, None);
    assert_eq!(retained_used.received_at_epoch, NOW + 10);
    assert_eq!(retained_used.input_sequence, stale_account.input_sequence);
    assert!(retained_used.input_sequence > paired_account.input_sequence);
    assert!(stale_window.paired.is_none());
    let original_reset = paired_account.sessions["statusline-paired-session"].windows[0]
        .reset
        .as_ref()
        .expect("original paired statusline reset");
    assert_eq!(original_reset.value, NOW + 3_600);
    assert_eq!(
        stale_window.reset.as_ref().map(|reset| reset.value),
        Some(original_reset.value)
    );
    assert_eq!(
        stale_account.latest_reset_epochs,
        paired_account.latest_reset_epochs
    );
    assert_eq!(
        stale_account.reset_admission_fences,
        paired_account.reset_admission_fences
    );
    assert!(super::resetless::valid_statusline_observed_window(
        stale_window,
        stale_account.input_sequence,
        NOW + 12
    ));
    let stale_candidate = resetless_candidate(
        retained_used.received_at_epoch,
        NOW + 12,
        stale_account.input_sequence + 1,
    );
    // A source-less statusline usage uses its receipt as the ordering boundary.
    assert_eq!(
        stale_candidate.evidence_at_epoch,
        Some(retained_used.received_at_epoch)
    );
    assert_used_only_rejection_preserves_state(
        &stale_account,
        &stale_candidate,
        NOW + 12,
        "a candidate equal to the retained receipt fallback is stale",
    );
    let newer_source_epoch = retained_used.received_at_epoch + 1;
    let newer_source_candidate = resetless_candidate(
        newer_source_epoch,
        stale_candidate.received_at_epoch,
        stale_candidate.input_sequence,
    );
    assert_eq!(
        newer_source_candidate.evidence_at_epoch,
        Some(retained_used.received_at_epoch + 1)
    );
    assert!(super::resetless::used_only_is_admissible(
        &stale_account,
        0,
        &newer_source_candidate,
        newer_source_candidate.input_sequence,
        NOW + 12
    ));
}

fn assert_exact_raw_candidate(exact: &UsageProjectionV1) {
    let account = &exact.providers[0].accounts[0];
    let candidate = super::resetless::strict_resetless_candidates(account, NOW, NOW)[1]
        .expect("exact fractional raw values are admissible");
    assert_eq!(candidate.value, 5_412);
    assert_eq!(
        monitor_basis_points(RawPercentPoints::new(54.124).expect("same rounded value")),
        Some(candidate.value),
        "the counterexample intentionally rounds to the same monitor basis points"
    );
}

fn assert_fractional_raw_candidates() {
    for raw in [54.125, 0.125, 95.005] {
        let expected =
            monitor_basis_points(RawPercentPoints::new(raw).expect("finite fractional percentage"))
                .expect("nonnegative raw percentage converts at the monitor boundary");
        let projection = quota_projection(QuotaProjectionSpec {
            account_id: "acct-resetless-fractional",
            generation: 10,
            category: UsageWindowCategoryV1::Session,
            label: "session",
            used_points: Some(raw),
            reset_at_epoch: None,
            source_at_epoch: NOW,
            last_good_at_epoch: NOW,
        });
        let admitted = super::resetless::strict_resetless_candidates(
            &projection.providers[0].accounts[0],
            NOW,
            NOW,
        )[0]
        .expect("fractional raw source equality remains admissible");
        assert_eq!(admitted.value, expected);
    }
}

fn assert_raw_and_cardinality_rejections(exact: &UsageProjectionV1) {
    let mut raw_mismatch = exact.clone();
    let UsageMetricValueV1::Window { used_percent, .. } =
        &mut raw_mismatch.providers[0].accounts[0].metric_groups[0].value
    else {
        unreachable!("principal group contains a window value");
    };
    *used_percent = Some(RawPercentPoints::new(54.124).expect("fractional raw value"));
    assert!(
        super::resetless::strict_resetless_candidates(
            &raw_mismatch.providers[0].accounts[0],
            NOW,
            NOW,
        )[1]
        .is_none()
    );

    let mut missing_group = exact.clone();
    missing_group.providers[0].accounts[0].metric_groups.clear();
    assert!(
        super::resetless::strict_resetless_candidates(
            &missing_group.providers[0].accounts[0],
            NOW,
            NOW,
        )[1]
        .is_none()
    );

    let mut ambiguous = exact.clone();
    let mut duplicate = ambiguous.providers[0].accounts[0].metric_groups[0].clone();
    duplicate.group_id = "second-principal-window".to_owned();
    if let UsageMetricValueV1::Window { used_percent, .. } = &mut duplicate.value {
        *used_percent = Some(RawPercentPoints::new(55.0).expect("different principal raw"));
    }
    ambiguous.providers[0].accounts[0]
        .metric_groups
        .push(duplicate);
    assert!(
        super::resetless::strict_resetless_candidates(
            &ambiguous.providers[0].accounts[0],
            NOW,
            NOW,
        )[1]
        .is_none()
    );

    let mut same_value_ambiguous = exact.clone();
    let mut duplicate = same_value_ambiguous.providers[0].accounts[0].metric_groups[0].clone();
    duplicate.group_id = "duplicate-same-value".to_owned();
    same_value_ambiguous.providers[0].accounts[0]
        .metric_groups
        .push(duplicate);
    assert!(
        super::resetless::strict_resetless_candidates(
            &same_value_ambiguous.providers[0].accounts[0],
            NOW,
            NOW,
        )[1]
        .is_none()
    );

    let mut duplicate_window = exact.clone();
    let mut second_window = duplicate_window.providers[0].accounts[0].windows[0].clone();
    second_window.window_id = "ambiguous-second-window".to_owned();
    duplicate_window.providers[0].accounts[0]
        .windows
        .push(second_window);
    assert!(
        super::resetless::strict_resetless_candidates(
            &duplicate_window.providers[0].accounts[0],
            NOW,
            NOW,
        )[1]
        .is_none()
    );
}

fn assert_principal_scope_rejections(exact: &UsageProjectionV1) {
    let mut scoped = exact.clone();
    scoped.providers[0].accounts[0].metric_groups[0]
        .scope
        .service = Some("anthropic".to_owned());
    assert!(
        super::resetless::strict_resetless_candidates(&scoped.providers[0].accounts[0], NOW, NOW,)
            [1]
        .is_none()
    );

    for axis in ["model", "pool", "key_id"] {
        let mut scoped = exact.clone();
        match axis {
            "model" => {
                scoped.providers[0].accounts[0].metric_groups[0].scope.model =
                    Some("sonnet".to_owned());
            }
            "pool" => {
                scoped.providers[0].accounts[0].metric_groups[0].scope.pool =
                    Some("work".to_owned());
            }
            _ => {
                scoped.providers[0].accounts[0].metric_groups[0]
                    .scope
                    .key_id = Some("key-1".to_owned());
            }
        }
        assert!(
            super::resetless::strict_resetless_candidates(
                &scoped.providers[0].accounts[0],
                NOW,
                NOW,
            )[1]
            .is_none()
        );
    }
}

fn assert_principal_shape_rejections(exact: &UsageProjectionV1) {
    let mut mismatched_label = exact.clone();
    mismatched_label.providers[0].accounts[0].metric_groups[0].label = "other".to_owned();
    assert!(
        super::resetless::strict_resetless_candidates(
            &mismatched_label.providers[0].accounts[0],
            NOW,
            NOW,
        )[1]
        .is_none()
    );

    let mut mismatched_period = exact.clone();
    let UsageMetricValueV1::Window { period, .. } =
        &mut mismatched_period.providers[0].accounts[0].metric_groups[0].value
    else {
        unreachable!("principal group contains a window value");
    };
    *period = UsageMetricPeriodV1::Unknown;
    assert!(
        super::resetless::strict_resetless_candidates(
            &mismatched_period.providers[0].accounts[0],
            NOW,
            NOW,
        )[1]
        .is_none()
    );

    let mut mismatched_reset = exact.clone();
    mismatched_reset.providers[0].accounts[0].metric_groups[0].reset_at_epoch = Some(NOW + 1);
    assert!(
        super::resetless::strict_resetless_candidates(
            &mismatched_reset.providers[0].accounts[0],
            NOW,
            NOW,
        )[1]
        .is_none()
    );

    let mut stale_group = exact.clone();
    stale_group.providers[0].accounts[0].metric_groups[0].is_stale = true;
    assert!(
        super::resetless::strict_resetless_candidates(
            &stale_group.providers[0].accounts[0],
            NOW,
            NOW,
        )[1]
        .is_none()
    );
}

fn assert_source_time_and_negative_rejections() {
    for source_at_epoch in [NOW - 301, NOW + 61] {
        let out_of_bounds = quota_projection(QuotaProjectionSpec {
            account_id: "acct-resetless-time",
            generation: 2,
            category: UsageWindowCategoryV1::Session,
            label: "session",
            used_points: Some(0.125),
            reset_at_epoch: None,
            source_at_epoch,
            last_good_at_epoch: NOW,
        });
        assert!(
            super::resetless::strict_resetless_candidates(
                &out_of_bounds.providers[0].accounts[0],
                NOW,
                NOW,
            )[0]
            .is_none()
        );
    }

    let negative = quota_projection(QuotaProjectionSpec {
        account_id: "acct-resetless-negative",
        generation: 3,
        category: UsageWindowCategoryV1::Session,
        label: "session",
        used_points: Some(-0.125),
        reset_at_epoch: None,
        source_at_epoch: NOW,
        last_good_at_epoch: NOW,
    });
    assert!(super::resetless::strict_resetless_candidates(
        &negative.providers[0].accounts[0],
        NOW,
        NOW,
    )[0]
        .is_none());
}

#[test]
fn resetless_raw_matching_is_exact_and_requires_one_fresh_unscoped_principal_group() {
    let exact = quota_projection(QuotaProjectionSpec {
        account_id: "acct-resetless-raw",
        generation: 1,
        category: UsageWindowCategoryV1::LongRange,
        label: "weekly",
        used_points: Some(54.121),
        reset_at_epoch: None,
        source_at_epoch: NOW,
        last_good_at_epoch: NOW,
    });
    assert_exact_raw_candidate(&exact);

    assert_fractional_raw_candidates();

    assert_raw_and_cardinality_rejections(&exact);

    assert_principal_scope_rejections(&exact);

    assert_principal_shape_rejections(&exact);

    assert_source_time_and_negative_rejections();
}

#[test]
fn resetless_principal_period_classes_match_session_and_long_range_symmetrically() {
    let cases = [
        (
            UsageWindowCategoryV1::Session,
            "session",
            0,
            UsageMetricPeriodV1::Rolling { window_secs: 3_600 },
        ),
        (
            UsageWindowCategoryV1::Session,
            "session",
            0,
            UsageMetricPeriodV1::Calendar {
                granularity: UsageCalendarPeriodV1::Daily,
            },
        ),
        (
            UsageWindowCategoryV1::Session,
            "session",
            0,
            UsageMetricPeriodV1::ProviderDefined,
        ),
        (
            UsageWindowCategoryV1::LongRange,
            "weekly",
            1,
            UsageMetricPeriodV1::Rolling {
                window_secs: 7 * 24 * 60 * 60,
            },
        ),
        (
            UsageWindowCategoryV1::LongRange,
            "weekly",
            1,
            UsageMetricPeriodV1::Calendar {
                granularity: UsageCalendarPeriodV1::Weekly,
            },
        ),
        (
            UsageWindowCategoryV1::LongRange,
            "monthly",
            1,
            UsageMetricPeriodV1::Calendar {
                granularity: UsageCalendarPeriodV1::Monthly,
            },
        ),
    ];
    for (category, label, index, period) in cases {
        let mut projection = quota_projection(QuotaProjectionSpec {
            account_id: "acct-resetless-period-class",
            generation: 1,
            category,
            label,
            used_points: Some(54.125),
            reset_at_epoch: None,
            source_at_epoch: NOW,
            last_good_at_epoch: NOW,
        });
        let UsageMetricValueV1::Window {
            period: group_period,
            ..
        } = &mut projection.providers[0].accounts[0].metric_groups[0].value
        else {
            unreachable!("principal group contains a window value");
        };
        *group_period = period;
        assert!(
            super::resetless::strict_resetless_candidates(
                &projection.providers[0].accounts[0],
                NOW,
                NOW,
            )[index]
                .is_some()
        );
    }

    for (category, label, index, period) in [
        (
            UsageWindowCategoryV1::Session,
            "session",
            0,
            UsageMetricPeriodV1::Rolling {
                window_secs: 7 * 24 * 60 * 60,
            },
        ),
        (
            UsageWindowCategoryV1::LongRange,
            "weekly",
            1,
            UsageMetricPeriodV1::Calendar {
                granularity: UsageCalendarPeriodV1::Daily,
            },
        ),
        (
            UsageWindowCategoryV1::LongRange,
            "weekly",
            1,
            UsageMetricPeriodV1::Rolling {
                window_secs: 86_400,
            },
        ),
    ] {
        let mut projection = quota_projection(QuotaProjectionSpec {
            account_id: "acct-resetless-period-mismatch",
            generation: 2,
            category,
            label,
            used_points: Some(54.125),
            reset_at_epoch: None,
            source_at_epoch: NOW,
            last_good_at_epoch: NOW,
        });
        let UsageMetricValueV1::Window {
            period: group_period,
            ..
        } = &mut projection.providers[0].accounts[0].metric_groups[0].value
        else {
            unreachable!("principal group contains a window value");
        };
        *group_period = period;
        assert!(
            super::resetless::strict_resetless_candidates(
                &projection.providers[0].accounts[0],
                NOW,
                NOW,
            )[index]
                .is_none()
        );
    }
}

#[test]
fn resetless_admission_orders_against_source_receipt_and_sequence_fences() {
    let mut account = super::AccountObservations {
        input_sequence: 7,
        ..super::AccountObservations::default()
    };
    account.latest_reset_epochs[0] = Some(NOW + 3_600);
    account.reset_admission_fences[0] = super::ResetAdmissionFence::Known {
        reset_at_epoch: NOW + 3_600,
        source_after_epoch: NOW,
        receipt_after_epoch: NOW,
        input_sequence: 7,
    };
    account.broker_windows[0].used = Some(super::ObservedPercentage {
        value: 5_400,
        reset_at_epoch: None,
        evidence_at_epoch: None,
        received_at_epoch: NOW,
        input_sequence: 7,
        claude_code_version: None,
    });
    let candidate =
        |source_at_epoch, received_at_epoch, input_sequence| super::ObservedPercentage {
            value: 0,
            reset_at_epoch: None,
            evidence_at_epoch: Some(source_at_epoch),
            received_at_epoch,
            input_sequence,
            claude_code_version: None,
        };

    assert!(super::resetless::used_only_is_admissible(
        &account,
        0,
        &candidate(NOW + 2, NOW + 2, 8),
        8,
        NOW + 2,
    ));
    assert!(!super::resetless::used_only_is_admissible(
        &account,
        0,
        &candidate(NOW, NOW + 3, 8),
        8,
        NOW + 3,
    ));
    assert!(!super::resetless::used_only_is_admissible(
        &account,
        0,
        &candidate(NOW + 4, NOW, 8),
        8,
        NOW,
    ));
    assert!(!super::resetless::used_only_is_admissible(
        &account,
        0,
        &candidate(NOW + 2, NOW + 2, 7),
        7,
        NOW + 2,
    ));
}

fn assert_pair_field_value_and_receipt_rejections(window: &super::ObservedWindow) {
    let mut separate_fields = window.clone();
    separate_fields.paired = None;
    assert!(super::resetless::exact_observed_pair(&separate_fields).is_none());

    let mut mismatched_value = window.clone();
    mismatched_value
        .paired
        .as_mut()
        .expect("pair")
        .used_percentage_basis_points += 1;
    assert!(super::resetless::exact_observed_pair(&mismatched_value).is_none());

    let mut mismatched_sequence = window.clone();
    mismatched_sequence
        .paired
        .as_mut()
        .expect("pair")
        .input_sequence += 1;
    assert!(super::resetless::exact_observed_pair(&mismatched_sequence).is_none());

    let mut mismatched_reset_sequence = window.clone();
    mismatched_reset_sequence
        .reset
        .as_mut()
        .expect("reset field")
        .input_sequence += 1;
    assert!(super::resetless::exact_observed_pair(&mismatched_reset_sequence).is_none());

    let mut mismatched_receipt = window.clone();
    mismatched_receipt
        .paired
        .as_mut()
        .expect("pair")
        .received_at_epoch += 1;
    assert!(super::resetless::exact_observed_pair(&mismatched_receipt).is_none());

    let mut mismatched_used_sequence = window.clone();
    mismatched_used_sequence
        .used
        .as_mut()
        .expect("used field")
        .input_sequence += 1;
    assert!(super::resetless::exact_observed_pair(&mismatched_used_sequence).is_none());

    let mut mismatched_reset_receipt = window.clone();
    mismatched_reset_receipt
        .reset
        .as_mut()
        .expect("reset field")
        .received_at_epoch += 1;
    assert!(super::resetless::exact_observed_pair(&mismatched_reset_receipt).is_none());
}

fn assert_pair_source_identity_rejections(window: &super::ObservedWindow) {
    let mut mismatched_source = window.clone();
    mismatched_source
        .reset
        .as_mut()
        .expect("reset field")
        .evidence_at_epoch = Some(NOW + 1);
    assert_eq!(
        mismatched_source
            .used
            .as_ref()
            .expect("used field")
            .evidence_at_epoch
            .unwrap()
            .min(
                mismatched_source
                    .reset
                    .as_ref()
                    .expect("reset field")
                    .evidence_at_epoch
                    .unwrap()
            ),
        window
            .paired
            .as_ref()
            .expect("pair")
            .evidence_at_epoch
            .unwrap(),
        "changing only the later reset source leaves the old minimum summary unchanged"
    );
    assert!(super::resetless::exact_observed_pair(&mismatched_source).is_none());

    let mut mismatched_used_source = window.clone();
    mismatched_used_source
        .used
        .as_mut()
        .expect("used field")
        .evidence_at_epoch = Some(NOW + 1);
    assert_eq!(
        mismatched_used_source
            .used
            .as_ref()
            .expect("used field")
            .evidence_at_epoch
            .unwrap()
            .min(
                mismatched_used_source
                    .reset
                    .as_ref()
                    .expect("reset field")
                    .evidence_at_epoch
                    .unwrap()
            ),
        window
            .paired
            .as_ref()
            .expect("pair")
            .evidence_at_epoch
            .unwrap(),
        "changing only the later used source leaves the old minimum summary unchanged"
    );
    assert!(super::resetless::exact_observed_pair(&mismatched_used_source).is_none());

    let mut mismatched_source_tuple = window.clone();
    mismatched_source_tuple
        .paired
        .as_mut()
        .expect("pair")
        .source_times[1] = Some(NOW + 1);
    assert!(super::resetless::exact_observed_pair(&mismatched_source_tuple).is_none());

    let mut mismatched_pair_summary = window.clone();
    mismatched_pair_summary
        .paired
        .as_mut()
        .expect("pair")
        .evidence_at_epoch = Some(NOW + 1);
    assert!(super::resetless::exact_observed_pair(&mismatched_pair_summary).is_none());

    let mut mismatched_version = window.clone();
    mismatched_version
        .reset
        .as_mut()
        .expect("reset field")
        .claude_code_version = Some("2.1.81".to_owned());
    assert!(super::resetless::exact_observed_pair(&mismatched_version).is_none());
}

fn assert_pair_freshness_and_store_validation(window: &super::ObservedWindow) {
    let stale_reset_source = NOW - super::MONITOR_EVIDENCE_TTL_SECS - 1;
    let mut stale_constituent = window.clone();
    stale_constituent
        .reset
        .as_mut()
        .expect("reset field")
        .evidence_at_epoch = Some(stale_reset_source);
    let stale_pair = stale_constituent.paired.as_mut().expect("pair");
    stale_pair.source_times[1] = Some(stale_reset_source);
    stale_pair.evidence_at_epoch = Some(stale_reset_source);
    assert!(super::resetless::exact_observed_pair(&stale_constituent).is_some());
    assert!(super::resetless::valid_broker_observed_window(
        &stale_constituent,
        8,
        NOW + 1,
    ));

    let mut future_constituent = window.clone();
    let future_reset_source = NOW + super::MAX_FUTURE_SKEW_SECS + super::MAX_FUTURE_SKEW_SECS + 1;
    future_constituent
        .reset
        .as_mut()
        .expect("reset field")
        .evidence_at_epoch = Some(future_reset_source);
    future_constituent
        .paired
        .as_mut()
        .expect("pair")
        .source_times[1] = Some(future_reset_source);
    assert!(super::resetless::exact_observed_pair(&future_constituent).is_some());
    assert!(!super::resetless::valid_broker_observed_window(
        &future_constituent,
        8,
        NOW + 1,
    ));

    let mut state = super::StoreState {
        last_now_epoch: NOW + 1,
        next_input_sequence: 8,
        ..super::StoreState::default()
    };
    let mut account = super::AccountObservations {
        input_sequence: 8,
        latest_reset_epochs: [Some(NOW + 3_600), None],
        reset_admission_fences: [
            super::ResetAdmissionFence::Known {
                reset_at_epoch: NOW + 3_600,
                source_after_epoch: NOW,
                receipt_after_epoch: NOW,
                input_sequence: 8,
            },
            super::ResetAdmissionFence::KnownNone,
        ],
        ..super::AccountObservations::default()
    };
    account.broker_windows[0] = window.clone();
    state
        .accounts
        .insert("acct-pair-validation".to_owned(), account);
    super::validation::validate_store_state(&state)
        .expect("a fully matched persisted pair passes store validation");

    let mut corrupt = state.clone();
    corrupt
        .accounts
        .get_mut("acct-pair-validation")
        .expect("persisted account")
        .broker_windows[0]
        .paired
        .as_mut()
        .expect("persisted pair")
        .used_percentage_basis_points += 1;
    assert!(super::validation::validate_store_state(&corrupt).is_err());

    let mut source_corrupt = state.clone();
    source_corrupt
        .accounts
        .get_mut("acct-pair-validation")
        .expect("persisted account")
        .broker_windows[0]
        .paired
        .as_mut()
        .expect("persisted pair")
        .source_times[1] = Some(NOW + 1);
    assert!(super::validation::validate_store_state(&source_corrupt).is_err());
}

#[test]
fn paired_quota_identity_requires_matching_values_source_receipt_and_sequence() {
    let pair = super::ObservedQuotaPair {
        used_percentage_basis_points: 2_000,
        reset_at_epoch: NOW + 3_600,
        evidence_at_epoch: Some(NOW),
        source_times: [Some(NOW), Some(NOW)],
        received_at_epoch: NOW,
        input_sequence: 8,
        claude_code_version: Some("2.1.80".to_owned()),
    };
    let window = super::ObservedWindow {
        used: Some(super::ObservedPercentage {
            value: 2_000,
            reset_at_epoch: Some(NOW + 3_600),
            evidence_at_epoch: Some(NOW),
            received_at_epoch: NOW,
            input_sequence: 8,
            claude_code_version: Some("2.1.80".to_owned()),
        }),
        reset: Some(super::Observed {
            value: NOW + 3_600,
            evidence_at_epoch: Some(NOW),
            received_at_epoch: NOW,
            input_sequence: 8,
            claude_code_version: Some("2.1.80".to_owned()),
        }),
        paired: Some(pair),
    };
    assert!(super::resetless::exact_observed_pair(&window).is_some());

    assert_pair_field_value_and_receipt_rejections(&window);

    assert_pair_source_identity_rejections(&window);

    assert_pair_freshness_and_store_validation(&window);
}

#[test]
fn schema5_pair_source_times_must_match_the_containing_window_origin() {
    let reset_at_epoch = NOW + 3_600;
    let broker_window = super::ObservedWindow {
        used: Some(super::ObservedPercentage {
            value: 2_000,
            reset_at_epoch: Some(reset_at_epoch),
            evidence_at_epoch: Some(NOW),
            received_at_epoch: NOW,
            input_sequence: 8,
            claude_code_version: None,
        }),
        reset: Some(super::Observed {
            value: reset_at_epoch,
            evidence_at_epoch: Some(NOW + 1),
            received_at_epoch: NOW,
            input_sequence: 8,
            claude_code_version: None,
        }),
        paired: Some(super::ObservedQuotaPair {
            used_percentage_basis_points: 2_000,
            reset_at_epoch,
            evidence_at_epoch: Some(NOW),
            source_times: [Some(NOW), Some(NOW + 1)],
            received_at_epoch: NOW,
            input_sequence: 8,
            claude_code_version: None,
        }),
    };
    let mut state = super::StoreState {
        last_now_epoch: NOW,
        next_input_sequence: 8,
        ..super::StoreState::default()
    };
    let mut account = super::AccountObservations {
        input_sequence: 8,
        latest_reset_epochs: [Some(reset_at_epoch), None],
        reset_admission_fences: [
            super::ResetAdmissionFence::Known {
                reset_at_epoch,
                source_after_epoch: NOW,
                receipt_after_epoch: NOW,
                input_sequence: 8,
            },
            super::ResetAdmissionFence::KnownNone,
        ],
        ..super::AccountObservations::default()
    };
    account.broker_windows[0] = broker_window.clone();
    state
        .accounts
        .insert("acct-origin-pair".to_owned(), account);
    super::validation::validate_store_state(&state)
        .expect("broker pair with two actual source times is valid");

    let mut receipt_only_broker = state.clone();
    let broker = &mut receipt_only_broker
        .accounts
        .get_mut("acct-origin-pair")
        .expect("persisted account")
        .broker_windows[0];
    broker.used.as_mut().expect("used field").evidence_at_epoch = None;
    broker
        .reset
        .as_mut()
        .expect("reset field")
        .evidence_at_epoch = None;
    let pair = broker.paired.as_mut().expect("pair");
    pair.source_times = [None, None];
    pair.evidence_at_epoch = None;
    assert!(super::resetless::exact_observed_pair(broker).is_some());
    assert!(super::validation::validate_store_state(&receipt_only_broker).is_err());

    let mut source_timed_statusline = state.clone();
    source_timed_statusline
        .accounts
        .get_mut("acct-origin-pair")
        .expect("persisted account")
        .sessions
        .insert(
            "session-origin-pair".to_owned(),
            super::SessionObservation {
                windows: [broker_window, super::ObservedWindow::default()],
                ..super::SessionObservation::default()
            },
        );
    assert!(super::validation::validate_store_state(&source_timed_statusline).is_err());
}

fn assert_receipt_only_pair_is_refused(
    account: &super::AccountObservations,
    monitor: &super::DurableMonitor,
    barrier_before: &impl Fn() -> super::AccountResetBarrier,
) {
    use jackin_protocol::usage_monitor::MonitorQuotaWindowStatus;
    let mut receipt_only_broker_pair = account.clone();
    let broker = &mut receipt_only_broker_pair.broker_windows[0];
    broker.used.as_mut().expect("used field").evidence_at_epoch = None;
    broker
        .reset
        .as_mut()
        .expect("reset field")
        .evidence_at_epoch = None;
    let pair = broker.paired.as_mut().expect("pair");
    pair.source_times = [None, None];
    pair.evidence_at_epoch = None;
    assert!(super::resetless::exact_observed_pair(broker).is_some());
    receipt_only_broker_pair.reset_barriers[0] = Some(barrier_before());
    assert!(!super::advance_account_reset_barriers(
        &mut receipt_only_broker_pair,
        NOW + 60,
    ));
    assert!(receipt_only_broker_pair.reset_barriers[0].is_some());

    let mut receipt_only_monitor = monitor.clone();
    receipt_only_monitor.reset_barriers[0] = Some(super::ResetBarrier {
        started_at_epoch: NOW,
        prior_reset_at_epoch: Some(NOW - 400),
        prior_used_percentage_basis_points: 9_500,
        evidence_sequence_before: 0,
        dependency_evidence_sequence: None,
        pause_reason: MonitorIssueCode::LimitGuardReached,
    });
    assert!(!super::lifecycle::update_reset_barrier(
        &mut receipt_only_monitor,
        Some(&receipt_only_broker_pair),
        0,
        &MonitorQuotaWindowStatus::default(),
        NOW + 60,
        &mut Vec::new(),
    ));
    assert!(receipt_only_monitor.reset_barriers[0].is_some());
}

fn assert_reset_source_mutation_cannot_release(
    account_before_release: &super::AccountObservations,
    monitor: &super::DurableMonitor,
) {
    use jackin_protocol::usage_monitor::MonitorQuotaWindowStatus;
    let mut mutated_account = account_before_release.clone();
    assert!(mutated_account.reset_barriers[0].is_some());
    mutated_account.broker_windows[0]
        .reset
        .as_mut()
        .expect("retained reset field")
        .evidence_at_epoch = Some(NOW + 2);
    assert!(super::resetless::exact_observed_pair(&mutated_account.broker_windows[0]).is_none());
    assert_eq!(
        mutated_account.broker_windows[0]
            .paired
            .as_ref()
            .expect("retained pair")
            .source_times,
        [Some(NOW), Some(NOW + 1)]
    );
    let mutated_account_before_release = serde_json::to_value(&mutated_account)
        .expect("serialize reset-source mutation before release");
    assert!(!super::advance_account_reset_barriers(
        &mut mutated_account,
        NOW + 60,
    ));
    assert!(mutated_account.reset_barriers[0].is_some());
    assert_eq!(
        serde_json::to_value(&mutated_account)
            .expect("serialize reset-source mutation after release"),
        mutated_account_before_release
    );

    let mut invalid_monitor = monitor.clone();
    invalid_monitor.reset_barriers[0] = Some(super::ResetBarrier {
        started_at_epoch: NOW,
        prior_reset_at_epoch: Some(NOW - 400),
        prior_used_percentage_basis_points: 9_500,
        evidence_sequence_before: 0,
        dependency_evidence_sequence: None,
        pause_reason: MonitorIssueCode::LimitGuardReached,
    });
    let mut invalid_issues = Vec::new();
    assert!(!super::lifecycle::update_reset_barrier(
        &mut invalid_monitor,
        Some(&mutated_account),
        0,
        &MonitorQuotaWindowStatus::default(),
        NOW + 60,
        &mut invalid_issues,
    ));
    assert!(invalid_monitor.reset_barriers[0].is_some());
}

fn assert_stale_source_cannot_release(
    account: &super::AccountObservations,
    monitor: &super::DurableMonitor,
) {
    use jackin_protocol::usage_monitor::{
        MonitorEvidenceSource, MonitorEvidenceValue, MonitorQuotaWindow, MonitorQuotaWindowStatus,
    };
    let stale_reset_source = NOW + 60 - super::MONITOR_EVIDENCE_TTL_SECS - 1;
    let mut stale_account = account.clone();
    stale_account.broker_windows[0]
        .reset
        .as_mut()
        .expect("reset field")
        .evidence_at_epoch = Some(stale_reset_source);
    let stale_pair = stale_account.broker_windows[0]
        .paired
        .as_mut()
        .expect("pair");
    stale_pair.source_times[1] = Some(stale_reset_source);
    stale_pair.evidence_at_epoch = Some(stale_reset_source);
    stale_account.reset_barriers[0] = Some(super::AccountResetBarrier {
        started_at_epoch: stale_reset_source,
        prior_reset_at_epoch: Some(stale_reset_source - super::MONITOR_RESET_GRACE_SECS),
        prior_used_percentage_basis_points: 9_500,
        input_sequence_before: 7,
        source: MonitorEvidenceSource::BrokerProjection,
        session_id: None,
        pause_reason: MonitorIssueCode::LimitGuardReached,
    });
    assert!(super::resetless::exact_observed_pair(&stale_account.broker_windows[0]).is_some());
    assert!(super::resetless::valid_broker_observed_window(
        &stale_account.broker_windows[0],
        8,
        NOW + 60,
    ));
    assert!(!super::advance_account_reset_barriers(
        &mut stale_account,
        NOW + 60
    ));
    assert!(stale_account.reset_barriers[0].is_some());

    let mut stale_monitor = monitor.clone();
    stale_monitor.reset_barriers[0] = Some(super::ResetBarrier {
        started_at_epoch: stale_reset_source,
        prior_reset_at_epoch: Some(stale_reset_source - super::MONITOR_RESET_GRACE_SECS),
        prior_used_percentage_basis_points: 9_500,
        evidence_sequence_before: 0,
        dependency_evidence_sequence: None,
        pause_reason: MonitorIssueCode::LimitGuardReached,
    });
    let stale_reset_evidence = stale_monitor
        .evidence
        .iter_mut()
        .find(|evidence| {
            matches!(
                evidence.value,
                MonitorEvidenceValue::QuotaReset {
                    window: MonitorQuotaWindow::FiveHour,
                    ..
                }
            )
        })
        .expect("broker reset evidence");
    stale_reset_evidence.evidence_at_epoch = Some(stale_reset_source);
    stale_reset_evidence.evidence_received_at_epoch = NOW + 60;
    assert!(!super::lifecycle::update_reset_barrier(
        &mut stale_monitor,
        Some(&stale_account),
        0,
        &MonitorQuotaWindowStatus::default(),
        NOW + 60,
        &mut Vec::new(),
    ));
    assert!(stale_monitor.reset_barriers[0].is_some());
}

fn assert_used_source_mutation_cannot_release(
    account_before_release: &super::AccountObservations,
    monitor: &super::DurableMonitor,
) {
    use jackin_protocol::usage_monitor::MonitorQuotaWindowStatus;
    let mut used_mutated_account = account_before_release.clone();
    assert!(used_mutated_account.reset_barriers[0].is_some());
    used_mutated_account.broker_windows[0]
        .used
        .as_mut()
        .expect("used field")
        .evidence_at_epoch = Some(NOW + 2);
    assert!(
        super::resetless::exact_observed_pair(&used_mutated_account.broker_windows[0]).is_none()
    );
    assert_eq!(
        used_mutated_account.broker_windows[0]
            .paired
            .as_ref()
            .expect("retained pair")
            .source_times,
        [Some(NOW), Some(NOW + 1)]
    );
    let used_mutated_account_before_release = serde_json::to_value(&used_mutated_account)
        .expect("serialize used-source mutation before release");
    assert!(!super::advance_account_reset_barriers(
        &mut used_mutated_account,
        NOW + 60,
    ));
    assert!(used_mutated_account.reset_barriers[0].is_some());
    assert_eq!(
        serde_json::to_value(&used_mutated_account)
            .expect("serialize used-source mutation after release"),
        used_mutated_account_before_release
    );

    let mut used_mutated_monitor = monitor.clone();
    used_mutated_monitor.reset_barriers[0] = Some(super::ResetBarrier {
        started_at_epoch: NOW,
        prior_reset_at_epoch: Some(NOW - 400),
        prior_used_percentage_basis_points: 9_500,
        evidence_sequence_before: 0,
        dependency_evidence_sequence: None,
        pause_reason: MonitorIssueCode::LimitGuardReached,
    });
    assert!(!super::lifecycle::update_reset_barrier(
        &mut used_mutated_monitor,
        Some(&used_mutated_account),
        0,
        &MonitorQuotaWindowStatus::default(),
        NOW + 60,
        &mut Vec::new(),
    ));
    assert!(used_mutated_monitor.reset_barriers[0].is_some());
}

fn assert_wrong_monitor_identity_cannot_release(
    account: &super::AccountObservations,
    monitor: &super::DurableMonitor,
) {
    use jackin_protocol::usage_monitor::{MonitorEvidenceSource, MonitorQuotaWindowStatus};
    for mismatch in ["account", "session", "source"] {
        let mut wrong_identity_monitor = monitor.clone();
        wrong_identity_monitor.reset_barriers[0] = Some(super::ResetBarrier {
            started_at_epoch: NOW,
            prior_reset_at_epoch: Some(NOW - 400),
            prior_used_percentage_basis_points: 9_500,
            evidence_sequence_before: 0,
            dependency_evidence_sequence: None,
            pause_reason: MonitorIssueCode::LimitGuardReached,
        });
        let evidence = wrong_identity_monitor
            .evidence
            .first_mut()
            .expect("projection usage evidence");
        match mismatch {
            "account" => evidence.account_id = Some("acct-other".to_owned()),
            "session" => evidence.session_id = Some("session-other".to_owned()),
            "source" => evidence.source = MonitorEvidenceSource::Statusline,
            _ => unreachable!("listed mismatch kind"),
        }
        assert!(!super::lifecycle::update_reset_barrier(
            &mut wrong_identity_monitor,
            Some(account),
            0,
            &MonitorQuotaWindowStatus::default(),
            NOW + 60,
            &mut Vec::new(),
        ));
        assert!(wrong_identity_monitor.reset_barriers[0].is_some());
    }
}

#[test]
fn unequal_source_pair_releases_both_barriers_but_later_source_mutation_blocks_both() {
    use jackin_protocol::usage_monitor::{MonitorEvidenceSource, MonitorQuotaWindowStatus};

    let account_id = "acct-unequal-pair-release";
    let reset_at_epoch = NOW + 3_600;
    let window = super::ObservedWindow {
        used: Some(super::ObservedPercentage {
            value: 2_000,
            reset_at_epoch: Some(reset_at_epoch),
            evidence_at_epoch: Some(NOW),
            received_at_epoch: NOW + 60,
            input_sequence: 8,
            claude_code_version: None,
        }),
        reset: Some(super::Observed {
            value: reset_at_epoch,
            evidence_at_epoch: Some(NOW + 1),
            received_at_epoch: NOW + 60,
            input_sequence: 8,
            claude_code_version: None,
        }),
        paired: Some(super::ObservedQuotaPair {
            used_percentage_basis_points: 2_000,
            reset_at_epoch,
            evidence_at_epoch: Some(NOW),
            source_times: [Some(NOW), Some(NOW + 1)],
            received_at_epoch: NOW + 60,
            input_sequence: 8,
            claude_code_version: None,
        }),
    };
    assert!(super::resetless::exact_observed_pair(&window).is_some());

    let barrier_before = || super::AccountResetBarrier {
        started_at_epoch: NOW,
        prior_reset_at_epoch: Some(NOW - 400),
        prior_used_percentage_basis_points: 9_500,
        input_sequence_before: 7,
        source: MonitorEvidenceSource::BrokerProjection,
        session_id: None,
        pause_reason: MonitorIssueCode::LimitGuardReached,
    };
    let mut account = super::AccountObservations {
        input_sequence: 8,
        latest_reset_epochs: [Some(reset_at_epoch), None],
        reset_admission_fences: [
            super::ResetAdmissionFence::Known {
                reset_at_epoch,
                source_after_epoch: NOW + 60,
                receipt_after_epoch: NOW + 60,
                input_sequence: 8,
            },
            super::ResetAdmissionFence::KnownNone,
        ],
        ..super::AccountObservations::default()
    };
    account.broker_windows[0] = window.clone();
    account.reset_barriers[0] = Some(barrier_before());
    let account_before_release = account.clone();
    assert!(super::advance_account_reset_barriers(
        &mut account,
        NOW + 60
    ));
    assert!(account.reset_barriers[0].is_none());

    let (_directory, store) = open_store();
    let started = start(
        &store,
        config(account_id, "goal-unequal-pair-release", None, None),
        NOW,
    );
    let mut monitor = store
        .lock()
        .monitors
        .get(&started.monitor_id)
        .expect("started monitor")
        .clone();
    let accounts = std::collections::BTreeMap::from([(account_id.to_owned(), account.clone())]);
    super::observations::sync_monitor_from_account(&accounts, account_id, &mut monitor, NOW + 60);
    monitor.reset_barriers[0] = Some(super::ResetBarrier {
        started_at_epoch: NOW,
        prior_reset_at_epoch: Some(NOW - 400),
        prior_used_percentage_basis_points: 9_500,
        evidence_sequence_before: 0,
        dependency_evidence_sequence: None,
        pause_reason: MonitorIssueCode::LimitGuardReached,
    });
    let mut valid_monitor = monitor.clone();
    let mut issues = Vec::new();
    assert!(super::lifecycle::update_reset_barrier(
        &mut valid_monitor,
        Some(&account),
        0,
        &MonitorQuotaWindowStatus::default(),
        NOW + 60,
        &mut issues,
    ));
    assert!(valid_monitor.reset_barriers[0].is_none());

    assert_receipt_only_pair_is_refused(&account, &monitor, &barrier_before);

    assert_reset_source_mutation_cannot_release(&account_before_release, &monitor);
    assert!(issues.is_empty());

    assert_stale_source_cannot_release(&account, &monitor);

    assert_used_source_mutation_cannot_release(&account_before_release, &monitor);

    assert_wrong_monitor_identity_cannot_release(&account, &monitor);
}

#[test]
fn unequal_source_projection_pair_survives_reopen_and_passes_both_release_paths() {
    use jackin_protocol::usage_monitor::MonitorEvidenceSource;

    let account_id = "acct-unequal-projection-pair";
    let reset_at_epoch = NOW + 3_600;
    let used_source = NOW + 10;
    let reset_source = NOW + 11;
    let mut projection = long_range_projection(
        account_id,
        1,
        "reset-source",
        40,
        Some(reset_at_epoch),
        NOW + 60,
        vec![principal_window_group(PrincipalWindowGroupSpec {
            group_id: "reset-source-group",
            rank: 0,
            label: "reset-source",
            used_raw_percent: 40,
            reset_at_epoch: Some(reset_at_epoch),
            observed_at_epoch: Some(reset_source),
            last_success_at_epoch: Some(reset_source),
            scope: UsageMetricScopeV1::default(),
            current: true,
        })],
    );
    let projected_account = &mut projection.providers[0].accounts[0];
    let mut used_window = projected_account.windows[0].clone();
    used_window.window_id = "used-source".to_owned();
    used_window.rank = 1;
    used_window.label = "used-source".to_owned();
    used_window.value_label = "72% used".to_owned();
    let used_raw = RawPercentPoints::new(72.0).expect("raw used percent");
    used_window.used_percent = Some(used_raw);
    used_window.meter_percent = Some(used_raw.meter_geometry());
    projected_account.windows.push(used_window);
    projected_account.metric_groups = vec![
        principal_window_group(PrincipalWindowGroupSpec {
            group_id: "reset-source-group",
            rank: 0,
            label: "reset-source",
            used_raw_percent: 40,
            reset_at_epoch: Some(reset_at_epoch),
            observed_at_epoch: Some(reset_source),
            last_success_at_epoch: Some(reset_source),
            scope: UsageMetricScopeV1::default(),
            current: true,
        }),
        principal_window_group(PrincipalWindowGroupSpec {
            group_id: "used-source-group",
            rank: 1,
            label: "used-source",
            used_raw_percent: 72,
            reset_at_epoch: Some(reset_at_epoch),
            observed_at_epoch: Some(used_source),
            last_success_at_epoch: Some(used_source),
            scope: UsageMetricScopeV1::default(),
            current: true,
        }),
    ];

    let projected = super::projection_windows(projected_account, NOW + 60, 1);
    let projected = projected[1].as_ref().expect("long-range aggregate");
    let projected_pair = super::resetless::exact_observed_pair(projected)
        .expect("same projection operation supplies the aggregate pair");
    assert_eq!(
        projected_pair.source_times,
        [Some(used_source), Some(reset_source)]
    );
    assert_eq!(projected_pair.evidence_at_epoch, Some(used_source));

    let (directory, store) = open_store();
    let started = start(
        &store,
        config(account_id, "goal-unequal-projection-pair", None, None),
        NOW,
    );
    store
        .observe_projection(&projection, NOW + 60)
        .expect("store the unequal-source aggregate");
    drop(store);

    let reopened = MonitorStore::open(directory.path()).expect("reopen projected pair");
    let (mut account, mut monitor) = {
        let state = reopened.lock();
        (
            state.accounts[account_id].clone(),
            state.monitors[&started.monitor_id].clone(),
        )
    };
    let stored_pair = super::resetless::exact_observed_pair(&account.broker_windows[1])
        .expect("reopened aggregate remains bound to both source times");
    assert_eq!(
        stored_pair.source_times,
        [Some(used_source), Some(reset_source)]
    );
    assert_eq!(stored_pair.evidence_at_epoch, Some(used_source));

    account.reset_barriers[1] = Some(super::AccountResetBarrier {
        started_at_epoch: NOW,
        prior_reset_at_epoch: Some(NOW - 400),
        prior_used_percentage_basis_points: 9_500,
        input_sequence_before: account.input_sequence.saturating_sub(1),
        source: MonitorEvidenceSource::BrokerProjection,
        session_id: None,
        pause_reason: MonitorIssueCode::LimitGuardReached,
    });
    assert!(super::advance_account_reset_barriers(
        &mut account,
        NOW + 60
    ));
    assert!(account.reset_barriers[1].is_none());

    monitor.reset_barriers[1] = Some(super::ResetBarrier {
        started_at_epoch: NOW,
        prior_reset_at_epoch: Some(NOW - 400),
        prior_used_percentage_basis_points: 9_500,
        evidence_sequence_before: 0,
        dependency_evidence_sequence: None,
        pause_reason: MonitorIssueCode::LimitGuardReached,
    });
    let mut issues = Vec::new();
    assert!(super::lifecycle::update_reset_barrier(
        &mut monitor,
        Some(&account),
        1,
        &jackin_protocol::usage_monitor::MonitorQuotaWindowStatus::default(),
        NOW + 60,
        &mut issues,
    ));
    assert!(monitor.reset_barriers[1].is_none());
}

#[test]
fn later_used_projection_does_not_repair_an_older_reset_sequence_into_a_pair() {
    use jackin_protocol::usage_monitor::MonitorEvidenceSource;

    let account_id = "acct-independent-projection-fields";
    let reset_at_epoch = NOW + 3_600;
    let (_directory, store) = open_store();
    let started = start(
        &store,
        config(account_id, "goal-independent-projection-fields", None, None),
        NOW,
    );
    let baseline = quota_projection(QuotaProjectionSpec {
        account_id,
        generation: 1,
        category: UsageWindowCategoryV1::LongRange,
        label: "weekly",
        used_points: Some(54.0),
        reset_at_epoch: Some(reset_at_epoch),
        source_at_epoch: NOW,
        last_good_at_epoch: NOW,
    });
    store
        .observe_projection(&baseline, NOW)
        .expect("store first complete callback");

    let reset_source = principal_window_group(PrincipalWindowGroupSpec {
        group_id: "retained-reset-source",
        rank: 0,
        label: "retained-reset-source",
        used_raw_percent: 40,
        reset_at_epoch: Some(reset_at_epoch),
        observed_at_epoch: Some(NOW),
        last_success_at_epoch: Some(NOW),
        scope: UsageMetricScopeV1::default(),
        current: true,
    });
    let used_source = principal_window_group(PrincipalWindowGroupSpec {
        group_id: "new-used-source",
        rank: 1,
        label: "new-used-source",
        used_raw_percent: 54,
        reset_at_epoch: Some(reset_at_epoch),
        observed_at_epoch: Some(NOW + 2),
        last_success_at_epoch: Some(NOW + 2),
        scope: UsageMetricScopeV1::default(),
        current: true,
    });
    let mut second = long_range_projection(
        account_id,
        2,
        "retained-reset-source",
        40,
        Some(reset_at_epoch),
        NOW + 2,
        vec![reset_source, used_source],
    );
    let projected_account = &mut second.providers[0].accounts[0];
    let mut used_window = projected_account.windows[0].clone();
    used_window.window_id = "new-used-source".to_owned();
    used_window.rank = 1;
    used_window.label = "new-used-source".to_owned();
    used_window.value_label = "54% used".to_owned();
    let used_raw = RawPercentPoints::new(54.0).expect("raw used percent");
    used_window.used_percent = Some(used_raw);
    used_window.meter_percent = Some(used_raw.meter_geometry());
    projected_account.windows.push(used_window);
    store
        .observe_projection(&second, NOW + 2)
        .expect("store newer used with an unrefreshed reset");

    let (mut account, mut monitor) = {
        let state = store.lock();
        (
            state.accounts[account_id].clone(),
            state.monitors[&started.monitor_id].clone(),
        )
    };
    let window = &account.broker_windows[1];
    assert_eq!(window.used.as_ref().map(|used| used.value), Some(5_400));
    assert_eq!(
        window.used.as_ref().map(|used| used.input_sequence),
        Some(2)
    );
    assert_eq!(
        window.used.as_ref().map(|used| used.received_at_epoch),
        Some(NOW + 2)
    );
    assert_eq!(
        window.reset.as_ref().map(|reset| reset.value),
        Some(reset_at_epoch)
    );
    assert_eq!(
        window.reset.as_ref().map(|reset| reset.input_sequence),
        Some(1)
    );
    assert_eq!(
        window.reset.as_ref().map(|reset| reset.received_at_epoch),
        Some(NOW)
    );
    assert!(window.paired.is_none());

    account.reset_barriers[1] = Some(super::AccountResetBarrier {
        started_at_epoch: NOW,
        prior_reset_at_epoch: Some(NOW - 400),
        prior_used_percentage_basis_points: 9_500,
        input_sequence_before: 1,
        source: MonitorEvidenceSource::BrokerProjection,
        session_id: None,
        pause_reason: MonitorIssueCode::LimitGuardReached,
    });
    assert!(!super::advance_account_reset_barriers(
        &mut account,
        NOW + 2
    ));
    assert!(account.reset_barriers[1].is_some());

    monitor.reset_barriers[1] = Some(super::ResetBarrier {
        started_at_epoch: NOW,
        prior_reset_at_epoch: Some(NOW - 400),
        prior_used_percentage_basis_points: 9_500,
        evidence_sequence_before: 0,
        dependency_evidence_sequence: None,
        pause_reason: MonitorIssueCode::LimitGuardReached,
    });
    assert!(!super::lifecycle::update_reset_barrier(
        &mut monitor,
        Some(&account),
        1,
        &jackin_protocol::usage_monitor::MonitorQuotaWindowStatus::default(),
        NOW + 2,
        &mut Vec::new(),
    ));
    assert!(monitor.reset_barriers[1].is_some());
}

#[test]
fn resetless_threshold_without_prior_reset_requires_a_new_post_grace_reset() {
    let mut account = super::AccountObservations {
        input_sequence: 8,
        ..super::AccountObservations::default()
    };
    account.reset_barriers[0] = Some(super::AccountResetBarrier {
        started_at_epoch: NOW,
        prior_reset_at_epoch: None,
        prior_used_percentage_basis_points: 9_500,
        input_sequence_before: 7,
        source: jackin_protocol::usage_monitor::MonitorEvidenceSource::BrokerProjection,
        session_id: None,
        pause_reason: MonitorIssueCode::LimitGuardReached,
    });
    account.broker_windows[0] = {
        let reset = NOW;
        super::ObservedWindow {
            used: Some(super::ObservedPercentage {
                value: 2_000,
                reset_at_epoch: Some(reset),
                evidence_at_epoch: Some(NOW + 60),
                received_at_epoch: NOW + 60,
                input_sequence: 8,
                claude_code_version: None,
            }),
            reset: Some(super::Observed {
                value: reset,
                evidence_at_epoch: Some(NOW + 60),
                received_at_epoch: NOW + 60,
                input_sequence: 8,
                claude_code_version: None,
            }),
            paired: Some(super::ObservedQuotaPair {
                used_percentage_basis_points: 2_000,
                reset_at_epoch: reset,
                evidence_at_epoch: Some(NOW + 60),
                source_times: [Some(NOW + 60), Some(NOW + 60)],
                received_at_epoch: NOW + 60,
                input_sequence: 8,
                claude_code_version: None,
            }),
        }
    };
    assert!(!super::advance_account_reset_barriers(
        &mut account,
        NOW + 60
    ));
    assert!(account.reset_barriers[0].is_some());

    account.broker_windows[0]
        .used
        .as_mut()
        .expect("used field")
        .reset_at_epoch = Some(NOW + 1);
    account.broker_windows[0]
        .reset
        .as_mut()
        .expect("reset field")
        .value = NOW + 1;
    account.broker_windows[0]
        .paired
        .as_mut()
        .expect("pair")
        .reset_at_epoch = NOW + 1;
    assert!(super::advance_account_reset_barriers(
        &mut account,
        NOW + 60
    ));
    assert!(account.reset_barriers[0].is_none());
}

#[test]
fn resetless_statusline_threshold_without_history_recovers_after_a_new_post_grace_pair() {
    let account_id = "acct-resetless-no-history";
    let (_directory, store) = open_store();
    let started = start(
        &store,
        config(account_id, "goal-resetless-no-history", None, None),
        NOW,
    );
    ingest(
        &store,
        account_id,
        observation(
            "session-resetless-no-history",
            None,
            (Some(9_500), None),
            (None, None),
        ),
        NOW,
    );
    {
        let state = store.lock();
        assert!(state.accounts[account_id].reset_barriers[0].is_some());
        assert!(state.monitors[&started.monitor_id].reset_barriers[0].is_some());
        assert!(
            state.monitors[&started.monitor_id].reset_barriers[0]
                .as_ref()
                .is_some_and(|barrier| barrier.prior_reset_at_epoch.is_none())
        );
    }

    let recovery_at = NOW + super::MONITOR_RESET_GRACE_SECS;
    store
        .tick(recovery_at)
        .expect("advance through resetless grace");
    let due = status(&store, &started.monitor_id, recovery_at);
    assert!(!due.runnable);
    assert!(has_issue(&due, MonitorIssueCode::ResetDueUnverified));

    ingest(
        &store,
        account_id,
        observation(
            "session-resetless-no-history",
            None,
            (Some(2_000), Some(recovery_at + 1)),
            (None, None),
        ),
        recovery_at,
    );
    let state = store.lock();
    assert!(state.accounts[account_id].reset_barriers[0].is_none());
    assert!(state.monitors[&started.monitor_id].reset_barriers[0].is_none());
    let session = &state.accounts[account_id].sessions["session-resetless-no-history"];
    let used = session.windows[0]
        .used
        .as_ref()
        .expect("recovered complete used field");
    assert_eq!(used.value, 2_000);
    assert_eq!(used.reset_at_epoch, Some(recovery_at + 1));
}

fn assert_high_broker_orphan_state(
    store: &MonitorStore,
    account_id: &str,
    original_reset: i64,
    high_water_reset: i64,
    sibling_before: BrokerWindowIdentity,
) {
    {
        let state = store.lock();
        let account = &state.accounts[account_id];
        assert_eq!(account.latest_reset_epochs[0], Some(high_water_reset));
        assert_eq!(
            account.broker_windows[0]
                .reset
                .as_ref()
                .map(|reset| reset.value),
            Some(original_reset),
            "the broker's retained reset identity is not rewritten by the orphan"
        );
        assert_eq!(
            account.reset_barriers[0]
                .as_ref()
                .and_then(|barrier| barrier.prior_reset_at_epoch),
            Some(high_water_reset),
            "a high-only barrier anchors to the greatest reset high-water"
        );
        assert_eq!(
            account.reset_barriers[0]
                .as_ref()
                .map(|barrier| barrier.prior_used_percentage_basis_points),
            Some(9_600)
        );
        assert_eq!(
            broker_window_identity(&account.broker_windows[1]),
            sibling_before,
            "the LongRange sibling retains its independent identity"
        );
    }
}

fn assert_high_broker_orphan_status(high_status: &MonitorStatus) {
    assert_eq!(
        high_status.five_hour.used_percentage_basis_points,
        Some(9_600)
    );
    assert!(!high_status.runnable);
    assert!(has_issue(high_status, MonitorIssueCode::MissingReset));
    assert_eq!(high_status.readiness.quota, MonitorQuotaReadiness::Unknown);
}

fn assert_low_broker_orphan_status(low_status: &MonitorStatus) {
    assert_eq!(low_status.five_hour.used_percentage_basis_points, Some(0));
    assert!(!low_status.runnable);
    assert!(has_issue(low_status, MonitorIssueCode::MissingReset));
}

fn assert_low_broker_orphan_state(
    store: &MonitorStore,
    account_id: &str,
    monitor_id: &str,
    high_water_reset: i64,
    sibling_before: BrokerWindowIdentity,
) {
    {
        let state = store.lock();
        let account = &state.accounts[account_id];
        assert_eq!(
            account.reset_barriers[0]
                .as_ref()
                .and_then(|barrier| barrier.prior_reset_at_epoch),
            Some(high_water_reset)
        );
        assert_eq!(
            account.reset_barriers[0]
                .as_ref()
                .map(|barrier| barrier.prior_used_percentage_basis_points),
            Some(9_600),
            "a low orphan cannot lower the sticky peak"
        );
        let monitor_barrier = state.monitors[monitor_id].reset_barriers[0]
            .as_ref()
            .expect("high orphan also latches the monitor-local threshold barrier");
        assert_eq!(monitor_barrier.prior_reset_at_epoch, Some(high_water_reset));
        assert_eq!(monitor_barrier.prior_used_percentage_basis_points, 9_600);
        assert_eq!(
            broker_window_identity(&account.broker_windows[1]),
            sibling_before
        );
    }
}

fn assert_due_broker_status(due: &MonitorStatus) {
    assert!(!due.runnable);
    assert!(has_issue(due, MonitorIssueCode::ResetDueUnverified));
}

fn assert_same_reset_broker_status(same_reset_status: &MonitorStatus) {
    assert!(!same_reset_status.runnable);
    assert!(has_issue(
        same_reset_status,
        MonitorIssueCode::ResetDueUnverified
    ));
}

fn recover_broker_resetless_high_risk(
    store: &MonitorStore,
    account_id: &str,
    monitor_id: &str,
    sibling_reset: i64,
    high_water_reset: i64,
    due_at: i64,
) {
    let advanced_reset = high_water_reset + 3_600;
    let sibling_refresh = quota_projection(QuotaProjectionSpec {
        account_id,
        generation: 6,
        category: UsageWindowCategoryV1::LongRange,
        label: "weekly",
        used_points: Some(92.0),
        reset_at_epoch: Some(sibling_reset),
        source_at_epoch: due_at + 2,
        last_good_at_epoch: due_at + 2,
    });
    store
        .observe_projection(&sibling_refresh, due_at + 2)
        .expect("independently refresh the LongRange sibling at recovery");
    seed_zero_sgd_spend(store, account_id, due_at + 2);
    let advanced = quota_projection(QuotaProjectionSpec {
        account_id,
        generation: 7,
        category: UsageWindowCategoryV1::Session,
        label: "session",
        used_points: Some(20.0),
        reset_at_epoch: Some(advanced_reset),
        source_at_epoch: due_at + 2,
        last_good_at_epoch: due_at + 2,
    });
    store
        .observe_projection(&advanced, due_at + 2)
        .expect("admit a fresh lower-use pair with an advanced reset");
    let recovered = status(store, monitor_id, due_at + 2);
    assert!(
        recovered.runnable,
        "only a fresh complete pair with a strictly advanced reset clears the barrier"
    );
    assert!(!has_issue(&recovered, MonitorIssueCode::ResetDueUnverified));
}

fn capture_statusline_risk_phase(
    store: &MonitorStore,
    account_id: &str,
    monitor_id: &str,
    reset_at_epoch: i64,
    fence_before: &super::ResetAdmissionFence,
    risk_at: i64,
) -> (u64, i64) {
    let (risk_sequence, barrier_started_at) = {
        let state = store.lock();
        let account = &state.accounts[account_id];
        let risk = &account.sessions["session-statusline-risk"].windows[0];
        let used = risk.used.as_ref().expect("persist resetless risk usage");
        assert_eq!(used.value, 9_500);
        assert_eq!(used.reset_at_epoch, None);
        assert!(risk.reset.is_none());
        assert!(risk.paired.is_none());
        assert_eq!(account.latest_reset_epochs[0], Some(reset_at_epoch));
        assert_eq!(account.reset_admission_fences[0], *fence_before);
        let barrier = account.reset_barriers[0]
            .as_ref()
            .expect("statusline risk latches the account barrier immediately");
        assert_eq!(barrier.prior_reset_at_epoch, Some(reset_at_epoch));
        assert_eq!(barrier.prior_used_percentage_basis_points, 9_500);
        assert_eq!(
            barrier.source,
            jackin_protocol::usage_monitor::MonitorEvidenceSource::Statusline
        );
        assert_eq!(
            barrier.session_id.as_deref(),
            Some("session-statusline-risk")
        );
        assert!(state.monitors[monitor_id].reset_barriers[0].is_none());
        (used.input_sequence, barrier.started_at_epoch)
    };
    let risk_status = status(store, monitor_id, risk_at);
    assert_ne!(
        risk_status.five_hour.used_percentage_basis_points,
        Some(9_500),
        "risk-only statusline evidence does not become authoritative quota usage"
    );
    assert_eq!(risk_status.readiness.quota, MonitorQuotaReadiness::Unknown);
    assert!(!risk_status.runnable);
    assert!(has_issue(&risk_status, MonitorIssueCode::MissingReset));
    (risk_sequence, barrier_started_at)
}

fn assert_statusline_duplicate_callbacks(
    store: &MonitorStore,
    account_id: &str,
    fence_before: &super::ResetAdmissionFence,
    risk_sequence: u64,
    barrier_started_at: i64,
    risk_at: i64,
) {
    {
        let state = store.lock();
        let account = &state.accounts[account_id];
        let used = account.sessions["session-statusline-risk"].windows[0]
            .used
            .as_ref()
            .expect("same or lower risk callback leaves accepted evidence intact");
        assert_eq!(used.value, 9_500);
        assert_eq!(used.input_sequence, risk_sequence);
        assert_eq!(used.received_at_epoch, risk_at);
        assert_eq!(account.reset_admission_fences[0], *fence_before);
        assert_eq!(
            account.reset_barriers[0]
                .as_ref()
                .map(|barrier| barrier.started_at_epoch),
            Some(barrier_started_at)
        );
    }
}

fn assert_reopened_statusline_risk(
    store: &MonitorStore,
    account_id: &str,
    monitor_id: &str,
    reset_at_epoch: i64,
    fence_before: &super::ResetAdmissionFence,
) {
    {
        let state = store.lock();
        let account = &state.accounts[account_id];
        assert_eq!(account.reset_admission_fences[0], *fence_before);
        assert_eq!(
            account.reset_barriers[0]
                .as_ref()
                .and_then(|barrier| barrier.prior_reset_at_epoch),
            Some(reset_at_epoch)
        );
        assert_eq!(
            account.sessions["session-statusline-risk"].windows[0]
                .used
                .as_ref()
                .map(|used| used.reset_at_epoch),
            Some(None)
        );
        assert!(state.monitors[monitor_id].reset_barriers[0].is_none());
    }
}

fn assert_statusline_due_status(due: &MonitorStatus) {
    assert!(!due.runnable);
    assert!(has_issue(due, MonitorIssueCode::ResetDueUnverified));
}

fn assert_statusline_due_barrier(store: &MonitorStore, monitor_id: &str) {
    {
        let state = store.lock();
        assert!(state.monitors[monitor_id].reset_barriers[0].is_some());
    }
}

#[test]
fn broker_resetless_high_risk_keeps_the_historical_fence_until_an_advanced_pair() {
    for initial_used in [5_400, 9_500] {
        let account_id = format!("acct-resetless-barrier-{initial_used}");
        let (_directory, store) = open_store();
        seed_zero_sgd_spend(&store, &account_id, NOW);
        let started = start(
            &store,
            config(
                &account_id,
                "goal-resetless-barrier",
                None,
                Some(Money::new(5_000, "SGD", 2)),
            ),
            NOW,
        );
        let original_reset = NOW + 3_600;
        let high_water_reset = original_reset + 50;
        let sibling_reset = NOW + 7 * 24 * 60 * 60;
        let session = quota_projection(QuotaProjectionSpec {
            account_id: &account_id,
            generation: 1,
            category: UsageWindowCategoryV1::Session,
            label: "session",
            used_points: Some(f64::from(initial_used) / 100.0),
            reset_at_epoch: Some(original_reset),
            source_at_epoch: NOW,
            last_good_at_epoch: NOW,
        });
        store
            .observe_projection(&session, NOW)
            .expect("store paired Session baseline");
        let long_range = quota_projection(QuotaProjectionSpec {
            account_id: &account_id,
            generation: 2,
            category: UsageWindowCategoryV1::LongRange,
            label: "weekly",
            used_points: Some(92.0),
            reset_at_epoch: Some(sibling_reset),
            source_at_epoch: NOW,
            last_good_at_epoch: NOW,
        });
        store
            .observe_projection(&long_range, NOW)
            .expect("store independent LongRange baseline");

        ingest(
            &store,
            &account_id,
            observation(
                "session-reset-high-water",
                None,
                (None, Some(high_water_reset)),
                (None, None),
            ),
            NOW + 250,
        );
        let sibling_before = {
            let state = store.lock();
            broker_window_identity(&state.accounts[&account_id].broker_windows[1])
        };

        let high_at = NOW + 301;
        let high = quota_projection(QuotaProjectionSpec {
            account_id: &account_id,
            generation: 3,
            category: UsageWindowCategoryV1::Session,
            label: "session",
            used_points: Some(96.0),
            reset_at_epoch: None,
            source_at_epoch: high_at,
            last_good_at_epoch: high_at,
        });
        store
            .observe_projection(&high, high_at)
            .expect("admit fresh resetless high-risk usage");
        assert_high_broker_orphan_state(
            &store,
            &account_id,
            original_reset,
            high_water_reset,
            sibling_before,
        );
        let high_status = status(&store, &started.monitor_id, high_at);
        assert_high_broker_orphan_status(&high_status);

        let low_at = NOW + 302;
        let low = quota_projection(QuotaProjectionSpec {
            account_id: &account_id,
            generation: 4,
            category: UsageWindowCategoryV1::Session,
            label: "session",
            used_points: Some(0.0),
            reset_at_epoch: None,
            source_at_epoch: low_at,
            last_good_at_epoch: low_at,
        });
        store
            .observe_projection(&low, low_at)
            .expect("admit fresh lower resetless usage");
        let low_status = status(&store, &started.monitor_id, low_at);
        assert_low_broker_orphan_status(&low_status);
        assert_low_broker_orphan_state(
            &store,
            &account_id,
            &started.monitor_id,
            high_water_reset,
            sibling_before,
        );

        let due_at = high_water_reset + 60;
        store
            .tick(due_at)
            .expect("reach the exact reset grace boundary");
        let due = status(&store, &started.monitor_id, due_at);
        assert_due_broker_status(&due);

        let same_reset = quota_projection(QuotaProjectionSpec {
            account_id: &account_id,
            generation: 5,
            category: UsageWindowCategoryV1::Session,
            label: "session",
            used_points: Some(20.0),
            reset_at_epoch: Some(high_water_reset),
            source_at_epoch: due_at + 1,
            last_good_at_epoch: due_at + 1,
        });
        store
            .observe_projection(&same_reset, due_at + 1)
            .expect("admit a fresh complete pair with the old reset");
        let same_reset_status = status(&store, &started.monitor_id, due_at + 1);
        assert_same_reset_broker_status(&same_reset_status);

        recover_broker_resetless_high_risk(
            &store,
            &account_id,
            &started.monitor_id,
            sibling_reset,
            high_water_reset,
            due_at,
        );
    }
}

fn assert_statusline_old_reset_refused(old_reset_status: &MonitorStatus) {
    assert!(!old_reset_status.runnable);
    assert!(has_issue(
        old_reset_status,
        MonitorIssueCode::ResetDueUnverified
    ));
}

fn assert_statusline_recovery_status(recovered: &MonitorStatus) {
    assert!(recovered.runnable);
    assert!(!has_issue(recovered, MonitorIssueCode::ResetDueUnverified));
}

fn assert_statusline_reopened_status(reopened_status: &MonitorStatus) {
    assert!(reopened_status.runnable);
}

fn recover_statusline_resetless_risk_with_advanced_pair(
    store: &MonitorStore,
    account_id: &str,
    monitor_id: &str,
    weekly_reset: i64,
    reset_at_epoch: i64,
    due_at: i64,
) {
    let weekly_refresh = quota_projection(QuotaProjectionSpec {
        account_id,
        generation: 3,
        category: UsageWindowCategoryV1::LongRange,
        label: "weekly",
        used_points: Some(92.0),
        reset_at_epoch: Some(weekly_reset),
        source_at_epoch: due_at + 2,
        last_good_at_epoch: due_at + 2,
    });
    store
        .observe_projection(&weekly_refresh, due_at + 2)
        .expect("refresh LongRange independently at recovery");
    seed_zero_sgd_spend(store, account_id, due_at + 2);
    ingest(
        store,
        account_id,
        observation(
            "session-statusline-recovery",
            None,
            (Some(2_000), Some(reset_at_epoch + 3_600)),
            (None, None),
        ),
        due_at + 2,
    );
    let recovered = status(store, monitor_id, due_at + 2);
    assert_statusline_recovery_status(&recovered);
}

#[test]
fn statusline_high_without_reset_keeps_provider_fence_until_an_advanced_pair() {
    let account_id = "acct-statusline-resetless-risk";
    let (directory, store) = open_store();
    seed_zero_sgd_spend(&store, account_id, NOW);
    let started = start(
        &store,
        config(
            account_id,
            "goal-statusline-resetless-risk",
            None,
            Some(Money::new(5_000, "SGD", 2)),
        ),
        NOW,
    );
    let reset_at_epoch = NOW + 3_600;
    let weekly_reset = NOW + 7 * 24 * 60 * 60;
    let baseline = quota_projection(QuotaProjectionSpec {
        account_id,
        generation: 1,
        category: UsageWindowCategoryV1::Session,
        label: "session",
        used_points: Some(54.0),
        reset_at_epoch: Some(reset_at_epoch),
        source_at_epoch: NOW,
        last_good_at_epoch: NOW,
    });
    store
        .observe_projection(&baseline, NOW)
        .expect("store provider reset and usage baseline");
    let weekly = quota_projection(QuotaProjectionSpec {
        account_id,
        generation: 2,
        category: UsageWindowCategoryV1::LongRange,
        label: "weekly",
        used_points: Some(92.0),
        reset_at_epoch: Some(weekly_reset),
        source_at_epoch: NOW,
        last_good_at_epoch: NOW,
    });
    store
        .observe_projection(&weekly, NOW)
        .expect("store independent LongRange baseline");
    let fence_before = {
        let state = store.lock();
        state.accounts[account_id].reset_admission_fences[0].clone()
    };

    let risk_at = NOW + 301;
    ingest(
        &store,
        account_id,
        observation(
            "session-statusline-risk",
            None,
            (Some(9_500), None),
            (None, None),
        ),
        risk_at,
    );
    let (risk_sequence, barrier_started_at) = capture_statusline_risk_phase(
        &store,
        account_id,
        &started.monitor_id,
        reset_at_epoch,
        &fence_before,
        risk_at,
    );

    ingest(
        &store,
        account_id,
        observation(
            "session-statusline-risk",
            None,
            (Some(1_000), None),
            (None, None),
        ),
        risk_at + 1,
    );
    ingest(
        &store,
        account_id,
        observation(
            "session-statusline-risk",
            None,
            (Some(9_500), None),
            (None, None),
        ),
        risk_at + 2,
    );
    assert_statusline_duplicate_callbacks(
        &store,
        account_id,
        &fence_before,
        risk_sequence,
        barrier_started_at,
        risk_at,
    );

    drop(store);
    let store = MonitorStore::open(directory.path()).expect("reopen sticky statusline risk");
    assert_reopened_statusline_risk(
        &store,
        account_id,
        &started.monitor_id,
        reset_at_epoch,
        &fence_before,
    );

    let due_at = reset_at_epoch + 60;
    store.tick(due_at).expect("advance through reset grace");
    let due = status(&store, &started.monitor_id, due_at);
    assert_statusline_due_status(&due);
    assert_statusline_due_barrier(&store, &started.monitor_id);

    let old_reset_recovery = observation(
        "session-statusline-recovery",
        None,
        (Some(2_000), Some(reset_at_epoch)),
        (None, None),
    );
    ingest(&store, account_id, old_reset_recovery, due_at + 1);
    let old_reset_status = status(&store, &started.monitor_id, due_at + 1);
    assert_statusline_old_reset_refused(&old_reset_status);

    recover_statusline_resetless_risk_with_advanced_pair(
        &store,
        account_id,
        &started.monitor_id,
        weekly_reset,
        reset_at_epoch,
        due_at,
    );
    drop(store);

    let reopened = MonitorStore::open(directory.path()).expect("reopen sticky resetless risk");
    let reopened_status = status(&reopened, &started.monitor_id, due_at + 2);
    assert_statusline_reopened_status(&reopened_status);
}

fn assert_partial_statusline_increase_visible(
    store: &MonitorStore,
    account_id: &str,
    monitor_id: &str,
    reset_at_epoch: i64,
) -> u64 {
    let partial_sequence = {
        let state = store.lock();
        let window =
            &state.accounts[account_id].sessions["session-same-session-partial-risk"].windows[0];
        let used = window.used.as_ref().expect("accepted partial increase");
        assert_eq!(used.value, 6_000);
        assert_eq!(used.reset_at_epoch, None);
        assert_eq!(used.received_at_epoch, NOW + 1);
        assert_eq!(
            window.reset.as_ref().map(|reset| reset.value),
            Some(reset_at_epoch)
        );
        assert!(window.paired.is_none());
        used.input_sequence
    };
    let visible = status(store, monitor_id, NOW + 1);
    assert_eq!(visible.five_hour.used_percentage_basis_points, Some(6_000));
    assert_eq!(visible.readiness.quota, MonitorQuotaReadiness::Unknown);
    assert!(has_issue(&visible, MonitorIssueCode::MissingReset));
    assert!(!visible.runnable);
    partial_sequence
}

#[test]
fn same_session_partial_increases_remain_visible_unpaired_until_advanced_pair_recovery() {
    let account_id = "acct-same-session-partial-risk";
    let (_directory, store) = open_store();
    let started = start(
        &store,
        config(account_id, "goal-same-session-partial-risk", None, None),
        NOW,
    );
    let reset_at_epoch = NOW + 3_600;
    ingest(
        &store,
        account_id,
        observation(
            "session-same-session-partial-risk",
            None,
            (Some(5_400), Some(reset_at_epoch)),
            (None, None),
        ),
        NOW,
    );

    ingest(
        &store,
        account_id,
        observation(
            "session-same-session-partial-risk",
            None,
            (Some(6_000), None),
            (None, None),
        ),
        NOW + 1,
    );
    let partial_sequence = assert_partial_statusline_increase_visible(
        &store,
        account_id,
        &started.monitor_id,
        reset_at_epoch,
    );

    ingest(
        &store,
        account_id,
        observation(
            "session-same-session-partial-risk",
            None,
            (Some(6_000), None),
            (None, None),
        ),
        NOW + 2,
    );
    {
        let state = store.lock();
        let used = state.accounts[account_id].sessions["session-same-session-partial-risk"].windows
            [0]
        .used
        .as_ref()
        .expect("duplicate leaves accepted partial intact");
        assert_eq!(used.input_sequence, partial_sequence);
        assert_eq!(used.received_at_epoch, NOW + 1);
    }

    for (at, used) in [(NOW + 3, 9_400), (NOW + 4, 9_500)] {
        ingest(
            &store,
            account_id,
            observation(
                "session-same-session-partial-risk",
                None,
                (Some(used), None),
                (None, None),
            ),
            at,
        );
    }
    let high_risk = status(&store, &started.monitor_id, NOW + 4);
    assert_eq!(
        high_risk.five_hour.used_percentage_basis_points,
        Some(9_500)
    );
    assert_eq!(high_risk.readiness.quota, MonitorQuotaReadiness::Unknown);
    assert!(has_issue(&high_risk, MonitorIssueCode::MissingReset));
    assert!(!high_risk.runnable);
    {
        let state = store.lock();
        let account = &state.accounts[account_id];
        let window = &account.sessions["session-same-session-partial-risk"].windows[0];
        assert_eq!(
            window.used.as_ref().map(|used| used.reset_at_epoch),
            Some(None)
        );
        assert_eq!(
            window.reset.as_ref().map(|reset| reset.value),
            Some(reset_at_epoch)
        );
        assert!(window.paired.is_none());
        let barrier = account.reset_barriers[0]
            .as_ref()
            .expect("high partial usage latches account risk");
        assert_eq!(barrier.prior_reset_at_epoch, Some(reset_at_epoch));
        assert_eq!(barrier.prior_used_percentage_basis_points, 9_500);
    }

    let grace_at = reset_at_epoch + super::MONITOR_RESET_GRACE_SECS;
    ingest(
        &store,
        account_id,
        observation(
            "session-same-session-partial-risk",
            None,
            (Some(6_000), Some(reset_at_epoch)),
            (None, None),
        ),
        grace_at,
    );
    {
        let state = store.lock();
        assert!(state.accounts[account_id].reset_barriers[0].is_some());
    }
    let old_reset = status(&store, &started.monitor_id, grace_at);
    assert!(!old_reset.runnable);
    assert!(has_issue(&old_reset, MonitorIssueCode::ResetDueUnverified));

    let recovery_at = grace_at + 1;
    ingest(
        &store,
        account_id,
        observation(
            "session-same-session-partial-risk",
            None,
            (Some(4_000), Some(reset_at_epoch + 3_600)),
            (None, None),
        ),
        recovery_at,
    );
    {
        let state = store.lock();
        let account = &state.accounts[account_id];
        let window = &account.sessions["session-same-session-partial-risk"].windows[0];
        assert!(super::resetless::exact_observed_pair(window).is_some());
        assert!(account.reset_barriers[0].is_none());
    }
    let recovered = status(&store, &started.monitor_id, recovery_at);
    assert!(!has_issue(&recovered, MonitorIssueCode::ResetDueUnverified));
}

#[test]
fn provider_used_only_requires_source_and_receipt_after_pruned_statusline_reset_fence() {
    let account_id = "acct-resetless-receipt-fence";
    let (directory, store) = open_store();
    let started = start(
        &store,
        config(account_id, "goal-resetless-receipt-fence", None, None),
        NOW,
    );
    let reset_at_epoch = NOW + 3_600;
    let baseline = quota_projection(QuotaProjectionSpec {
        account_id,
        generation: 1,
        category: UsageWindowCategoryV1::Session,
        label: "session",
        used_points: Some(54.0),
        reset_at_epoch: Some(reset_at_epoch),
        source_at_epoch: NOW,
        last_good_at_epoch: NOW,
    });
    store
        .observe_projection(&baseline, NOW)
        .expect("store paired provider baseline");
    ingest(
        &store,
        account_id,
        observation(
            "session-receipt-only-reset",
            None,
            (None, Some(reset_at_epoch)),
            (None, None),
        ),
        reset_at_epoch,
    );
    let (reset_identity, fence) = {
        let state = store.lock();
        let account = &state.accounts[account_id];
        let fence = account.reset_admission_fences[0].clone();
        assert!(matches!(
            &fence,
            super::ResetAdmissionFence::Known {
                source_after_epoch,
                receipt_after_epoch,
                ..
            } if *source_after_epoch == reset_at_epoch && *receipt_after_epoch == reset_at_epoch
        ));
        (broker_window_identity(&account.broker_windows[0]).1, fence)
    };

    let equality_case = quota_projection(QuotaProjectionSpec {
        account_id,
        generation: 2,
        category: UsageWindowCategoryV1::Session,
        label: "session",
        used_points: Some(0.0),
        reset_at_epoch: None,
        source_at_epoch: reset_at_epoch,
        last_good_at_epoch: reset_at_epoch + 1,
    });
    store
        .observe_projection(&equality_case, reset_at_epoch + 1)
        .expect("ignore source time equal to the reset receipt fence");
    {
        let state = store.lock();
        let account = &state.accounts[account_id];
        assert_eq!(
            broker_window_identity(&account.broker_windows[0]).1,
            reset_identity,
            "an equality replay cannot rewrite the retained reset"
        );
        assert_eq!(
            account.reset_admission_fences[0], fence,
            "an equality replay cannot renew the receipt fence"
        );
        assert_eq!(account.used_only_admission_sequences[0], None);
    }

    ingest(
        &store,
        account_id,
        observation(
            "session-reset-fence-pruner",
            None,
            (None, None),
            (None, None),
        ),
        reset_at_epoch + 301,
    );
    store
        .tick(reset_at_epoch + 301)
        .expect("persist monotonic time before reopen");
    drop(store);

    let reopened = MonitorStore::open(directory.path()).expect("reopen durable reset fence");
    {
        let state = reopened.lock();
        let account = &state.accounts[account_id];
        assert!(!account.sessions.contains_key("session-receipt-only-reset"));
        assert_eq!(account.latest_reset_epochs[0], Some(reset_at_epoch));
        assert_eq!(account.reset_admission_fences[0], fence);
        assert_eq!(account.used_only_admission_sequences[0], None);
    }

    let later_source = reset_at_epoch + 2;
    let accepted = quota_projection(QuotaProjectionSpec {
        account_id,
        generation: 3,
        category: UsageWindowCategoryV1::Session,
        label: "session",
        used_points: Some(0.0),
        reset_at_epoch: None,
        source_at_epoch: later_source,
        last_good_at_epoch: reset_at_epoch + 302,
    });
    reopened
        .observe_projection(&accepted, reset_at_epoch + 302)
        .expect("admit source and receipt strictly after persisted fence");
    let state = reopened.lock();
    let account = &state.accounts[account_id];
    let used = account.broker_windows[0]
        .used
        .as_ref()
        .expect("persist accepted resetless usage");
    assert_eq!(used.value, 0);
    assert_eq!(used.evidence_at_epoch, Some(later_source));
    assert_eq!(used.received_at_epoch, reset_at_epoch + 302);
    assert_eq!(account.reset_admission_fences[0], fence);
    assert_eq!(account.latest_reset_epochs[0], Some(reset_at_epoch));
    assert!(account.broker_windows[0].paired.is_none());
    assert_eq!(
        account.used_only_admission_sequences[0],
        Some(used.input_sequence)
    );
    drop(state);
    let orphan_status = status(&reopened, &started.monitor_id, reset_at_epoch + 302);
    assert_eq!(
        orphan_status.five_hour.used_percentage_basis_points,
        Some(0)
    );
    assert_eq!(
        orphan_status.readiness.quota,
        MonitorQuotaReadiness::Unknown
    );
    assert!(!orphan_status.runnable);
    assert!(has_issue(&orphan_status, MonitorIssueCode::MissingReset));
}

#[test]
fn missing_prior_source_uses_its_receipt_as_the_strict_ordering_boundary() {
    let mut account = super::AccountObservations::default();
    let prior_receipt = NOW + 500;
    account.input_sequence = 7;
    account.broker_windows[0].used = Some(super::ObservedPercentage {
        value: 5_400,
        reset_at_epoch: None,
        evidence_at_epoch: None,
        received_at_epoch: prior_receipt,
        input_sequence: 7,
        claude_code_version: None,
    });
    let candidate =
        |source_at_epoch, received_at_epoch, input_sequence| super::ObservedPercentage {
            value: 0,
            reset_at_epoch: None,
            evidence_at_epoch: Some(source_at_epoch),
            received_at_epoch,
            input_sequence,
            claude_code_version: None,
        };

    assert!(!super::resetless::used_only_is_admissible(
        &account,
        0,
        &candidate(prior_receipt, prior_receipt + 2, 8),
        8,
        prior_receipt + 2,
    ));
    assert!(!super::resetless::used_only_is_admissible(
        &account,
        0,
        &candidate(prior_receipt + 2, prior_receipt, 8),
        8,
        prior_receipt + 2,
    ));
    assert!(super::resetless::used_only_is_admissible(
        &account,
        0,
        &candidate(prior_receipt + 1, prior_receipt + 2, 8),
        8,
        prior_receipt + 2,
    ));
    assert!(!super::resetless::used_only_is_admissible(
        &account,
        0,
        &candidate(prior_receipt + 1, prior_receipt + 2, 7),
        7,
        prior_receipt + 2,
    ));

    let mut account = super::AccountObservations {
        input_sequence: 7,
        ..super::AccountObservations::default()
    };
    let mut statusline = super::SessionObservation::default();
    statusline.windows[0].used = Some(super::ObservedPercentage {
        value: 5_400,
        reset_at_epoch: None,
        evidence_at_epoch: None,
        received_at_epoch: prior_receipt,
        input_sequence: 7,
        claude_code_version: None,
    });
    account
        .sessions
        .insert("statusline-prior".to_owned(), statusline);
    assert!(!super::resetless::used_only_is_admissible(
        &account,
        0,
        &candidate(prior_receipt, prior_receipt + 2, 8),
        8,
        prior_receipt + 2,
    ));
    assert!(super::resetless::used_only_is_admissible(
        &account,
        0,
        &candidate(prior_receipt + 1, prior_receipt + 2, 8),
        8,
        prior_receipt + 2,
    ));
}

#[test]
fn unique_unscoped_metric_group_supplies_window_evidence_time() {
    let fixture = WeeklyWindowFixture::new();
    reject_duplicate_and_scoped_windows(&fixture);
    reject_missing_or_stale_windows(&fixture);
    accept_unique_unscoped_window(&fixture);
}

struct WeeklyWindowFixture {
    _directory: tempfile::TempDir,
    store: MonitorStore,
    account_id: &'static str,
    monitor_id: String,
    reset_at: i64,
    before: WeeklyWindowEvidence,
}

impl WeeklyWindowFixture {
    fn new() -> Self {
        let (directory, store) = open_store();
        let account_id = "acct-principal-window";
        let reset_at = NOW + 86_400;
        let started = start(
            &store,
            config(account_id, "goal-principal-window", None, None),
            NOW,
        );
        let baseline = long_range_projection(
            account_id,
            1,
            "All models",
            72,
            Some(reset_at),
            NOW - 30,
            vec![principal_window_group(PrincipalWindowGroupSpec {
                group_id: "all-models-weekly",
                rank: 0,
                label: "All models",
                used_raw_percent: 72,
                reset_at_epoch: Some(reset_at),
                observed_at_epoch: Some(NOW),
                last_success_at_epoch: Some(NOW),
                scope: UsageMetricScopeV1::default(),
                current: true,
            })],
        );
        let (used_at, reset_evidence_at) = projected_weekly_evidence_at(&baseline, NOW);
        store
            .observe_projection(&baseline, NOW)
            .expect("observe the unique principal window");
        let before = weekly_window_evidence(&store, account_id);
        assert_eq!(before.used_basis_points, Some(7_200));
        assert_eq!(before.reset_at, Some(reset_at));
        assert_eq!(before.used_at, used_at);
        assert_eq!(before.reset_evidence_at, reset_evidence_at);
        assert_eq!(used_at, Some(NOW));
        assert_ne!(before.used_at, Some(NOW - 30));
        assert_weekly_status_evidence(&status(&store, &started.monitor_id, NOW), &before, NOW);
        Self {
            _directory: directory,
            store,
            account_id,
            monitor_id: started.monitor_id,
            reset_at,
            before,
        }
    }

    fn reject(&self, name: &str, projection: UsageProjectionV1, received_at: i64) {
        let (used_at, reset_at) = projected_weekly_evidence_at(&projection, received_at);
        assert_eq!(used_at, None, "{name} must not supply a used timestamp");
        assert_eq!(reset_at, None, "{name} must not supply a reset timestamp");
        self.store
            .observe_projection(&projection, received_at)
            .unwrap_or_else(|error| panic!("observe {name} projection: {error:?}"));
        let actual = weekly_window_evidence(&self.store, self.account_id);
        assert_eq!(
            actual, self.before,
            "{name} must not move window evidence time"
        );
        assert_weekly_status_evidence(
            &status(&self.store, &self.monitor_id, received_at),
            &self.before,
            self.before.used_received_at.expect("baseline receipt"),
        );
    }
}

fn reject_duplicate_and_scoped_windows(fixture: &WeeklyWindowFixture) {
    let source = NOW + 8;
    let duplicate = long_range_projection(
        fixture.account_id,
        2,
        "All models",
        90,
        Some(fixture.reset_at),
        source,
        vec![
            principal_window_group(PrincipalWindowGroupSpec {
                group_id: "all-models-weekly-a",
                rank: 0,
                label: "All models",
                used_raw_percent: 90,
                reset_at_epoch: Some(fixture.reset_at),
                observed_at_epoch: Some(source),
                last_success_at_epoch: Some(source),
                scope: UsageMetricScopeV1::default(),
                current: true,
            }),
            principal_window_group(PrincipalWindowGroupSpec {
                group_id: "all-models-weekly-b",
                rank: 1,
                label: "All models",
                used_raw_percent: 90,
                reset_at_epoch: Some(fixture.reset_at),
                observed_at_epoch: Some(source + 20),
                last_success_at_epoch: Some(source + 20),
                scope: UsageMetricScopeV1::default(),
                current: true,
            }),
        ],
    );
    fixture.reject("duplicate-label", duplicate, source + 1);

    let mut generation = 2;
    let mut received = source;
    for (name, axis) in [
        ("scoped-service", PrincipalScopeAxis::Service),
        ("scoped-model", PrincipalScopeAxis::Model),
        ("scoped-pool", PrincipalScopeAxis::Pool),
        ("scoped-key", PrincipalScopeAxis::KeyId),
    ] {
        generation += 1;
        received += 8;
        let scoped = long_range_projection(
            fixture.account_id,
            generation,
            "All models",
            90,
            Some(fixture.reset_at),
            received,
            vec![principal_window_group(PrincipalWindowGroupSpec {
                group_id: &format!("{name}-weekly"),
                rank: 0,
                label: "All models",
                used_raw_percent: 90,
                reset_at_epoch: Some(fixture.reset_at),
                observed_at_epoch: Some(received),
                last_success_at_epoch: Some(received),
                scope: scoped_metric_scope(axis),
                current: true,
            })],
        );
        fixture.reject(name, scoped, received + 1);
    }
}

fn reject_missing_or_stale_windows(fixture: &WeeklyWindowFixture) {
    let missing_source = NOW + 48;
    let missing_reset = long_range_projection(
        fixture.account_id,
        7,
        "All models",
        90,
        Some(fixture.reset_at),
        missing_source,
        vec![principal_window_group(PrincipalWindowGroupSpec {
            group_id: "all-models-missing-reset",
            rank: 0,
            label: "All models",
            used_raw_percent: 90,
            reset_at_epoch: None,
            observed_at_epoch: Some(missing_source),
            last_success_at_epoch: Some(missing_source),
            scope: UsageMetricScopeV1::default(),
            current: true,
        })],
    );
    fixture.reject("missing-group-reset", missing_reset, missing_source + 1);

    let stale_source = NOW + 56;
    let stale = long_range_projection(
        fixture.account_id,
        8,
        "All models",
        90,
        Some(fixture.reset_at),
        stale_source,
        vec![principal_window_group(PrincipalWindowGroupSpec {
            group_id: "all-models-stale",
            rank: 0,
            label: "All models",
            used_raw_percent: 90,
            reset_at_epoch: Some(fixture.reset_at),
            observed_at_epoch: None,
            last_success_at_epoch: Some(stale_source),
            scope: UsageMetricScopeV1::default(),
            current: false,
        })],
    );
    fixture.reject("stale-last-success", stale, stale_source + 1);
}

fn accept_unique_unscoped_window(fixture: &WeeklyWindowFixture) {
    let source = NOW + 64;
    let sibling_source = source + 20;
    let accepted = long_range_projection(
        fixture.account_id,
        9,
        "All models",
        82,
        Some(fixture.reset_at),
        source - 15,
        vec![
            principal_window_group(PrincipalWindowGroupSpec {
                group_id: "all-models-weekly-current",
                rank: 0,
                label: "All models",
                used_raw_percent: 82,
                reset_at_epoch: Some(fixture.reset_at),
                observed_at_epoch: Some(source),
                last_success_at_epoch: Some(source),
                scope: UsageMetricScopeV1::default(),
                current: true,
            }),
            principal_window_group(PrincipalWindowGroupSpec {
                group_id: "all-models-weekly-scoped",
                rank: 1,
                label: "All models",
                used_raw_percent: 82,
                reset_at_epoch: Some(fixture.reset_at),
                observed_at_epoch: Some(sibling_source),
                last_success_at_epoch: Some(sibling_source),
                scope: scoped_metric_scope(PrincipalScopeAxis::Service),
                current: true,
            }),
            principal_window_group(PrincipalWindowGroupSpec {
                group_id: "fable-weekly-newer",
                rank: 2,
                label: "Fable",
                used_raw_percent: 82,
                reset_at_epoch: Some(fixture.reset_at),
                observed_at_epoch: Some(sibling_source),
                last_success_at_epoch: Some(sibling_source),
                scope: scoped_metric_scope(PrincipalScopeAxis::Model),
                current: true,
            }),
        ],
    );
    let received = source + 1;
    let (used_at, reset_evidence_at) = projected_weekly_evidence_at(&accepted, received);
    fixture
        .store
        .observe_projection(&accepted, received)
        .expect("observe the unique unscoped principal window");
    let after = weekly_window_evidence(&fixture.store, fixture.account_id);
    assert_eq!(after.used_basis_points, Some(8_200));
    assert_eq!(after.reset_at, Some(fixture.reset_at));
    assert_eq!(after.used_at, used_at);
    assert_eq!(after.reset_evidence_at, reset_evidence_at);
    assert_eq!(used_at, Some(source));
    assert_ne!(after.used_at, Some(sibling_source));
    assert_ne!(after.used_at, Some(source - 15));
    assert_eq!(after.used_received_at, Some(received));
    assert_eq!(after.reset_received_at, Some(received));
    assert_weekly_status_evidence(
        &status(&fixture.store, &fixture.monitor_id, received),
        &after,
        received,
    );
}

#[test]
fn failed_projection_commit_keeps_staged_state_private_and_does_not_wake_watchers() {
    let account_id = "acct-resetless-save-failure";
    let (_directory, store) = open_store();
    let started = start(
        &store,
        config(account_id, "goal-resetless-save-failure", None, None),
        NOW,
    );
    let cursor = match store
        .operate(
            MonitorOperation::Watch {
                monitor_id: started.monitor_id.clone(),
                after_sequence: 0,
                timeout_ms: 0,
            },
            NOW,
        )
        .expect("read existing watch cursor")
    {
        MonitorReply::Watch { next_sequence, .. } => next_sequence,
        other => panic!("expected watch result, got {other:?}"),
    };
    let before = {
        let state = store.lock();
        (
            state.next_input_sequence,
            state.accounts.get(account_id).map(|account| {
                serde_json::to_value(account).expect("serialize pre-commit account")
            }),
            state.monitors[&started.monitor_id].next_event_sequence,
        )
    };

    let watch_store = store.clone();
    let monitor_id = started.monitor_id.clone();
    let (ready_sender, ready_receiver) = std::sync::mpsc::channel();
    let watcher = std::thread::spawn(move || {
        ready_sender.send(()).expect("signal watch worker startup");
        watch_store
            .operate(
                MonitorOperation::Watch {
                    monitor_id,
                    after_sequence: cursor,
                    timeout_ms: 100,
                },
                NOW,
            )
            .expect("watch completes after its bounded timeout")
    });
    ready_receiver
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("watch worker starts");

    let projection = quota_projection(QuotaProjectionSpec {
        account_id,
        generation: 1,
        category: UsageWindowCategoryV1::Session,
        label: "session",
        used_points: Some(54.0),
        reset_at_epoch: Some(NOW + 3_600),
        source_at_epoch: NOW + 1,
        last_good_at_epoch: NOW + 1,
    });
    super::storage::set_test_save_failure_point(super::storage::SaveFailurePoint::BeforeRename);
    assert!(store.observe_projection(&projection, NOW + 1).is_err());

    let after = {
        let state = store.lock();
        (
            state.next_input_sequence,
            state.accounts.get(account_id).map(|account| {
                serde_json::to_value(account).expect("serialize post-failure account")
            }),
            state.monitors[&started.monitor_id].next_event_sequence,
        )
    };
    assert_eq!(
        after, before,
        "failed staging never replaces in-memory state"
    );
    let reply = watcher.join().expect("join watch worker");
    let MonitorReply::Watch {
        events, timed_out, ..
    } = reply
    else {
        panic!("expected watch reply");
    };
    assert!(timed_out);
    assert!(events.is_empty(), "a failed commit emits no watch event");
}

mod policy_regression_tests;
mod session_capacity_tests;
mod v2_tests;
mod watch_restart_tests;
