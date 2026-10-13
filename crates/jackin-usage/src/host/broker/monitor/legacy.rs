// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Explicit migration from the original durable usage monitor schema.

use std::collections::BTreeMap;

use jackin_protocol::usage_monitor::{MonitorIssue, MonitorIssueCode, MonitorPolicyRecord};
use serde::Deserialize;

const V1_SCHEMA_VERSION: u16 = 1;
const MAX_MONITORS: usize = 128;
const MAX_ACCOUNTS: usize = 128;
const MAX_GOALS: usize = 128;
const MAX_SESSIONS_PER_ACCOUNT: usize = 16;
const MAX_EVIDENCE_PER_MONITOR: usize = 96;
const MAX_EVENTS_PER_MONITOR: usize = 8;
const MAX_ID_LENGTH: usize = 128;
const MAX_GOAL_ID_LENGTH: usize = 256;
const MAX_MODEL_LENGTH: usize = 128;
const MAX_FUTURE_SKEW_SECS: i64 = 60;
const LEGACY_BINDING_LABEL: &str = "Migrated V1 binding (unconfirmed)";

mod conversion;
mod validation;

pub(super) use conversion::migrate_v1;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1StoreState {
    schema_version: u16,
    next_monitor_id: u64,
    next_input_sequence: u64,
    last_now_epoch: i64,
    accounts: BTreeMap<String, V1AccountObservations>,
    monitors: BTreeMap<String, V1DurableMonitor>,
    goals: BTreeMap<String, V1DurableGoalSpend>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1AccountObservations {
    sessions: BTreeMap<String, V1SessionObservation>,
    broker_windows: [V1ObservedWindow; 2],
    latest_reset_epochs: [Option<i64>; 2],
    reset_barriers: [Option<V1AccountResetBarrier>; 2],
    input_sequence: u64,
    spend: V1SpendAccountState,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1SessionObservation {
    #[serde(deserialize_with = "required")]
    model: Option<V1Observed<String>>,
    windows: [V1ObservedWindow; 2],
    #[serde(deserialize_with = "required")]
    last_observation_received_at_epoch: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1ObservedWindow {
    #[serde(deserialize_with = "required")]
    used: Option<V1ObservedPercentage>,
    #[serde(deserialize_with = "required")]
    reset: Option<V1Observed<i64>>,
    #[serde(deserialize_with = "required")]
    paired: Option<V1ObservedQuotaPair>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1ObservedQuotaPair {
    used_percentage_basis_points: i32,
    reset_at_epoch: i64,
    #[serde(deserialize_with = "required")]
    evidence_at_epoch: Option<i64>,
    received_at_epoch: i64,
    input_sequence: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1Observed<T> {
    value: T,
    #[serde(deserialize_with = "required")]
    evidence_at_epoch: Option<i64>,
    received_at_epoch: i64,
    input_sequence: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1ObservedPercentage {
    value: i32,
    #[serde(deserialize_with = "required")]
    reset_at_epoch: Option<i64>,
    #[serde(deserialize_with = "required")]
    evidence_at_epoch: Option<i64>,
    received_at_epoch: i64,
    input_sequence: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1DurableMonitor {
    config: V1MonitorConfig,
    created_at_epoch: i64,
    #[serde(deserialize_with = "required")]
    stopped_at_epoch: Option<i64>,
    updated_at_epoch: i64,
    last_reconciled_at_epoch: i64,
    next_evidence_sequence: u64,
    next_decision_sequence: u64,
    next_event_sequence: u64,
    evidence: Vec<V1MonitorEvidence>,
    evidence_fingerprints: BTreeMap<String, String>,
    reset_barriers: [Option<V1ResetBarrier>; 2],
    #[serde(deserialize_with = "required")]
    latest_decision: Option<V1MonitorDecision>,
    #[serde(deserialize_with = "required")]
    decision_fingerprint: Option<String>,
    events: Vec<V1MonitorEvent>,
    spend_state: V1SpendState,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1MonitorConfig {
    provider: V1MonitorProvider,
    account_id: String,
    goal_id: String,
    #[serde(deserialize_with = "required")]
    session_id: Option<String>,
    #[serde(deserialize_with = "required")]
    expected_model: Option<String>,
    #[serde(deserialize_with = "required")]
    budget: Option<V1Money>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1DurableGoalSpend {
    account_id: String,
    #[serde(deserialize_with = "required")]
    budget: Option<V1Money>,
    spend_state: V1SpendState,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1ResetBarrier {
    started_at_epoch: i64,
    #[serde(deserialize_with = "required")]
    prior_reset_at_epoch: Option<i64>,
    prior_used_percentage_basis_points: i32,
    evidence_sequence_before: u64,
    #[serde(deserialize_with = "required")]
    dependency_evidence_sequence: Option<u64>,
    pause_reason: V1MonitorIssueCode,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1AccountResetBarrier {
    started_at_epoch: i64,
    #[serde(deserialize_with = "required")]
    prior_reset_at_epoch: Option<i64>,
    prior_used_percentage_basis_points: i32,
    input_sequence_before: u64,
    source: V1MonitorEvidenceSource,
    #[serde(deserialize_with = "required")]
    session_id: Option<String>,
    pause_reason: V1MonitorIssueCode,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1SpendAccountState {
    #[serde(deserialize_with = "required")]
    latest_record: Option<V1SpendRecord>,
    #[serde(deserialize_with = "required")]
    current_period_record: Option<V1SpendRecord>,
    #[serde(deserialize_with = "required")]
    previous_period_record: Option<V1SpendRecord>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct V1SpendState {
    #[serde(deserialize_with = "required")]
    baseline: Option<V1SpendRecord>,
    #[serde(deserialize_with = "required")]
    period_anchor: Option<V1SpendRecord>,
    #[serde(deserialize_with = "required")]
    cumulative_goal_spend: Option<V1Money>,
    rollover_unknown: bool,
    cumulative_complete: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct V1SpendRecord {
    account_id: String,
    billing_period_start_epoch: i64,
    billing_period_end_epoch: i64,
    amount: V1Money,
    #[serde(deserialize_with = "required")]
    evidence_at_epoch: Option<i64>,
    evidence_received_at_epoch: i64,
    source: V1SpendRecordSource,
    verification: V1SpendVerification,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct V1Money {
    amount_minor: i64,
    currency: String,
    exponent: u8,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum V1MonitorProvider {
    Claude,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum V1MonitorLifecycle {
    Active,
    Waiting,
    Paused,
    Stopped,
    NeedsEvidence,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum V1MonitorEvidenceFreshness {
    Current,
    Stale,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum V1MonitorEvidenceSource {
    BrokerProjection,
    Statusline,
    ProviderSpend,
    Operator,
    LocalSessionLog,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum V1SpendVerification {
    Verified,
    Unverified,
    Stale,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum V1SpendRecordSource {
    OperatorReceipt,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum V1MonitorQuotaWindow {
    FiveHour,
    SevenDay,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum V1MonitorEvidenceValue {
    QuotaWindow {
        window_id: String,
        #[serde(deserialize_with = "required")]
        remaining_raw_percent: Option<i32>,
        #[serde(deserialize_with = "required")]
        reset_at_epoch: Option<i64>,
    },
    QuotaUsedPercentage {
        window: V1MonitorQuotaWindow,
        used_percentage_basis_points: i32,
    },
    QuotaReset {
        window: V1MonitorQuotaWindow,
        reset_at_epoch: i64,
    },
    Model {
        model: String,
    },
    Spend {
        amount: V1Money,
        billing_period_start_epoch: i64,
        billing_period_end_epoch: i64,
        verification: V1SpendVerification,
    },
    Budget {
        amount: V1Money,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1MonitorEvidence {
    sequence: u64,
    account_id: String,
    #[serde(deserialize_with = "required")]
    session_id: Option<String>,
    source: V1MonitorEvidenceSource,
    #[serde(deserialize_with = "required")]
    evidence_at_epoch: Option<i64>,
    evidence_received_at_epoch: i64,
    age_seconds: u64,
    value: V1MonitorEvidenceValue,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1MonitorFieldEvidence {
    evidence_sequence: u64,
    #[serde(deserialize_with = "required")]
    evidence_at_epoch: Option<i64>,
    evidence_received_at_epoch: i64,
    age_seconds: u64,
    freshness: V1MonitorEvidenceFreshness,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1MonitorQuotaWindowStatus {
    #[serde(deserialize_with = "required")]
    used_percentage_basis_points: Option<i32>,
    #[serde(deserialize_with = "required")]
    used_evidence: Option<V1MonitorFieldEvidence>,
    #[serde(deserialize_with = "required")]
    reset_at_epoch: Option<i64>,
    #[serde(deserialize_with = "required")]
    reset_evidence: Option<V1MonitorFieldEvidence>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum V1MonitorAction {
    Checkpoint {
        goal_id: String,
    },
    ReduceDispatch {
        #[serde(deserialize_with = "required")]
        max_parallel: Option<u32>,
    },
    Pause {
        reason: V1MonitorIssueCode,
    },
    Wait {
        #[serde(deserialize_with = "required")]
        until_epoch: Option<i64>,
    },
    Warn {
        reason: V1MonitorIssueCode,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1MonitorDecision {
    sequence: u64,
    decided_at_epoch: i64,
    evidence_sequences: Vec<u64>,
    actions: Vec<V1MonitorAction>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum V1MonitorIssueCode {
    IndependentRefreshDisabled,
    AuthStatusUnknown,
    BrokerUnavailable,
    StatuslineIngressUnsupported,
    StatuslineTooLarge,
    StatuslineInvalid,
    AccountMismatch,
    ObservationStale,
    QuotaUnknown,
    SpendUnverified,
    SpendStale,
    SpendUnavailable,
    BudgetUnverifiable,
    ResetDueUnverified,
    LimitExhausted,
    LimitGuardReached,
    InteractionRequired,
    QuotaStale,
    MissingReset,
    BudgetWarn,
    BudgetCheckpoint,
    BudgetPause,
    SpendRolloverUnverified,
    ModelMismatch,
    ModelUnknown,
    MonitorNotFound,
    MonitorStoreUnavailable,
    WaitTimeout,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1MonitorIssue {
    code: V1MonitorIssueCode,
    message: String,
    #[serde(deserialize_with = "required")]
    retry_at_epoch: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1MonitorStatus {
    schema_version: u16,
    monitor_id: String,
    provider: V1MonitorProvider,
    account_id: String,
    goal_id: String,
    #[serde(deserialize_with = "required")]
    session_id: Option<String>,
    #[serde(deserialize_with = "required")]
    model: Option<String>,
    #[serde(deserialize_with = "required")]
    model_evidence: Option<V1MonitorFieldEvidence>,
    lifecycle: V1MonitorLifecycle,
    runnable: bool,
    five_hour: V1MonitorQuotaWindowStatus,
    seven_day: V1MonitorQuotaWindowStatus,
    #[serde(deserialize_with = "required")]
    budget: Option<V1Money>,
    #[serde(deserialize_with = "required")]
    cumulative_goal_spend: Option<V1Money>,
    #[serde(deserialize_with = "required")]
    spend_period_baseline: Option<V1SpendRecord>,
    evidence: Vec<V1MonitorEvidence>,
    #[serde(deserialize_with = "required")]
    latest_decision: Option<V1MonitorDecision>,
    updated_at_epoch: i64,
    issues: Vec<V1MonitorIssue>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1MonitorEvent {
    sequence: u64,
    occurred_at_epoch: i64,
    status: V1MonitorStatus,
}

struct V1StatusMigrationContext<'a> {
    monitor_id: &'a str,
    config: &'a V1MonitorConfig,
    binding_id: &'a str,
    policy: &'a MonitorPolicyRecord,
    next_evidence_sequence: u64,
    next_decision_sequence: u64,
    last_now_epoch: i64,
}

fn required<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer)
}

fn unavailable() -> MonitorIssue {
    MonitorIssue {
        code: MonitorIssueCode::MonitorStoreUnavailable,
        message: "monitor state is unavailable or invalid".to_owned(),
        retry_at_epoch: None,
    }
}

#[cfg(test)]
mod tests;
