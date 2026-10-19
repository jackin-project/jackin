// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::control::Money;

pub const USAGE_MONITOR_SCHEMA_VERSION: u16 = 4;

pub const USAGE_STATUSLINE_INPUT_SCHEMA_VERSION: u16 = 2;

pub const USAGE_MONITOR_MAX_STATUSLINE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MonitorProvider {
    Claude,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MonitorPurpose {
    ObserveOnly,
    DispatchGuard,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "scope", rename_all = "snake_case", deny_unknown_fields)]
pub enum MonitorScope {
    Session {
        session_id: String,
    },
    BoundAccount {
        binding_id: String,
        binding_revision: u64,
        session_id: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MonitorPolicy {
    StrictSgd,
    QuotaOnly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MonitorPolicyOrigin {
    Operator,
    MigratedV1,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorTrackingReadiness {
    Ready,
    Waiting,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorQuotaReadiness {
    Ready,
    Unknown,
    Stale,
    Exhausted,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorBudgetReadiness {
    Verified,
    Unknown,
    Stale,
    Disabled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorDispatchReadiness {
    Ready,
    Blocked,
    NotAuthorized,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorReadiness {
    pub tracking: MonitorTrackingReadiness,
    pub quota: MonitorQuotaReadiness,
    pub budget: MonitorBudgetReadiness,
    pub dispatch: MonitorDispatchReadiness,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorAccountBindingInput {
    pub provider: MonitorProvider,
    pub account_id: String,
    #[serde(default)]
    pub provider_account_id: Option<String>,
    #[serde(default)]
    pub experimental_collector_approved: bool,
    pub operator_label: String,
    pub operator_confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorAccountBinding {
    pub binding_id: String,
    pub provider: MonitorProvider,
    pub account_id: String,
    #[serde(default)]
    pub provider_account_id: Option<String>,
    #[serde(default)]
    pub experimental_collector_approved: bool,
    pub operator_label: String,
    pub revision: u64,
    pub operator_confirmed: bool,
    pub confirmed_at_epoch: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorPolicyApprovalInput {
    pub binding_id: String,
    pub binding_revision: u64,
    pub goal_id: String,
    pub new_policy: MonitorPolicy,
    pub budget: Option<Money>,
    pub operator_label: String,
    pub operator_confirmed: bool,
    pub acknowledge_no_sgd_cap: bool,
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorPolicyRecord {
    pub provider: MonitorProvider,
    pub account_id: String,
    pub binding_id: Option<String>,
    pub binding_revision: Option<u64>,
    pub goal_id: String,
    pub previous_policy: Option<MonitorPolicy>,
    pub new_policy: MonitorPolicy,
    pub budget: Option<Money>,
    pub operator_label: Option<String>,
    pub operator_confirmed: bool,
    pub acknowledge_no_sgd_cap: bool,
    pub recorded_at_epoch: Option<i64>,
    pub revision: u64,
    pub origin: MonitorPolicyOrigin,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum MonitorOperation {
    Start {
        config: MonitorConfig,
        idempotency_key: String,
    },
    BindAccount {
        binding: MonitorAccountBindingInput,
    },
    ApprovePolicy {
        approval: MonitorPolicyApprovalInput,
    },
    Stop {
        monitor_id: String,
    },
    Status {
        monitor_id: String,
    },
    Doctor {
        provider: MonitorProvider,
    },
    Ingest {
        scope: MonitorScope,
        observation: StatuslineObservation,
    },
    RecordSpend {
        record: SpendRecordInput,
    },
    Refresh {
        monitor_id: String,
    },
    ServiceStatus,
    ServiceStop,
    Watch {
        monitor_id: String,
        after_sequence: u64,
        timeout_ms: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorConfig {
    pub provider: MonitorProvider,
    pub purpose: MonitorPurpose,
    pub scope: MonitorScope,
    pub goal_id: Option<String>,
    pub expected_model: Option<String>,
    pub policy_revision: Option<u64>,
    #[serde(default)]
    pub experimental_collector: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StatuslineQuotaWindow {
    pub used_percentage_basis_points: Option<i32>,
    pub reset_at_epoch: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StatuslineRateLimits {
    pub five_hour: Option<StatuslineQuotaWindow>,
    pub seven_day: Option<StatuslineQuotaWindow>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StatuslineObservation {
    pub schema_version: u16,
    pub session_id: String,
    pub model: Option<String>,
    pub claude_code_version: Option<String>,
    pub rate_limits: StatuslineRateLimits,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorEvidenceFreshness {
    Current,
    Stale,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorResetValidity {
    #[default]
    Unknown,
    Future,
    Due,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorModelGuardValidity {
    NotConfigured,
    Unknown,
    Match,
    Mismatch,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorFieldEvidence {
    pub evidence_sequence: u64,
    pub evidence_at_epoch: Option<i64>,
    pub evidence_received_at_epoch: i64,
    pub age_seconds: u64,
    pub freshness: MonitorEvidenceFreshness,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorQuotaWindowStatus {
    pub used_percentage_basis_points: Option<i32>,
    pub used_evidence: Option<MonitorFieldEvidence>,
    pub reset_at_epoch: Option<i64>,
    pub reset_validity: MonitorResetValidity,
    pub reset_evidence: Option<MonitorFieldEvidence>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorLifecycle {
    Active,
    Waiting,
    Paused,
    Stopped,
    NeedsEvidence,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorEvidenceSource {
    BrokerProjection,
    Statusline,
    ProviderSpend,
    Operator,
    LocalSessionLog,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpendVerification {
    Verified,
    Unverified,
    Stale,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpendRecordSource {
    OperatorReceipt,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SpendRecordInput {
    pub account_id: String,
    pub billing_period_start_epoch: i64,
    pub billing_period_end_epoch: i64,
    pub amount: Money,
    pub evidence_at_epoch: Option<i64>,
    pub verified: bool,
    pub source: SpendRecordSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SpendRecord {
    pub account_id: String,
    pub billing_period_start_epoch: i64,
    pub billing_period_end_epoch: i64,
    pub amount: Money,
    pub evidence_at_epoch: Option<i64>,
    pub evidence_received_at_epoch: i64,
    pub source: SpendRecordSource,
    pub verification: SpendVerification,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MonitorEvidenceValue {
    QuotaWindow {
        window_id: String,
        remaining_raw_percent: Option<i32>,
        reset_at_epoch: Option<i64>,
    },
    QuotaUsedPercentage {
        window: MonitorQuotaWindow,
        used_percentage_basis_points: i32,
    },
    QuotaReset {
        window: MonitorQuotaWindow,
        reset_at_epoch: i64,
    },
    Model {
        model: String,
    },
    Spend {
        amount: Money,
        billing_period_start_epoch: i64,
        billing_period_end_epoch: i64,
        verification: SpendVerification,
    },
    Budget {
        amount: Money,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorQuotaWindow {
    FiveHour,
    SevenDay,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorEvidence {
    pub sequence: u64,
    pub account_id: Option<String>,
    pub session_id: Option<String>,
    pub claude_code_version: Option<String>,
    pub source: MonitorEvidenceSource,
    pub evidence_at_epoch: Option<i64>,
    pub evidence_received_at_epoch: i64,
    pub age_seconds: u64,
    pub value: MonitorEvidenceValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum MonitorAction {
    Checkpoint { goal_id: String },

    ReduceDispatch { max_parallel: Option<u32> },

    Pause { reason: MonitorIssueCode },

    Wait { until_epoch: Option<i64> },

    Warn { reason: MonitorIssueCode },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorDecision {
    pub sequence: u64,
    pub decided_at_epoch: i64,
    pub evidence_sequences: Vec<u64>,
    pub actions: Vec<MonitorAction>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorIssueCode {
    IndependentRefreshDisabled,
    AuthStatusUnknown,
    BrokerUnavailable,
    StatuslineIngressUnsupported,
    StatuslineTooLarge,
    StatuslineInvalid,
    AccountMismatch,
    BindingRequired,
    BindingMismatch,
    PolicyRequired,
    PolicyConflict,
    IdempotencyConflict,
    ObservationOnly,
    OperatorConfirmationRequired,
    SgdCapAcknowledgementRequired,
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
    CollectorAuthRequired,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorIssue {
    pub code: MonitorIssueCode,
    pub message: String,
    pub retry_at_epoch: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorStatus {
    pub schema_version: u16,
    pub monitor_id: String,
    pub provider: MonitorProvider,
    pub purpose: MonitorPurpose,
    pub scope: MonitorScope,
    pub account_id: Option<String>,
    pub goal_id: Option<String>,
    pub session_id: Option<String>,
    pub claude_code_version: Option<String>,
    pub policy: Option<MonitorPolicyRecord>,
    pub expected_model: Option<String>,
    pub model: Option<String>,
    pub model_evidence: Option<MonitorFieldEvidence>,
    pub model_guard_validity: MonitorModelGuardValidity,
    pub lifecycle: MonitorLifecycle,
    pub readiness: MonitorReadiness,
    pub runnable: bool,
    pub five_hour: MonitorQuotaWindowStatus,
    pub seven_day: MonitorQuotaWindowStatus,
    pub budget: Option<Money>,
    pub cumulative_goal_spend: Option<Money>,
    pub spend_period_baseline: Option<SpendRecord>,
    pub evidence: Vec<MonitorEvidence>,
    pub latest_decision: Option<MonitorDecision>,
    pub updated_at_epoch: i64,
    pub issues: Vec<MonitorIssue>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorAuthState {
    Unknown,
    Prepared,
    ActionRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorDoctorReport {
    pub provider: MonitorProvider,
    pub broker_available: bool,
    pub statusline_ingress_supported: bool,
    pub auth_state: MonitorAuthState,
    pub issues: Vec<MonitorIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorServiceStatus {
    pub running: bool,
    pub experimental_collector_source: Option<String>,
    pub active_monitors: u32,
    pub next_wake_epoch: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorEvent {
    pub sequence: u64,
    pub occurred_at_epoch: i64,
    pub status: MonitorStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum MonitorReply {
    AccountBound {
        binding: MonitorAccountBinding,
    },
    PolicyApproved {
        policy: MonitorPolicyRecord,
    },
    Started {
        status: Box<MonitorStatus>,
    },
    Stopped {
        status: Box<MonitorStatus>,
    },
    Status {
        status: Box<MonitorStatus>,
    },
    Doctor {
        report: MonitorDoctorReport,
    },
    Ingested {
        scope: MonitorScope,
        account_id: Option<String>,
        session_id: String,
        evidence_sequence: u64,
    },
    SpendRecorded {
        record: SpendRecord,
    },
    Refreshed {
        status: Box<MonitorStatus>,
    },
    Watch {
        events: Vec<MonitorEvent>,
        next_sequence: u64,
        timed_out: bool,
    },
    ServiceStatus {
        status: MonitorServiceStatus,
    },
    ServiceStopped,
}

#[cfg(test)]
mod tests;
