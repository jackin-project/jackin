// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Versioned, secret-free monitor control and evidence records.

use serde::{Deserialize, Serialize};

use crate::control::Money;

/// Version of the durable monitor records and statusline input.
pub const USAGE_MONITOR_SCHEMA_VERSION: u16 = 1;

/// Maximum UTF-8 bytes accepted for one statusline JSON input.
pub const USAGE_MONITOR_MAX_STATUSLINE_BYTES: usize = 16 * 1024;

/// Provider supported by the first unattended monitor contract.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MonitorProvider {
    /// Anthropic Claude Code.
    Claude,
}

/// One host-broker operation for durable monitors, statusline evidence, or service control.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum MonitorOperation {
    /// Persist a monitor definition. The broker assigns its stable monitor ID.
    Start {
        /// Initial monitor configuration.
        config: MonitorConfig,
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
        /// Existing canonical account identity selected by the operator.
        account_id: String,
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
    /// Explicit operator action to prepare provider authentication.
    PrepareAuth {
        /// Provider whose interactive authentication should be prepared.
        provider: MonitorProvider,
    },
    /// Read reconciled monitor events with a bounded long poll.
    /// `after_sequence == 0` attaches at the latest current event without replaying history;
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
    /// Stable canonical account identifier, never a discovery ordinal.
    pub account_id: String,
    /// Operator's task or goal identifier.
    pub goal_id: String,
    /// Optional session identifier to narrow statusline evidence.
    pub session_id: Option<String>,
    /// Optional model guard. Mismatched fresh evidence blocks runnable decisions.
    pub expected_model: Option<String>,
    /// Operator-defined spend ceiling in an explicit currency/scale.
    /// Missing or unsupported values block work with `budget_unverifiable`.
    pub budget: Option<Money>,
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
/// The caller supplies the selected account; the broker stamps receipt time.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StatuslineObservation {
    /// Exact normalized input schema version.
    pub schema_version: u16,
    /// Claude session that emitted this observation.
    pub session_id: String,
    /// Model label used for monitor model guards.
    pub model: Option<String>,
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
    /// Account identity to which the value applies.
    pub account_id: String,
    /// Session identity when the source is session-scoped.
    pub session_id: Option<String>,
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
    /// Stable canonical account ID.
    pub account_id: String,
    /// Operator goal linked to the monitor.
    pub goal_id: String,
    /// Optional session filter.
    pub session_id: Option<String>,
    /// Latest model accepted by the configured model guard.
    pub model: Option<String>,
    /// Evidence metadata for the latest accepted model.
    pub model_evidence: Option<MonitorFieldEvidence>,
    /// Current monitor lifecycle.
    pub lifecycle: MonitorLifecycle,
    /// Whether current evidence explicitly permits runnable work.
    pub runnable: bool,
    /// Five-hour quota fields and their independent timestamps/freshness.
    pub five_hour: MonitorQuotaWindowStatus,
    /// Seven-day quota fields and their independent timestamps/freshness.
    pub seven_day: MonitorQuotaWindowStatus,
    /// Optional operator-defined spend ceiling.
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
        /// Account that owns the observation.
        account_id: String,
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
    /// Explicit authentication preparation completed without returning secrets.
    AuthPrepared {
        /// Provider prepared.
        provider: MonitorProvider,
        /// Stable result issues; raw credential material is never returned.
        issues: Vec<MonitorIssue>,
    },
}

#[cfg(test)]
mod tests {
    use super::{
        MonitorEvidenceFreshness, MonitorFieldEvidence, MonitorOperation, MonitorQuotaWindowStatus,
        SpendRecordInput, SpendRecordSource, StatuslineObservation,
        USAGE_MONITOR_MAX_STATUSLINE_BYTES,
    };
    use crate::control::Money;

    #[test]
    fn statusline_contract_preserves_window_values_in_basis_points() {
        let input = r#"{
            "schema_version": 1,
            "session_id": "session-1",
            "model": "claude-sonnet",
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
        assert_eq!(USAGE_MONITOR_MAX_STATUSLINE_BYTES, 16 * 1024);
    }

    #[test]
    fn statusline_ingest_is_a_typed_monitor_operation() {
        let operation = MonitorOperation::Ingest {
            account_id: "account-1".to_owned(),
            observation: StatuslineObservation {
                schema_version: 1,
                session_id: "session-1".to_owned(),
                model: Some("claude-sonnet".to_owned()),
                ..StatuslineObservation::default()
            },
        };
        let value = serde_json::to_value(operation).expect("monitor operation should encode");

        assert_eq!(value["operation"], "ingest");
        assert_eq!(value["account_id"], "account-1");
        assert!(value["observation"].get("evidence_at_epoch").is_none());
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
        assert_eq!(value["reset_evidence"]["evidence_sequence"], 8);
        assert_eq!(value["reset_evidence"]["freshness"], "stale");
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
