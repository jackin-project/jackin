// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Explicit migration from the original durable usage monitor schema.

use std::collections::{BTreeMap, BTreeSet};

use jackin_protocol::control::Money;
use jackin_protocol::usage_monitor::{
    MonitorAccountBinding, MonitorAction, MonitorBudgetReadiness, MonitorDecision,
    MonitorDispatchReadiness, MonitorEvent, MonitorEvidence, MonitorEvidenceFreshness,
    MonitorEvidenceSource, MonitorEvidenceValue, MonitorFieldEvidence, MonitorIssue,
    MonitorIssueCode, MonitorLifecycle, MonitorModelGuardValidity, MonitorPolicy,
    MonitorPolicyOrigin, MonitorPolicyRecord, MonitorProvider, MonitorPurpose,
    MonitorQuotaReadiness, MonitorQuotaWindow, MonitorQuotaWindowStatus, MonitorReadiness,
    MonitorResetValidity, MonitorScope, MonitorStatus, MonitorTrackingReadiness, SpendRecord,
    SpendRecordSource, SpendVerification, USAGE_MONITOR_SCHEMA_VERSION,
};
use serde::Deserialize;

use super::spend::{SpendAccountState, SpendState};
use super::{
    AccountObservations, AccountResetBarrier, DurableGoalSpend, DurableMonitor, Observed,
    ObservedPercentage, ObservedQuotaPair, ObservedWindow, ResetBarrier, SessionObservation,
    StoreState,
};

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

/// Decode and migrate one complete V1 snapshot without touching persistence.
pub(super) fn migrate_v1(bytes: &[u8]) -> Result<StoreState, MonitorIssue> {
    let legacy: V1StoreState = serde_json::from_slice(bytes).map_err(|_| unavailable())?;
    validate_v1(&legacy)?;

    let binding_ids = binding_ids(&legacy)?;
    let bindings = build_bindings(&binding_ids)?;
    let mut policy_records = BTreeMap::new();
    let mut goals = BTreeMap::new();

    for (goal_id, legacy_goal) in &legacy.goals {
        let binding_id = binding_ids
            .get(&legacy_goal.account_id)
            .ok_or_else(unavailable)?;
        let budget = legacy_goal.budget.clone().map(V1Money::into_money);
        let policy = MonitorPolicy::StrictSgd;
        let record = MonitorPolicyRecord {
            provider: MonitorProvider::Claude,
            account_id: legacy_goal.account_id.clone(),
            binding_id: None,
            binding_revision: None,
            goal_id: goal_id.clone(),
            previous_policy: None,
            new_policy: policy,
            budget: budget.clone(),
            operator_label: None,
            operator_confirmed: false,
            acknowledge_no_sgd_cap: false,
            recorded_at_epoch: None,
            revision: 1,
            origin: MonitorPolicyOrigin::MigratedV1,
        };
        policy_records.insert(goal_id.clone(), vec![record]);
        goals.insert(
            goal_id.clone(),
            DurableGoalSpend {
                account_id: legacy_goal.account_id.clone(),
                binding_id: binding_id.clone(),
                binding_revision: 1,
                policy_revision: 1,
                policy,
                budget,
                spend_state: Some(legacy_goal.spend_state.clone().into_spend_state()),
            },
        );
    }

    let accounts = legacy
        .accounts
        .iter()
        .map(|(account_id, account)| {
            Ok((
                account_id.clone(),
                account.clone().into_account_observations(account_id)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, MonitorIssue>>()?;

    let mut monitors = BTreeMap::new();
    for (monitor_id, legacy_monitor) in &legacy.monitors {
        let binding_id = binding_ids
            .get(&legacy_monitor.config.account_id)
            .ok_or_else(unavailable)?;
        let goal = legacy
            .goals
            .get(&legacy_monitor.config.goal_id)
            .ok_or_else(unavailable)?;
        let policy = policy_records
            .get(&legacy_monitor.config.goal_id)
            .and_then(|history| history.last())
            .ok_or_else(unavailable)?;
        let retained_session_ids = legacy
            .accounts
            .get(&legacy_monitor.config.account_id)
            .map(|account| account.sessions.keys().cloned().collect::<BTreeSet<_>>())
            .unwrap_or_default();
        let monitor = legacy_monitor.migrate(
            monitor_id,
            binding_id,
            goal,
            policy,
            &retained_session_ids,
            legacy.last_now_epoch,
        )?;
        monitors.insert(monitor_id.clone(), monitor);
    }

    let next_binding_id = u64::try_from(binding_ids.len())
        .ok()
        .and_then(|count| count.checked_add(1))
        .ok_or_else(unavailable)?;

    Ok(StoreState {
        schema_version: USAGE_MONITOR_SCHEMA_VERSION,
        next_monitor_id: legacy.next_monitor_id,
        next_input_sequence: legacy.next_input_sequence,
        last_now_epoch: legacy.last_now_epoch,
        accounts,
        unbound_sessions: BTreeMap::new(),
        bindings,
        policy_records,
        monitors,
        goals,
        next_binding_id,
    })
}

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

impl V1Money {
    fn into_money(self) -> Money {
        Money {
            amount_minor: self.amount_minor,
            currency: self.currency,
            exponent: self.exponent,
        }
    }

    fn is_valid(&self) -> bool {
        !self.currency.is_empty()
            && self.currency.len() <= 16
            && self.currency.is_ascii()
            && self.exponent <= 9
            && self.amount_minor >= 0
    }
}

impl V1SpendRecord {
    fn into_spend_record(self) -> SpendRecord {
        SpendRecord {
            account_id: self.account_id,
            billing_period_start_epoch: self.billing_period_start_epoch,
            billing_period_end_epoch: self.billing_period_end_epoch,
            amount: self.amount.into_money(),
            evidence_at_epoch: self.evidence_at_epoch,
            evidence_received_at_epoch: self.evidence_received_at_epoch,
            source: self.source.into(),
            verification: self.verification.into(),
        }
    }

    fn is_valid_for(&self, account_id: &str, last_now_epoch: i64) -> bool {
        self.account_id == account_id
            && self.billing_period_start_epoch >= 0
            && self.billing_period_end_epoch > self.billing_period_start_epoch
            && self.amount.is_valid()
            && self.evidence_at_epoch.is_none_or(|epoch| epoch >= 0)
            && self.evidence_received_at_epoch >= 0
            && self.evidence_received_at_epoch <= last_now_epoch
    }
}

impl V1MonitorEvidence {
    fn is_valid_for(
        &self,
        account_id: &str,
        session_filter: Option<&str>,
        next_evidence_sequence: u64,
        last_now_epoch: i64,
    ) -> bool {
        self.sequence > 0
            && self.sequence <= next_evidence_sequence
            && self.account_id == account_id
            && self.session_id.as_deref().is_none_or(valid_identifier)
            && session_filter.is_none_or(|expected| {
                self.session_id
                    .as_deref()
                    .is_none_or(|session_id| session_id == expected)
            })
            && match self.source {
                V1MonitorEvidenceSource::Statusline => self.session_id.is_some(),
                V1MonitorEvidenceSource::BrokerProjection => self.session_id.is_none(),
                _ => true,
            }
            && valid_timestamp(
                self.evidence_at_epoch,
                self.evidence_received_at_epoch,
                last_now_epoch,
            )
            && self.value.is_valid()
    }
}

impl V1MonitorEvidenceValue {
    fn is_valid(&self) -> bool {
        match self {
            Self::QuotaWindow {
                window_id,
                remaining_raw_percent,
                reset_at_epoch,
            } => {
                valid_bounded_text(window_id, MAX_ID_LENGTH)
                    && remaining_raw_percent.is_none_or(|value| (0..=10_000).contains(&value))
                    && reset_at_epoch.is_none_or(|epoch| epoch >= 0)
            }
            Self::QuotaUsedPercentage {
                used_percentage_basis_points,
                ..
            } => (0..=10_000).contains(used_percentage_basis_points),
            Self::QuotaReset { reset_at_epoch, .. } => *reset_at_epoch >= 0,
            Self::Model { model } => valid_bounded_text(model, MAX_MODEL_LENGTH),
            Self::Spend {
                amount,
                billing_period_start_epoch,
                billing_period_end_epoch,
                ..
            } => {
                amount.is_valid()
                    && *billing_period_start_epoch >= 0
                    && *billing_period_end_epoch > *billing_period_start_epoch
            }
            Self::Budget { amount } => amount.is_valid(),
        }
    }
}

impl V1MonitorFieldEvidence {
    fn is_valid_for(&self, next_evidence_sequence: u64, last_now_epoch: i64) -> bool {
        self.evidence_sequence > 0
            && self.evidence_sequence <= next_evidence_sequence
            && valid_timestamp(
                self.evidence_at_epoch,
                self.evidence_received_at_epoch,
                last_now_epoch,
            )
    }
}

impl V1MonitorQuotaWindowStatus {
    fn is_valid_for(&self, next_evidence_sequence: u64, last_now_epoch: i64) -> bool {
        self.used_percentage_basis_points
            .is_none_or(|value| (0..=10_000).contains(&value))
            && self.reset_at_epoch.is_none_or(|epoch| epoch >= 0)
            && self.used_percentage_basis_points.is_some() == self.used_evidence.is_some()
            && self.reset_at_epoch.is_some() == self.reset_evidence.is_some()
            && self
                .used_evidence
                .as_ref()
                .is_none_or(|field| field.is_valid_for(next_evidence_sequence, last_now_epoch))
            && self
                .reset_evidence
                .as_ref()
                .is_none_or(|field| field.is_valid_for(next_evidence_sequence, last_now_epoch))
    }
}

impl V1MonitorDecision {
    fn is_valid_for(
        &self,
        goal_id: &str,
        next_evidence_sequence: u64,
        last_now_epoch: i64,
    ) -> bool {
        let mut evidence_sequences = BTreeSet::new();
        self.sequence > 0
            && self.decided_at_epoch >= 0
            && self.decided_at_epoch <= last_now_epoch
            && self.evidence_sequences.iter().all(|sequence| {
                *sequence > 0
                    && *sequence <= next_evidence_sequence
                    && evidence_sequences.insert(*sequence)
            })
            && self
                .actions
                .iter()
                .all(|action| action.is_valid_for(goal_id))
    }
}

impl V1MonitorAction {
    fn is_valid_for(&self, goal_id: &str) -> bool {
        match self {
            Self::Checkpoint {
                goal_id: action_goal,
            } => valid_goal_id(action_goal) && action_goal == goal_id,
            Self::ReduceDispatch { .. } | Self::Pause { .. } | Self::Warn { .. } => true,
            Self::Wait { until_epoch } => until_epoch.is_none_or(|epoch| epoch >= 0),
        }
    }
}

impl V1MonitorIssue {
    fn is_valid(&self) -> bool {
        !self.message.trim().is_empty()
            && self.message.len() <= 1_024
            && !self.message.chars().any(char::is_control)
            && self.retry_at_epoch.is_none_or(|epoch| epoch >= 0)
    }
}

impl V1SpendState {
    fn into_spend_state(self) -> SpendState {
        SpendState {
            baseline: self.baseline.map(V1SpendRecord::into_spend_record),
            period_anchor: self.period_anchor.map(V1SpendRecord::into_spend_record),
            // V1 did not retain the last closed receipt folded into its
            // cumulative value. Preserve its totals; reconciliation marks any
            // already-rolled state incomplete instead of guessing this value.
            closed_period_anchor: None,
            cumulative_goal_spend: self.cumulative_goal_spend.map(V1Money::into_money),
            rollover_unknown: self.rollover_unknown,
            cumulative_complete: self.cumulative_complete,
        }
    }

    fn is_valid_for(&self, account_id: &str, last_now_epoch: i64) -> bool {
        self.baseline
            .as_ref()
            .is_none_or(|record| record.is_valid_for(account_id, last_now_epoch))
            && self
                .period_anchor
                .as_ref()
                .is_none_or(|record| record.is_valid_for(account_id, last_now_epoch))
            && self
                .cumulative_goal_spend
                .as_ref()
                .is_none_or(V1Money::is_valid)
    }
}

impl V1SpendAccountState {
    fn into_spend_account_state(self) -> SpendAccountState {
        SpendAccountState {
            latest_record: self.latest_record.map(V1SpendRecord::into_spend_record),
            current_period_record: self
                .current_period_record
                .map(V1SpendRecord::into_spend_record),
            previous_period_record: self
                .previous_period_record
                .map(V1SpendRecord::into_spend_record),
        }
    }

    fn is_valid_for(&self, account_id: &str, last_now_epoch: i64) -> bool {
        self.latest_record
            .as_ref()
            .is_none_or(|record| record.is_valid_for(account_id, last_now_epoch))
            && self
                .current_period_record
                .as_ref()
                .is_none_or(|record| record.is_valid_for(account_id, last_now_epoch))
            && self
                .previous_period_record
                .as_ref()
                .is_none_or(|record| record.is_valid_for(account_id, last_now_epoch))
    }
}

impl V1AccountObservations {
    fn into_account_observations(
        self,
        account_id: &str,
    ) -> Result<AccountObservations, MonitorIssue> {
        Ok(AccountObservations {
            sessions: self
                .sessions
                .into_iter()
                .map(|(session_id, session)| Ok((session_id, session.into_session_observation()?)))
                .collect::<Result<BTreeMap<_, _>, MonitorIssue>>()?,
            broker_windows: self
                .broker_windows
                .map(V1ObservedWindow::into_observed_window),
            latest_reset_epochs: self.latest_reset_epochs,
            reset_barriers: self
                .reset_barriers
                .map(|barrier| barrier.map(V1AccountResetBarrier::into_account_reset_barrier)),
            input_sequence: self.input_sequence,
            spend: self.spend.into_spend_account_state(),
        })
        .and_then(|account| {
            if account
                .spend
                .latest_record
                .as_ref()
                .is_some_and(|record| record.account_id != account_id)
                || account
                    .spend
                    .current_period_record
                    .as_ref()
                    .is_some_and(|record| record.account_id != account_id)
                || account
                    .spend
                    .previous_period_record
                    .as_ref()
                    .is_some_and(|record| record.account_id != account_id)
            {
                return Err(unavailable());
            }
            Ok(account)
        })
    }
}

impl V1SessionObservation {
    fn into_session_observation(self) -> Result<SessionObservation, MonitorIssue> {
        Ok(SessionObservation {
            model: self.model.map(V1Observed::into_observed),
            windows: self.windows.map(V1ObservedWindow::into_observed_window),
            last_observation_received_at_epoch: self.last_observation_received_at_epoch,
            claude_code_version: None,
            last_callback_received_at_epoch: None,
        })
    }
}

impl V1ObservedWindow {
    fn into_observed_window(self) -> ObservedWindow {
        ObservedWindow {
            used: self
                .used
                .map(V1ObservedPercentage::into_observed_percentage),
            reset: self.reset.map(V1Observed::into_observed),
            paired: self
                .paired
                .map(V1ObservedQuotaPair::into_observed_quota_pair),
        }
    }
}

impl V1ObservedQuotaPair {
    fn into_observed_quota_pair(self) -> ObservedQuotaPair {
        ObservedQuotaPair {
            used_percentage_basis_points: self.used_percentage_basis_points,
            reset_at_epoch: self.reset_at_epoch,
            evidence_at_epoch: self.evidence_at_epoch,
            received_at_epoch: self.received_at_epoch,
            input_sequence: self.input_sequence,
            claude_code_version: None,
        }
    }
}

impl<T> V1Observed<T> {
    fn into_observed(self) -> Observed<T> {
        Observed {
            value: self.value,
            evidence_at_epoch: self.evidence_at_epoch,
            received_at_epoch: self.received_at_epoch,
            input_sequence: self.input_sequence,
            claude_code_version: None,
        }
    }
}

impl V1ObservedPercentage {
    fn into_observed_percentage(self) -> ObservedPercentage {
        ObservedPercentage {
            value: self.value,
            reset_at_epoch: self.reset_at_epoch,
            evidence_at_epoch: self.evidence_at_epoch,
            received_at_epoch: self.received_at_epoch,
            input_sequence: self.input_sequence,
            claude_code_version: None,
        }
    }
}

impl V1ResetBarrier {
    fn into_reset_barrier(self) -> ResetBarrier {
        ResetBarrier {
            started_at_epoch: self.started_at_epoch,
            prior_reset_at_epoch: self.prior_reset_at_epoch,
            prior_used_percentage_basis_points: self.prior_used_percentage_basis_points,
            evidence_sequence_before: self.evidence_sequence_before,
            dependency_evidence_sequence: self.dependency_evidence_sequence,
            pause_reason: self.pause_reason.into(),
        }
    }
}

impl V1AccountResetBarrier {
    fn into_account_reset_barrier(self) -> AccountResetBarrier {
        AccountResetBarrier {
            started_at_epoch: self.started_at_epoch,
            prior_reset_at_epoch: self.prior_reset_at_epoch,
            prior_used_percentage_basis_points: self.prior_used_percentage_basis_points,
            input_sequence_before: self.input_sequence_before,
            source: self.source.into(),
            session_id: self.session_id,
            pause_reason: self.pause_reason.into(),
        }
    }
}

impl V1DurableMonitor {
    fn migrate(
        &self,
        monitor_id: &str,
        binding_id: &str,
        goal: &V1DurableGoalSpend,
        policy: &MonitorPolicyRecord,
        retained_session_ids: &BTreeSet<String>,
        last_now_epoch: i64,
    ) -> Result<DurableMonitor, MonitorIssue> {
        if self.config.provider.into() != policy.provider
            || self.config.account_id != goal.account_id
            || self.config.goal_id != policy.goal_id
            || self.config.budget != goal.budget
        {
            return Err(unavailable());
        }
        let config = self.config.to_v2_config(binding_id);
        let mut events = Vec::with_capacity(self.events.len());
        for event in &self.events {
            let mut event_policy = policy.clone();
            event_policy.budget = event.status.budget.clone().map(V1Money::into_money);
            events.push(MonitorEvent {
                sequence: event.sequence,
                occurred_at_epoch: event.occurred_at_epoch,
                status: event.status.migrate(V1StatusMigrationContext {
                    monitor_id,
                    config: &self.config,
                    binding_id,
                    policy: &event_policy,
                    next_evidence_sequence: self.next_evidence_sequence,
                    next_decision_sequence: self.next_decision_sequence,
                    last_now_epoch,
                })?,
            });
        }

        Ok(DurableMonitor {
            config,
            account_id: Some(self.config.account_id.clone()),
            idempotency_key: format!("v1-migrated-{monitor_id}"),
            policy: Some(policy.clone()),
            created_at_epoch: self.created_at_epoch,
            stopped_at_epoch: self.stopped_at_epoch,
            updated_at_epoch: self.updated_at_epoch,
            last_reconciled_at_epoch: self.last_reconciled_at_epoch,
            next_evidence_sequence: self.next_evidence_sequence,
            next_decision_sequence: self.next_decision_sequence,
            next_event_sequence: self.next_event_sequence,
            evidence: self
                .evidence
                .iter()
                .cloned()
                .map(V1MonitorEvidence::into_monitor_evidence)
                .collect(),
            evidence_fingerprints: retain_migrated_evidence_fingerprints(
                self.evidence_fingerprints.clone(),
                retained_session_ids,
            ),
            reset_barriers: self
                .reset_barriers
                .clone()
                .map(|barrier| barrier.map(V1ResetBarrier::into_reset_barrier)),
            latest_decision: self
                .latest_decision
                .clone()
                .map(V1MonitorDecision::into_monitor_decision),
            decision_fingerprint: self.decision_fingerprint.clone(),
            events,
            spend_state: Some(self.spend_state.clone().into_spend_state()),
        })
    }
}

fn retain_migrated_evidence_fingerprints(
    fingerprints: BTreeMap<String, String>,
    retained_session_ids: &BTreeSet<String>,
) -> BTreeMap<String, String> {
    fingerprints
        .into_iter()
        .filter(|(key, value)| {
            super::valid_evidence_fingerprint(key, value)
                && (is_account_fingerprint_key(key)
                    || retained_session_ids
                        .iter()
                        .any(|session_id| fingerprint_key_references_session(key, session_id)))
        })
        .collect()
}

fn is_account_fingerprint_key(key: &str) -> bool {
    key == "spend:account"
        || ((key.starts_with("used:") || key.starts_with("reset:"))
            && key
                .rsplit_once(':')
                .is_some_and(|(_, scope)| scope == "account"))
}

fn fingerprint_key_references_session(key: &str, session_id: &str) -> bool {
    if key.strip_prefix("model:") == Some(session_id) {
        return true;
    }
    key.strip_prefix("used:")
        .or_else(|| key.strip_prefix("reset:"))
        .and_then(|rest| rest.rsplit_once(':'))
        .is_some_and(|(_, scope)| scope == session_id)
}

impl V1MonitorConfig {
    fn to_v2_config(&self, binding_id: &str) -> jackin_protocol::usage_monitor::MonitorConfig {
        jackin_protocol::usage_monitor::MonitorConfig {
            provider: self.provider.into(),
            purpose: MonitorPurpose::DispatchGuard,
            scope: MonitorScope::BoundAccount {
                binding_id: binding_id.to_owned(),
                binding_revision: 1,
                session_id: self.session_id.clone(),
            },
            goal_id: Some(self.goal_id.clone()),
            expected_model: self.expected_model.clone(),
            policy_revision: Some(1),
        }
    }
}

impl V1MonitorStatus {
    fn migrate(
        &self,
        context: V1StatusMigrationContext<'_>,
    ) -> Result<MonitorStatus, MonitorIssue> {
        let V1StatusMigrationContext {
            monitor_id,
            config,
            binding_id,
            policy,
            next_evidence_sequence,
            next_decision_sequence,
            last_now_epoch,
        } = context;
        if !self.is_valid_for(
            monitor_id,
            config,
            next_evidence_sequence,
            next_decision_sequence,
            last_now_epoch,
        ) {
            return Err(unavailable());
        }

        let evidence = self
            .evidence
            .iter()
            .cloned()
            .map(V1MonitorEvidence::into_monitor_evidence)
            .collect::<Vec<_>>();

        let latest_decision = self
            .latest_decision
            .clone()
            .map(V1MonitorDecision::into_monitor_decision);

        let mut migrated_policy = policy.clone();
        migrated_policy.budget = self.budget.clone().map(V1Money::into_money);
        let readiness = self.readiness();
        Ok(MonitorStatus {
            schema_version: USAGE_MONITOR_SCHEMA_VERSION,
            monitor_id: self.monitor_id.clone(),
            provider: self.provider.into(),
            purpose: MonitorPurpose::DispatchGuard,
            scope: MonitorScope::BoundAccount {
                binding_id: binding_id.to_owned(),
                binding_revision: 1,
                session_id: config.session_id.clone(),
            },
            account_id: Some(self.account_id.clone()),
            goal_id: Some(self.goal_id.clone()),
            session_id: self.session_id.clone(),
            claude_code_version: None,
            policy: Some(migrated_policy),
            expected_model: config.expected_model.clone(),
            model: self.model.clone(),
            model_evidence: self
                .model_evidence
                .clone()
                .map(V1MonitorFieldEvidence::into_field_evidence),
            model_guard_validity: self.model_guard_validity_for(config.expected_model.as_deref()),
            lifecycle: self.lifecycle.into(),
            readiness,
            runnable: self.runnable,
            five_hour: self
                .five_hour
                .clone()
                .into_quota_window_status(self.updated_at_epoch),
            seven_day: self
                .seven_day
                .clone()
                .into_quota_window_status(self.updated_at_epoch),
            budget: self.budget.clone().map(V1Money::into_money),
            cumulative_goal_spend: self.cumulative_goal_spend.clone().map(V1Money::into_money),
            spend_period_baseline: self
                .spend_period_baseline
                .clone()
                .map(V1SpendRecord::into_spend_record),
            evidence,
            latest_decision,
            updated_at_epoch: self.updated_at_epoch,
            issues: self
                .issues
                .iter()
                .cloned()
                .map(V1MonitorIssue::into_monitor_issue)
                .collect(),
        })
    }

    fn is_valid_for(
        &self,
        monitor_id: &str,
        config: &V1MonitorConfig,
        next_evidence_sequence: u64,
        next_decision_sequence: u64,
        last_now_epoch: i64,
    ) -> bool {
        let mut evidence_sequences = BTreeSet::new();
        self.schema_version == V1_SCHEMA_VERSION
            && self.monitor_id == monitor_id
            && self.provider.into() == config.provider.into()
            && self.account_id == config.account_id
            && self.goal_id == config.goal_id
            && self.session_id == config.session_id
            && valid_identifier(&self.account_id)
            && valid_goal_id(&self.goal_id)
            && self.session_id.as_deref().is_none_or(valid_identifier)
            && self
                .model
                .as_deref()
                .is_none_or(|model| valid_bounded_text(model, MAX_MODEL_LENGTH))
            && self.updated_at_epoch >= 0
            && self.updated_at_epoch <= last_now_epoch
            && self.evidence.len() <= MAX_EVIDENCE_PER_MONITOR
            && self.budget.as_ref().is_none_or(V1Money::is_valid)
            && self
                .cumulative_goal_spend
                .as_ref()
                .is_none_or(V1Money::is_valid)
            && self
                .spend_period_baseline
                .as_ref()
                .is_none_or(|record| record.is_valid_for(&self.account_id, last_now_epoch))
            && self.evidence.iter().all(|item| {
                item.is_valid_for(
                    &self.account_id,
                    config.session_id.as_deref(),
                    next_evidence_sequence,
                    last_now_epoch,
                ) && evidence_sequences.insert(item.sequence)
            })
            && self
                .five_hour
                .is_valid_for(next_evidence_sequence, last_now_epoch)
            && self
                .seven_day
                .is_valid_for(next_evidence_sequence, last_now_epoch)
            && self
                .model_evidence
                .as_ref()
                .is_none_or(|field| field.is_valid_for(next_evidence_sequence, last_now_epoch))
            && self.latest_decision.as_ref().is_none_or(|decision| {
                decision.is_valid_for(&self.goal_id, next_evidence_sequence, last_now_epoch)
                    && decision.sequence <= next_decision_sequence
            })
            && self.issues.iter().all(V1MonitorIssue::is_valid)
    }

    fn model_guard_validity_for(&self, expected_model: Option<&str>) -> MonitorModelGuardValidity {
        let Some(expected_model) = expected_model else {
            return MonitorModelGuardValidity::NotConfigured;
        };
        let current_model_evidence = self
            .model_evidence
            .as_ref()
            .is_some_and(|evidence| evidence.freshness == V1MonitorEvidenceFreshness::Current);
        let mismatch = self
            .issues
            .iter()
            .any(|issue| issue.code == V1MonitorIssueCode::ModelMismatch)
            || (current_model_evidence
                && self
                    .model
                    .as_deref()
                    .is_some_and(|model| model != expected_model));
        if mismatch {
            MonitorModelGuardValidity::Mismatch
        } else if current_model_evidence && self.model.as_deref() == Some(expected_model) {
            MonitorModelGuardValidity::Match
        } else {
            MonitorModelGuardValidity::Unknown
        }
    }

    fn readiness(&self) -> MonitorReadiness {
        let issue = |code| self.issues.iter().any(|item| item.code == code);
        let tracking = if self.lifecycle == V1MonitorLifecycle::Stopped
            || issue(V1MonitorIssueCode::BrokerUnavailable)
            || issue(V1MonitorIssueCode::MonitorStoreUnavailable)
        {
            MonitorTrackingReadiness::Unavailable
        } else if self.evidence.is_empty() {
            MonitorTrackingReadiness::Waiting
        } else {
            MonitorTrackingReadiness::Ready
        };

        let windows = [&self.five_hour, &self.seven_day];
        let exhausted = issue(V1MonitorIssueCode::LimitExhausted)
            || issue(V1MonitorIssueCode::LimitGuardReached)
            || windows.iter().any(|window| {
                window.used_percentage_basis_points == Some(10_000)
                    && window.used_evidence.as_ref().is_some_and(|evidence| {
                        evidence.freshness == V1MonitorEvidenceFreshness::Current
                    })
            });
        let stale =
            issue(V1MonitorIssueCode::QuotaStale)
                || windows.iter().any(|window| {
                    window.used_evidence.as_ref().is_some_and(|evidence| {
                        evidence.freshness == V1MonitorEvidenceFreshness::Stale
                    }) || window.reset_evidence.as_ref().is_some_and(|evidence| {
                        evidence.freshness == V1MonitorEvidenceFreshness::Stale
                    })
                });
        let complete = windows.iter().all(|window| {
            window.used_percentage_basis_points.is_some()
                && window.reset_at_epoch.is_some()
                && window.used_evidence.as_ref().is_some_and(|evidence| {
                    evidence.freshness == V1MonitorEvidenceFreshness::Current
                })
                && window.reset_evidence.as_ref().is_some_and(|evidence| {
                    evidence.freshness == V1MonitorEvidenceFreshness::Current
                })
        });
        let quota = if exhausted {
            MonitorQuotaReadiness::Exhausted
        } else if stale {
            MonitorQuotaReadiness::Stale
        } else if complete {
            MonitorQuotaReadiness::Ready
        } else {
            MonitorQuotaReadiness::Unknown
        };

        let budget_stale = issue(V1MonitorIssueCode::SpendStale)
            || self
                .spend_period_baseline
                .as_ref()
                .is_some_and(|record| record.verification == V1SpendVerification::Stale);
        let budget_verified = self.budget.is_some()
            && self
                .spend_period_baseline
                .as_ref()
                .is_some_and(|record| record.verification == V1SpendVerification::Verified)
            && self.cumulative_goal_spend.is_some()
            && !issue(V1MonitorIssueCode::BudgetUnverifiable)
            && !issue(V1MonitorIssueCode::SpendUnavailable)
            && !issue(V1MonitorIssueCode::SpendUnverified)
            && !issue(V1MonitorIssueCode::SpendRolloverUnverified);
        let budget = if budget_stale {
            MonitorBudgetReadiness::Stale
        } else if budget_verified {
            MonitorBudgetReadiness::Verified
        } else {
            MonitorBudgetReadiness::Unknown
        };

        MonitorReadiness {
            tracking,
            quota,
            budget,
            dispatch: if self.runnable {
                MonitorDispatchReadiness::Ready
            } else {
                MonitorDispatchReadiness::Blocked
            },
        }
    }
}

impl V1MonitorQuotaWindowStatus {
    fn into_quota_window_status(self, snapshot_epoch: i64) -> MonitorQuotaWindowStatus {
        MonitorQuotaWindowStatus {
            used_percentage_basis_points: self.used_percentage_basis_points,
            used_evidence: self
                .used_evidence
                .map(V1MonitorFieldEvidence::into_field_evidence),
            reset_at_epoch: self.reset_at_epoch,
            reset_validity: match self.reset_at_epoch {
                None => MonitorResetValidity::Unknown,
                Some(reset) if reset <= snapshot_epoch => MonitorResetValidity::Due,
                Some(_) => MonitorResetValidity::Future,
            },
            reset_evidence: self
                .reset_evidence
                .map(V1MonitorFieldEvidence::into_field_evidence),
        }
    }
}

impl V1MonitorFieldEvidence {
    fn into_field_evidence(self) -> MonitorFieldEvidence {
        MonitorFieldEvidence {
            evidence_sequence: self.evidence_sequence,
            evidence_at_epoch: self.evidence_at_epoch,
            evidence_received_at_epoch: self.evidence_received_at_epoch,
            age_seconds: self.age_seconds,
            freshness: self.freshness.into(),
        }
    }
}

impl V1MonitorEvidence {
    fn into_monitor_evidence(self) -> MonitorEvidence {
        MonitorEvidence {
            sequence: self.sequence,
            account_id: Some(self.account_id),
            session_id: self.session_id,
            claude_code_version: None,
            source: self.source.into(),
            evidence_at_epoch: self.evidence_at_epoch,
            evidence_received_at_epoch: self.evidence_received_at_epoch,
            age_seconds: self.age_seconds,
            value: self.value.into(),
        }
    }
}

impl V1MonitorEvidenceValue {
    fn into(self) -> MonitorEvidenceValue {
        match self {
            Self::QuotaWindow {
                window_id,
                remaining_raw_percent,
                reset_at_epoch,
            } => MonitorEvidenceValue::QuotaWindow {
                window_id,
                remaining_raw_percent,
                reset_at_epoch,
            },
            Self::QuotaUsedPercentage {
                window,
                used_percentage_basis_points,
            } => MonitorEvidenceValue::QuotaUsedPercentage {
                window: window.into(),
                used_percentage_basis_points,
            },
            Self::QuotaReset {
                window,
                reset_at_epoch,
            } => MonitorEvidenceValue::QuotaReset {
                window: window.into(),
                reset_at_epoch,
            },
            Self::Model { model } => MonitorEvidenceValue::Model { model },
            Self::Spend {
                amount,
                billing_period_start_epoch,
                billing_period_end_epoch,
                verification,
            } => MonitorEvidenceValue::Spend {
                amount: amount.into_money(),
                billing_period_start_epoch,
                billing_period_end_epoch,
                verification: verification.into(),
            },
            Self::Budget { amount } => MonitorEvidenceValue::Budget {
                amount: amount.into_money(),
            },
        }
    }
}

impl V1MonitorDecision {
    fn into_monitor_decision(self) -> MonitorDecision {
        MonitorDecision {
            sequence: self.sequence,
            decided_at_epoch: self.decided_at_epoch,
            evidence_sequences: self.evidence_sequences,
            actions: self
                .actions
                .into_iter()
                .map(V1MonitorAction::into_monitor_action)
                .collect(),
        }
    }
}

impl V1MonitorAction {
    fn into_monitor_action(self) -> MonitorAction {
        match self {
            Self::Checkpoint { goal_id } => MonitorAction::Checkpoint { goal_id },
            Self::ReduceDispatch { max_parallel } => MonitorAction::ReduceDispatch { max_parallel },
            Self::Pause { reason } => MonitorAction::Pause {
                reason: reason.into(),
            },
            Self::Wait { until_epoch } => MonitorAction::Wait { until_epoch },
            Self::Warn { reason } => MonitorAction::Warn {
                reason: reason.into(),
            },
        }
    }
}

impl V1MonitorIssue {
    fn into_monitor_issue(self) -> MonitorIssue {
        MonitorIssue {
            code: self.code.into(),
            message: self.message,
            retry_at_epoch: self.retry_at_epoch,
        }
    }
}

impl V1MonitorProvider {
    fn into(self) -> MonitorProvider {
        match self {
            Self::Claude => MonitorProvider::Claude,
        }
    }
}

impl V1MonitorLifecycle {
    fn into(self) -> MonitorLifecycle {
        match self {
            Self::Active => MonitorLifecycle::Active,
            Self::Waiting => MonitorLifecycle::Waiting,
            Self::Paused => MonitorLifecycle::Paused,
            Self::Stopped => MonitorLifecycle::Stopped,
            Self::NeedsEvidence => MonitorLifecycle::NeedsEvidence,
        }
    }
}

impl V1MonitorEvidenceFreshness {
    fn into(self) -> MonitorEvidenceFreshness {
        match self {
            Self::Current => MonitorEvidenceFreshness::Current,
            Self::Stale => MonitorEvidenceFreshness::Stale,
            Self::Unavailable => MonitorEvidenceFreshness::Unavailable,
        }
    }
}

impl V1MonitorEvidenceSource {
    fn into(self) -> MonitorEvidenceSource {
        match self {
            Self::BrokerProjection => MonitorEvidenceSource::BrokerProjection,
            Self::Statusline => MonitorEvidenceSource::Statusline,
            Self::ProviderSpend => MonitorEvidenceSource::ProviderSpend,
            Self::Operator => MonitorEvidenceSource::Operator,
            Self::LocalSessionLog => MonitorEvidenceSource::LocalSessionLog,
        }
    }
}

impl V1SpendVerification {
    fn into(self) -> SpendVerification {
        match self {
            Self::Verified => SpendVerification::Verified,
            Self::Unverified => SpendVerification::Unverified,
            Self::Stale => SpendVerification::Stale,
            Self::Unavailable => SpendVerification::Unavailable,
        }
    }
}

impl V1SpendRecordSource {
    fn into(self) -> SpendRecordSource {
        match self {
            Self::OperatorReceipt => SpendRecordSource::OperatorReceipt,
        }
    }
}

impl V1MonitorQuotaWindow {
    fn into(self) -> MonitorQuotaWindow {
        match self {
            Self::FiveHour => MonitorQuotaWindow::FiveHour,
            Self::SevenDay => MonitorQuotaWindow::SevenDay,
        }
    }
}

impl V1MonitorIssueCode {
    fn into(self) -> MonitorIssueCode {
        match self {
            Self::IndependentRefreshDisabled => MonitorIssueCode::IndependentRefreshDisabled,
            Self::AuthStatusUnknown => MonitorIssueCode::AuthStatusUnknown,
            Self::BrokerUnavailable => MonitorIssueCode::BrokerUnavailable,
            Self::StatuslineIngressUnsupported => MonitorIssueCode::StatuslineIngressUnsupported,
            Self::StatuslineTooLarge => MonitorIssueCode::StatuslineTooLarge,
            Self::StatuslineInvalid => MonitorIssueCode::StatuslineInvalid,
            Self::AccountMismatch => MonitorIssueCode::AccountMismatch,
            Self::ObservationStale => MonitorIssueCode::ObservationStale,
            Self::QuotaUnknown => MonitorIssueCode::QuotaUnknown,
            Self::SpendUnverified => MonitorIssueCode::SpendUnverified,
            Self::SpendStale => MonitorIssueCode::SpendStale,
            Self::SpendUnavailable => MonitorIssueCode::SpendUnavailable,
            Self::BudgetUnverifiable => MonitorIssueCode::BudgetUnverifiable,
            Self::ResetDueUnverified => MonitorIssueCode::ResetDueUnverified,
            Self::LimitExhausted => MonitorIssueCode::LimitExhausted,
            Self::LimitGuardReached => MonitorIssueCode::LimitGuardReached,
            Self::InteractionRequired => MonitorIssueCode::InteractionRequired,
            Self::QuotaStale => MonitorIssueCode::QuotaStale,
            Self::MissingReset => MonitorIssueCode::MissingReset,
            Self::BudgetWarn => MonitorIssueCode::BudgetWarn,
            Self::BudgetCheckpoint => MonitorIssueCode::BudgetCheckpoint,
            Self::BudgetPause => MonitorIssueCode::BudgetPause,
            Self::SpendRolloverUnverified => MonitorIssueCode::SpendRolloverUnverified,
            Self::ModelMismatch => MonitorIssueCode::ModelMismatch,
            Self::ModelUnknown => MonitorIssueCode::ModelUnknown,
            Self::MonitorNotFound => MonitorIssueCode::MonitorNotFound,
            Self::MonitorStoreUnavailable => MonitorIssueCode::MonitorStoreUnavailable,
            Self::WaitTimeout => MonitorIssueCode::WaitTimeout,
        }
    }
}

fn validate_v1(state: &V1StoreState) -> Result<(), MonitorIssue> {
    validate_v1_header(state)?;
    validate_v1_accounts(state)?;
    validate_v1_goals(state)?;
    validate_v1_monitors(state)
}

fn validate_v1_header(state: &V1StoreState) -> Result<(), MonitorIssue> {
    if state.schema_version != V1_SCHEMA_VERSION
        || state.next_monitor_id == 0
        || state.next_monitor_id.checked_add(1).is_none()
        || state.next_input_sequence == u64::MAX
        || state.last_now_epoch < 0
        || state.accounts.len() > MAX_ACCOUNTS
        || state.monitors.len() > MAX_MONITORS
        || state.goals.len() > MAX_GOALS
    {
        return Err(unavailable());
    }

    let mut max_monitor_id = 0;
    for monitor_id in state.monitors.keys() {
        if !valid_monitor_id(monitor_id) {
            return Err(unavailable());
        }
        let numeric_id = monitor_id
            .strip_prefix("monitor-")
            .and_then(|suffix| suffix.parse::<u64>().ok())
            .ok_or_else(unavailable)?;
        max_monitor_id = max_monitor_id.max(numeric_id);
    }
    if state.next_monitor_id <= max_monitor_id {
        return Err(unavailable());
    }
    Ok(())
}

fn validate_v1_accounts(state: &V1StoreState) -> Result<(), MonitorIssue> {
    for (account_id, account) in &state.accounts {
        if !valid_identifier(account_id)
            || account.sessions.len() > MAX_SESSIONS_PER_ACCOUNT
            || account.input_sequence > state.next_input_sequence
            || !account.spend.is_valid_for(account_id, state.last_now_epoch)
        {
            return Err(unavailable());
        }
        for reset in account.latest_reset_epochs.iter().flatten() {
            if *reset < 0 {
                return Err(unavailable());
            }
        }
        for barrier in account.reset_barriers.iter().flatten() {
            if !(9_500..=10_000).contains(&barrier.prior_used_percentage_basis_points)
                || barrier.started_at_epoch < 0
                || barrier.started_at_epoch > state.last_now_epoch
                || barrier.prior_reset_at_epoch.is_some_and(|epoch| epoch < 0)
                || barrier.input_sequence_before > account.input_sequence
                || match barrier.source {
                    V1MonitorEvidenceSource::Statusline => {
                        barrier.session_id.as_deref().is_none_or(|session_id| {
                            !valid_identifier(session_id)
                                || !account.sessions.contains_key(session_id)
                        })
                    }
                    V1MonitorEvidenceSource::BrokerProjection => barrier.session_id.is_some(),
                    _ => true,
                }
            {
                return Err(unavailable());
            }
        }
        for (session_id, session) in &account.sessions {
            if !valid_identifier(session_id)
                || session
                    .last_observation_received_at_epoch
                    .is_some_and(|epoch| epoch < 0 || epoch > state.last_now_epoch)
            {
                return Err(unavailable());
            }
            if session
                .model
                .as_ref()
                .is_some_and(|model| model.input_sequence > account.input_sequence)
                || session.model.as_ref().is_some_and(|model| {
                    !valid_bounded_text(&model.value, MAX_MODEL_LENGTH)
                        || !valid_observed(model, state.last_now_epoch)
                })
            {
                return Err(unavailable());
            }
            for window in &session.windows {
                if !valid_observed_window(window, account.input_sequence, state.last_now_epoch) {
                    return Err(unavailable());
                }
            }
        }
        for window in &account.broker_windows {
            if !valid_observed_window(window, account.input_sequence, state.last_now_epoch) {
                return Err(unavailable());
            }
        }
    }
    Ok(())
}

fn validate_v1_goals(state: &V1StoreState) -> Result<(), MonitorIssue> {
    for (goal_id, goal) in &state.goals {
        if !valid_goal_id(goal_id)
            || !valid_identifier(&goal.account_id)
            || goal
                .budget
                .as_ref()
                .is_some_and(|budget| !budget.is_valid())
            || !goal
                .spend_state
                .is_valid_for(&goal.account_id, state.last_now_epoch)
        {
            return Err(unavailable());
        }
    }
    Ok(())
}

fn validate_v1_monitors(state: &V1StoreState) -> Result<(), MonitorIssue> {
    for (monitor_id, monitor) in &state.monitors {
        let config = &monitor.config;
        if !monitor_id.starts_with("monitor-")
            || !valid_monitor_id(monitor_id)
            || !valid_identifier(&config.account_id)
            || !valid_goal_id(&config.goal_id)
            || config
                .session_id
                .as_deref()
                .is_some_and(|session| !valid_identifier(session))
            || config
                .expected_model
                .as_deref()
                .is_some_and(|model| !valid_bounded_text(model, MAX_MODEL_LENGTH))
            || config
                .budget
                .as_ref()
                .is_some_and(|budget| !budget.is_valid())
            || monitor.created_at_epoch < 0
            || monitor.created_at_epoch > state.last_now_epoch
            || monitor.updated_at_epoch < monitor.created_at_epoch
            || monitor.updated_at_epoch > state.last_now_epoch
            || monitor.last_reconciled_at_epoch < monitor.created_at_epoch
            || monitor.last_reconciled_at_epoch > state.last_now_epoch
            || monitor.evidence.len() > MAX_EVIDENCE_PER_MONITOR
            || monitor.events.len() > MAX_EVENTS_PER_MONITOR
            || monitor.stopped_at_epoch.is_some_and(|epoch| {
                epoch < monitor.created_at_epoch || epoch > state.last_now_epoch
            })
            || monitor.reset_barriers.iter().flatten().any(|barrier| {
                !(9_500..=10_000).contains(&barrier.prior_used_percentage_basis_points)
                    || barrier.started_at_epoch < monitor.created_at_epoch
                    || barrier.started_at_epoch > state.last_now_epoch
                    || barrier.prior_reset_at_epoch.is_some_and(|epoch| epoch < 0)
                    || barrier.evidence_sequence_before > monitor.next_evidence_sequence
                    || barrier
                        .dependency_evidence_sequence
                        .is_some_and(|sequence| sequence > monitor.next_evidence_sequence)
            })
            || monitor.next_evidence_sequence == u64::MAX
            || monitor.next_decision_sequence == u64::MAX
            || monitor.next_event_sequence == u64::MAX
            || monitor.next_evidence_sequence
                < monitor
                    .evidence
                    .iter()
                    .map(|item| item.sequence)
                    .max()
                    .unwrap_or(0)
            || monitor.next_decision_sequence
                < monitor
                    .latest_decision
                    .as_ref()
                    .map_or(0, |decision| decision.sequence)
            || monitor.next_event_sequence
                < monitor
                    .events
                    .iter()
                    .map(|event| event.sequence)
                    .max()
                    .unwrap_or(0)
            || state.goals.get(&config.goal_id).is_none_or(|goal| {
                goal.account_id != config.account_id || monitor.spend_state != goal.spend_state
            })
        {
            return Err(unavailable());
        }
        let mut previous_event_sequence = 0;
        for event in &monitor.events {
            if event.sequence <= previous_event_sequence
                || event.sequence > monitor.next_event_sequence
                || event.occurred_at_epoch < 0
                || event.occurred_at_epoch > state.last_now_epoch
            {
                return Err(unavailable());
            }
            previous_event_sequence = event.sequence;
            if event.status.monitor_id != *monitor_id
                || event.status.account_id != config.account_id
                || event.status.goal_id != config.goal_id
                || !event.status.is_valid_for(
                    monitor_id,
                    config,
                    monitor.next_evidence_sequence,
                    monitor.next_decision_sequence,
                    state.last_now_epoch,
                )
            {
                return Err(unavailable());
            }
        }
        let mut evidence_sequences = BTreeSet::new();
        for evidence in &monitor.evidence {
            if !evidence.is_valid_for(
                &config.account_id,
                config.session_id.as_deref(),
                monitor.next_evidence_sequence,
                state.last_now_epoch,
            ) || !evidence_sequences.insert(evidence.sequence)
            {
                return Err(unavailable());
            }
        }
        if monitor.latest_decision.as_ref().is_some_and(|decision| {
            !decision.is_valid_for(
                &config.goal_id,
                monitor.next_evidence_sequence,
                state.last_now_epoch,
            ) || decision.sequence > monitor.next_decision_sequence
        }) {
            return Err(unavailable());
        }
    }
    Ok(())
}

fn valid_observed_window(
    window: &V1ObservedWindow,
    input_sequence: u64,
    last_now_epoch: i64,
) -> bool {
    window.used.as_ref().is_none_or(|used| {
        (0..=10_000).contains(&used.value)
            && used.reset_at_epoch.is_none_or(|epoch| epoch >= 0)
            && used.input_sequence <= input_sequence
            && used.input_sequence > 0
            && valid_timestamp(
                used.evidence_at_epoch,
                used.received_at_epoch,
                last_now_epoch,
            )
    }) && window.reset.as_ref().is_none_or(|reset| {
        reset.value >= 0
            && reset.input_sequence <= input_sequence
            && valid_observed(reset, last_now_epoch)
    }) && window.paired.as_ref().is_none_or(|pair| {
        (0..=10_000).contains(&pair.used_percentage_basis_points)
            && pair.reset_at_epoch >= 0
            && pair.input_sequence > 0
            && pair.input_sequence <= input_sequence
            && valid_timestamp(
                pair.evidence_at_epoch,
                pair.received_at_epoch,
                last_now_epoch,
            )
    })
}

fn valid_observed<T>(observed: &V1Observed<T>, last_now_epoch: i64) -> bool {
    observed.input_sequence > 0
        && valid_timestamp(
            observed.evidence_at_epoch,
            observed.received_at_epoch,
            last_now_epoch,
        )
}

fn valid_timestamp(evidence_at: Option<i64>, received_at: i64, last_now_epoch: i64) -> bool {
    received_at >= 0
        && received_at <= last_now_epoch
        && evidence_at.is_none_or(|epoch| {
            epoch >= 0 && epoch <= last_now_epoch.saturating_add(MAX_FUTURE_SKEW_SECS)
        })
}

fn binding_ids(state: &V1StoreState) -> Result<BTreeMap<String, String>, MonitorIssue> {
    let mut account_ids = BTreeSet::new();
    account_ids.extend(state.accounts.keys().cloned());
    account_ids.extend(state.goals.values().map(|goal| goal.account_id.clone()));
    account_ids.extend(
        state
            .monitors
            .values()
            .map(|monitor| monitor.config.account_id.clone()),
    );
    if account_ids.len() > super::MAX_BINDINGS {
        return Err(unavailable());
    }
    account_ids
        .into_iter()
        .enumerate()
        .map(|(index, account_id)| {
            let ordinal = u64::try_from(index)
                .ok()
                .and_then(|index| index.checked_add(1))
                .ok_or_else(unavailable)?;
            Ok((account_id, format!("binding-{ordinal:08}")))
        })
        .collect()
}

fn build_bindings(
    binding_ids: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, Vec<MonitorAccountBinding>>, MonitorIssue> {
    binding_ids
        .iter()
        .map(|(account_id, binding_id)| {
            Ok((
                binding_id.clone(),
                vec![MonitorAccountBinding {
                    binding_id: binding_id.clone(),
                    provider: MonitorProvider::Claude,
                    account_id: account_id.clone(),
                    operator_label: LEGACY_BINDING_LABEL.to_owned(),
                    revision: 1,
                    operator_confirmed: false,
                    confirmed_at_epoch: None,
                }],
            ))
        })
        .collect()
}

fn valid_monitor_id(value: &str) -> bool {
    let Some(suffix) = value.strip_prefix("monitor-") else {
        return false;
    };
    (8..=20).contains(&suffix.len())
        && suffix.parse::<u64>().is_ok()
        && suffix.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_LENGTH
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn valid_goal_id(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_GOAL_ID_LENGTH
        && !value.chars().any(char::is_control)
}

fn valid_bounded_text(value: &str, limit: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= limit
        && value.is_ascii()
        && !value.chars().any(char::is_control)
}

fn unavailable() -> MonitorIssue {
    MonitorIssue {
        code: MonitorIssueCode::MonitorStoreUnavailable,
        message: "monitor state is unavailable or invalid".to_owned(),
        retry_at_epoch: None,
    }
}

#[cfg(test)]
mod binding_union_tests {
    use super::*;

    fn empty_spend() -> V1SpendState {
        V1SpendState {
            baseline: None,
            period_anchor: None,
            cumulative_goal_spend: None,
            rollover_unknown: false,
            cumulative_complete: false,
        }
    }

    fn empty_account() -> V1AccountObservations {
        V1AccountObservations {
            sessions: BTreeMap::new(),
            broker_windows: std::array::from_fn(|_| V1ObservedWindow {
                used: None,
                reset: None,
                paired: None,
            }),
            latest_reset_epochs: [None, None],
            reset_barriers: [None, None],
            input_sequence: 0,
            spend: V1SpendAccountState {
                latest_record: None,
                current_period_record: None,
                previous_period_record: None,
            },
        }
    }

    fn empty_monitor(account_id: String, goal_id: String) -> V1DurableMonitor {
        V1DurableMonitor {
            config: V1MonitorConfig {
                provider: V1MonitorProvider::Claude,
                account_id,
                goal_id,
                session_id: None,
                expected_model: None,
                budget: None,
            },
            created_at_epoch: 0,
            stopped_at_epoch: None,
            updated_at_epoch: 0,
            last_reconciled_at_epoch: 0,
            next_evidence_sequence: 0,
            next_decision_sequence: 0,
            next_event_sequence: 0,
            evidence: Vec::new(),
            evidence_fingerprints: BTreeMap::new(),
            reset_barriers: [None, None],
            latest_decision: None,
            decision_fingerprint: None,
            events: Vec::new(),
            spend_state: empty_spend(),
        }
    }

    #[test]
    fn migration_binding_bound_covers_full_v1_account_goal_monitor_union() {
        let accounts = (0..MAX_ACCOUNTS)
            .map(|index| (format!("acct-account-{index}"), empty_account()))
            .collect();
        let goals = (0..MAX_GOALS)
            .map(|index| {
                (
                    format!("goal-{index}"),
                    V1DurableGoalSpend {
                        account_id: format!("acct-goal-{index}"),
                        budget: None,
                        spend_state: empty_spend(),
                    },
                )
            })
            .collect();
        let monitors = (0..MAX_MONITORS)
            .map(|index| {
                (
                    format!("monitor-{:08}", index + 1),
                    empty_monitor(format!("acct-monitor-{index}"), "goal-0".to_owned()),
                )
            })
            .collect();
        let state = V1StoreState {
            schema_version: V1_SCHEMA_VERSION,
            next_monitor_id: MAX_MONITORS as u64 + 1,
            next_input_sequence: 0,
            last_now_epoch: 0,
            accounts,
            monitors,
            goals,
        };

        let ids = binding_ids(&state).expect("allocate every bounded legacy account identity");
        assert_eq!(ids.len(), MAX_ACCOUNTS + MAX_GOALS + MAX_MONITORS);
        let bindings = build_bindings(&ids).expect("build migrated unconfirmed bindings");
        assert_eq!(bindings.len(), super::super::MAX_BINDINGS);
    }

    #[test]
    fn migration_drops_only_fingerprints_for_pruned_legacy_sessions() {
        let fingerprints = BTreeMap::from([
            ("model:active-session".to_owned(), "model-active".to_owned()),
            ("model:old-session".to_owned(), "model-old".to_owned()),
            (
                "used:five_hour:active-session".to_owned(),
                "used-active".to_owned(),
            ),
            (
                "reset:seven_day:old-session".to_owned(),
                "reset-old".to_owned(),
            ),
            (
                "used:five_hour:account".to_owned(),
                "account-used".to_owned(),
            ),
            ("spend:account".to_owned(), "account-spend".to_owned()),
        ]);
        let retained = retain_migrated_evidence_fingerprints(
            fingerprints,
            &BTreeSet::from(["active-session".to_owned()]),
        );
        assert_eq!(retained.len(), 4);
        assert!(retained.contains_key("model:active-session"));
        assert!(retained.contains_key("used:five_hour:active-session"));
        assert!(retained.contains_key("used:five_hour:account"));
        assert!(retained.contains_key("spend:account"));
        assert!(!retained.contains_key("model:old-session"));
        assert!(!retained.contains_key("reset:seven_day:old-session"));
    }

    #[test]
    fn v1_event_status_evidence_requires_unique_sequences_within_the_snapshot() {
        let config = V1MonitorConfig {
            provider: V1MonitorProvider::Claude,
            account_id: "acct-v1-event-evidence".to_owned(),
            goal_id: "goal-v1-event-evidence".to_owned(),
            session_id: Some("session-v1-event-evidence".to_owned()),
            expected_model: None,
            budget: None,
        };
        let evidence = || V1MonitorEvidence {
            sequence: 1,
            account_id: config.account_id.clone(),
            session_id: config.session_id.clone(),
            source: V1MonitorEvidenceSource::Statusline,
            evidence_at_epoch: Some(0),
            evidence_received_at_epoch: 0,
            age_seconds: 0,
            value: V1MonitorEvidenceValue::Model {
                model: "claude-sonnet".to_owned(),
            },
        };
        let mut status = V1MonitorStatus {
            schema_version: V1_SCHEMA_VERSION,
            monitor_id: "monitor-00000001".to_owned(),
            provider: V1MonitorProvider::Claude,
            account_id: config.account_id.clone(),
            goal_id: config.goal_id.clone(),
            session_id: config.session_id.clone(),
            model: Some("claude-sonnet".to_owned()),
            model_evidence: None,
            lifecycle: V1MonitorLifecycle::Active,
            runnable: false,
            five_hour: V1MonitorQuotaWindowStatus {
                used_percentage_basis_points: None,
                used_evidence: None,
                reset_at_epoch: None,
                reset_evidence: None,
            },
            seven_day: V1MonitorQuotaWindowStatus {
                used_percentage_basis_points: None,
                used_evidence: None,
                reset_at_epoch: None,
                reset_evidence: None,
            },
            budget: None,
            cumulative_goal_spend: None,
            spend_period_baseline: None,
            evidence: vec![evidence(), evidence()],
            latest_decision: None,
            updated_at_epoch: 0,
            issues: Vec::new(),
        };
        assert!(!status.is_valid_for("monitor-00000001", &config, 1, 0, 0));
        status.evidence.pop();
        assert!(status.is_valid_for("monitor-00000001", &config, 1, 0, 0));
        status.evidence[0].sequence = 2;
        assert!(!status.is_valid_for("monitor-00000001", &config, 1, 0, 0));
    }
}
