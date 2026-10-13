// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Versioned, secret-free monitor control and evidence records.

use serde::{Deserialize, Serialize};

use crate::control::Money;

/// Version of durable monitor-state records.
pub const USAGE_MONITOR_SCHEMA_VERSION: u16 = 4;

/// Version of the normalized statusline input accepted by the monitor.
pub const USAGE_STATUSLINE_INPUT_SCHEMA_VERSION: u16 = 2;

/// Maximum UTF-8 bytes accepted for one statusline JSON input.
pub const USAGE_MONITOR_MAX_STATUSLINE_BYTES: usize = 16 * 1024;

/// Provider supported by the first unattended monitor contract.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MonitorProvider {
    /// Anthropic Claude Code.
    Claude,
}

/// Whether a monitor only observes evidence or can authorize dispatch.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MonitorPurpose {
    /// Persist and report observations without creating a goal or permitting work.
    ObserveOnly,
    /// Evaluate an explicitly approved policy for a linked goal.
    DispatchGuard,
}

/// Evidence partition selected by the operator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "scope", rename_all = "snake_case", deny_unknown_fields)]
pub enum MonitorScope {
    /// Session evidence without any asserted account identity.
    Session {
        /// Exact Claude Code session identifier.
        session_id: String,
    },
    /// Evidence assigned to a separately confirmed local account binding.
    BoundAccount {
        /// Broker-assigned binding identifier.
        binding_id: String,
        /// Binding revision confirmed by the operator.
        binding_revision: u64,
        /// Optional session restriction for this account-scoped monitor.
        session_id: Option<String>,
    },
}

/// Explicit policy selected by a durable operator approval.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MonitorPolicy {
    /// Enforce an account-bound SGD spend budget in addition to quota guards.
    StrictSgd,
    /// Enforce quota guards while explicitly disabling SGD spend enforcement.
    QuotaOnly,
}

/// Provenance of a durable policy record.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MonitorPolicyOrigin {
    /// Explicit operator action recorded by the broker.
    Operator,
    /// Strict behavior carried forward from the pre-v2 monitor schema.
    MigratedV1,
}

/// Tracking capability for the selected monitor scope.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorTrackingReadiness {
    /// The local observer can accept evidence for this scope.
    Ready,
    /// The observer is configured but has not received required evidence yet.
    Waiting,
    /// The local broker or evidence path is unavailable.
    Unavailable,
}

/// Readiness of required quota evidence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorQuotaReadiness {
    /// All required quota evidence is current and usable.
    Ready,
    /// Required quota evidence has not been received.
    Unknown,
    /// Required quota evidence is present but stale.
    Stale,
    /// A current quota observation is exhausted or past its guard.
    Exhausted,
}

/// Readiness of the spend guard.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorBudgetReadiness {
    /// Required spend and baseline evidence is verified and current.
    Verified,
    /// Required spend or baseline evidence is unavailable or unverifiable.
    Unknown,
    /// Required spend evidence is present but stale.
    Stale,
    /// The approved policy explicitly disables spend enforcement.
    Disabled,
}

/// Final dispatch readiness, separate from tracking and evidence status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorDispatchReadiness {
    /// Current evidence and policy permit dispatch.
    Ready,
    /// Dispatch is authorized by policy but blocked by current conditions.
    Blocked,
    /// This monitor's purpose never grants dispatch authority.
    NotAuthorized,
}

/// Independent readiness dimensions for a durable monitor.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorReadiness {
    /// Whether evidence can be tracked for the selected scope.
    pub tracking: MonitorTrackingReadiness,
    /// Whether required quota evidence is current and usable.
    pub quota: MonitorQuotaReadiness,
    /// Whether the approved spend policy is satisfied or disabled.
    pub budget: MonitorBudgetReadiness,
    /// Final dispatch state; must agree with the authoritative runnable field.
    pub dispatch: MonitorDispatchReadiness,
}

/// Operator confirmation input for a local account label.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorAccountBindingInput {
    /// Provider whose account is represented by the operator label.
    pub provider: MonitorProvider,
    /// Stable local account partition key selected by the operator.
    pub account_id: String,
    /// Opaque local source capability ID, derived from the exact selected
    /// Keychain service. This is not a provider-authenticated account identity
    /// and never contains the service name or credential material.
    #[serde(default)]
    pub provider_account_id: Option<String>,
    /// Explicit operator approval for this mapped source to use the
    /// experimental collector. Separate from monitor policy and disabled by
    /// default.
    #[serde(default)]
    pub experimental_collector_approved: bool,
    /// Human-readable operator-supplied label; it is not a credential.
    pub operator_label: String,
    /// Explicit operator confirmation required by the broker.
    pub operator_confirmed: bool,
}

/// Persisted local account binding, including unconfirmed migration records.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorAccountBinding {
    /// Broker-assigned stable binding identifier.
    pub binding_id: String,
    /// Provider represented by this binding.
    pub provider: MonitorProvider,
    /// Stable local account partition key.
    pub account_id: String,
    /// Opaque local source capability ID, derived from the exact selected
    /// Keychain service. This is not a provider-authenticated account identity
    /// and never contains the service name or credential material.
    #[serde(default)]
    pub provider_account_id: Option<String>,
    /// Whether the operator explicitly enabled experimental collection for
    /// this exact source. Older persisted records deserialize as disabled.
    #[serde(default)]
    pub experimental_collector_approved: bool,
    /// Human-readable operator-supplied label; it is not a credential.
    pub operator_label: String,
    /// Revision increments whenever its source mapping or collector approval changes.
    pub revision: u64,
    /// Whether the operator explicitly confirmed this mapping.
    pub operator_confirmed: bool,
    /// Broker time at which the operator confirmed this binding; absent when unconfirmed.
    pub confirmed_at_epoch: Option<i64>,
}

/// Explicit operator approval input for a goal policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorPolicyApprovalInput {
    /// Binding whose account and revision scope this policy approval.
    pub binding_id: String,
    /// Confirmed binding revision selected by the operator.
    pub binding_revision: u64,
    /// Operator goal governed by this policy.
    pub goal_id: String,
    /// Policy being explicitly approved.
    pub new_policy: MonitorPolicy,
    /// Positive SGD amount with exponent 2 for `StrictSgd`; absent for `QuotaOnly`.
    pub budget: Option<Money>,
    /// Human-readable operator-supplied audit label; it is not authentication.
    pub operator_label: String,
    /// Explicit operator confirmation required by the broker. This is a same-user
    /// trust-boundary assertion, not cryptographic proof of human presence.
    pub operator_confirmed: bool,
    /// Explicit acceptance of having no Jackin SGD spend cap for `QuotaOnly`.
    pub acknowledge_no_sgd_cap: bool,
    /// Optional compare-and-swap revision for an existing policy record.
    pub expected_revision: Option<u64>,
}

/// Durable policy revision and audit provenance for one account and goal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorPolicyRecord {
    /// Provider in force for this record.
    pub provider: MonitorProvider,
    /// Local account partition key in force for this record.
    pub account_id: String,
    /// Binding identifier for an operator approval; absent only for V1 migration.
    pub binding_id: Option<String>,
    /// Binding revision for an operator approval; absent only for V1 migration.
    pub binding_revision: Option<u64>,
    /// Operator goal governed by this policy.
    pub goal_id: String,
    /// Policy that was effective before this revision.
    pub previous_policy: Option<MonitorPolicy>,
    /// Policy made effective by this revision.
    pub new_policy: MonitorPolicy,
    /// SGD ceiling for `StrictSgd`; absent for `QuotaOnly`.
    pub budget: Option<Money>,
    /// Human-readable operator label; absent for V1 migration provenance.
    pub operator_label: Option<String>,
    /// Whether an operator explicitly confirmed this policy record.
    pub operator_confirmed: bool,
    /// Whether the operator acknowledged the missing SGD cap.
    pub acknowledge_no_sgd_cap: bool,
    /// Time the broker recorded this policy revision in UTC Unix seconds.
    /// Unknown for V1 migration; migration must not invent an approval time.
    pub recorded_at_epoch: Option<i64>,
    /// Monotonic policy revision within this account and goal scope.
    pub revision: u64,
    /// Explicit operator approval or preserved V1 strict-policy provenance.
    pub origin: MonitorPolicyOrigin,
}

/// One host-broker operation for durable monitors, statusline evidence, or service control.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum MonitorOperation {
    /// Persist a monitor definition. The broker assigns its stable monitor ID.
    Start {
        /// Initial monitor configuration.
        config: MonitorConfig,
        /// Caller-generated key making retries of this exact start idempotent.
        idempotency_key: String,
    },
    /// Persist an operator-confirmed account binding without reading credentials.
    BindAccount {
        /// Account binding to create or revise.
        binding: MonitorAccountBindingInput,
    },
    /// Persist an explicit policy approval for one binding and goal.
    ApprovePolicy {
        /// Policy approval to record.
        approval: MonitorPolicyApprovalInput,
    },
    /// Stop one monitor while retaining its final record.
    Stop {
        /// Stable broker-assigned monitor ID.
        monitor_id: String,
    },
    /// Read one monitor's durable status and latest evidence.
    Status {
        /// Stable broker-assigned monitor ID.
        monitor_id: String,
    },
    /// Report passive, noninteractive readiness checks for one provider.
    Doctor {
        /// Provider to inspect without reading credentials or contacting it.
        provider: MonitorProvider,
    },
    /// Ingest a bounded statusline JSON document. It carries no cost field.
    Ingest {
        /// Explicit unbound session scope or operator-confirmed account binding.
        scope: MonitorScope,
        /// Normalized statusline fields from one bounded stdin document.
        observation: StatuslineObservation,
    },
    /// Record an explicit account-bound spend observation.
    RecordSpend {
        /// Spend amount and billing period supplied by the operator.
        record: SpendRecordInput,
    },
    /// Reconcile a monitor against broker-owned and locally ingested evidence.
    /// This operation never performs an independent provider refresh.
    Refresh {
        /// Stable broker-assigned monitor ID.
        monitor_id: String,
    },
    /// Read whether the broker service is running and when it will next wake.
    ServiceStatus,
    /// Ask the broker service to stop after it commits current monitor state.
    ServiceStop,
    /// Read reconciled monitor events with a bounded long poll.
    /// A zero cursor attaches at the latest current event without replaying history;
    /// nonzero cursors return retained events newer than that sequence.
    Watch {
        /// Stable broker-assigned monitor ID.
        monitor_id: String,
        /// Last event sequence already observed by the caller; zero means fresh attach.
        after_sequence: u64,
        /// Maximum wait in milliseconds; the broker clamps this to its limit.
        timeout_ms: u64,
    },
}

/// Durable input used to create one monitor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorConfig {
    /// Provider whose evidence is monitored.
    pub provider: MonitorProvider,
    /// Whether this monitor only observes or evaluates dispatch permission.
    pub purpose: MonitorPurpose,
    /// Evidence partition selected by the operator.
    pub scope: MonitorScope,
    /// Operator goal; required only for dispatch guards.
    pub goal_id: Option<String>,
    /// Optional model guard. Mismatched fresh evidence blocks runnable decisions.
    pub expected_model: Option<String>,
    /// Exact approved policy revision consumed by a dispatch guard.
    pub policy_revision: Option<u64>,
    /// Opt in to the experimental foreground collector. Defaults off and does
    /// not grant authority without a confirmed source binding and consent.
    #[serde(default)]
    pub experimental_collector: bool,
}

/// One Claude quota window from normalized statusline input.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StatuslineQuotaWindow {
    /// `used_percentage` normalized to integer basis points (e.g. 12.34% = 1234).
    pub used_percentage_basis_points: Option<i32>,
    /// Provider `resets_at`, in UTC Unix seconds, when present.
    pub reset_at_epoch: Option<i64>,
}

/// The supported Claude statusline `rate_limits` fields.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StatuslineRateLimits {
    /// Five-hour rate limit.
    pub five_hour: Option<StatuslineQuotaWindow>,
    /// Seven-day rate limit.
    pub seven_day: Option<StatuslineQuotaWindow>,
}

/// Normalized Claude statusline input accepted by the monitor.
///
/// The source does not provide a trustworthy account ID or observation time.
/// The caller supplies an explicit scope; the broker stamps receipt time.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StatuslineObservation {
    /// Exact normalized input schema version.
    pub schema_version: u16,
    /// Claude session that emitted this observation.
    pub session_id: String,
    /// Model label used for monitor model guards.
    pub model: Option<String>,
    /// Claude Code version reported by the statusline, when present.
    pub claude_code_version: Option<String>,
    /// Claude provider quota windows. The raw statusline percentage is a float;
    /// the adapter converts it to basis points before sending this record.
    pub rate_limits: StatuslineRateLimits,
}

/// Freshness for one independently updated quota field.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorEvidenceFreshness {
    /// The field is within the monitor's accepted age.
    Current,
    /// The field is present but older than the accepted age.
    Stale,
    /// No observation for this field has been received.
    Unavailable,
}

/// Interpreted state of a reported quota reset epoch, independent of its age.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorResetValidity {
    /// No reset epoch is available to interpret.
    #[default]
    Unknown,
    /// The reported reset epoch is later than the broker's current time.
    Future,
    /// The reported reset epoch is at or before the broker's current time.
    Due,
}

/// Result of comparing current model evidence with the configured model guard.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorModelGuardValidity {
    /// No expected model is configured.
    NotConfigured,
    /// A model guard is configured, but fresh evidence cannot establish a result.
    Unknown,
    /// Fresh scoped model evidence matches the configured expected model.
    Match,
    /// At least one fresh scoped model observation differs from the expected model.
    /// This takes precedence over Unknown when another scoped session lacks fresh evidence.
    Mismatch,
}

/// Per-field provenance and age. Statusline has no source timestamp, so its
/// `evidence_at_epoch` is absent and its broker receipt time remains visible.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorFieldEvidence {
    /// Evidence sequence whose account, session, and source are in `MonitorStatus.evidence`.
    pub evidence_sequence: u64,
    /// Source observation time when the source provides one.
    pub evidence_at_epoch: Option<i64>,
    /// Trusted broker receipt time in UTC Unix seconds.
    pub evidence_received_at_epoch: i64,
    /// Age of the field at status assembly time.
    pub age_seconds: u64,
    /// Freshness evaluated independently for this field.
    pub freshness: MonitorEvidenceFreshness,
}

/// One quota window with independent used-percent and reset provenance.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorQuotaWindowStatus {
    /// Used percentage in integer basis points, when observed (e.g. `9000` = 90%).
    pub used_percentage_basis_points: Option<i32>,
    /// Evidence metadata for `used_percentage_basis_points` only.
    pub used_evidence: Option<MonitorFieldEvidence>,
    /// Reset epoch, when observed.
    pub reset_at_epoch: Option<i64>,
    /// Whether the epoch is unknown, future, or due, independent of evidence freshness.
    pub reset_validity: MonitorResetValidity,
    /// Evidence metadata for `reset_at_epoch` only.
    pub reset_evidence: Option<MonitorFieldEvidence>,
}

/// Current lifecycle state of one durable monitor.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorLifecycle {
    /// Monitor is active and evaluating new evidence.
    Active,
    /// Monitor is waiting for a reset or fresh evidence.
    Waiting,
    /// Monitor is paused by a decision or operator action.
    Paused,
    /// Monitor was stopped and its final record was retained.
    Stopped,
    /// Monitor lacks enough evidence to make a safe decision.
    NeedsEvidence,
}

/// Stable source category for one monitor evidence field.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorEvidenceSource {
    /// Canonical, broker-published quota projection.
    BrokerProjection,
    /// Bounded statusline input from the monitored session.
    Statusline,
    /// Provider-supplied billing or spend record.
    ProviderSpend,
    /// Operator-supplied spend baseline or budget.
    Operator,
    /// Provider-local session log parsed by the token monitor.
    LocalSessionLog,
}

/// Verification state for a monetary baseline.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpendVerification {
    /// Provider or operator record was validated for the stated account/period.
    Verified,
    /// Source could not establish the account or billing-period binding.
    Unverified,
    /// The record exceeds the monitor's accepted freshness interval.
    Stale,
    /// No usable spend record is available.
    Unavailable,
}

/// Source an operator can explicitly attest when recording a spend baseline.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpendRecordSource {
    /// Operator supplied a source record bound to the selected account and period.
    OperatorReceipt,
}

/// Spend record submitted by the operator. Broker receipt time is assigned on ingest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SpendRecordInput {
    /// Canonical account identity to which the record applies.
    pub account_id: String,
    /// Inclusive billing-period start in UTC Unix seconds.
    pub billing_period_start_epoch: i64,
    /// Exclusive billing-period end in UTC Unix seconds.
    pub billing_period_end_epoch: i64,
    /// Explicit currency and decimal scale.
    pub amount: Money,
    /// Source-record time when known; never treated as broker receipt time.
    pub evidence_at_epoch: Option<i64>,
    /// Explicit operator attestation for this account and billing-period record.
    /// This is not independent provider verification.
    pub verified: bool,
    /// Describes the operator-provided evidence; it does not imply verification.
    pub source: SpendRecordSource,
}

/// Persisted spend record with the independent receipt time and verification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SpendRecord {
    /// Canonical account identity to which the record applies.
    pub account_id: String,
    /// Inclusive billing-period start in UTC Unix seconds.
    pub billing_period_start_epoch: i64,
    /// Exclusive billing-period end in UTC Unix seconds.
    pub billing_period_end_epoch: i64,
    /// Explicit currency and decimal scale.
    pub amount: Money,
    /// Source-record time when known.
    pub evidence_at_epoch: Option<i64>,
    /// Trusted broker receipt time in UTC Unix seconds.
    pub evidence_received_at_epoch: i64,
    /// Source category used for verification decisions.
    pub source: SpendRecordSource,
    /// Account and period verification state.
    pub verification: SpendVerification,
}

/// Typed value for one independently sourced monitor field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MonitorEvidenceValue {
    /// Provider quota window from the canonical projection.
    QuotaWindow {
        /// Stable Rust-owned quota window ID.
        window_id: String,
        /// Provider-reported remaining percentage when available.
        remaining_raw_percent: Option<i32>,
        /// Provider-reported reset time in UTC Unix seconds.
        reset_at_epoch: Option<i64>,
    },
    /// Context information reported by the Claude statusline.
    QuotaUsedPercentage {
        /// Claude quota window.
        window: MonitorQuotaWindow,
        /// Used percentage in integer basis points (e.g. `9000` = 90%).
        used_percentage_basis_points: i32,
    },
    /// Quota reset evidence for one independently updated field.
    QuotaReset {
        /// Claude quota window.
        window: MonitorQuotaWindow,
        /// Reset epoch in UTC Unix seconds.
        reset_at_epoch: i64,
    },
    /// Model evidence used by the configured model guard.
    Model {
        /// Model label emitted by the statusline.
        model: String,
    },
    /// Monetary spend baseline for one account and billing period.
    Spend {
        /// Amount with explicit currency and decimal scale.
        amount: Money,
        /// Inclusive billing-period start in UTC Unix seconds.
        billing_period_start_epoch: i64,
        /// Exclusive billing-period end in UTC Unix seconds.
        billing_period_end_epoch: i64,
        /// Whether the source verified this account/period binding.
        verification: SpendVerification,
    },
    /// Operator budget ceiling; it is not provider-reported spend.
    Budget {
        /// Amount with explicit currency and decimal scale.
        amount: Money,
    },
}

/// Quota window names supported by the Claude statusline contract.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorQuotaWindow {
    /// `rate_limits.five_hour`.
    FiveHour,
    /// `rate_limits.seven_day`.
    SevenDay,
}

/// One field-level observation with provenance and age.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorEvidence {
    /// Monotonic evidence sequence within this monitor.
    pub sequence: u64,
    /// Account identity to which the value applies, absent for unbound session evidence.
    pub account_id: Option<String>,
    /// Session identity when the source is session-scoped.
    pub session_id: Option<String>,
    /// Claude Code version reported alongside this statusline evidence.
    pub claude_code_version: Option<String>,
    /// Source category for this field.
    pub source: MonitorEvidenceSource,
    /// Field name and value are encoded by `value`; account/session/source apply to this field.
    /// Source observation time when the source provides one.
    pub evidence_at_epoch: Option<i64>,
    /// Trusted broker receipt time in UTC Unix seconds.
    pub evidence_received_at_epoch: i64,
    /// Age at the time this status was assembled, in seconds.
    pub age_seconds: u64,
    /// Typed value and its field-specific semantics.
    pub value: MonitorEvidenceValue,
}

/// Monitor action selected from current evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum MonitorAction {
    /// Save a durable checkpoint for the linked goal.
    Checkpoint {
        /// Goal identifier associated with the checkpoint.
        goal_id: String,
    },
    /// Lower future dispatch concurrency.
    ReduceDispatch {
        /// Suggested maximum concurrent dispatches, when known.
        max_parallel: Option<u32>,
    },
    /// Pause dispatch until an operator resumes it or evidence changes.
    Pause {
        /// Stable reason code.
        reason: MonitorIssueCode,
    },
    /// Wait until the stated UTC Unix time or a new evidence event.
    Wait {
        /// Earliest time at which the condition should be re-evaluated.
        until_epoch: Option<i64>,
    },
    /// Surface a nonblocking warning.
    Warn {
        /// Stable warning code.
        reason: MonitorIssueCode,
    },
}

/// One ordered monitor decision and the actions it emits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorDecision {
    /// Monotonic decision sequence within this monitor.
    pub sequence: u64,
    /// Decision time in UTC Unix seconds.
    pub decided_at_epoch: i64,
    /// Evidence sequence numbers consumed by this decision.
    pub evidence_sequences: Vec<u64>,
    /// Ordered actions: checkpoint, adjust dispatch, pause, wait, or warn.
    pub actions: Vec<MonitorAction>,
}

/// Stable issue category carried by monitor status and doctor responses.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorIssueCode {
    /// Independent provider refresh is disabled for monitors.
    IndependentRefreshDisabled,
    /// Provider authentication was not inspected by this passive operation.
    AuthStatusUnknown,
    /// Broker service is unavailable.
    BrokerUnavailable,
    /// Statusline ingress is unsupported by the installed integration.
    StatuslineIngressUnsupported,
    /// Statusline input exceeded its byte limit.
    StatuslineTooLarge,
    /// Statusline input failed schema or value validation.
    StatuslineInvalid,
    /// Observation belongs to another canonical account.
    AccountMismatch,
    /// A required account binding was not supplied.
    BindingRequired,
    /// The binding identifier or revision does not match the requested scope.
    BindingMismatch,
    /// No explicit policy approval exists for this account and goal.
    PolicyRequired,
    /// The expected policy revision does not match the current revision.
    PolicyConflict,
    /// A retry key was reused with a different monitor configuration.
    IdempotencyConflict,
    /// An observation-only monitor was used for a dispatch-authorizing operation.
    ObservationOnly,
    /// Operator confirmation was not explicitly supplied.
    OperatorConfirmationRequired,
    /// Quota-only policy did not include acknowledgement of the missing SGD cap.
    SgdCapAcknowledgementRequired,
    /// Observation is outside the accepted freshness interval.
    ObservationStale,
    /// No trusted quota observation is available.
    QuotaUnknown,
    /// Spend baseline cannot be verified against its account or period.
    SpendUnverified,
    /// Spend baseline is older than the accepted freshness interval.
    SpendStale,
    /// No spend baseline is available.
    SpendUnavailable,
    /// Spend cannot be compared to the configured budget with verified evidence.
    BudgetUnverifiable,
    /// Reset time passed, but a fresh post-reset limit observation is unavailable.
    ResetDueUnverified,
    /// A fresh quota observation reports no remaining allowance.
    LimitExhausted,
    /// The configured usage guard requires pausing before full exhaustion.
    LimitGuardReached,
    /// The monitored operation requires interactive operator input.
    InteractionRequired,
    /// Experimental collection lacks an acquired foreground credential lease
    /// for the source selected by the approved binding.
    CollectorAuthRequired,
    /// Quota evidence is older than the accepted freshness interval.
    QuotaStale,
    /// Quota evidence has no reset time, so runnable-after-reset cannot be proved.
    MissingReset,
    /// Budget policy selected a warning.
    BudgetWarn,
    /// Budget policy selected a durable checkpoint.
    BudgetCheckpoint,
    /// Budget policy selected a pause.
    BudgetPause,
    /// Spend could not be safely carried across a billing-period rollover.
    SpendRolloverUnverified,
    /// Statusline model does not match the monitor's configured guard.
    ModelMismatch,
    /// No statusline model is available to satisfy the monitor's model guard.
    ModelUnknown,
    /// Requested monitor does not exist.
    MonitorNotFound,
    /// Durable monitor state is invalid or unreadable.
    MonitorStoreUnavailable,
    /// Bounded event wait expired.
    WaitTimeout,
}

/// One stable issue and an optional safe operator-facing explanation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorIssue {
    /// Stable machine-readable issue code.
    pub code: MonitorIssueCode,
    /// Bounded explanation with no credential or raw provider response data.
    pub message: String,
    /// Earliest retry or re-evaluation time in UTC Unix seconds.
    pub retry_at_epoch: Option<i64>,
}

/// Durable state returned for one monitor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorStatus {
    /// Exact monitor schema version.
    pub schema_version: u16,
    /// Broker-assigned stable monitor ID.
    pub monitor_id: String,
    /// Provider whose evidence is monitored.
    pub provider: MonitorProvider,
    /// Whether this monitor only observes or evaluates dispatch permission.
    pub purpose: MonitorPurpose,
    /// Evidence partition selected by the operator.
    pub scope: MonitorScope,
    /// Stable local account ID, absent for an unbound session observer.
    pub account_id: Option<String>,
    /// Operator goal linked to a dispatch guard.
    pub goal_id: Option<String>,
    /// Session restriction or latest observed session.
    pub session_id: Option<String>,
    /// Latest Claude Code version reported by an accepted statusline observation.
    pub claude_code_version: Option<String>,
    /// Effective policy approval and audit provenance, when applicable.
    pub policy: Option<MonitorPolicyRecord>,
    /// Configured model guard target, when present.
    pub expected_model: Option<String>,
    /// Latest model accepted by the configured model guard.
    pub model: Option<String>,
    /// Evidence metadata for the latest accepted model.
    pub model_evidence: Option<MonitorFieldEvidence>,
    /// Guard comparison result, separate from model evidence freshness.
    pub model_guard_validity: MonitorModelGuardValidity,
    /// Current monitor lifecycle.
    pub lifecycle: MonitorLifecycle,
    /// Independent tracking, quota, budget, and dispatch readiness.
    pub readiness: MonitorReadiness,
    /// Authoritative final decision about whether work may run.
    /// This must be false whenever readiness.dispatch is not Ready.
    pub runnable: bool,
    /// Five-hour quota fields and their independent timestamps/freshness.
    pub five_hour: MonitorQuotaWindowStatus,
    /// Seven-day quota fields and their independent timestamps/freshness.
    pub seven_day: MonitorQuotaWindowStatus,
    /// Active operator-defined spend ceiling, absent when spend enforcement is disabled.
    pub budget: Option<Money>,
    /// Cumulative spend attributed to the goal across billing-period rollovers.
    pub cumulative_goal_spend: Option<Money>,
    /// Latest period-specific spend record; it does not replace cumulative spend.
    pub spend_period_baseline: Option<SpendRecord>,
    /// Current field-level evidence with independent source times and ages.
    pub evidence: Vec<MonitorEvidence>,
    /// Latest ordered decision, if one has been made.
    pub latest_decision: Option<MonitorDecision>,
    /// Last durable update time in UTC Unix seconds.
    pub updated_at_epoch: i64,
    /// Stable current issues.
    pub issues: Vec<MonitorIssue>,
}

/// Authentication state reported by passive readiness checks.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorAuthState {
    /// Authentication was deliberately not inspected.
    Unknown,
    /// An explicit auth-preparation operation reported ready.
    Prepared,
    /// An explicit auth-preparation operation reported action is required.
    ActionRequired,
}

/// Passive provider readiness result. Doctor must not access Keychain or call a provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorDoctorReport {
    /// Provider inspected.
    pub provider: MonitorProvider,
    /// Whether the host broker is reachable.
    pub broker_available: bool,
    /// Whether the installed integration supports bounded statusline ingress.
    pub statusline_ingress_supported: bool,
    /// Passive auth result; normally `unknown` until explicit preparation.
    pub auth_state: MonitorAuthState,
    /// Stable readiness issues.
    pub issues: Vec<MonitorIssue>,
}

/// Service state returned by `usage service status`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorServiceStatus {
    /// Whether the host broker service is running.
    pub running: bool,
    /// Opaque local source capability ID selected by a foreground experimental
    /// collector, or `None` for a passive service. This reports service mode
    /// only, not credential freshness or provider readiness.
    pub experimental_collector_source: Option<String>,
    /// Number of active durable monitors.
    pub active_monitors: u32,
    /// Earliest persisted monitor wake time.
    pub next_wake_epoch: Option<i64>,
}

/// Event emitted when a durable monitor record or decision changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MonitorEvent {
    /// Monotonic event sequence within the monitor.
    pub sequence: u64,
    /// Event time in UTC Unix seconds.
    pub occurred_at_epoch: i64,
    /// Current status after applying the event.
    pub status: MonitorStatus,
}

/// Successful monitor operation result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum MonitorReply {
    /// Operator-confirmed account binding was durably recorded.
    AccountBound {
        /// Current binding record and revision.
        binding: MonitorAccountBinding,
    },
    /// Explicit policy approval or migration-provenance revision was persisted.
    PolicyApproved {
        /// Durable policy record.
        policy: MonitorPolicyRecord,
    },
    /// Monitor was created and durably recorded.
    Started {
        /// New monitor status.
        status: Box<MonitorStatus>,
    },
    /// Monitor was stopped and its final record retained.
    Stopped {
        /// Final monitor status.
        status: Box<MonitorStatus>,
    },
    /// Current monitor status.
    Status {
        /// Durable monitor status.
        status: Box<MonitorStatus>,
    },
    /// Passive provider readiness report.
    Doctor {
        /// Readiness report.
        report: MonitorDoctorReport,
    },
    /// Statusline observation was accepted as local evidence.
    Ingested {
        /// Scope under which the observation was accepted.
        scope: MonitorScope,
        /// Account that owns the observation, absent for unbound session evidence.
        account_id: Option<String>,
        /// Session that emitted the observation.
        session_id: String,
        /// Monotonic evidence sequence assigned by the broker.
        evidence_sequence: u64,
    },
    /// Operator spend record was accepted and persisted.
    SpendRecorded {
        /// Persisted record with broker receipt time and verification state.
        record: SpendRecord,
    },
    /// Monitor was reconciled without an independent provider fetch.
    Refreshed {
        /// Updated durable monitor status.
        status: Box<MonitorStatus>,
    },
    /// Bounded event watch returned the current event on fresh attach, newer events,
    /// or reached its timeout.
    Watch {
        /// Events newer than the caller's cursor.
        events: Vec<MonitorEvent>,
        /// Latest sequence available after this response.
        next_sequence: u64,
        /// True when the bounded wait elapsed without a newer event.
        timed_out: bool,
    },
    /// Current broker service state.
    ServiceStatus {
        /// Service status snapshot.
        status: MonitorServiceStatus,
    },
    /// Broker accepted a stop request.
    ServiceStopped,
}

#[cfg(test)]
mod tests {
    use super::{
        MonitorEvidenceFreshness, MonitorFieldEvidence, MonitorOperation, MonitorQuotaWindowStatus,
        SpendRecordInput, SpendRecordSource, StatuslineObservation,
        USAGE_MONITOR_MAX_STATUSLINE_BYTES, USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
    };
    use crate::control::Money;

    #[test]
    fn statusline_contract_preserves_window_values_in_basis_points() {
        let input = r#"{
            "schema_version": 2,
            "session_id": "session-1",
            "model": "claude-sonnet",
            "claude_code_version": "2.1.80",
            "rate_limits": {
                "five_hour": {"used_percentage_basis_points": 1234, "reset_at_epoch": 1800000000},
                "seven_day": {"used_percentage_basis_points": 9000, "reset_at_epoch": 1800600000}
            }
        }"#;
        let observation: StatuslineObservation =
            serde_json::from_str(input).expect("normalized statusline input should decode");

        assert_eq!(
            observation
                .rate_limits
                .five_hour
                .as_ref()
                .and_then(|w| w.used_percentage_basis_points),
            Some(1234)
        );
        assert_eq!(
            observation
                .rate_limits
                .seven_day
                .as_ref()
                .and_then(|w| w.used_percentage_basis_points),
            Some(9000)
        );
        assert_eq!(observation.claude_code_version.as_deref(), Some("2.1.80"));
        assert_eq!(USAGE_MONITOR_MAX_STATUSLINE_BYTES, 16 * 1024);
        assert_eq!(
            observation.schema_version,
            USAGE_STATUSLINE_INPUT_SCHEMA_VERSION
        );
    }

    #[test]
    fn statusline_ingest_is_a_typed_monitor_operation() {
        let operation = MonitorOperation::Ingest {
            scope: super::MonitorScope::Session {
                session_id: "session-1".to_owned(),
            },
            observation: StatuslineObservation {
                schema_version: USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
                session_id: "session-1".to_owned(),
                model: Some("claude-sonnet".to_owned()),
                claude_code_version: Some("2.1.80".to_owned()),
                ..StatuslineObservation::default()
            },
        };
        let value = serde_json::to_value(operation).expect("monitor operation should encode");

        assert_eq!(value["operation"], "ingest");
        assert_eq!(value["scope"]["scope"], "session");
        assert_eq!(value["scope"]["session_id"], "session-1");
        assert_eq!(value["observation"]["claude_code_version"], "2.1.80");
        assert!(value["observation"].get("evidence_at_epoch").is_none());
    }

    #[test]
    fn v4_monitor_control_shapes_are_tagged_and_secret_free() {
        use super::{
            MonitorAccountBindingInput, MonitorConfig, MonitorOperation, MonitorPolicy,
            MonitorPolicyApprovalInput, MonitorPurpose, MonitorScope, USAGE_MONITOR_SCHEMA_VERSION,
            USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
        };

        let source_capability_id = "c".repeat(64);
        let binding = MonitorOperation::BindAccount {
            binding: MonitorAccountBindingInput {
                provider: super::MonitorProvider::Claude,
                account_id: "local-account".to_owned(),
                provider_account_id: Some(source_capability_id.clone()),
                experimental_collector_approved: true,
                operator_label: "work account".to_owned(),
                operator_confirmed: true,
            },
        };
        let binding_value = serde_json::to_value(binding).expect("binding should encode");
        assert_eq!(binding_value["operation"], "bind_account");
        assert_eq!(binding_value["binding"]["operator_confirmed"], true);
        assert_eq!(
            binding_value["binding"]["provider_account_id"],
            source_capability_id
        );
        assert_eq!(
            binding_value["binding"]["experimental_collector_approved"],
            true
        );
        assert!(binding_value["binding"].get("credential").is_none());

        let mut old_binding = binding_value["binding"].clone();
        old_binding
            .as_object_mut()
            .expect("binding should encode as an object")
            .remove("provider_account_id");
        old_binding
            .as_object_mut()
            .expect("binding should encode as an object")
            .remove("experimental_collector_approved");
        let decoded: MonitorAccountBindingInput =
            serde_json::from_value(old_binding).expect("new binding fields default safely");
        assert_eq!(decoded.provider_account_id, None);
        assert!(!decoded.experimental_collector_approved);

        let approval = MonitorOperation::ApprovePolicy {
            approval: MonitorPolicyApprovalInput {
                binding_id: "binding-1".to_owned(),
                binding_revision: 1,
                goal_id: "goal-1".to_owned(),
                new_policy: MonitorPolicy::QuotaOnly,
                budget: None,
                operator_label: "operator".to_owned(),
                operator_confirmed: true,
                acknowledge_no_sgd_cap: true,
                expected_revision: None,
            },
        };
        let approval_value = serde_json::to_value(approval).expect("approval should encode");
        assert_eq!(approval_value["operation"], "approve_policy");
        assert_eq!(approval_value["approval"]["new_policy"], "quota_only");
        assert_eq!(approval_value["approval"]["acknowledge_no_sgd_cap"], true);

        let start = MonitorOperation::Start {
            config: MonitorConfig {
                provider: super::MonitorProvider::Claude,
                purpose: MonitorPurpose::ObserveOnly,
                scope: MonitorScope::BoundAccount {
                    binding_id: "binding-1".to_owned(),
                    binding_revision: 1,
                    session_id: Some("session-1".to_owned()),
                },
                goal_id: None,
                expected_model: None,
                policy_revision: None,
                experimental_collector: false,
            },
            idempotency_key: "retry-1".to_owned(),
        };
        let start_value = serde_json::to_value(&start).expect("start should encode");
        assert_eq!(start_value["config"]["experimental_collector"], false);
        let decoded: MonitorOperation =
            serde_json::from_value(start_value.clone()).expect("v4 start should decode");
        assert_eq!(decoded, start);
        assert_eq!(start_value["config"]["purpose"], "observe_only");
        assert_eq!(start_value["idempotency_key"], "retry-1");
        let mut legacy_start = start_value.clone();
        legacy_start["config"]["budget"] = serde_json::json!({
            "amount_minor": 5_000,
            "currency": "SGD",
            "exponent": 2
        });
        serde_json::from_value::<MonitorOperation>(legacy_start)
            .expect_err("legacy budget override must be rejected");
        let mut v3_start = start_value.clone();
        v3_start["config"]
            .as_object_mut()
            .expect("config object")
            .remove("experimental_collector");
        let decoded: MonitorOperation =
            serde_json::from_value(v3_start).expect("collector opt-in defaults off");
        assert!(matches!(
            decoded,
            MonitorOperation::Start {
                config: MonitorConfig {
                    experimental_collector: false,
                    ..
                },
                ..
            }
        ));
        assert_eq!(USAGE_MONITOR_SCHEMA_VERSION, 4);
        assert_eq!(USAGE_STATUSLINE_INPUT_SCHEMA_VERSION, 2);
        assert_eq!(crate::usage_broker::USAGE_BROKER_PROTOCOL_VERSION, "v8");
        assert_eq!(
            serde_json::to_value(super::MonitorBudgetReadiness::Disabled)
                .expect("readiness should encode"),
            "disabled"
        );
        assert_eq!(
            serde_json::to_value(super::MonitorDispatchReadiness::NotAuthorized)
                .expect("readiness should encode"),
            "not_authorized"
        );

        serde_json::from_value::<MonitorOperation>(serde_json::json!({
            "operation": "prepare_auth",
            "provider": "claude"
        }))
        .expect_err("credential bootstrap is not a monitor RPC");
        serde_json::from_value::<super::MonitorReply>(serde_json::json!({
            "result": "auth_prepared",
            "provider": "claude",
            "issues": []
        }))
        .expect_err("credential bootstrap has no monitor reply");
        assert_eq!(
            serde_json::to_value(super::MonitorIssueCode::CollectorAuthRequired)
                .expect("issue code should encode"),
            "collector_auth_required"
        );
    }

    #[test]
    fn missing_statusline_version_stays_unknown() {
        let input = r#"{
            "schema_version": 2,
            "session_id": "session-1",
            "model": null,
            "rate_limits": {"five_hour": null, "seven_day": null}
        }"#;
        let observation: StatuslineObservation =
            serde_json::from_str(input).expect("missing optional version should decode as unknown");

        assert_eq!(observation.claude_code_version, None);
        let encoded = serde_json::to_value(observation).expect("observation should encode");
        assert!(encoded["claude_code_version"].is_null());
        assert!(encoded.get("evidence_at_epoch").is_none());
    }

    #[test]
    fn model_guard_validity_states_have_stable_names() {
        use super::MonitorModelGuardValidity;

        assert_eq!(
            serde_json::to_value(MonitorModelGuardValidity::NotConfigured)
                .expect("model guard state should encode"),
            "not_configured"
        );
        assert_eq!(
            serde_json::to_value(MonitorModelGuardValidity::Unknown)
                .expect("model guard state should encode"),
            "unknown"
        );
        assert_eq!(
            serde_json::to_value(MonitorModelGuardValidity::Match)
                .expect("model guard state should encode"),
            "match"
        );
        assert_eq!(
            serde_json::to_value(MonitorModelGuardValidity::Mismatch)
                .expect("model guard state should encode"),
            "mismatch"
        );
    }

    #[test]
    fn migrated_policy_timestamp_remains_unknown() {
        use super::{MonitorPolicy, MonitorPolicyOrigin, MonitorPolicyRecord, MonitorProvider};

        let migrated = MonitorPolicyRecord {
            provider: MonitorProvider::Claude,
            account_id: "account-1".to_owned(),
            binding_id: None,
            binding_revision: None,
            goal_id: "goal-1".to_owned(),
            previous_policy: None,
            new_policy: MonitorPolicy::StrictSgd,
            budget: None,
            operator_label: None,
            operator_confirmed: false,
            acknowledge_no_sgd_cap: false,
            recorded_at_epoch: None,
            revision: 1,
            origin: MonitorPolicyOrigin::MigratedV1,
        };
        let encoded = serde_json::to_value(&migrated).expect("policy should encode");
        assert!(encoded["recorded_at_epoch"].is_null());

        let mut unknown_timestamp = encoded;
        unknown_timestamp
            .as_object_mut()
            .expect("policy should encode as an object")
            .remove("recorded_at_epoch");
        let decoded: MonitorPolicyRecord = serde_json::from_value(unknown_timestamp)
            .expect("an absent migrated policy timestamp should remain unknown");
        assert_eq!(decoded.recorded_at_epoch, None);
    }

    #[test]
    fn migrated_binding_confirmation_time_remains_unknown() {
        use super::{MonitorAccountBinding, MonitorProvider};

        let migrated = MonitorAccountBinding {
            binding_id: "legacy-binding-1".to_owned(),
            provider: MonitorProvider::Claude,
            account_id: "account-1".to_owned(),
            operator_label: "migrated v1 binding".to_owned(),
            revision: 1,
            operator_confirmed: false,
            confirmed_at_epoch: None,
            provider_account_id: None,
            experimental_collector_approved: false,
        };
        let encoded = serde_json::to_value(&migrated).expect("binding should encode");
        assert!(encoded["confirmed_at_epoch"].is_null());

        let mut unknown_timestamp = encoded;
        unknown_timestamp
            .as_object_mut()
            .expect("binding should encode as an object")
            .remove("confirmed_at_epoch");
        unknown_timestamp
            .as_object_mut()
            .expect("binding should encode as an object")
            .remove("provider_account_id");
        unknown_timestamp
            .as_object_mut()
            .expect("binding should encode as an object")
            .remove("experimental_collector_approved");
        let decoded: MonitorAccountBinding = serde_json::from_value(unknown_timestamp)
            .expect("old binding fields default without granting collection");
        assert_eq!(decoded.confirmed_at_epoch, None);
        assert!(!decoded.operator_confirmed);
        assert_eq!(decoded.provider_account_id, None);
        assert!(!decoded.experimental_collector_approved);

        let confirmed = MonitorAccountBinding {
            operator_confirmed: true,
            confirmed_at_epoch: Some(1_800_000_000),
            ..decoded
        };
        let confirmed_value = serde_json::to_value(confirmed).expect("binding should encode");
        assert_eq!(confirmed_value["confirmed_at_epoch"], 1_800_000_000);
    }

    #[test]
    fn quota_window_status_keeps_used_and_reset_evidence_independent() {
        let status = MonitorQuotaWindowStatus {
            used_percentage_basis_points: Some(9_000),
            used_evidence: Some(MonitorFieldEvidence {
                evidence_sequence: 12,
                evidence_at_epoch: None,
                evidence_received_at_epoch: 1_800_000_000,
                age_seconds: 5,
                freshness: MonitorEvidenceFreshness::Current,
            }),
            reset_at_epoch: Some(1_800_060_000),
            reset_validity: super::MonitorResetValidity::Future,
            reset_evidence: Some(MonitorFieldEvidence {
                evidence_sequence: 8,
                evidence_at_epoch: Some(1_799_999_000),
                evidence_received_at_epoch: 1_800_000_000,
                age_seconds: 1_000,
                freshness: MonitorEvidenceFreshness::Stale,
            }),
        };
        let value = serde_json::to_value(status).expect("window status should encode");

        assert_eq!(value["used_percentage_basis_points"], 9_000);
        assert_eq!(value["used_evidence"]["evidence_sequence"], 12);
        assert_eq!(value["reset_validity"], "future");
        assert_eq!(value["reset_evidence"]["evidence_sequence"], 8);
        assert_eq!(value["reset_evidence"]["freshness"], "stale");
    }

    #[test]
    fn unbound_session_evidence_does_not_claim_an_account() {
        let evidence = super::MonitorEvidence {
            sequence: 1,
            account_id: None,
            session_id: Some("session-1".to_owned()),
            claude_code_version: Some("2.1.80".to_owned()),
            source: super::MonitorEvidenceSource::Statusline,
            evidence_at_epoch: None,
            evidence_received_at_epoch: 1_800_000_000,
            age_seconds: 0,
            value: super::MonitorEvidenceValue::Model {
                model: "claude-sonnet".to_owned(),
            },
        };
        let value = serde_json::to_value(evidence).expect("evidence should encode");

        assert!(value["account_id"].is_null());
        assert_eq!(value["session_id"], "session-1");
        assert_eq!(value["claude_code_version"], "2.1.80");
    }

    #[test]
    fn spend_input_requires_account_period_currency_and_source() {
        let input = SpendRecordInput {
            account_id: "account-1".to_owned(),
            billing_period_start_epoch: 1_800_000_000,
            billing_period_end_epoch: 1_802_592_000,
            amount: Money::new(5_000, "SGD", 2),
            evidence_at_epoch: Some(1_800_000_000),
            verified: true,
            source: SpendRecordSource::OperatorReceipt,
        };
        let value = serde_json::to_value(input).expect("spend input should encode");

        assert_eq!(value["account_id"], "account-1");
        assert_eq!(value["amount"]["currency"], "SGD");
        assert_eq!(value["verified"], true);
        assert_eq!(value["source"], "operator_receipt");
        assert!(value.get("evidence_received_at_epoch").is_none());
    }
}
