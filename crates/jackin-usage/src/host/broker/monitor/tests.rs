// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::empty_projection;
use super::*;
use jackin_protocol::control::Money;
use jackin_protocol::usage_broker::{
    UsageCalendarPeriodV1, UsageFreshnessPhaseV1, UsageFreshnessV1, UsageIdentityKindV1,
    UsageIssueRecoverabilityV1, UsageIssueScopeV1, UsageIssueV1, UsageLifecycleV1,
    UsageLimitWindowV1, UsageMembershipStateV1, UsageMetricGroupKindV1, UsageMetricGroupV1,
    UsageMetricPeriodV1, UsageMetricScopeV1, UsageMetricValueV1, UsagePercent,
    UsageProjectionRefreshStateV1, UsageProjectionSchemaV1, UsageProviderV1, UsageQuotaStateV1,
};
use jackin_protocol::usage_monitor::{
    MonitorAccountBinding, MonitorAccountBindingInput, MonitorAction, MonitorBudgetReadiness,
    MonitorConfig, MonitorDispatchReadiness, MonitorEvidenceFreshness, MonitorIssueCode,
    MonitorLifecycle, MonitorModelGuardValidity, MonitorOperation, MonitorPolicy,
    MonitorPolicyApprovalInput, MonitorPolicyOrigin, MonitorPolicyRecord, MonitorProvider,
    MonitorProviderReadiness, MonitorPurpose, MonitorQuotaReadiness, MonitorReply,
    MonitorResetValidity, MonitorScope, MonitorStatus, SpendRecord, SpendRecordInput,
    SpendRecordSource, SpendVerification, StatuslineObservation, StatuslineQuotaWindow,
    StatuslineRateLimits, USAGE_MONITOR_SCHEMA_VERSION, USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[path = "tests/fingerprint_migration.rs"]
mod fingerprint_migration;
#[path = "tests/provider_diagnostics.rs"]
mod provider_diagnostics;

#[test]
fn current_quota_fingerprint_keys_bind_source_and_scope() {
    assert!(valid_evidence_fingerprint(
        "broker_projection:used:five_hour:account",
        "fingerprint"
    ));
    assert!(valid_evidence_fingerprint(
        "statusline:used:five_hour:session:account",
        "fingerprint"
    ));
    assert!(!valid_evidence_fingerprint(
        "used:five_hour:account",
        "fingerprint"
    ));
    assert!(valid_legacy_evidence_fingerprint(
        "used:five_hour:account",
        "fingerprint"
    ));
}

const NOW: i64 = 1_800_000_000;

fn open_store() -> (tempfile::TempDir, MonitorStore) {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let store = MonitorStore::open(directory.path()).expect("open monitor store");
    (directory, store)
}

fn persisted_state_path(directory: &tempfile::TempDir) -> std::path::PathBuf {
    directory
        .path()
        .join(super::super::BROKER_DIR)
        .join("monitor")
        .join("state.json")
}

fn downgrade_quota_fingerprints_for_v3(snapshot: &mut serde_json::Value) {
    for monitor in snapshot["monitors"]
        .as_object_mut()
        .expect("persisted monitors")
        .values_mut()
    {
        let fingerprints = monitor["evidence_fingerprints"]
            .as_object_mut()
            .expect("persisted evidence fingerprints");
        let current = std::mem::take(fingerprints);
        let mut legacy = serde_json::Map::new();
        let mut sources = BTreeMap::<String, String>::new();
        for (key, value) in current {
            let parts = key.split(':').collect::<Vec<_>>();
            let (legacy_key, source) = match parts.as_slice() {
                [source, field @ ("used" | "reset"), window, "account"]
                    if matches!(
                        *source,
                        "broker_projection"
                            | "statusline"
                            | "provider_spend"
                            | "operator"
                            | "local_session_log"
                    ) =>
                {
                    (Some(format!("{field}:{window}:account")), Some(*source))
                }
                [
                    source,
                    field @ ("used" | "reset"),
                    window,
                    "session",
                    session_id,
                ] if matches!(
                    *source,
                    "broker_projection"
                        | "statusline"
                        | "provider_spend"
                        | "operator"
                        | "local_session_log"
                ) =>
                {
                    (
                        Some(format!("{field}:{window}:{session_id}")),
                        Some(*source),
                    )
                }
                _ => (None, None),
            };
            let (Some(legacy_key), Some(source)) = (legacy_key, source) else {
                legacy.insert(key, value);
                continue;
            };
            match sources.get(&legacy_key) {
                None => {
                    legacy.insert(legacy_key.clone(), value);
                    sources.insert(legacy_key, source.to_owned());
                }
                Some(_) if source == "broker_projection" => {
                    legacy.insert(legacy_key.clone(), value);
                    sources.insert(legacy_key, source.to_owned());
                }
                Some(previous) => assert_eq!(
                    previous, "broker_projection",
                    "pre-V5 quota fingerprint collision must match old writer order"
                ),
            }
        }
        *fingerprints = legacy;
    }
}

fn overwrite_persisted_state(directory: &tempfile::TempDir, snapshot: &serde_json::Value) {
    std::fs::write(
        persisted_state_path(directory),
        serde_json::to_vec(snapshot).expect("serialize migration fixture"),
    )
    .expect("write migration fixture");
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
                    provider_account_id: None,
                    experimental_collector_approved: false,
                    operator_label: "isolated-test-operator".to_owned(),
                    operator_confirmed: true,
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

fn collector_config(binding: &MonitorAccountBinding, purpose: MonitorPurpose) -> MonitorConfig {
    MonitorConfig {
        provider: MonitorProvider::Claude,
        purpose,
        scope: MonitorScope::BoundAccount {
            binding_id: binding.binding_id.clone(),
            binding_revision: binding.revision,
            session_id: None,
        },
        goal_id: None,
        expected_model: None,
        policy_revision: None,
        experimental_collector: true,
    }
}

fn service_collector_source(store: &MonitorStore, now_epoch: i64) -> Option<String> {
    let MonitorReply::ServiceStatus { status } = store
        .operate(MonitorOperation::ServiceStatus, now_epoch)
        .expect("read service status")
    else {
        panic!("expected service status reply");
    };
    status.experimental_collector_source
}

#[test]
fn configured_collector_source_is_ephemeral_and_reported_in_service_status() {
    let (directory, store) = open_store();
    assert_eq!(service_collector_source(&store, NOW), None);

    store.set_experimental_collector_source(Some("source-account-1".to_owned()));
    assert_eq!(
        service_collector_source(&store, NOW + 1),
        Some("source-account-1".to_owned())
    );

    let reopened = MonitorStore::open(directory.path()).expect("reopen monitor store");
    assert_eq!(service_collector_source(&reopened, NOW + 2), None);
}

#[test]
fn durable_collector_observer_reopens_passively_until_foreground_source_is_configured() {
    let (directory, store) = open_store();
    let binding = match store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: "acct-restarted-observer".to_owned(),
                    provider_account_id: Some("source-account-1".to_owned()),
                    experimental_collector_approved: true,
                    operator_label: "isolated-test-operator".to_owned(),
                    operator_confirmed: true,
                },
            },
            NOW,
        )
        .expect("bind and approve local source")
    {
        MonitorReply::AccountBound { binding } => binding,
        other => panic!("expected account-bound reply, got {other:?}"),
    };
    let config = collector_config(&binding, MonitorPurpose::ObserveOnly);
    store.set_experimental_collector_source(Some("source-account-1".to_owned()));
    store
        .operate(
            MonitorOperation::Start {
                config: config.clone(),
                idempotency_key: "restarted-collector-observer".to_owned(),
            },
            NOW,
        )
        .expect("configured foreground source can start collection");
    assert_eq!(
        store.collection_accounts(),
        vec!["source-account-1".to_owned()]
    );

    drop(store);
    let reopened = MonitorStore::open(directory.path()).expect("reopen monitor store");
    assert_eq!(service_collector_source(&reopened, NOW + 1), None);
    assert!(reopened.collection_accounts().is_empty());
    let before_replay =
        serde_json::to_value(&*reopened.lock()).expect("serialize reopened observer state");
    let replay = reopened
        .operate(
            MonitorOperation::Start {
                config,
                idempotency_key: "restarted-collector-observer".to_owned(),
            },
            NOW + 1,
        )
        .expect_err("reopened passive broker cannot resume collection from persisted intent");
    assert_eq!(replay.code, MonitorIssueCode::CollectorAuthRequired);
    assert_eq!(
        serde_json::to_value(&*reopened.lock()).expect("serialize observer state after replay"),
        before_replay
    );
}

#[test]
fn experimental_collection_requires_current_binding_approval() {
    let (_directory, store) = open_store();
    assert_unmapped_collector_approval_requires_a_source(&store);

    let binding =
        bind_experimental_source(&store, false, NOW, "bind source without collector approval");
    assert_eq!(binding.revision, 1);
    assert!(!binding.experimental_collector_approved);
    let config = collector_config(&binding, MonitorPurpose::ObserveOnly);
    assert_collector_start_requires_approval(&store, &config);

    let approved =
        bind_experimental_source(&store, true, NOW + 1, "record explicit collector approval");
    assert_eq!(approved.revision, 2);
    assert!(approved.experimental_collector_approved);
    let config = collector_config(&approved, MonitorPurpose::ObserveOnly);
    assert_matching_foreground_source_is_required(&store, &config);
    assert_revoked_approval_rejects_replay(&store, config);
}

fn assert_unmapped_collector_approval_requires_a_source(store: &MonitorStore) {
    let unmapped_approval = store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: "acct-unmapped-approval".to_owned(),
                    provider_account_id: None,
                    experimental_collector_approved: true,
                    operator_label: "isolated-test-operator".to_owned(),
                    operator_confirmed: true,
                },
            },
            NOW,
        )
        .expect_err("collector approval without a mapped source is invalid");
    assert_eq!(unmapped_approval.code, MonitorIssueCode::BindingRequired);
}

fn bind_experimental_source(
    store: &MonitorStore,
    approved: bool,
    now_epoch: i64,
    expectation: &str,
) -> MonitorAccountBinding {
    match store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: "acct-experimental-binding".to_owned(),
                    provider_account_id: Some("source-account-1".to_owned()),
                    experimental_collector_approved: approved,
                    operator_label: "isolated-test-operator".to_owned(),
                    operator_confirmed: true,
                },
            },
            now_epoch,
        )
        .expect(expectation)
    {
        MonitorReply::AccountBound { binding } => binding,
        other => panic!("expected account-bound reply, got {other:?}"),
    }
}

fn assert_collector_start_requires_approval(store: &MonitorStore, config: &MonitorConfig) {
    let denied = store
        .operate(
            MonitorOperation::Start {
                config: config.clone(),
                idempotency_key: "experimental-approval-retry".to_owned(),
            },
            NOW,
        )
        .expect_err("mapped binding without approval must not enable collection");
    assert_eq!(denied.code, MonitorIssueCode::OperatorConfirmationRequired);
    assert!(store.collection_accounts().is_empty());
}

fn assert_matching_foreground_source_is_required(store: &MonitorStore, config: &MonitorConfig) {
    let state_before_unconfigured_start =
        serde_json::to_value(&*store.lock()).expect("serialize store before unconfigured start");
    let unconfigured = store
        .operate(
            MonitorOperation::Start {
                config: config.clone(),
                idempotency_key: "experimental-approval-retry".to_owned(),
            },
            NOW + 1,
        )
        .expect_err("passive broker cannot start experimental collection");
    assert_eq!(unconfigured.code, MonitorIssueCode::CollectorAuthRequired);
    assert_eq!(
        serde_json::to_value(&*store.lock()).expect("serialize store after unconfigured start"),
        state_before_unconfigured_start
    );
    assert!(store.collection_accounts().is_empty());

    store.set_experimental_collector_source(Some("source-account-other".to_owned()));
    let state_before_mismatched_start =
        serde_json::to_value(&*store.lock()).expect("serialize store before mismatched start");
    let mismatched = store
        .operate(
            MonitorOperation::Start {
                config: config.clone(),
                idempotency_key: "experimental-approval-retry".to_owned(),
            },
            NOW + 1,
        )
        .expect_err("wrong foreground source cannot enable collection");
    assert_eq!(mismatched.code, MonitorIssueCode::CollectorAuthRequired);
    assert_eq!(
        serde_json::to_value(&*store.lock()).expect("serialize store after mismatched start"),
        state_before_mismatched_start
    );
    assert!(store.collection_accounts().is_empty());

    store.set_experimental_collector_source(Some("source-account-1".to_owned()));
    store
        .operate(
            MonitorOperation::Start {
                config: config.clone(),
                idempotency_key: "experimental-approval-retry".to_owned(),
            },
            NOW + 1,
        )
        .expect("approved mapped binding starts the opted-in observer");
    assert_eq!(
        store.collection_accounts(),
        vec!["source-account-1".to_owned()]
    );

    store.set_experimental_collector_source(Some("source-account-other".to_owned()));
    let state_before_mismatched_replay =
        serde_json::to_value(&*store.lock()).expect("serialize store before mismatched replay");
    let mismatched_replay = store
        .operate(
            MonitorOperation::Start {
                config: config.clone(),
                idempotency_key: "experimental-approval-retry".to_owned(),
            },
            NOW + 2,
        )
        .expect_err("idempotent replay still requires its configured source");
    assert_eq!(
        mismatched_replay.code,
        MonitorIssueCode::CollectorAuthRequired
    );
    assert_eq!(
        serde_json::to_value(&*store.lock()).expect("serialize store after mismatched replay"),
        state_before_mismatched_replay
    );
    assert!(store.collection_accounts().is_empty());
    store.set_experimental_collector_source(Some("source-account-1".to_owned()));
}

fn assert_revoked_approval_rejects_replay(store: &MonitorStore, config: MonitorConfig) {
    let revoked = bind_experimental_source(
        store,
        false,
        NOW + 2,
        "new binding revision can revoke collector approval",
    );
    assert_eq!(revoked.revision, 3);
    assert!(!revoked.experimental_collector_approved);
    assert!(store.collection_accounts().is_empty());
    let replay = store
        .operate(
            MonitorOperation::Start {
                config: config.clone(),
                idempotency_key: "experimental-approval-retry".to_owned(),
            },
            NOW + 2,
        )
        .expect_err("stale binding revision must be rejected before approval is checked");
    assert_eq!(replay.code, MonitorIssueCode::BindingMismatch);

    let mut current_revision_config = config;
    if let MonitorScope::BoundAccount {
        binding_revision, ..
    } = &mut current_revision_config.scope
    {
        *binding_revision = revoked.revision;
    } else {
        panic!("collector config uses a bound account scope");
    }
    let replay = store
        .operate(
            MonitorOperation::Start {
                config: current_revision_config,
                idempotency_key: "experimental-approval-retry".to_owned(),
            },
            NOW + 2,
        )
        .expect_err("current binding revision must still reject revoked approval");
    assert_eq!(replay.code, MonitorIssueCode::OperatorConfirmationRequired);
}

#[test]
fn experimental_collection_is_observe_only_and_source_mapping_cannot_change_after_goal_history() {
    let (_directory, store) = open_store();
    let binding = match store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: "acct-fixed-source".to_owned(),
                    provider_account_id: Some("source-account-a".to_owned()),
                    experimental_collector_approved: true,
                    operator_label: "isolated-test-operator".to_owned(),
                    operator_confirmed: true,
                },
            },
            NOW,
        )
        .expect("bind and approve a source account")
    {
        MonitorReply::AccountBound { binding } => binding,
        other => panic!("expected account-bound reply, got {other:?}"),
    };
    let dispatch = collector_config(&binding, MonitorPurpose::DispatchGuard);
    let denied = store
        .operate(
            MonitorOperation::Start {
                config: dispatch,
                idempotency_key: "experimental-dispatch-guard".to_owned(),
            },
            NOW,
        )
        .expect_err("experimental collection cannot be used by dispatch guards");
    assert_eq!(denied.code, MonitorIssueCode::ObservationOnly);

    let policy = match store
        .operate(
            MonitorOperation::ApprovePolicy {
                approval: MonitorPolicyApprovalInput {
                    binding_id: binding.binding_id.clone(),
                    binding_revision: binding.revision,
                    goal_id: "goal-fixed-source".to_owned(),
                    new_policy: MonitorPolicy::QuotaOnly,
                    budget: None,
                    operator_label: "isolated-test-operator".to_owned(),
                    operator_confirmed: true,
                    acknowledge_no_sgd_cap: true,
                    expected_revision: None,
                },
            },
            NOW,
        )
        .expect("approve a quota-only goal")
    {
        MonitorReply::PolicyApproved { policy } => policy,
        other => panic!("expected policy-approved reply, got {other:?}"),
    };
    store
        .operate(
            MonitorOperation::Start {
                config: MonitorConfig {
                    provider: MonitorProvider::Claude,
                    purpose: MonitorPurpose::DispatchGuard,
                    scope: binding_scope(&binding),
                    goal_id: Some(policy.goal_id.clone()),
                    expected_model: None,
                    policy_revision: Some(policy.revision),
                    experimental_collector: false,
                },
                idempotency_key: "fixed-source-goal".to_owned(),
            },
            NOW,
        )
        .expect("create durable goal history");

    let reassign = store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: binding.account_id,
                    provider_account_id: Some("source-account-b".to_owned()),
                    experimental_collector_approved: false,
                    operator_label: "isolated-test-operator".to_owned(),
                    operator_confirmed: true,
                },
            },
            NOW + 1,
        )
        .expect_err("source identity cannot be reassigned after goal history exists");
    assert_eq!(reassign.code, MonitorIssueCode::AccountMismatch);
}

fn binding_scope(binding: &MonitorAccountBinding) -> MonitorScope {
    MonitorScope::BoundAccount {
        binding_id: binding.binding_id.clone(),
        binding_revision: binding.revision,
        session_id: None,
    }
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
) -> Result<MonitorStatus, MonitorIssue> {
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
        schema_version: USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
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
        .find(|evidence| matches!(&evidence.value, MonitorEvidenceValue::Model { .. }))
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
    let state_path = Path::new(directory.path())
        .join(super::super::BROKER_DIR)
        .join("monitor")
        .join("state.json");
    let before_failed_start = std::fs::read(&state_path).expect("read state after setup");
    let error = start_prepared(&store, &prepared, NOW + 1)
        .expect_err("strict activation requires a fresh verified SGD baseline");
    assert_eq!(error.code, MonitorIssueCode::BudgetUnverifiable);
    let after_failed_start = std::fs::read(&state_path).expect("read state after rejected start");
    assert_eq!(after_failed_start, before_failed_start);

    // A failed activation did not reserve its retry key or persist a goal.
    // A later current-period receipt permits that exact request to activate.
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
    assert!(!spend_state_matches_account(
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
fn unretained_historical_spend_correction_blocks_goal_across_reopen() {
    let (directory, store) = open_store();
    let account_id = "acct-historical-correction";
    let goal_id = "goal-historical-correction";
    let p0_start = NOW - 100;
    let p0_end = NOW + 10;
    let p1_start = p0_end;
    let p1_end = p1_start + 100;
    let p2_start = p1_end;
    let p2_end = p2_start + 10_000;
    let budget = Money::new(100_000, "SGD", 2);
    let reset = NOW + 10_000;

    ingest(
        &store,
        account_id,
        observation(
            "session-historical-correction",
            None,
            (Some(1_000), Some(reset)),
            (Some(1_000), Some(reset)),
        ),
        NOW,
    );
    record_spend(
        &store,
        spend_input(account_id, p0_start, p0_end, 700, NOW, "SGD"),
        NOW,
    );
    let started = start(
        &store,
        config(account_id, goal_id, None, Some(budget.clone())),
        NOW,
    );

    record_spend(
        &store,
        spend_input(account_id, p0_start, p0_end, 1_300, p0_end - 1, "SGD"),
        p0_end - 1,
    );
    record_spend(
        &store,
        spend_input(account_id, p1_start, p1_end, 20, p1_start + 1, "SGD"),
        p1_start + 1,
    );
    record_spend(
        &store,
        spend_input(account_id, p0_start, p0_end, 1_400, p0_end + 2, "SGD"),
        p0_end + 2,
    );
    record_spend(
        &store,
        spend_input(account_id, p1_start, p1_end, 100, p1_end - 1, "SGD"),
        p1_end - 1,
    );
    record_spend(
        &store,
        spend_input(account_id, p1_start, p1_end, 120, p1_end + 1, "SGD"),
        p1_end + 1,
    );
    record_spend(
        &store,
        spend_input(account_id, p2_start, p2_end, 30, p2_start + 1, "SGD"),
        p2_start + 1,
    );

    let before_correction = status(&store, &started.monitor_id, p2_start + 1);
    assert_eq!(
        before_correction.cumulative_goal_spend,
        Some(Money::new(850, "SGD", 2))
    );
    assert!(before_correction.runnable);

    let rejected_correction = record_spend(
        &store,
        spend_input(account_id, p0_start, p0_end, 1_600, p2_start + 2, "SGD"),
        p2_start + 2,
    );
    assert_eq!(
        rejected_correction.verification,
        SpendVerification::Unverified
    );
    let latest_p2 = store
        .lock()
        .accounts
        .get(account_id)
        .expect("account state")
        .spend
        .latest_record
        .clone()
        .expect("latest p2 record remains available");
    assert_eq!(latest_p2.billing_period_start_epoch, p2_start);
    assert_eq!(latest_p2.verification, SpendVerification::Verified);

    let mut invalid_horizon = store.lock().clone();
    let impossible_horizon = invalid_horizon.last_now_epoch + 1;
    invalid_horizon
        .accounts
        .get_mut(account_id)
        .expect("account state")
        .spend
        .historical_correction_horizon_epoch = Some(impossible_horizon);
    assert_invalid_state(&invalid_horizon);

    let newer_p2 = record_spend(
        &store,
        spend_input(account_id, p2_start, p2_end, 40, p2_start + 3, "SGD"),
        p2_start + 3,
    );
    assert_eq!(newer_p2.verification, SpendVerification::Verified);
    let blocked = status(&store, &started.monitor_id, p2_start + 3);
    assert_eq!(
        blocked.cumulative_goal_spend,
        Some(Money::new(850, "SGD", 2))
    );
    assert!(!blocked.runnable);
    assert!(has_issue(&blocked, MonitorIssueCode::BudgetUnverifiable));
    assert!(has_pause(&blocked, MonitorIssueCode::BudgetUnverifiable));

    drop(store);
    let reopened = MonitorStore::open(directory.path()).expect("reopen monitor store");
    let reopened_status = status(&reopened, &started.monitor_id, p2_start + 4);
    assert_eq!(
        reopened_status.cumulative_goal_spend,
        Some(Money::new(850, "SGD", 2))
    );
    assert!(!reopened_status.runnable);
    assert!(has_issue(
        &reopened_status,
        MonitorIssueCode::BudgetUnverifiable
    ));
    assert!(has_pause(
        &reopened_status,
        MonitorIssueCode::BudgetUnverifiable
    ));
}

const ACCOUNT: &str = "acct-policy-regressions";
const BILLING_PERIOD_END: i64 = NOW + 100_000;
const POLICY_TEST_OPERATOR: &str = "isolated-test-operator";

fn policy_quota_window(used: Option<i32>, reset: Option<i64>) -> Option<StatuslineQuotaWindow> {
    Some(StatuslineQuotaWindow {
        used_percentage_basis_points: used,
        reset_at_epoch: reset,
    })
}

fn policy_observation(
    session_id: &str,
    used: Option<i32>,
    reset: Option<i64>,
    model: Option<&str>,
) -> StatuslineObservation {
    StatuslineObservation {
        schema_version: USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
        session_id: session_id.to_owned(),
        model: model.map(str::to_owned),
        claude_code_version: Some("2.1.80".to_owned()),
        rate_limits: StatuslineRateLimits {
            five_hour: policy_quota_window(used, reset),
            seven_day: policy_quota_window(used, reset),
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
        schema_version: USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
        session_id: session_id.to_owned(),
        model: None,
        claude_code_version: Some("2.1.80".to_owned()),
        rate_limits: StatuslineRateLimits {
            five_hour: policy_quota_window(five_hour_used, five_hour_reset),
            seven_day: policy_quota_window(seven_day_used, seven_day_reset),
        },
    }
}

fn policy_spend_input(account_id: &str, evidence_at_epoch: i64) -> SpendRecordInput {
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

fn policy_ingest(
    store: &MonitorStore,
    account_id: &str,
    policy_observation: StatuslineObservation,
    now_epoch: i64,
) -> u64 {
    let binding = policy_bind_account(store, account_id, now_epoch);
    let reply = store
        .operate(
            MonitorOperation::Ingest {
                scope: MonitorScope::BoundAccount {
                    binding_id: binding.binding_id,
                    binding_revision: binding.revision,
                    session_id: None,
                },
                observation: policy_observation,
            },
            now_epoch,
        )
        .expect("policy_ingest statusline evidence");
    let MonitorReply::Ingested {
        evidence_sequence, ..
    } = reply
    else {
        panic!("expected statusline policy_ingest result");
    };
    evidence_sequence
}

fn policy_bind_account(
    store: &MonitorStore,
    account_id: &str,
    now_epoch: i64,
) -> MonitorAccountBinding {
    match store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: account_id.to_owned(),
                    provider_account_id: None,
                    experimental_collector_approved: false,
                    operator_label: POLICY_TEST_OPERATOR.to_owned(),
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
struct PolicyPreparedStart {
    config: MonitorConfig,
    idempotency_key: String,
}

fn policy_approve_strict_policy(
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
                    operator_label: POLICY_TEST_OPERATOR.to_owned(),
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

fn policy_prepare_start(
    store: &MonitorStore,
    goal_id: &str,
    expected_model: Option<&str>,
    now_epoch: i64,
) -> PolicyPreparedStart {
    let binding = policy_bind_account(store, ACCOUNT, now_epoch);
    let policy = policy_approve_strict_policy(store, &binding, goal_id, now_epoch);
    PolicyPreparedStart {
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
            experimental_collector: false,
        },
        idempotency_key: format!("fixture-start:{goal_id}:{now_epoch}"),
    }
}

fn policy_start_prepared(
    store: &MonitorStore,
    prepared: &PolicyPreparedStart,
    now_epoch: i64,
) -> Result<MonitorStatus, MonitorIssue> {
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

fn policy_record_spend(store: &MonitorStore, record: SpendRecordInput, now_epoch: i64) {
    let reply = store
        .operate(MonitorOperation::RecordSpend { record }, now_epoch)
        .expect("record verified SGD spend");
    assert!(matches!(reply, MonitorReply::SpendRecorded { .. }));
}

fn start_monitor(store: &MonitorStore, goal_id: &str, now_epoch: i64) -> MonitorStatus {
    let prepared = policy_prepare_start(store, goal_id, None, now_epoch);
    policy_start_prepared(store, &prepared, now_epoch).expect("start monitor")
}

fn start_monitor_with_expected_model(
    store: &MonitorStore,
    goal_id: &str,
    expected_model: &str,
    now_epoch: i64,
) -> MonitorStatus {
    let prepared = policy_prepare_start(store, goal_id, Some(expected_model), now_epoch);
    policy_start_prepared(store, &prepared, now_epoch).expect("start model-guarded monitor")
}

fn policy_status(store: &MonitorStore, monitor_id: &str, now_epoch: i64) -> MonitorStatus {
    let reply = store
        .operate(
            MonitorOperation::Status {
                monitor_id: monitor_id.to_owned(),
            },
            now_epoch,
        )
        .expect("read monitor policy_status");
    let MonitorReply::Status { status } = reply else {
        panic!("expected monitor policy_status result");
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
    let (events, next_sequence) =
        watch_snapshot(&state, monitor_id, after_sequence).expect("read initial cursor snapshot");
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
        let (events, next_sequence) = watch_snapshot(&state, monitor_id, after_sequence)
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
    let original = policy_observation("session-expiry", Some(1_000), Some(NOW + 3_600), None);
    let original_sequence = policy_ingest(&store, ACCOUNT, original.clone(), NOW);
    let original_spend = policy_spend_input(ACCOUNT, NOW);
    policy_record_spend(&store, original_spend, NOW);
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
    assert_eq!(used.freshness, MonitorEvidenceFreshness::Stale);
}

#[test]
fn identical_statusline_after_ttl_reconciles_and_notifies_without_refreshing_age() {
    let original = policy_observation("session-expiry", Some(1_000), Some(NOW + 3_600), None);
    assert_expiry_is_reconciled_and_notified(move |store, now_epoch, original_sequence| {
        let evidence_sequence = policy_ingest(store, ACCOUNT, original, now_epoch);
        assert_eq!(
            evidence_sequence, original_sequence,
            "identical input must not refresh evidence"
        );
    });
}

#[test]
fn duplicate_spend_after_ttl_reconciles_and_notifies_without_refreshing_receipt() {
    let duplicate = policy_spend_input(ACCOUNT, NOW);
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
    policy_ingest(
        &store,
        ACCOUNT,
        policy_observation(
            "session-projection-expiry",
            Some(1_000),
            Some(NOW + 3_600),
            None,
        ),
        NOW,
    );
    policy_record_spend(&store, policy_spend_input(ACCOUNT, NOW), NOW);
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
    let triggered = policy_observation("session-trigger", Some(9_500), Some(old_reset), None);
    let store = MonitorStore::open(directory.path()).expect("open monitor store");

    // The account threshold arrives before any monitor exists and must still
    // latch an account-wide barrier.
    policy_ingest(&store, ACCOUNT, triggered, NOW);
    policy_record_spend(&store, policy_spend_input(ACCOUNT, NOW), NOW);
    let first_setup = policy_prepare_start(&store, "goal-recreated", None, NOW);
    let first = policy_start_prepared(&store, &first_setup, NOW).expect("start first monitor");
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
    policy_ingest(
        &reopened,
        ACCOUNT,
        policy_observation("session-low-old-reset", Some(1_000), Some(old_reset), None),
        NOW + 302,
    );
    policy_record_spend(&reopened, policy_spend_input(ACCOUNT, NOW + 302), NOW + 302);
    let mut recreated_setup = first_setup.clone();
    recreated_setup.idempotency_key = "fixture-start:goal-recreated:second-run".to_owned();
    let recreated = policy_start_prepared(&reopened, &recreated_setup, NOW + 302)
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
        let paused = policy_status(&reopened, &current.monitor_id, deadline);
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
    policy_ingest(
        &reopened,
        ACCOUNT,
        policy_observation(recovery_session, None, Some(advanced_reset), None),
        deadline + 1,
    );
    let paused_after_reset_only = policy_status(&reopened, &recreated.monitor_id, deadline + 1);
    assert!(!paused_after_reset_only.runnable);
    assert!(
        paused_after_reset_only
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::ResetDueUnverified)
    );

    policy_ingest(
        &reopened,
        ACCOUNT,
        policy_observation(recovery_session, None, None, Some("claude-sonnet-4")),
        deadline + 2,
    );
    let paused_after_model_only =
        policy_status(&reopened, &different_goal.monitor_id, deadline + 2);
    assert!(!paused_after_model_only.runnable);
    assert!(
        paused_after_model_only
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::ResetDueUnverified)
    );

    // A full paired policy_observation from a different session clears the account
    // barrier. The lower utilization equals the old-window reading above.
    let recovery_at = deadline + 3;
    policy_record_spend(
        &reopened,
        policy_spend_input(ACCOUNT, recovery_at),
        recovery_at,
    );
    for current in [&recreated, &different_goal] {
        let after_spend_only = policy_status(&reopened, &current.monitor_id, recovery_at);
        assert!(!after_spend_only.runnable);
        assert!(
            after_spend_only
                .issues
                .iter()
                .any(|issue| issue.code == MonitorIssueCode::ResetDueUnverified)
        );
    }
    policy_ingest(
        &reopened,
        ACCOUNT,
        policy_observation(recovery_session, Some(1_000), Some(advanced_reset), None),
        recovery_at,
    );
    for current in [&recreated, &different_goal] {
        let resumed = policy_status(&reopened, &current.monitor_id, recovery_at);
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

    policy_ingest(
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
    policy_record_spend(&store, policy_spend_input(ACCOUNT, NOW), NOW);
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
    let five_hour_due = policy_status(&store, &started.monitor_id, five_hour_deadline);
    assert!(!five_hour_due.runnable);
    assert!(
        five_hour_due
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::ResetDueUnverified)
    );

    let recovery_at = five_hour_deadline + 1;
    policy_record_spend(
        &store,
        policy_spend_input(ACCOUNT, recovery_at),
        recovery_at,
    );
    policy_ingest(
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
    let after_five_hour_reset = policy_status(&store, &started.monitor_id, recovery_at);
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
    policy_ingest(
        &store,
        ACCOUNT,
        policy_observation("session-sequence", Some(8_900), Some(reset), None),
        NOW,
    );
    policy_record_spend(&store, policy_spend_input(ACCOUNT, NOW), NOW);
    let started = start_monitor(&store, "goal-sequence", NOW);
    let initial_sequence = started.latest_decision.as_ref().unwrap().sequence;
    drop(store);

    let reopened = MonitorStore::open(directory.path()).expect("reopen monitor store");
    let after_reopen = policy_status(&reopened, &started.monitor_id, NOW);
    assert_eq!(
        after_reopen.latest_decision.as_ref().unwrap().sequence,
        initial_sequence,
        "unchanged persisted state must keep its decision sequence"
    );

    let threshold_sequence_input = policy_ingest(
        &reopened,
        ACCOUNT,
        policy_observation("session-sequence", Some(9_000), Some(reset), None),
        NOW + 1,
    );
    let threshold = policy_status(&reopened, &started.monitor_id, NOW + 1);
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
    let after_threshold_reopen = policy_status(&reopened_again, &started.monitor_id, NOW + 1);
    assert_eq!(
        after_threshold_reopen
            .latest_decision
            .as_ref()
            .unwrap()
            .sequence,
        threshold_sequence,
        "the same monitor ID must retain its current decision across reopen"
    );
    let duplicate_sequence = policy_ingest(
        &reopened_again,
        ACCOUNT,
        policy_observation("session-sequence", Some(9_000), Some(reset), None),
        NOW + 2,
    );
    assert_eq!(duplicate_sequence, threshold_sequence_input);
    let after_duplicate = policy_status(&reopened_again, &started.monitor_id, NOW + 2);
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
    policy_ingest(
        &store,
        ACCOUNT,
        policy_observation(
            "session-model-reset",
            Some(9_600),
            Some(reset),
            Some("claude-opus-4"),
        ),
        NOW,
    );
    policy_record_spend(&store, policy_spend_input(ACCOUNT, NOW), NOW);
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
    let due = policy_status(&store, &started.monitor_id, deadline);
    assert!(
        due.issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::ResetDueUnverified)
    );

    let recovery_at = deadline + 1;
    policy_ingest(
        &store,
        ACCOUNT,
        policy_observation(
            "session-model-recovery",
            Some(1_000),
            Some(reset + 3_600),
            Some("claude-opus-4"),
        ),
        recovery_at,
    );
    let recovered_quota = policy_status(&store, &started.monitor_id, recovery_at);
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
    policy_ingest(
        &store,
        ACCOUNT,
        policy_observation("session-spend-reset", Some(9_600), Some(reset), None),
        NOW,
    );
    policy_record_spend(&store, policy_spend_input(ACCOUNT, NOW), NOW);
    let started = start_monitor(&store, "goal-spend-reset", NOW);
    policy_record_spend(
        &store,
        SpendRecordInput {
            amount: Money::new(4_800, "SGD", 2),
            ..policy_spend_input(ACCOUNT, NOW + 1)
        },
        NOW + 1,
    );
    let spend_paused = policy_status(&store, &started.monitor_id, NOW + 1);
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
    policy_record_spend(
        &store,
        SpendRecordInput {
            amount: Money::new(4_800, "SGD", 2),
            ..policy_spend_input(ACCOUNT, recovery_at)
        },
        recovery_at,
    );
    policy_ingest(
        &store,
        ACCOUNT,
        policy_observation(
            "session-spend-recovery",
            Some(1_000),
            Some(reset + 3_600),
            None,
        ),
        recovery_at,
    );
    let after_quota_reset = policy_status(&store, &started.monitor_id, recovery_at);
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
    policy_ingest(
        &store,
        ACCOUNT,
        policy_observation(
            "session-unknown-spend-reset",
            Some(9_600),
            Some(reset),
            None,
        ),
        NOW,
    );
    policy_record_spend(&store, policy_spend_input(ACCOUNT, NOW), NOW);
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
    policy_ingest(
        &store,
        ACCOUNT,
        policy_observation(
            "session-unknown-spend-recovery",
            Some(1_000),
            Some(reset + 3_600),
            None,
        ),
        recovery_at,
    );
    let after_quota_reset = policy_status(&store, &started.monitor_id, recovery_at);
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

fn session_capacity_observation(
    session_id: &str,
    used: i32,
    reset_at_epoch: i64,
) -> StatuslineObservation {
    let window = Some(StatuslineQuotaWindow {
        used_percentage_basis_points: Some(used),
        reset_at_epoch: Some(reset_at_epoch),
    });
    StatuslineObservation {
        schema_version: USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
        session_id: session_id.to_owned(),
        model: None,
        claude_code_version: Some("2.1.80".to_owned()),
        rate_limits: StatuslineRateLimits {
            five_hour: window.clone(),
            seven_day: window,
        },
    }
}

fn session_capacity_ingest(
    store: &MonitorStore,
    binding_id: &str,
    binding_revision: u64,
    session_capacity_observation: StatuslineObservation,
    now_epoch: i64,
) -> Result<MonitorReply, MonitorIssue> {
    store.operate(
        MonitorOperation::Ingest {
            scope: MonitorScope::BoundAccount {
                binding_id: binding_id.to_owned(),
                binding_revision,
                session_id: None,
            },
            observation: session_capacity_observation,
        },
        now_epoch,
    )
}

#[test]
fn stale_sessions_are_pruned_without_lowering_the_account_reset_watermark() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let store = MonitorStore::open(directory.path()).expect("open monitor store");
    let reset = NOW + 3_600;
    let binding = match store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: "acct-session-cap".to_owned(),
                    provider_account_id: None,
                    experimental_collector_approved: false,
                    operator_label: "isolated-test-operator".to_owned(),
                    operator_confirmed: true,
                },
            },
            NOW,
        )
        .expect("bind isolated test account")
    {
        MonitorReply::AccountBound { binding } => binding,
        other => panic!("expected account-bound reply, got {other:?}"),
    };

    let invalid = session_capacity_ingest(
        &store,
        &binding.binding_id,
        binding.revision,
        session_capacity_observation("poison-reset", 10_000, NOW + 5 * 60 * 60 + 301),
        NOW,
    )
    .expect_err("implausible reset is rejected before touching the watermark");
    assert_eq!(invalid.code, MonitorIssueCode::StatuslineInvalid);

    for index in 0..16 {
        session_capacity_ingest(
            &store,
            &binding.binding_id,
            binding.revision,
            session_capacity_observation(&format!("stale-{index}"), 8_000, reset),
            NOW,
        )
        .expect("session_capacity_ingest within the session cap");
    }

    // A unique seventeenth session arrives after all prior account sessions
    // expired. The old reset watermark must survive pruning and reject its
    // stale lower reset instead of turning that row into quota evidence.
    let stale_reset = session_capacity_observation("session-new", 1_000, reset - 1);
    session_capacity_ingest(
        &store,
        &binding.binding_id,
        binding.revision,
        stale_reset.clone(),
        NOW + 301,
    )
    .expect("prune stale sessions before enforcing the cap");
    session_capacity_ingest(
        &store,
        &binding.binding_id,
        binding.revision,
        stale_reset,
        NOW + 302,
    )
    .expect("repeated stale reset remains rejected");

    let started = match store
        .operate(
            MonitorOperation::Start {
                config: MonitorConfig {
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
                    experimental_collector: false,
                },
                idempotency_key: "fixture-session-cap-observer".to_owned(),
            },
            NOW + 302,
        )
        .expect("start account-scoped monitor")
    {
        MonitorReply::Started { status } => *status,
        other => panic!("expected started reply, got {other:?}"),
    };
    assert_eq!(started.five_hour.used_percentage_basis_points, None);
    assert_eq!(started.purpose, MonitorPurpose::ObserveOnly);
    assert_eq!(started.goal_id, None);
    assert_eq!(started.policy, None);
    assert_eq!(started.cumulative_goal_spend, None);
    assert_eq!(
        started.readiness.dispatch,
        MonitorDispatchReadiness::NotAuthorized
    );
    if let Some(decision) = &started.latest_decision {
        assert!(decision.actions.is_empty());
    }
    assert!(
        started
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::QuotaUnknown)
    );
    assert!(!started.runnable);
}

const V2_TEST_OPERATOR: &str = "isolated-v2-test-operator";

fn assert_invalid_state(state: &StoreState) {
    drop(validate_store_state(state).expect_err("corrupted monitor store must be rejected"));
}

fn assert_invalid_v1(fixture: &[u8]) {
    drop(legacy::migrate_v1(fixture).expect_err("malformed V1 store must be rejected"));
}

#[test]
fn v2_store_migrates_explicitly_to_current_schema() {
    let (directory, store) = open_store();
    start_result(
        &store,
        observe_config("session-v2-migration"),
        "v2-migration-event",
        NOW,
    )
    .expect("start monitor with a persisted event");
    let mut snapshot = serde_json::to_value(store.lock().clone()).expect("serialize V2 fixture");
    snapshot["schema_version"] = serde_json::json!(2);
    for monitor in snapshot["monitors"]
        .as_object_mut()
        .expect("persisted monitors")
        .values_mut()
    {
        for event in monitor["events"].as_array_mut().expect("monitor events") {
            event["status"]["schema_version"] = serde_json::json!(2);
        }
    }
    drop(store);
    overwrite_persisted_state(&directory, &snapshot);

    let migrated = MonitorStore::open(directory.path()).expect("migrate V2 store");
    assert_eq!(migrated.lock().schema_version, USAGE_MONITOR_SCHEMA_VERSION);
    let persisted: serde_json::Value = serde_json::from_slice(
        &std::fs::read(persisted_state_path(&directory)).expect("read migrated state"),
    )
    .expect("parse migrated state");
    assert_eq!(
        persisted["schema_version"],
        serde_json::json!(USAGE_MONITOR_SCHEMA_VERSION)
    );
    assert!(
        persisted["monitors"]
            .as_object()
            .expect("persisted monitors")
            .values()
            .flat_map(|monitor| monitor["events"].as_array().expect("monitor events"))
            .all(|event| event["status"]["schema_version"]
                == serde_json::json!(USAGE_MONITOR_SCHEMA_VERSION))
    );
}

#[test]
fn v2_store_rejects_wrong_nested_event_schema_without_rewriting_source() {
    let (directory, store) = open_store();
    start_result(
        &store,
        observe_config("session-v2-wrong-event-schema"),
        "v2-wrong-event-schema",
        NOW,
    )
    .expect("start monitor with a persisted event");
    let mut snapshot = serde_json::to_value(store.lock().clone()).expect("serialize V2 fixture");
    snapshot["schema_version"] = serde_json::json!(2);
    let monitor = snapshot["monitors"]
        .as_object_mut()
        .expect("persisted monitors")
        .values_mut()
        .next()
        .expect("persisted monitor");
    monitor["events"][0]["status"]["schema_version"] =
        serde_json::json!(USAGE_MONITOR_SCHEMA_VERSION);
    let source_bytes = serde_json::to_vec(&snapshot).expect("serialize wrong-schema source");
    drop(store);
    std::fs::write(persisted_state_path(&directory), &source_bytes)
        .expect("write wrong-schema source");

    let error = MonitorStore::open(directory.path())
        .expect_err("wrong nested event schema must be rejected");
    assert_eq!(error.code, MonitorIssueCode::MonitorStoreUnavailable);
    assert_eq!(
        std::fs::read(persisted_state_path(&directory)).expect("read rejected source"),
        source_bytes,
        "a rejected migration must not rewrite its source"
    );
}

#[test]
fn v3_store_migration_defaults_horizon_and_preserves_goal_history() {
    let (directory, store) = open_store();
    seed_zero_sgd_spend(&store, "acct-v3-migration", NOW);
    let started = start(
        &store,
        config(
            "acct-v3-migration",
            "goal-v3-migration",
            None,
            Some(Money::new(5_000, "SGD", 2)),
        ),
        NOW + 1,
    );

    let mut snapshot = serde_json::to_value(store.lock().clone()).expect("serialize V3 fixture");
    snapshot["schema_version"] = serde_json::json!(3);
    let account = snapshot["accounts"]["acct-v3-migration"]
        .as_object_mut()
        .expect("persisted account");
    account["broker_windows"][0] = serde_json::json!({
        "used": {
            "value": 5_000,
            "reset_at_epoch": null,
            "evidence_at_epoch": NOW,
            "received_at_epoch": NOW,
            "input_sequence": 1,
            "claude_code_version": null
        },
        "reset": {
            "value": NOW + 3_600,
            "evidence_at_epoch": NOW,
            "received_at_epoch": NOW,
            "input_sequence": 1,
            "claude_code_version": null
        },
        "paired": null
    });
    account["input_sequence"] = serde_json::json!(1);
    account["latest_reset_epochs"] = serde_json::json!([NOW + 3_600, NOW + 86_400]);
    account["spend"]
        .as_object_mut()
        .expect("persisted account spend")
        .remove("historical_correction_horizon_epoch");
    snapshot["next_input_sequence"] = serde_json::json!(1);

    let goal_spend = snapshot["goals"]["goal-v3-migration"]["spend_state"]
        .as_object_mut()
        .expect("persisted goal spend");
    goal_spend["rollover_unknown"] = serde_json::json!(true);
    goal_spend["cumulative_complete"] = serde_json::json!(false);
    let updated_goal_spend = snapshot["goals"]["goal-v3-migration"]["spend_state"].clone();
    let input_sequence = snapshot["next_input_sequence"].clone();
    let next_monitor_id = snapshot["next_monitor_id"].clone();
    for monitor in snapshot["monitors"]
        .as_object_mut()
        .expect("persisted monitors")
        .values_mut()
    {
        if monitor["config"]["goal_id"] == "goal-v3-migration" {
            monitor["spend_state"] = updated_goal_spend.clone();
        }
        for event in monitor["events"].as_array_mut().expect("monitor events") {
            event["status"]["schema_version"] = serde_json::json!(3);
        }
    }
    downgrade_quota_fingerprints_for_v3(&mut snapshot);
    drop(store);
    overwrite_persisted_state(&directory, &snapshot);

    let migrated = MonitorStore::open(directory.path()).expect("migrate V3 store");
    let state = migrated.lock();
    assert_eq!(state.schema_version, USAGE_MONITOR_SCHEMA_VERSION);
    assert_eq!(state.next_input_sequence, input_sequence.as_u64().unwrap());
    assert_eq!(state.next_monitor_id, next_monitor_id.as_u64().unwrap());
    assert_eq!(
        state.accounts["acct-v3-migration"]
            .spend
            .historical_correction_horizon_epoch,
        None
    );
    assert!(
        state.accounts["acct-v3-migration"]
            .broker_windows
            .iter()
            .all(|window| window.used.is_none()
                && window.reset.is_none()
                && window.paired.is_none())
    );
    assert_eq!(
        state.accounts["acct-v3-migration"].latest_reset_epochs,
        [None, None],
        "legacy unscoped provider resets are removed during pre-v5 migration"
    );
    let goal = &state.goals["goal-v3-migration"];
    assert_eq!(goal.budget, Some(Money::new(5_000, "SGD", 2)));
    let spend = goal.spend_state.as_ref().expect("preserved goal spend");
    assert_eq!(
        spend.baseline.as_ref().unwrap().amount,
        Money::new(0, "SGD", 2)
    );
    assert_eq!(spend.cumulative_goal_spend, Some(Money::new(0, "SGD", 2)));
    assert!(spend.rollover_unknown);
    assert!(!spend.cumulative_complete);
    for monitor in state.monitors.values() {
        assert!(
            monitor
                .events
                .iter()
                .all(|event| event.status.schema_version == USAGE_MONITOR_SCHEMA_VERSION)
        );
    }
    assert!(
        state.monitors[&started.monitor_id]
            .events
            .iter()
            .any(|event| event.status.schema_version == USAGE_MONITOR_SCHEMA_VERSION),
        "the fixture exercises persisted monitor events"
    );
    drop(state);

    let reopened_status = status(&migrated, &started.monitor_id, NOW + 2);
    assert_eq!(
        reopened_status.readiness.budget,
        MonitorBudgetReadiness::Unknown
    );
    assert_eq!(
        reopened_status.readiness.dispatch,
        MonitorDispatchReadiness::Blocked
    );
    assert!(!reopened_status.runnable);
}

#[test]
fn v3_migration_rejects_goal_monitor_mismatch_without_rewriting_source() {
    let (directory, store) = open_store();
    seed_zero_sgd_spend(&store, "acct-v3-invalid-mismatch", NOW);
    start(
        &store,
        config(
            "acct-v3-invalid-mismatch",
            "goal-v3-invalid-mismatch",
            None,
            Some(Money::new(5_000, "SGD", 2)),
        ),
        NOW + 1,
    );

    let mut snapshot = serde_json::to_value(store.lock().clone()).expect("serialize V3 fixture");
    snapshot["schema_version"] = serde_json::json!(3);
    snapshot["accounts"]["acct-v3-invalid-mismatch"]["spend"]
        .as_object_mut()
        .expect("persisted account spend")
        .remove("historical_correction_horizon_epoch");
    snapshot["goals"]["goal-v3-invalid-mismatch"]["spend_state"]["rollover_unknown"] =
        serde_json::json!(true);
    snapshot["goals"]["goal-v3-invalid-mismatch"]["spend_state"]["cumulative_complete"] =
        serde_json::json!(false);
    for monitor in snapshot["monitors"]
        .as_object_mut()
        .expect("persisted monitors")
        .values_mut()
    {
        for event in monitor["events"].as_array_mut().expect("monitor events") {
            event["status"]["schema_version"] = serde_json::json!(3);
        }
    }
    let source_bytes = serde_json::to_vec(&snapshot).expect("serialize inconsistent source");
    drop(store);
    std::fs::write(persisted_state_path(&directory), &source_bytes)
        .expect("write inconsistent V3 source");

    let error =
        MonitorStore::open(directory.path()).expect_err("inconsistent V3 source must be rejected");
    assert_eq!(error.code, MonitorIssueCode::MonitorStoreUnavailable);
    assert_eq!(
        std::fs::read(persisted_state_path(&directory)).expect("read rejected source"),
        source_bytes,
        "a rejected migration must not rewrite or partially migrate its source"
    );
}

#[test]
fn v3_migration_rejects_wrong_or_future_nested_event_schema_without_rewriting_source() {
    for nested_version in [USAGE_MONITOR_SCHEMA_VERSION, u16::MAX] {
        let (directory, store) = open_store();
        start_result(
            &store,
            observe_config("session-v3-wrong-event-schema"),
            "v3-wrong-event-schema",
            NOW,
        )
        .expect("start monitor with a persisted event");
        let mut snapshot =
            serde_json::to_value(store.lock().clone()).expect("serialize V3 fixture");
        snapshot["schema_version"] = serde_json::json!(3);
        let monitor = snapshot["monitors"]
            .as_object_mut()
            .expect("persisted monitors")
            .values_mut()
            .next()
            .expect("persisted monitor");
        for event in monitor["events"].as_array_mut().expect("monitor events") {
            event["status"]["schema_version"] = serde_json::json!(3);
        }
        monitor["events"][0]["status"]["schema_version"] = serde_json::json!(nested_version);
        let source_bytes = serde_json::to_vec(&snapshot).expect("serialize wrong-schema source");
        drop(store);
        std::fs::write(persisted_state_path(&directory), &source_bytes)
            .expect("write wrong-schema source");

        let error = MonitorStore::open(directory.path())
            .expect_err("wrong nested event schema must be rejected");
        assert_eq!(error.code, MonitorIssueCode::MonitorStoreUnavailable);
        assert_eq!(
            std::fs::read(persisted_state_path(&directory)).expect("read rejected source"),
            source_bytes,
            "a rejected migration must not rewrite its source"
        );
    }
}

struct RolledGoalMigrationFixture {
    directory: tempfile::TempDir,
    store: MonitorStore,
    account_id: &'static str,
    rolled_goal_id: &'static str,
    later_goal_id: &'static str,
    rolled_monitor_id: String,
    later_monitor_id: String,
    p2_start: i64,
    p2_end: i64,
    p3_start: i64,
    p3_end: i64,
}

struct RolloverPeriodBounds {
    p0_start: i64,
    p0_end: i64,
    p1_start: i64,
    p1_end: i64,
    p2_start: i64,
    p2_end: i64,
}

struct StrictGoalStart<'a> {
    goal_id: &'a str,
    idempotency_key: &'a str,
    budget: &'a Money,
    approval_at: i64,
    start_at: i64,
    approval_expectation: &'a str,
    start_expectation: &'a str,
}

#[test]
fn v3_rolled_goal_migration_latches_uncertainty_across_later_rollovers() {
    let fixture = create_rolled_goal_migration_fixture();
    let fixture = migrate_rolled_goal_fixture(fixture);
    assert_rolled_goal_v3_migration_state(&fixture);
    assert_rolled_goal_v3_latch_survives_later_rollovers(&fixture);
}

fn create_rolled_goal_migration_fixture() -> RolledGoalMigrationFixture {
    let (directory, store) = open_store();
    let account_id = "acct-v3-rolled";
    let rolled_goal_id = "goal-v3-rolled";
    let later_goal_id = "goal-v3-later";
    let p0_start = NOW - 100;
    let p0_end = NOW + 10;
    let p1_start = p0_end;
    let p1_end = p1_start + 10;
    let p2_start = p1_end;
    let p2_end = p2_start + 10;
    let p3_start = p2_end;
    let p3_end = p3_start + 10_000;
    let budget = Money::new(100_000, "SGD", 2);

    record_spend(
        &store,
        spend_input(account_id, p0_start, p0_end, 7_000, NOW, "SGD"),
        NOW,
    );
    ingest(
        &store,
        account_id,
        observation(
            "session-v3-rolled",
            None,
            (Some(1_000), Some(p3_end + 1_000)),
            (Some(1_000), Some(p3_end + 1_000)),
        ),
        NOW,
    );
    let binding = v2_bind_account(&store, account_id, NOW + 1);
    let rolled_monitor = start_v3_strict_goal(
        &store,
        &binding,
        StrictGoalStart {
            goal_id: rolled_goal_id,
            idempotency_key: "v3-rolled-goal",
            budget: &budget,
            approval_at: NOW + 2,
            start_at: NOW + 3,
            approval_expectation: "approve first strict goal",
            start_expectation: "start first strict goal",
        },
    );
    record_v3_rollover_history(
        &store,
        account_id,
        RolloverPeriodBounds {
            p0_start,
            p0_end,
            p1_start,
            p1_end,
            p2_start,
            p2_end,
        },
    );
    let later_monitor = start_v3_strict_goal(
        &store,
        &binding,
        StrictGoalStart {
            goal_id: later_goal_id,
            idempotency_key: "v3-later-goal",
            budget: &budget,
            approval_at: p2_start + 3,
            start_at: p2_start + 4,
            approval_expectation: "approve later strict goal in current period",
            start_expectation: "start later strict goal in current period",
        },
    );
    assert_pre_migration_goal_states(&store, rolled_goal_id, later_goal_id);

    RolledGoalMigrationFixture {
        directory,
        store,
        account_id,
        rolled_goal_id,
        later_goal_id,
        rolled_monitor_id: rolled_monitor.monitor_id,
        later_monitor_id: later_monitor.monitor_id,
        p2_start,
        p2_end,
        p3_start,
        p3_end,
    }
}

fn start_v3_strict_goal(
    store: &MonitorStore,
    binding: &MonitorAccountBinding,
    request: StrictGoalStart<'_>,
) -> MonitorStatus {
    let policy = approve_policy(
        store,
        binding,
        request.goal_id,
        MonitorPolicy::StrictSgd,
        Some(request.budget.clone()),
        ApprovalOptions {
            acknowledge_no_sgd_cap: false,
            expected_revision: None,
            now_epoch: request.approval_at,
        },
    )
    .expect(request.approval_expectation);
    start_result(
        store,
        dispatch_config(binding, request.goal_id, policy.revision),
        request.idempotency_key,
        request.start_at,
    )
    .expect(request.start_expectation)
}

fn record_v3_rollover_history(
    store: &MonitorStore,
    account_id: &str,
    periods: RolloverPeriodBounds,
) {
    let RolloverPeriodBounds {
        p0_start,
        p0_end,
        p1_start,
        p1_end,
        p2_start,
        p2_end,
    } = periods;
    record_spend(
        store,
        spend_input(account_id, p0_start, p0_end, 13_000, p0_end - 1, "SGD"),
        p0_end - 1,
    );
    record_spend(
        store,
        spend_input(account_id, p1_start, p1_end, 200, p1_start + 1, "SGD"),
        p1_start + 1,
    );
    record_spend(
        store,
        spend_input(account_id, p0_start, p0_end, 14_000, p0_end + 2, "SGD"),
        p0_end + 2,
    );
    record_spend(
        store,
        spend_input(account_id, p1_start, p1_end, 1_000, p1_end - 1, "SGD"),
        p1_end - 1,
    );
    record_spend(
        store,
        spend_input(account_id, p1_start, p1_end, 1_200, p1_end + 1, "SGD"),
        p1_end + 1,
    );
    record_spend(
        store,
        spend_input(account_id, p2_start, p2_end, 300, p2_start + 1, "SGD"),
        p2_start + 1,
    );
}

fn assert_pre_migration_goal_states(
    store: &MonitorStore,
    rolled_goal_id: &str,
    later_goal_id: &str,
) {
    let before_migration = store.lock().clone();
    assert_eq!(
        before_migration.goals[rolled_goal_id]
            .spend_state
            .as_ref()
            .unwrap()
            .cumulative_goal_spend,
        Some(Money::new(8_500, "SGD", 2))
    );
    assert!(
        before_migration.goals[rolled_goal_id]
            .spend_state
            .as_ref()
            .unwrap()
            .cumulative_complete
    );
    assert!(
        before_migration.goals[later_goal_id]
            .spend_state
            .as_ref()
            .unwrap()
            .cumulative_complete
    );
}

fn migrate_rolled_goal_fixture(fixture: RolledGoalMigrationFixture) -> RolledGoalMigrationFixture {
    let RolledGoalMigrationFixture {
        directory,
        store,
        account_id,
        rolled_goal_id,
        later_goal_id,
        rolled_monitor_id,
        later_monitor_id,
        p2_start,
        p2_end,
        p3_start,
        p3_end,
    } = fixture;
    let mut snapshot = serde_json::to_value(store.lock().clone()).expect("serialize V3 fixture");
    snapshot["schema_version"] = serde_json::json!(3);
    snapshot["accounts"][account_id]["spend"]
        .as_object_mut()
        .expect("persisted account spend")
        .remove("historical_correction_horizon_epoch");
    for monitor in snapshot["monitors"]
        .as_object_mut()
        .expect("persisted monitors")
        .values_mut()
    {
        for event in monitor["events"].as_array_mut().expect("monitor events") {
            event["status"]["schema_version"] = serde_json::json!(3);
        }
    }
    downgrade_quota_fingerprints_for_v3(&mut snapshot);
    drop(store);
    overwrite_persisted_state(&directory, &snapshot);
    let store = MonitorStore::open(directory.path()).expect("migrate rolled V3 store");
    RolledGoalMigrationFixture {
        directory,
        store,
        account_id,
        rolled_goal_id,
        later_goal_id,
        rolled_monitor_id,
        later_monitor_id,
        p2_start,
        p2_end,
        p3_start,
        p3_end,
    }
}

fn assert_rolled_goal_v3_migration_state(fixture: &RolledGoalMigrationFixture) {
    let RolledGoalMigrationFixture {
        store,
        account_id,
        rolled_goal_id,
        later_goal_id,
        rolled_monitor_id,
        later_monitor_id,
        p2_start,
        ..
    } = fixture;
    {
        let state = store.lock();
        assert_eq!(
            state.accounts[*account_id]
                .spend
                .historical_correction_horizon_epoch,
            Some(*p2_start)
        );
        let rolled = state.goals[*rolled_goal_id]
            .spend_state
            .as_ref()
            .expect("preserved rolled goal estimate");
        assert_eq!(
            rolled.baseline.as_ref().unwrap().amount,
            Money::new(7_000, "SGD", 2)
        );
        assert_eq!(
            rolled.cumulative_goal_spend,
            Some(Money::new(8_500, "SGD", 2))
        );
        assert!(rolled.rollover_unknown);
        assert!(!rolled.cumulative_complete);

        let later = state.goals[*later_goal_id]
            .spend_state
            .as_ref()
            .expect("preserved current-period goal");
        assert_eq!(
            later.baseline.as_ref().unwrap().billing_period_start_epoch,
            *p2_start
        );
        assert!(!later.rollover_unknown);
        assert!(later.cumulative_complete);
    }
    let rolled_after_migration = status(store, rolled_monitor_id, *p2_start + 5);
    assert_eq!(
        rolled_after_migration.readiness.budget,
        MonitorBudgetReadiness::Unknown
    );
    assert_eq!(
        rolled_after_migration.readiness.dispatch,
        MonitorDispatchReadiness::Blocked
    );
    assert!(!rolled_after_migration.runnable);
    let later_after_migration = status(store, later_monitor_id, *p2_start + 5);
    assert_eq!(
        later_after_migration.readiness.budget,
        MonitorBudgetReadiness::Verified
    );
    assert!(later_after_migration.runnable);
}

fn assert_rolled_goal_v3_latch_survives_later_rollovers(fixture: &RolledGoalMigrationFixture) {
    let RolledGoalMigrationFixture {
        store,
        account_id,
        rolled_monitor_id,
        later_monitor_id,
        p2_start,
        p2_end,
        p3_start,
        p3_end,
        ..
    } = fixture;
    record_spend(
        store,
        spend_input(account_id, *p2_start, *p2_end, 400, *p2_end + 1, "SGD"),
        *p2_end + 1,
    );
    record_spend(
        store,
        spend_input(account_id, *p3_start, *p3_end, 50, *p3_start + 2, "SGD"),
        *p3_start + 2,
    );
    let rolled_after_p3 = status(store, rolled_monitor_id, *p3_start + 2);
    assert_eq!(
        rolled_after_p3.cumulative_goal_spend,
        Some(Money::new(8_500, "SGD", 2))
    );
    assert_eq!(
        rolled_after_p3.readiness.budget,
        MonitorBudgetReadiness::Unknown
    );
    assert!(!rolled_after_p3.runnable);
    let later_after_p3 = status(store, later_monitor_id, *p3_start + 2);
    assert_eq!(
        later_after_p3.cumulative_goal_spend,
        Some(Money::new(150, "SGD", 2))
    );
    assert_eq!(
        later_after_p3.readiness.budget,
        MonitorBudgetReadiness::Verified
    );
    assert!(later_after_p3.runnable);
}

#[test]
fn future_store_schema_is_rejected() {
    let (directory, store) = open_store();
    let mut future = store.lock().clone();
    future.schema_version = USAGE_MONITOR_SCHEMA_VERSION + 1;
    storage::save(&store.inner.directory, &future).expect("write future-version fixture");
    drop(store);

    let error =
        MonitorStore::open(directory.path()).expect_err("future monitor schema must be rejected");
    assert_eq!(error.code, MonitorIssueCode::MonitorStoreUnavailable);
}

fn install_store_state(store: &MonitorStore, state: StoreState) {
    validate_store_state(&state).expect("validate test state before installation");
    let mut current = store.lock();
    storage::save(&store.inner.directory, &state).expect("persist test state");
    *current = state;
}

fn v2_bind_account(
    store: &MonitorStore,
    account_id: &str,
    now_epoch: i64,
) -> MonitorAccountBinding {
    match store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: account_id.to_owned(),
                    provider_account_id: None,
                    experimental_collector_approved: false,
                    operator_label: V2_TEST_OPERATOR.to_owned(),
                    operator_confirmed: true,
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
                operator_label: V2_TEST_OPERATOR.to_owned(),
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

fn v2_status(store: &MonitorStore, monitor_id: &str, now_epoch: i64) -> MonitorStatus {
    match store
        .operate(
            MonitorOperation::Status {
                monitor_id: monitor_id.to_owned(),
            },
            now_epoch,
        )
        .expect("read monitor v2_status")
    {
        MonitorReply::Status { status } => *status,
        other => panic!("expected v2_status reply, got {other:?}"),
    }
}

fn v2_quota(used: i32, reset_at_epoch: i64) -> Option<StatuslineQuotaWindow> {
    Some(StatuslineQuotaWindow {
        used_percentage_basis_points: Some(used),
        reset_at_epoch: Some(reset_at_epoch),
    })
}

fn v2_observation(
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

fn v2_ingest(
    store: &MonitorStore,
    scope: MonitorScope,
    v2_observation: StatuslineObservation,
    now_epoch: i64,
) {
    assert!(matches!(
        store
            .operate(
                MonitorOperation::Ingest {
                    scope,
                    observation: v2_observation,
                },
                now_epoch,
            )
            .expect("v2_ingest statusline v2_observation"),
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

fn assert_observer_has_no_dispatch_actions(v2_status: &MonitorStatus) {
    assert_eq!(
        v2_status.readiness.dispatch,
        MonitorDispatchReadiness::NotAuthorized
    );
    assert!(!v2_status.runnable);
    let decision = v2_status
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
    // independently received v2_quota or model field.
    v2_ingest(
        store,
        MonitorScope::Session {
            session_id: session_id.to_owned(),
        },
        v2_observation(session_id, None, Some("2.1.81"), None, None),
        NOW + 120,
    );
    let version_only = v2_status(store, monitor_id, NOW + 120);
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
    .expect("start unbound v2_observation-only monitor");
    assert_eq!(initial.account_id, None);
    assert_eq!(initial.goal_id, None);
    assert_eq!(initial.policy, None);
    assert_eq!(initial.cumulative_goal_spend, None);
    assert_eq!(initial.spend_period_baseline, None);
    assert_observer_has_no_dispatch_actions(&initial);

    let reset = NOW + 10_000;
    v2_ingest(
        &store,
        MonitorScope::Session {
            session_id: session_id.to_owned(),
        },
        v2_observation(
            session_id,
            Some("claude-sonnet"),
            Some("2.1.80"),
            v2_quota(1_500, reset),
            v2_quota(2_500, reset),
        ),
        NOW,
    );
    let observed = v2_status(&store, &initial.monitor_id, NOW);
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
    let persisted = v2_status(&reopened, &initial.monitor_id, NOW + 120);
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
    let expired = v2_status(
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
    let binding = v2_bind_account(&store, account_id, NOW);
    let scope = MonitorScope::BoundAccount {
        binding_id: binding.binding_id.clone(),
        binding_revision: binding.revision,
        session_id: Some(session_id.to_owned()),
    };
    v2_ingest(
        &store,
        scope.clone(),
        v2_observation(
            session_id,
            Some("claude-sonnet"),
            None,
            v2_quota(1_000, NOW + 3_600),
            v2_quota(1_000, NOW + 7 * 24 * 60 * 60),
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
    let updated = v2_status(&store, &started.monitor_id, NOW + 1);
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
    let binding = v2_bind_account(&store, "acct-v2-zero-budget", NOW);
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
    let binding = v2_bind_account(&store, account_id, NOW);
    v2_ingest(
        &store,
        MonitorScope::BoundAccount {
            binding_id: binding.binding_id.clone(),
            binding_revision: binding.revision,
            session_id: None,
        },
        v2_observation(
            "session-v2-spend-watch",
            None,
            None,
            v2_quota(1_000, NOW + 3_600),
            v2_quota(1_000, NOW + 7 * 24 * 60 * 60),
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
    let binding = v2_bind_account(&store, account_id, NOW);
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
        v2_ingest(
            &store,
            scope.clone(),
            v2_observation(
                &session_id,
                Some("claude-sonnet"),
                None,
                v2_quota(1_000, now_epoch + 3_600),
                v2_quota(1_000, now_epoch + 7 * 24 * 60 * 60),
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
        "only the current model and four v2_quota field fingerprints remain"
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
    let event = monitor.events.last_mut().expect("v2_observation event");
    assert!(event.status.evidence.len() >= 2);
    event.status.evidence[1].sequence = event.status.evidence[0].sequence;
    assert_invalid_state(&duplicate_event_evidence);

    let mut future_event_evidence = baseline.clone();
    let monitor = future_event_evidence
        .monitors
        .get_mut(monitor_id)
        .expect("started monitor");
    let future_sequence = monitor.next_evidence_sequence + 1;
    let event = monitor.events.last_mut().expect("v2_observation event");
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
    v2_ingest(
        &store,
        scope.clone(),
        v2_observation(
            session_id,
            Some("model-a"),
            None,
            v2_quota(5_000, NOW + 3_600),
            v2_quota(2_000, NOW + 86_400),
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
        serde_json::to_value(&*store.lock()).expect("serialize state before rejected v2_ingest");
    let error = store
        .operate(
            MonitorOperation::Ingest {
                scope,
                observation: v2_observation(
                    session_id,
                    Some("model-b"),
                    None,
                    v2_quota(5_000, NOW + 3_600),
                    v2_quota(2_000, NOW + 86_400),
                ),
            },
            NOW + 2,
        )
        .expect_err("the MAX sequence must fail closed before persistence");
    assert_eq!(error.code, MonitorIssueCode::MonitorStoreUnavailable);
    let after =
        serde_json::to_value(&*store.lock()).expect("serialize state after rejected v2_ingest");
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
    let binding = v2_bind_account(&store, account_id, NOW);
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
    let binding = v2_bind_account(&store, account_id, NOW);
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
    let binding = v2_bind_account(&store, "acct-v2-v2_quota-only", NOW);
    let goal_id = "goal-v2-v2_quota-only";

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
    .expect_err("v2_quota-only policy requires acknowledgement of the absent SGD cap");
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
    .expect("approve acknowledged v2_quota-only policy");
    let started = start_result(
        &store,
        dispatch_config(&binding, goal_id, policy.revision),
        "v2-v2_quota-only-start",
        NOW,
    )
    .expect("start v2_quota-only dispatch guard without an SGD baseline");

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
    let binding = v2_bind_account(&store, "acct-v2-no-policy", NOW);
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
    let binding = v2_bind_account(&store, account_id, NOW);
    v2_ingest(
        &store,
        MonitorScope::BoundAccount {
            binding_id: binding.binding_id.clone(),
            binding_revision: binding.revision,
            session_id: None,
        },
        v2_observation(
            "session-v2-policy-revision",
            None,
            Some("2.1.80"),
            v2_quota(1_000, NOW + 3_600),
            v2_quota(1_000, NOW + 7 * 24 * 60 * 60),
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
        "fixture starts with verified v2_quota and spend evidence"
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

    let old_revision = v2_status(&store, &started.monitor_id, NOW + 2);
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
    let binding = v2_bind_account(&store, account_id, NOW);
    let five_hour_reset = NOW + 100;
    let seven_day_reset = NOW + 7 * 24 * 60 * 60;
    v2_ingest(
        &store,
        MonitorScope::BoundAccount {
            binding_id: binding.binding_id.clone(),
            binding_revision: binding.revision,
            session_id: None,
        },
        v2_observation(
            "session-v2-reset-model-descriptors",
            Some("claude-sonnet"),
            Some("2.1.80"),
            v2_quota(1_000, five_hour_reset),
            v2_quota(1_000, seven_day_reset),
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
    let due = v2_status(&store, &started.monitor_id, reset_deadline);
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

fn v1_durable_migration_fixture() -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "next_monitor_id": 1,
        "next_input_sequence": 0,
        "last_now_epoch": NOW,
        "accounts": {},
        "monitors": {},
        "goals": {
            "goal-v1-durable": {
                "account_id": "acct-v1-durable",
                "budget": {"amount_minor": 5_000, "currency": "SGD", "exponent": 2},
                "spend_state": {
                    "baseline": null,
                    "period_anchor": {
                        "account_id": "acct-v1-durable",
                        "billing_period_start_epoch": 1_799_999_900_i64,
                        "billing_period_end_epoch": 1_800_001_000_i64,
                        "amount": {"amount_minor": 1_500, "currency": "SGD", "exponent": 2},
                        "evidence_at_epoch": 1_799_999_990_i64,
                        "evidence_received_at_epoch": NOW,
                        "source": "operator_receipt",
                        "verification": "verified"
                    },
                    "cumulative_goal_spend": {
                        "amount_minor": 1_500,
                        "currency": "SGD",
                        "exponent": 2
                    },
                    "rollover_unknown": false,
                    "cumulative_complete": true
                }
            }
        }
    })
}

#[test]
fn v1_store_migrates_durably_and_keeps_dispatch_blocked_until_review() {
    let (directory, initial_store) = open_store();
    drop(initial_store);
    let fixture = v1_durable_migration_fixture();
    overwrite_persisted_state(&directory, &fixture);

    let migrated = MonitorStore::open(directory.path()).expect("migrate persisted V1 store");
    let persisted: serde_json::Value = serde_json::from_slice(
        &std::fs::read(persisted_state_path(&directory)).expect("read rewritten monitor store"),
    )
    .expect("parse rewritten monitor store");
    assert_eq!(
        persisted["schema_version"],
        serde_json::json!(USAGE_MONITOR_SCHEMA_VERSION)
    );
    drop(migrated);

    let reopened = MonitorStore::open(directory.path()).expect("reopen migrated monitor store");
    assert_eq!(reopened.lock().schema_version, USAGE_MONITOR_SCHEMA_VERSION);
    {
        let state = reopened.lock();
        let spend = state.goals["goal-v1-durable"]
            .spend_state
            .as_ref()
            .expect("reopened migrated spend history");
        assert!(spend.baseline.is_none(), "missing baseline remains unknown");
        assert_eq!(
            spend.cumulative_goal_spend,
            Some(Money::new(1_500, "SGD", 2))
        );
        let policy = state.policy_records["goal-v1-durable"]
            .last()
            .expect("reopened migrated policy");
        assert_eq!(policy.new_policy, MonitorPolicy::StrictSgd);
        assert_eq!(policy.origin, MonitorPolicyOrigin::MigratedV1);
        assert!(!policy.operator_confirmed);
    }

    let binding = v2_bind_account(&reopened, "acct-v1-durable", NOW);
    let rejected = start_result(
        &reopened,
        dispatch_config(&binding, "goal-v1-durable", 1),
        "v1-durable-requires-approval",
        NOW + 1,
    )
    .expect_err("migrated strict policy cannot authorize dispatch");
    assert_eq!(rejected.code, MonitorIssueCode::PolicyRequired);
    assert!(reopened.lock().monitors.is_empty());
    assert_eq!(reopened.lock().next_monitor_id, 1);

    let approved = approve_policy(
        &reopened,
        &binding,
        "goal-v1-durable",
        MonitorPolicy::StrictSgd,
        Some(Money::new(5_000, "SGD", 2)),
        ApprovalOptions {
            acknowledge_no_sgd_cap: false,
            expected_revision: Some(1),
            now_epoch: NOW + 2,
        },
    )
    .expect("explicit approval creates a new policy revision");
    let started = start_result(
        &reopened,
        dispatch_config(&binding, "goal-v1-durable", approved.revision),
        "v1-durable-after-approval",
        NOW + 3,
    )
    .expect("start monitoring after explicit approval");
    assert_eq!(started.readiness.budget, MonitorBudgetReadiness::Unknown);
    assert_eq!(
        started.readiness.dispatch,
        MonitorDispatchReadiness::Blocked
    );
    assert!(!started.runnable);
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
    let binding = v2_bind_account(&store, "acct-v1-history", NOW);
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
    let binding = v2_bind_account(&store, "acct-v1-zero-budget", NOW);
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

fn watch_restart_observation() -> StatuslineObservation {
    let quota = Some(StatuslineQuotaWindow {
        used_percentage_basis_points: Some(1_000),
        reset_at_epoch: Some(NOW + 3_600),
    });
    StatuslineObservation {
        schema_version: USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
        session_id: "session-watch-restart".to_owned(),
        model: None,
        claude_code_version: Some("2.1.80".to_owned()),
        rate_limits: StatuslineRateLimits {
            five_hour: quota.clone(),
            seven_day: quota,
        },
    }
}

#[test]
fn fresh_watch_after_expired_restart_returns_only_the_reconciled_current_event() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let store = MonitorStore::open(directory.path()).expect("open monitor store");
    let binding = match store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: "acct-watch-restart".to_owned(),
                    provider_account_id: None,
                    experimental_collector_approved: false,
                    operator_label: "isolated-test-operator".to_owned(),
                    operator_confirmed: true,
                },
            },
            NOW,
        )
        .expect("bind isolated test account")
    {
        MonitorReply::AccountBound { binding } => binding,
        other => panic!("expected account-bound reply, got {other:?}"),
    };
    store
        .operate(
            MonitorOperation::Ingest {
                scope: MonitorScope::BoundAccount {
                    binding_id: binding.binding_id.clone(),
                    binding_revision: binding.revision,
                    session_id: None,
                },
                observation: watch_restart_observation(),
            },
            NOW,
        )
        .expect("ingest fresh quota evidence");
    store
        .operate(
            MonitorOperation::RecordSpend {
                record: SpendRecordInput {
                    account_id: "acct-watch-restart".to_owned(),
                    billing_period_start_epoch: NOW - 100,
                    billing_period_end_epoch: NOW + 100_000,
                    amount: Money::new(0, "SGD", 2),
                    evidence_at_epoch: Some(NOW),
                    verified: true,
                    source: SpendRecordSource::OperatorReceipt,
                },
            },
            NOW,
        )
        .expect("record a current SGD baseline");
    let policy = match store
        .operate(
            MonitorOperation::ApprovePolicy {
                approval: MonitorPolicyApprovalInput {
                    binding_id: binding.binding_id.clone(),
                    binding_revision: binding.revision,
                    goal_id: "goal-watch-restart".to_owned(),
                    new_policy: MonitorPolicy::StrictSgd,
                    budget: Some(Money::new(5_000, "SGD", 2)),
                    operator_label: "isolated-test-operator".to_owned(),
                    operator_confirmed: true,
                    acknowledge_no_sgd_cap: false,
                    expected_revision: None,
                },
            },
            NOW,
        )
        .expect("approve isolated strict SGD policy")
    {
        MonitorReply::PolicyApproved { policy } => policy,
        other => panic!("expected policy-approved reply, got {other:?}"),
    };
    let started = store
        .operate(
            MonitorOperation::Start {
                config: MonitorConfig {
                    provider: MonitorProvider::Claude,
                    purpose: MonitorPurpose::DispatchGuard,
                    scope: MonitorScope::BoundAccount {
                        binding_id: binding.binding_id,
                        binding_revision: binding.revision,
                        session_id: None,
                    },
                    goal_id: Some("goal-watch-restart".to_owned()),
                    expected_model: None,
                    policy_revision: Some(policy.revision),
                    experimental_collector: false,
                },
                idempotency_key: "fixture-watch-restart".to_owned(),
            },
            NOW,
        )
        .expect("start monitor");
    let MonitorReply::Started { status } = started else {
        panic!("expected started status");
    };
    assert!(
        status.runnable,
        "fixture must begin with a runnable decision"
    );
    let monitor_id = status.monitor_id.clone();
    drop(store);

    // Reopening the state after its source evidence expired must reconcile and
    // persist the current blocked state before a new watcher receives anything.
    let reopened = MonitorStore::open(directory.path()).expect("reopen monitor store");
    let reply = reopened
        .operate(
            MonitorOperation::Watch {
                monitor_id,
                after_sequence: 0,
                timeout_ms: 0,
            },
            NOW + 301,
        )
        .expect("watch reconciled current state");
    let MonitorReply::Watch {
        events,
        next_sequence,
        timed_out,
    } = reply
    else {
        panic!("expected watch response");
    };

    assert!(!timed_out);
    assert_eq!(events.len(), 1, "fresh attach must not replay old history");
    let current = &events[0];
    assert_eq!(current.sequence, next_sequence);
    assert!(!current.status.runnable);
    assert!(current.status.issues.iter().any(|issue| {
        matches!(
            issue.code,
            MonitorIssueCode::QuotaStale | MonitorIssueCode::SpendStale
        )
    }));

    let persisted = reopened
        .operate(
            MonitorOperation::Status {
                monitor_id: current.status.monitor_id.clone(),
            },
            NOW + 301,
        )
        .expect("read persisted current status");
    let MonitorReply::Status { status } = persisted else {
        panic!("expected status response");
    };
    assert!(!status.runnable);
    assert_eq!(status.latest_decision, current.status.latest_decision);
}

fn model_guard_fixture(
    expected_model: &str,
    observed_model: Option<&str>,
    model_observed_at: i64,
    context_observed_at: i64,
) -> (DurableMonitor, SessionObservation) {
    let session_id = "descriptor-session".to_owned();
    let evidence = observed_model
        .map(|model| MonitorEvidence {
            sequence: 1,
            account_id: None,
            session_id: Some(session_id.clone()),
            source: MonitorEvidenceSource::Statusline,
            evidence_at_epoch: Some(model_observed_at),
            evidence_received_at_epoch: model_observed_at,
            age_seconds: 0,
            claude_code_version: None,
            value: MonitorEvidenceValue::Model {
                model: model.to_owned(),
            },
        })
        .into_iter()
        .collect();
    let monitor = DurableMonitor {
        config: MonitorConfig {
            provider: MonitorProvider::Claude,
            purpose: MonitorPurpose::DispatchGuard,
            scope: MonitorScope::Session {
                session_id: session_id.clone(),
            },
            goal_id: Some("goal-descriptor".to_owned()),
            expected_model: Some(expected_model.to_owned()),
            policy_revision: None,
            experimental_collector: false,
        },
        account_id: None,
        idempotency_key: "fixture-model-descriptor".to_owned(),
        policy: None,
        created_at_epoch: NOW,
        stopped_at_epoch: None,
        updated_at_epoch: NOW,
        last_reconciled_at_epoch: NOW,
        next_evidence_sequence: 1,
        next_decision_sequence: 0,
        next_event_sequence: 0,
        evidence,
        evidence_fingerprints: BTreeMap::new(),
        reset_barriers: [None, None],
        latest_decision: None,
        decision_fingerprint: None,
        events: Vec::new(),
        spend_state: None,
    };
    let session = SessionObservation {
        model: observed_model.map(|model| Observed {
            value: model.to_owned(),
            evidence_at_epoch: Some(model_observed_at),
            received_at_epoch: model_observed_at,
            input_sequence: 1,
            claude_code_version: None,
        }),
        windows: [ObservedWindow::default(), ObservedWindow::default()],
        last_observation_received_at_epoch: Some(context_observed_at),
        claude_code_version: None,
        last_callback_received_at_epoch: Some(context_observed_at),
    };
    (monitor, session)
}

fn evaluate_model(
    monitor: &DurableMonitor,
    session: &SessionObservation,
    now_epoch: i64,
) -> MonitorEvaluation {
    let mut evaluation = MonitorEvaluation::default();
    evaluate_model_guard(monitor, None, Some(session), now_epoch, &mut evaluation);
    evaluation
}

fn model_status_snapshot(
    monitor: &DurableMonitor,
    session: &SessionObservation,
    now_epoch: i64,
) -> MonitorStatus {
    let mut state = StoreState {
        last_now_epoch: now_epoch,
        ..StoreState::default()
    };
    let MonitorScope::Session { session_id } = &monitor.config.scope else {
        panic!("model descriptor fixture must use a session scope");
    };
    state
        .unbound_sessions
        .insert(session_id.clone(), session.clone());
    status_for(&state, "descriptor-monitor", monitor, now_epoch)
}

#[test]
fn stale_model_evidence_is_unknown_even_while_session_context_is_active() {
    let (monitor, session) =
        model_guard_fixture("claude-sonnet", Some("claude-sonnet"), NOW, NOW + 200);
    let now_epoch = NOW + 301;

    let (model, evidence, unknown, mismatch) =
        model_status(&monitor, None, Some(&session), now_epoch);
    assert_eq!(model.as_deref(), Some("claude-sonnet"));
    assert_eq!(
        evidence.as_ref().map(|field| field.freshness),
        Some(MonitorEvidenceFreshness::Stale),
        "descriptor evidence age remains independently visible"
    );
    assert!(unknown, "active context cannot refresh model evidence");
    assert!(!mismatch);

    let snapshot = model_status_snapshot(&monitor, &session, now_epoch);
    assert_eq!(
        snapshot.model_guard_validity,
        MonitorModelGuardValidity::Unknown
    );
    assert_eq!(
        snapshot
            .model_evidence
            .as_ref()
            .map(|field| field.freshness),
        Some(MonitorEvidenceFreshness::Stale),
        "descriptor_status keeps descriptor age visible while validity is unknown"
    );

    let evaluation = evaluate_model(&monitor, &session, now_epoch);
    assert!(evaluation.any_unknown);
    assert!(evaluation.blocked);
    assert!(
        evaluation
            .issues
            .iter()
            .any(|item| { item.code == MonitorIssueCode::ModelUnknown })
    );
    assert!(evaluation.actions.iter().any(|action| matches!(
        action,
        MonitorAction::Pause {
            reason: MonitorIssueCode::ModelUnknown
        }
    )));
}

#[test]
fn model_mismatch_and_missing_model_still_block_dispatch() {
    let (mismatch_monitor, mismatch_session) =
        model_guard_fixture("claude-sonnet", Some("claude-opus"), NOW, NOW + 200);
    let mismatch = evaluate_model(&mismatch_monitor, &mismatch_session, NOW + 200);
    assert!(mismatch.blocked);
    assert!(
        mismatch
            .issues
            .iter()
            .any(|item| { item.code == MonitorIssueCode::ModelMismatch })
    );

    let (missing_monitor, missing_session) =
        model_guard_fixture("claude-sonnet", None, NOW, NOW + 200);
    let missing = evaluate_model(&missing_monitor, &missing_session, NOW + 200);
    assert!(missing.blocked);
    assert!(missing.any_unknown);
    assert!(
        missing
            .issues
            .iter()
            .any(|item| item.code == MonitorIssueCode::ModelUnknown)
    );
}

#[test]
fn fresh_quota_callbacks_do_not_refresh_same_or_omitted_model_evidence() {
    for model_on_refresh in [Some("claude-sonnet"), None] {
        let (_directory, store) = open_store();
        let session_id = "session-model-field-ttl";
        let first_reset = NOW + 100;
        policy_ingest(
            &store,
            ACCOUNT,
            policy_observation(
                session_id,
                Some(1_000),
                Some(first_reset),
                Some("claude-sonnet"),
            ),
            NOW,
        );
        policy_record_spend(&store, policy_spend_input(ACCOUNT, NOW), NOW);
        let started =
            start_monitor_with_expected_model(&store, "goal-model-field-ttl", "claude-sonnet", NOW);
        assert!(started.runnable);
        assert_eq!(started.lifecycle, MonitorLifecycle::Waiting);
        assert_eq!(
            started.model_guard_validity,
            MonitorModelGuardValidity::Match
        );
        assert!(
            actions(&started)
                .iter()
                .any(|action| matches!(action, MonitorAction::Wait { .. }))
        );

        let reset_deadline = first_reset + MONITOR_RESET_GRACE_SECS;
        store
            .tick(reset_deadline)
            .expect("tick through reset grace with current model evidence");
        let due = policy_status(&store, &started.monitor_id, reset_deadline);
        assert_eq!(due.five_hour.reset_validity, MonitorResetValidity::Due);
        assert_eq!(due.model_guard_validity, MonitorModelGuardValidity::Match);
        assert!(
            due.issues
                .iter()
                .any(|issue| { issue.code == MonitorIssueCode::ResetDueUnverified })
        );
        assert!(!due.runnable);

        let refresh_at = NOW + MONITOR_EVIDENCE_TTL_SECS + 1;
        let advanced_reset = NOW + 7_200;
        policy_ingest(
            &store,
            ACCOUNT,
            policy_observation(
                session_id,
                Some(1_100),
                Some(advanced_reset),
                model_on_refresh,
            ),
            refresh_at,
        );
        let refreshed = policy_status(&store, &started.monitor_id, refresh_at);

        for window in [&refreshed.five_hour, &refreshed.seven_day] {
            assert_eq!(
                window.used_evidence.as_ref().unwrap().freshness,
                MonitorEvidenceFreshness::Current
            );
            assert_eq!(
                window.reset_evidence.as_ref().unwrap().freshness,
                MonitorEvidenceFreshness::Current
            );
            assert_eq!(window.reset_validity, MonitorResetValidity::Future);
        }
        let model_evidence = refreshed
            .model_evidence
            .as_ref()
            .expect("old model evidence remains visible");
        assert_eq!(model_evidence.evidence_received_at_epoch, NOW);
        assert_eq!(
            model_evidence.age_seconds,
            MONITOR_EVIDENCE_TTL_SECS as u64 + 1
        );
        assert_eq!(model_evidence.freshness, MonitorEvidenceFreshness::Stale);
        assert_eq!(refreshed.model.as_deref(), Some("claude-sonnet"));
        assert_eq!(
            refreshed.model_guard_validity,
            MonitorModelGuardValidity::Unknown
        );
        assert!(
            refreshed
                .issues
                .iter()
                .any(|issue| issue.code == MonitorIssueCode::ModelUnknown)
        );
        assert!(
            !refreshed
                .issues
                .iter()
                .any(|issue| issue.code == MonitorIssueCode::ModelMismatch)
        );
        assert_eq!(refreshed.lifecycle, MonitorLifecycle::Paused);
        assert!(actions(&refreshed).iter().any(|action| matches!(
            action,
            MonitorAction::Pause {
                reason: MonitorIssueCode::ModelUnknown
            }
        )));
        assert!(actions(&refreshed).iter().any(|action| matches!(
            action,
            MonitorAction::Pause {
                reason: MonitorIssueCode::ResetDueUnverified
            }
        )));
        assert!(
            !actions(&refreshed)
                .iter()
                .any(|action| matches!(action, MonitorAction::Wait { .. })),
            "the reset-grace barrier remains latched because usage did not fall"
        );
        assert!(!refreshed.runnable);
    }
}

#[test]
fn expired_session_context_makes_known_model_unknown() {
    let (monitor, session) = model_guard_fixture("claude-sonnet", Some("claude-sonnet"), NOW, NOW);
    let snapshot = model_status_snapshot(&monitor, &session, NOW + 301);
    assert_eq!(
        snapshot.model_guard_validity,
        MonitorModelGuardValidity::Unknown
    );

    let evaluation = evaluate_model(&monitor, &session, NOW + 301);

    assert!(evaluation.blocked);
    assert!(evaluation.any_unknown);
    assert!(
        evaluation
            .issues
            .iter()
            .any(|item| item.code == MonitorIssueCode::ModelUnknown)
    );
}

fn descriptor_quota_window(
    rank: u32,
    category: UsageWindowCategoryV1,
    label: &str,
    used: i32,
    reset_at_epoch: i64,
) -> UsageLimitWindowV1 {
    UsageLimitWindowV1 {
        window_id: format!("window-{rank}"),
        rank,
        category,
        label: label.to_owned(),
        value_label: format!("{used}% used"),
        reset_label: "reset time".to_owned(),
        remaining_percent: None,
        remaining_raw_percent: None,
        used_percent: Some(UsagePercent::clamp_raw(used)),
        used_raw_percent: Some(used),
        reset_at_epoch: Some(reset_at_epoch),
        quota_state: UsageQuotaStateV1::Available,
        pace_label: None,
        runs_out_label: None,
    }
}

fn quota_group(
    rank: u32,
    label: &str,
    used: i32,
    reset_at_epoch: i64,
    observed_at_epoch: i64,
    period: UsageMetricPeriodV1,
) -> UsageMetricGroupV1 {
    UsageMetricGroupV1 {
        group_id: format!("group-{rank}"),
        rank,
        kind: UsageMetricGroupKindV1::Window,
        label: label.to_owned(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch: Some(observed_at_epoch),
        fetched_at_epoch: observed_at_epoch,
        last_success_at_epoch: Some(observed_at_epoch),
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value: UsageMetricValueV1::Window {
            remaining_percent: None,
            remaining_raw_percent: None,
            used_percent: Some(UsagePercent::clamp_raw(used)),
            used_raw_percent: Some(used),
            period,
            unit: None,
        },
        reset_at_epoch: Some(reset_at_epoch),
        renews_at_epoch: None,
        issues: Vec::new(),
    }
}

fn provider_projection(
    observed_at_epoch: i64,
    account_generation: u64,
    broker_generation: u64,
) -> UsageProjectionV1 {
    let session_reset = NOW + 3_600;
    let weekly_reset = NOW + 86_400;
    let freshness = UsageFreshnessV1 {
        generation: account_generation,
        phase: UsageFreshnessPhaseV1::Current,
        last_good_at_epoch: Some(observed_at_epoch),
        retry_at_epoch: None,
        is_stale: false,
    };
    let account = UsageAccountV1 {
        canonical_account_id: "acct-provider-freshness".to_owned(),
        identity_kind: UsageIdentityKindV1::ProviderAccountId,
        rank: 0,
        display_label: "provider account".to_owned(),
        plan_label: None,
        status_label: Some("available".to_owned()),
        lifecycle: UsageLifecycleV1::Available,
        freshness: freshness.clone(),
        provenance_count: 1,
        windows: vec![
            descriptor_quota_window(
                0,
                UsageWindowCategoryV1::Session,
                "Session",
                20,
                session_reset,
            ),
            descriptor_quota_window(
                1,
                UsageWindowCategoryV1::LongRange,
                "Weekly",
                30,
                weekly_reset,
            ),
        ],
        metric_groups: vec![
            quota_group(
                0,
                "Session",
                20,
                session_reset,
                observed_at_epoch,
                UsageMetricPeriodV1::ProviderDefined,
            ),
            quota_group(
                1,
                "Weekly",
                30,
                weekly_reset,
                observed_at_epoch,
                UsageMetricPeriodV1::Calendar {
                    granularity: UsageCalendarPeriodV1::Weekly,
                },
            ),
        ],
        credential_expires_at_epoch: None,
        issues: Vec::new(),
    };
    UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: format!("provider-observation-{observed_at_epoch}-{broker_generation}"),
        generated_at_epoch: observed_at_epoch,
        discovery_revision: "fixture-discovery".to_owned(),
        broker_instance_id: "fixture-broker".to_owned(),
        broker_generation,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: vec![UsageProviderV1 {
            provider_id: "anthropic".to_owned(),
            display_name: "Anthropic".to_owned(),
            rank: 0,
            membership_state: UsageMembershipStateV1::Current,
            freshness,
            accounts: vec![account],
            issues: Vec::new(),
        }],
        unresolved: Vec::new(),
        issues: Vec::new(),
    }
}

fn start_approved_projection_observer(
    store: &MonitorStore,
    account_id: &str,
    source_account_id: &str,
    now_epoch: i64,
) -> (MonitorAccountBinding, String) {
    store.set_experimental_collector_source(Some(source_account_id.to_owned()));
    let binding = match store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: account_id.to_owned(),
                    provider_account_id: Some(source_account_id.to_owned()),
                    experimental_collector_approved: true,
                    operator_label: "test-operator".to_owned(),
                    operator_confirmed: true,
                },
            },
            now_epoch,
        )
        .expect("bind approved provider source")
    {
        MonitorReply::AccountBound { binding } => binding,
        other => panic!("expected account-bound reply, got {other:?}"),
    };
    let config = collector_config(&binding, MonitorPurpose::ObserveOnly);
    let monitor_id = match store
        .operate(
            MonitorOperation::Start {
                config,
                idempotency_key: format!("provider-freshness-observer-{account_id}"),
            },
            now_epoch,
        )
        .expect("start approved provider observer")
    {
        MonitorReply::Started { status } => status.monitor_id.clone(),
        other => panic!("expected started reply, got {other:?}"),
    };
    (binding, monitor_id)
}

fn descriptor_status(store: &MonitorStore, monitor_id: &str, now_epoch: i64) -> MonitorStatus {
    match store
        .operate(
            MonitorOperation::Status {
                monitor_id: monitor_id.to_owned(),
            },
            now_epoch,
        )
        .expect("read fixture monitor descriptor_status")
    {
        MonitorReply::Status { status } => *status,
        other => panic!("expected descriptor_status reply, got {other:?}"),
    }
}

#[test]
fn provider_observation_accepts_hashed_instance_id_and_rejects_unbounded_text() {
    let directory = tempfile::tempdir().expect("temporary monitor directory");
    let store = MonitorStore::open(directory.path()).expect("open monitor store");
    let local_account_id = "local-hashed-instance";
    start_approved_projection_observer(&store, local_account_id, "acct-provider-freshness", NOW);
    let mut projection = provider_projection(NOW, 1, 1);
    projection.broker_instance_id = format!("sha256:{}", "a".repeat(64));
    store
        .observe_projection(&projection, NOW)
        .expect("persist a production-format broker instance id");

    let state = store.lock().clone();
    assert_eq!(
        state.accounts[local_account_id]
            .provider_observation
            .as_ref()
            .unwrap()
            .broker_instance_id,
        projection.broker_instance_id
    );
    for invalid in [
        format!("sha256:\n{}", "a".repeat(64)),
        "a".repeat(MAX_ID_LENGTH + 1),
    ] {
        let mut corrupted = state.clone();
        corrupted
            .accounts
            .get_mut(local_account_id)
            .unwrap()
            .provider_observation
            .as_mut()
            .unwrap()
            .broker_instance_id = invalid;
        assert!(validate_store_state(&corrupted).is_err());
    }
}

#[test]
fn identical_provider_observation_refreshes_fields_but_replay_and_tick_do_not() {
    let directory = tempfile::tempdir().expect("temporary monitor directory");
    let store = MonitorStore::open(directory.path()).expect("open monitor store");
    let (_binding, monitor_id) = start_approved_projection_observer(
        &store,
        "local-provider-freshness",
        "acct-provider-freshness",
        NOW,
    );
    let initial_projection = provider_projection(NOW, 1, 1);
    store
        .observe_projection(&initial_projection, NOW)
        .expect("observe initial provider response");
    let initial = descriptor_status(&store, &monitor_id, NOW);
    assert_eq!(
        initial.readiness.provider,
        MonitorProviderReadiness::Current
    );
    assert_eq!(initial.five_hour.used_percentage_basis_points, Some(2_000));

    let refreshed_at = NOW + MONITOR_EVIDENCE_TTL_SECS + 1;
    let refreshed_projection = provider_projection(refreshed_at, 2, 2);
    store
        .observe_projection(&refreshed_projection, refreshed_at)
        .expect("observe genuine identical provider response");
    let refreshed = descriptor_status(&store, &monitor_id, refreshed_at);
    assert_eq!(
        refreshed.readiness.provider,
        MonitorProviderReadiness::Current
    );
    for window in [&refreshed.five_hour, &refreshed.seven_day] {
        for field in [
            window.used_evidence.as_ref(),
            window.reset_evidence.as_ref(),
        ] {
            let field = field.expect("provider response supplies each quota field");
            assert_eq!(field.evidence_received_at_epoch, refreshed_at);
            assert_eq!(field.age_seconds, 0);
            assert_eq!(field.freshness, MonitorEvidenceFreshness::Current);
        }
    }

    // A publication with the same account generation but an older broker
    // generation is stale even if it carries a later generated timestamp.
    let older_broker_generation = provider_projection(refreshed_at + 1, 2, 1);
    let sequence_before_stale_publication = store.lock().next_input_sequence;
    store
        .observe_projection(&older_broker_generation, refreshed_at + 1)
        .expect("ignore older broker generation");
    assert_eq!(
        store.lock().next_input_sequence,
        sequence_before_stale_publication
    );
    let after_older_publication = descriptor_status(&store, &monitor_id, refreshed_at + 2);
    for window in [
        &after_older_publication.five_hour,
        &after_older_publication.seven_day,
    ] {
        let used = window
            .used_evidence
            .as_ref()
            .expect("provider quota remains");
        assert_eq!(used.evidence_received_at_epoch, refreshed_at);
        assert_eq!(used.age_seconds, 2);
    }

    // Re-reading the same cached projection has a later receipt time but no
    // newer provider observation timestamp, so it cannot extend either TTL.
    let replayed_at = refreshed_at + MONITOR_EVIDENCE_TTL_SECS + 1;
    store
        .observe_projection(&refreshed_projection, replayed_at)
        .expect("replay cached projection");
    let replayed = descriptor_status(&store, &monitor_id, replayed_at);
    assert_eq!(
        replayed.readiness.provider,
        MonitorProviderReadiness::Current
    );
    for window in [&replayed.five_hour, &replayed.seven_day] {
        for field in [
            window.used_evidence.as_ref(),
            window.reset_evidence.as_ref(),
        ] {
            let field = field.expect("cached quota field remains visible");
            assert_eq!(field.evidence_received_at_epoch, refreshed_at);
            assert_eq!(field.age_seconds, 301);
            assert_eq!(field.freshness, MonitorEvidenceFreshness::Stale);
        }
    }

    store
        .tick(replayed_at + 1)
        .expect("timer-only reconciliation");
    let after_tick = descriptor_status(&store, &monitor_id, replayed_at + 1);
    assert!(!after_tick.runnable);
    assert_eq!(
        after_tick
            .five_hour
            .reset_evidence
            .as_ref()
            .unwrap()
            .evidence_received_at_epoch,
        refreshed_at,
        "timer reconciliation cannot create provider evidence"
    );
}

#[test]
fn provider_projection_maps_only_to_the_current_approved_local_account() {
    let directory = tempfile::tempdir().expect("temporary monitor directory");
    let store = MonitorStore::open(directory.path()).expect("open monitor store");
    let (_binding_a, approved_monitor) =
        start_approved_projection_observer(&store, "operator-local-a", "provider-source-a", NOW);
    let unapproved_binding = match store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: "provider-source-a".to_owned(),
                    provider_account_id: Some("provider-source-b".to_owned()),
                    experimental_collector_approved: false,
                    operator_label: "isolated-test-operator".to_owned(),
                    operator_confirmed: true,
                },
            },
            NOW,
        )
        .expect("bind a different unapproved source to a source-shaped local label")
    {
        MonitorReply::AccountBound { binding } => binding,
        other => panic!("expected account-bound reply, got {other:?}"),
    };
    let unapproved_monitor = match store
        .operate(
            MonitorOperation::Start {
                config: MonitorConfig {
                    provider: MonitorProvider::Claude,
                    purpose: MonitorPurpose::ObserveOnly,
                    scope: MonitorScope::BoundAccount {
                        binding_id: unapproved_binding.binding_id.clone(),
                        binding_revision: unapproved_binding.revision,
                        session_id: None,
                    },
                    goal_id: None,
                    expected_model: None,
                    policy_revision: None,
                    experimental_collector: false,
                },
                idempotency_key: "unapproved-source-observer".to_owned(),
            },
            NOW,
        )
        .expect("start passive observer without collector approval")
    {
        MonitorReply::Started { status } => status.monitor_id.clone(),
        other => panic!("expected started reply, got {other:?}"),
    };

    let mut projection = provider_projection(NOW, 1, 2);
    let mut second_source = projection.providers[0].accounts[0].clone();
    second_source.canonical_account_id = "provider-source-b".to_owned();
    second_source.rank = 1;
    projection.providers[0].accounts[0].canonical_account_id = "provider-source-a".to_owned();
    projection.providers[0].accounts.push(second_source);
    store
        .observe_projection(&projection, NOW)
        .expect("observe canonical provider projection");

    assert_eq!(store.collection_accounts(), vec!["provider-source-a"]);
    let approved = descriptor_status(&store, &approved_monitor, NOW);
    assert_eq!(approved.account_id.as_deref(), Some("operator-local-a"));
    assert_eq!(
        approved.readiness.provider,
        MonitorProviderReadiness::Current
    );
    assert_eq!(approved.five_hour.used_percentage_basis_points, Some(2_000));
    assert_eq!(approved.seven_day.used_percentage_basis_points, Some(3_000));

    let unapproved = descriptor_status(&store, &unapproved_monitor, NOW);
    assert_eq!(
        unapproved.readiness.provider,
        MonitorProviderReadiness::Unknown
    );
    assert_eq!(unapproved.five_hour.used_percentage_basis_points, None);
    assert_eq!(unapproved.seven_day.used_percentage_basis_points, None);
    let state = store.lock();
    assert!(state.accounts.contains_key("operator-local-a"));
    assert!(!state.accounts.contains_key("provider-source-a"));
    assert!(!state.accounts.contains_key("provider-source-b"));
}

#[test]
fn provider_incarnation_reset_preserves_last_good_and_exhausted_weekly_guard() {
    let directory = tempfile::tempdir().expect("temporary monitor directory");
    let store = MonitorStore::open(directory.path()).expect("open monitor store");
    let (_binding, _monitor_id) = start_approved_projection_observer(
        &store,
        "operator-local-incarnation-reset",
        "acct-provider-freshness",
        NOW,
    );
    let mut initial = provider_projection(NOW, 8, 12);
    initial.providers[0].accounts[0].windows[1].used_percent = Some(UsagePercent::clamp_raw(96));
    initial.providers[0].accounts[0].windows[1].used_raw_percent = Some(96);
    for group in &mut initial.providers[0].accounts[0].metric_groups {
        if group.label == "Weekly"
            && let UsageMetricValueV1::Window {
                used_percent,
                used_raw_percent,
                ..
            } = &mut group.value
        {
            *used_percent = Some(UsagePercent::clamp_raw(96));
            *used_raw_percent = Some(96);
        }
    }
    store
        .observe_projection(&initial, NOW)
        .expect("publish exhausted weekly provider quota");

    let before = store.lock();
    let account_before = &before.accounts["operator-local-incarnation-reset"];
    let barrier_before = account_before.reset_barriers[1]
        .as_ref()
        .expect("exhausted weekly window latches a reset barrier")
        .clone();
    assert_eq!(
        barrier_before.pause_reason,
        MonitorIssueCode::LimitGuardReached
    );
    assert_eq!(
        account_before
            .provider_observation
            .as_ref()
            .unwrap()
            .last_good_at_epoch,
        Some(NOW)
    );
    drop(before);

    // A newer catalog publication can restart the account generation while
    // retaining the broker incarnation. A new broker incarnation can restart
    // both counters. Neither ordering reset releases quota guards.
    for (step, account_generation, broker_generation, broker_instance_id) in [
        (1, 1, 13, "fixture-broker"),
        (2, 0, 1, "restarted-fixture-broker"),
    ] {
        let mut restarted = provider_projection(NOW - 1, account_generation, broker_generation);
        restarted.broker_instance_id = broker_instance_id.to_owned();
        let account = &mut restarted.providers[0].accounts[0];
        account.freshness = UsageFreshnessV1 {
            generation: account_generation,
            phase: UsageFreshnessPhaseV1::Failed,
            last_good_at_epoch: Some(NOW - 1),
            retry_at_epoch: None,
            is_stale: true,
        };
        account.issues = vec![UsageIssueV1 {
            code: "provider_timeout".to_owned(),
            scope: UsageIssueScopeV1::Account,
            recoverability: UsageIssueRecoverabilityV1::Retryable,
            message: "provider timeout".to_owned(),
            retry_at_epoch: None,
        }];
        for window in &mut account.windows {
            if window.category == UsageWindowCategoryV1::LongRange {
                window.used_percent = Some(UsagePercent::clamp_raw(96));
                window.used_raw_percent = Some(96);
            }
        }
        for group in &mut account.metric_groups {
            group.phase = UsageFreshnessPhaseV1::Failed;
            group.is_stale = true;
            group.observed_at_epoch = None;
            group.fetched_at_epoch = NOW + step;
            group.last_success_at_epoch = Some(NOW - 1);
            if group.label == "Weekly"
                && let UsageMetricValueV1::Window {
                    used_percent,
                    used_raw_percent,
                    ..
                } = &mut group.value
            {
                *used_percent = Some(UsagePercent::clamp_raw(96));
                *used_raw_percent = Some(96);
            }
        }
        store
            .observe_projection(&restarted, NOW + step)
            .expect("observe lower generation after catalog or broker reset");

        let after = store.lock();
        let account_after = &after.accounts["operator-local-incarnation-reset"];
        let observation = account_after
            .provider_observation
            .as_ref()
            .expect("provider failure diagnostics retained");
        assert_eq!(observation.generation, account_generation);
        assert_eq!(observation.broker_generation, broker_generation);
        assert_eq!(observation.broker_instance_id, broker_instance_id);
        assert_eq!(observation.last_good_at_epoch, Some(NOW));
        assert_eq!(observation.readiness, MonitorProviderReadiness::Stale);
        let barrier = account_after.reset_barriers[1]
            .as_ref()
            .expect("generation reset cannot release exhausted weekly guard");
        assert_eq!(barrier.started_at_epoch, barrier_before.started_at_epoch);
        assert_eq!(
            barrier.prior_reset_at_epoch,
            barrier_before.prior_reset_at_epoch
        );
        assert_eq!(
            account_after.latest_reset_epochs[1],
            barrier_before.prior_reset_at_epoch
        );
        assert_eq!(
            account_after.broker_windows[1]
                .used
                .as_ref()
                .expect("last good weekly value remains stored")
                .value,
            9_600
        );
    }
}

#[test]
fn rate_limit_keeps_original_quota_age_and_does_not_block_fresh_statusline() {
    let directory = tempfile::tempdir().expect("temporary monitor directory");
    let store = MonitorStore::open(directory.path()).expect("open monitor store");
    let (binding, monitor_id) = start_approved_projection_observer(
        &store,
        "operator-local-rate-limited",
        "acct-provider-freshness",
        NOW,
    );
    store
        .observe_projection(&provider_projection(NOW, 1, 1), NOW)
        .expect("publish initial provider success");

    let retry_at_epoch = NOW + 900;
    let mut failed = provider_projection(NOW + 10, 2, 2);
    let account = &mut failed.providers[0].accounts[0];
    account.freshness = UsageFreshnessV1 {
        generation: 2,
        phase: UsageFreshnessPhaseV1::Failed,
        last_good_at_epoch: Some(NOW),
        retry_at_epoch: Some(retry_at_epoch),
        is_stale: true,
    };
    account.issues = vec![UsageIssueV1 {
        code: "rate_limited".to_owned(),
        scope: UsageIssueScopeV1::Account,
        recoverability: UsageIssueRecoverabilityV1::Retryable,
        message: "rate limited".to_owned(),
        retry_at_epoch: Some(retry_at_epoch),
    }];
    for group in &mut account.metric_groups {
        group.phase = UsageFreshnessPhaseV1::Failed;
        group.is_stale = true;
        group.observed_at_epoch = None;
        group.fetched_at_epoch = NOW + 10;
        group.last_success_at_epoch = Some(NOW);
    }
    store
        .observe_projection(&failed, NOW + 10)
        .expect("publish rate-limited last-good state");

    let expired = descriptor_status(&store, &monitor_id, NOW + MONITOR_EVIDENCE_TTL_SECS + 1);
    assert_eq!(
        expired.readiness.provider,
        MonitorProviderReadiness::RateLimited
    );
    assert_eq!(expired.readiness.quota, MonitorQuotaReadiness::Stale);
    assert_eq!(expired.five_hour.used_percentage_basis_points, Some(2_000));
    assert_eq!(expired.seven_day.used_percentage_basis_points, Some(3_000));
    for window in [&expired.five_hour, &expired.seven_day] {
        let used = window.used_evidence.as_ref().expect("retained quota value");
        assert_eq!(used.evidence_at_epoch, Some(NOW));
        assert_eq!(used.evidence_received_at_epoch, NOW);
        assert_eq!(used.age_seconds, 301);
        assert_eq!(used.freshness, MonitorEvidenceFreshness::Stale);
    }
    let rate_issue = expired
        .issues
        .iter()
        .find(|issue| issue.code == MonitorIssueCode::ProviderRateLimited)
        .expect("provider rate-limit issue is visible");
    assert_eq!(rate_issue.retry_at_epoch, Some(retry_at_epoch));

    store
        .operate(
            MonitorOperation::Ingest {
                scope: MonitorScope::BoundAccount {
                    binding_id: binding.binding_id.clone(),
                    binding_revision: binding.revision,
                    session_id: None,
                },
                observation: observation_with_windows(
                    "fresh-statusline-after-provider-failure",
                    Some(42),
                    Some(NOW + 3_600),
                    Some(34),
                    Some(NOW + 86_400),
                ),
            },
            NOW + MONITOR_EVIDENCE_TTL_SECS + 2,
        )
        .expect("ingest independent fresh statusline quota");
    let recovered_quota =
        descriptor_status(&store, &monitor_id, NOW + MONITOR_EVIDENCE_TTL_SECS + 2);
    assert_eq!(
        recovered_quota.readiness.provider,
        MonitorProviderReadiness::RateLimited
    );
    assert_eq!(
        recovered_quota.readiness.quota,
        MonitorQuotaReadiness::Ready
    );
    assert!(
        recovered_quota
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::ProviderRateLimited),
        "provider diagnostics remain visible independently of fresh statusline quota"
    );
    assert!(
        !recovered_quota
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::QuotaStale)
    );
}
