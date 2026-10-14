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
