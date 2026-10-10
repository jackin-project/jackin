// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Durable, secret-free monitor state and local decision engine.

mod evaluation;
mod legacy;
mod lifecycle;
mod observations;
mod operations;
mod spend;
mod statusline;
mod storage;
mod validation;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::sync::{Condvar, Mutex};

use evaluation::{
    evidence_is_relevant, field_age, is_current, model_status, quota_pause_reason,
    quota_window_status, reconcile_all_monitors, refresh_all_goal_spend,
    spend_decision_blocks_dispatch, spend_issue, spend_policy, upsert_evidence,
};
use jackin_protocol::control::Money;
use lifecycle::{
    append_event, append_event_if_changed, prune_inactive_sessions,
    prune_inactive_unbound_sessions, session_is_active, status_for, update_reset_barrier,
};
use observations::{
    advance_account_reset_barriers, apply_statusline, apply_unbound_statusline,
    latch_account_reset_barrier, max_option, parse_counter_id, sync_monitor_from_account,
    sync_monitor_from_session, valid_bounded_text, valid_evidence_fingerprint, valid_identifier,
    valid_source_capability_id, validate_identifier, validate_observation,
};
use validation::validate_store_state;
#[cfg(test)]
use validation::{spend_snapshot_preserves_history, spend_state_matches_account};

use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageFreshnessPhaseV1, UsageMetricGroupKindV1, UsageMetricPeriodV1,
    UsageMetricValueV1, UsageProjectionV1, UsageWindowCategoryV1,
};
use jackin_protocol::usage_monitor::{
    MonitorAccountBinding, MonitorAccountBindingInput, MonitorAction, MonitorAuthState,
    MonitorBudgetReadiness, MonitorConfig, MonitorDecision, MonitorDispatchReadiness,
    MonitorDoctorReport, MonitorEvent, MonitorEvidence, MonitorEvidenceFreshness,
    MonitorEvidenceSource, MonitorEvidenceValue, MonitorFieldEvidence, MonitorIssue,
    MonitorIssueCode, MonitorLifecycle, MonitorModelGuardValidity, MonitorOperation, MonitorPolicy,
    MonitorPolicyApprovalInput, MonitorPolicyOrigin, MonitorPolicyRecord, MonitorPurpose,
    MonitorQuotaReadiness, MonitorQuotaWindow, MonitorQuotaWindowStatus, MonitorReadiness,
    MonitorReply, MonitorResetValidity, MonitorScope, MonitorServiceStatus, MonitorStatus,
    MonitorTrackingReadiness, SpendRecord, SpendRecordInput, SpendVerification,
    StatuslineObservation, StatuslineQuotaWindow, USAGE_MONITOR_SCHEMA_VERSION,
    USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};

use self::spend::{
    SpendAccountState, SpendDecision, SpendState, advance_goal_spend, capture_goal_baseline,
    evaluate_spend_policy, record_account_spend,
};

pub use statusline::parse_statusline;

/// Maximum age of one independently supplied monitor field.
pub(crate) const MONITOR_EVIDENCE_TTL_SECS: i64 = 300;
/// Additional time allowed after a reported reset for a fresh statusline event.
pub(crate) const MONITOR_RESET_GRACE_SECS: i64 = 60;
/// Maximum time one watch operation can hold a broker worker.
pub(crate) const MONITOR_WATCH_TIMEOUT_CAP_MS: u64 = 30_000;

const MAX_MONITORS: usize = 128;
const MAX_ACCOUNTS: usize = 128;
const MAX_GOALS: usize = 128;
const MAX_SESSIONS_PER_ACCOUNT: usize = 16;
const MAX_UNBOUND_SESSIONS: usize = MAX_ACCOUNTS * MAX_SESSIONS_PER_ACCOUNT;
const MAX_BINDINGS: usize = MAX_ACCOUNTS + MAX_GOALS + MAX_MONITORS;
const MAX_POLICY_REVISIONS: usize = MAX_GOALS * 4;
const MAX_EVIDENCE_PER_MONITOR: usize = 96;
const MAX_EVIDENCE_FINGERPRINTS: usize = MAX_SESSIONS_PER_ACCOUNT * 5 + 5;
const MAX_EVENTS_PER_MONITOR: usize = 8;
const MAX_ID_LENGTH: usize = 128;
const MAX_GOAL_ID_LENGTH: usize = 256;
const MAX_IDEMPOTENCY_KEY_LENGTH: usize = 512;
const MIGRATED_IDEMPOTENCY_KEY_PREFIX: &str = "v1-migrated-";
const MAX_MODEL_LENGTH: usize = 128;
const MAX_OPERATOR_LABEL_LENGTH: usize = 128;
const MAX_FUTURE_SKEW_SECS: i64 = 60;
const DECISION_MAX_PARALLEL: u32 = 1;

#[derive(Debug)]
struct MonitorStoreInner {
    directory: File,
    state: Mutex<StoreState>,
    experimental_collector_source: Mutex<Option<String>>,
    changed: Condvar,
}

/// Thread-safe durable store for local monitor evidence and decisions.
#[derive(Debug, Clone)]
pub(crate) struct MonitorStore {
    inner: std::sync::Arc<MonitorStoreInner>,
}

/// Persisted state schema owned by this broker implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoreState {
    schema_version: u16,
    next_monitor_id: u64,
    next_input_sequence: u64,
    next_binding_id: u64,
    last_now_epoch: i64,
    accounts: BTreeMap<String, AccountObservations>,
    unbound_sessions: BTreeMap<String, SessionObservation>,
    bindings: BTreeMap<String, Vec<MonitorAccountBinding>>,
    policy_records: BTreeMap<String, Vec<MonitorPolicyRecord>>,
    monitors: BTreeMap<String, DurableMonitor>,
    goals: BTreeMap<String, DurableGoalSpend>,
}

impl Default for StoreState {
    fn default() -> Self {
        Self {
            schema_version: USAGE_MONITOR_SCHEMA_VERSION,
            next_monitor_id: 1,
            next_input_sequence: 0,
            next_binding_id: 1,
            last_now_epoch: 0,
            accounts: BTreeMap::new(),
            unbound_sessions: BTreeMap::new(),
            bindings: BTreeMap::new(),
            policy_records: BTreeMap::new(),
            monitors: BTreeMap::new(),
            goals: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountObservations {
    sessions: BTreeMap<String, SessionObservation>,
    broker_windows: [ObservedWindow; 2],
    latest_reset_epochs: [Option<i64>; 2],
    reset_barriers: [Option<AccountResetBarrier>; 2],
    input_sequence: u64,
    spend: SpendAccountState,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionObservation {
    model: Option<Observed<String>>,
    windows: [ObservedWindow; 2],
    last_observation_received_at_epoch: Option<i64>,
    claude_code_version: Option<String>,
    last_callback_received_at_epoch: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedWindow {
    used: Option<ObservedPercentage>,
    reset: Option<Observed<i64>>,
    paired: Option<ObservedQuotaPair>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedQuotaPair {
    used_percentage_basis_points: i32,
    reset_at_epoch: i64,
    evidence_at_epoch: Option<i64>,
    received_at_epoch: i64,
    input_sequence: u64,
    claude_code_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Observed<T> {
    value: T,
    evidence_at_epoch: Option<i64>,
    received_at_epoch: i64,
    input_sequence: u64,
    claude_code_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedPercentage {
    value: i32,
    reset_at_epoch: Option<i64>,
    evidence_at_epoch: Option<i64>,
    received_at_epoch: i64,
    input_sequence: u64,
    claude_code_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableMonitor {
    config: MonitorConfig,
    account_id: Option<String>,
    idempotency_key: String,
    policy: Option<MonitorPolicyRecord>,
    created_at_epoch: i64,
    stopped_at_epoch: Option<i64>,
    updated_at_epoch: i64,
    last_reconciled_at_epoch: i64,
    next_evidence_sequence: u64,
    next_decision_sequence: u64,
    next_event_sequence: u64,
    evidence: Vec<MonitorEvidence>,
    evidence_fingerprints: BTreeMap<String, String>,
    reset_barriers: [Option<ResetBarrier>; 2],
    latest_decision: Option<MonitorDecision>,
    decision_fingerprint: Option<String>,
    events: Vec<MonitorEvent>,
    spend_state: Option<SpendState>,
}

/// Spend and selected policy are durable by operator goal, not monitor instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableGoalSpend {
    account_id: String,
    binding_id: String,
    binding_revision: u64,
    policy_revision: u64,
    policy: MonitorPolicy,
    budget: Option<Money>,
    spend_state: Option<SpendState>,
}

struct EvidenceInput<'a> {
    account_id: Option<&'a str>,
    session_id: Option<&'a str>,
    source: MonitorEvidenceSource,
    evidence_at_epoch: Option<i64>,
    received_at_epoch: i64,
    claude_code_version: Option<&'a str>,
    value: MonitorEvidenceValue,
    fingerprint_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResetBarrier {
    started_at_epoch: i64,
    prior_reset_at_epoch: Option<i64>,
    prior_used_percentage_basis_points: i32,
    evidence_sequence_before: u64,
    dependency_evidence_sequence: Option<u64>,
    pause_reason: MonitorIssueCode,
}

/// Account-wide quota pause that survives monitor stop/recreation and goal changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountResetBarrier {
    started_at_epoch: i64,
    prior_reset_at_epoch: Option<i64>,
    prior_used_percentage_basis_points: i32,
    input_sequence_before: u64,
    source: MonitorEvidenceSource,
    session_id: Option<String>,
    pause_reason: MonitorIssueCode,
}

struct StartAuthority {
    account_id: Option<String>,
    policy: Option<MonitorPolicyRecord>,
    goal_id: Option<String>,
}

fn prepare_start_authority(
    state: &mut StoreState,
    config: &MonitorConfig,
    now_epoch: i64,
) -> Result<StartAuthority, MonitorIssue> {
    match (&config.purpose, &config.scope) {
        (MonitorPurpose::ObserveOnly, MonitorScope::Session { .. }) => Ok(StartAuthority {
            account_id: None,
            policy: None,
            goal_id: None,
        }),
        (MonitorPurpose::ObserveOnly, MonitorScope::BoundAccount { .. }) => {
            let binding = resolve_scope_binding(state, &config.scope, config.provider)?.clone();
            if !binding.operator_confirmed {
                return Err(operator_confirmation_required());
            }
            validate_collection_binding(config, &binding)?;
            Ok(StartAuthority {
                account_id: Some(binding.account_id),
                policy: None,
                goal_id: None,
            })
        }
        (MonitorPurpose::DispatchGuard, MonitorScope::BoundAccount { .. }) => {
            prepare_dispatch_guard_authority(state, config, now_epoch)
        }
        (MonitorPurpose::DispatchGuard, MonitorScope::Session { .. }) => Err(binding_required()),
    }
}

fn validate_collection_binding(
    config: &MonitorConfig,
    binding: &MonitorAccountBinding,
) -> Result<(), MonitorIssue> {
    if config.experimental_collector {
        if binding.provider_account_id.is_none() {
            return Err(binding_required());
        }
        if !binding.experimental_collector_approved {
            return Err(operator_confirmation_required());
        }
    }
    Ok(())
}

fn prepare_dispatch_guard_authority(
    state: &mut StoreState,
    config: &MonitorConfig,
    now_epoch: i64,
) -> Result<StartAuthority, MonitorIssue> {
    let binding = resolve_scope_binding(state, &config.scope, config.provider)?.clone();
    if !binding.operator_confirmed {
        return Err(operator_confirmation_required());
    }
    let goal_id = config.goal_id.as_deref().ok_or_else(policy_required)?;
    let policy = current_policy(state, goal_id)
        .filter(|policy| Some(policy.revision) == config.policy_revision)
        .cloned()
        .ok_or_else(policy_required)?;
    validate_start_policy_for_binding(&binding, &policy)?;
    prepare_goal_activation(state, &binding, goal_id, &policy, now_epoch)?;
    Ok(StartAuthority {
        account_id: Some(binding.account_id),
        policy: Some(policy),
        goal_id: Some(goal_id.to_owned()),
    })
}

fn validate_start_policy_for_binding(
    binding: &MonitorAccountBinding,
    policy: &MonitorPolicyRecord,
) -> Result<(), MonitorIssue> {
    if policy.provider != binding.provider || policy.account_id != binding.account_id {
        return Err(binding_mismatch());
    }
    if policy.origin != MonitorPolicyOrigin::Operator || !policy.operator_confirmed {
        return Err(policy_required());
    }
    if policy.binding_id.as_deref() != Some(&binding.binding_id)
        || policy.binding_revision != Some(binding.revision)
    {
        return Err(binding_mismatch());
    }
    if policy.new_policy != MonitorPolicy::StrictSgd
        && (policy.new_policy != MonitorPolicy::QuotaOnly
            || !policy.acknowledge_no_sgd_cap
            || policy.budget.is_some())
    {
        return Err(policy_conflict());
    }
    Ok(())
}

fn prepare_goal_activation(
    state: &mut StoreState,
    binding: &MonitorAccountBinding,
    goal_id: &str,
    policy: &MonitorPolicyRecord,
    now_epoch: i64,
) -> Result<(), MonitorIssue> {
    if let Some(previous) = state.goals.get(goal_id) {
        if previous.account_id != binding.account_id || previous.binding_id != binding.binding_id {
            return Err(issue(
                MonitorIssueCode::AccountMismatch,
                "a durable goal cannot change its bound account",
                None,
            ));
        }
        if previous.policy != policy.new_policy
            || (previous.policy == MonitorPolicy::StrictSgd
                && !budget_is_same_or_tighter(previous.budget.as_ref(), policy.budget.as_ref()))
        {
            return Err(policy_conflict());
        }
    } else if state.goals.len() >= MAX_GOALS {
        return Err(store_unavailable());
    }

    if policy.new_policy == MonitorPolicy::StrictSgd && !state.goals.contains_key(goal_id) {
        let spend = state
            .accounts
            .get(&binding.account_id)
            .map(|account| &account.spend)
            .cloned()
            .unwrap_or_default();
        let spend_state = capture_goal_baseline(&spend, policy.budget.as_ref(), now_epoch);
        if spend_state.baseline.is_none() {
            return Err(issue(
                MonitorIssueCode::BudgetUnverifiable,
                "strict goal activation requires a fresh, verified, compatible SGD baseline",
                None,
            ));
        }
        state.goals.insert(
            goal_id.to_owned(),
            DurableGoalSpend {
                account_id: binding.account_id.clone(),
                binding_id: binding.binding_id.clone(),
                binding_revision: binding.revision,
                policy_revision: policy.revision,
                policy: policy.new_policy,
                budget: policy.budget.clone(),
                spend_state: Some(spend_state),
            },
        );
    } else if let Some(goal) = state.goals.get_mut(goal_id) {
        // A new binding revision is accepted only through an explicit
        // confirmed binding; the original spend baseline is retained.
        goal.binding_revision = binding.revision;
        goal.policy_revision = policy.revision;
        goal.policy = policy.new_policy;
        goal.budget = policy.budget.clone();
    } else {
        state.goals.insert(
            goal_id.to_owned(),
            DurableGoalSpend {
                account_id: binding.account_id.clone(),
                binding_id: binding.binding_id.clone(),
                binding_revision: binding.revision,
                policy_revision: policy.revision,
                policy: policy.new_policy,
                budget: None,
                spend_state: None,
            },
        );
    }
    Ok(())
}

fn budget_is_same_or_tighter(previous: Option<&Money>, next: Option<&Money>) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    let Some(next) = next else {
        return false;
    };
    previous.currency == next.currency
        && previous.exponent == next.exponent
        && next.amount_minor >= 0
        && next.amount_minor <= previous.amount_minor
}

fn is_migrated_zero_sgd_budget_repair(policy: &MonitorPolicyRecord) -> bool {
    policy.origin == MonitorPolicyOrigin::MigratedV1
        && policy.new_policy == MonitorPolicy::StrictSgd
        && policy.budget.as_ref().is_some_and(|budget| {
            budget.currency == "SGD" && budget.exponent == 2 && budget.amount_minor == 0
        })
}

fn validate_config(config: &MonitorConfig) -> Result<(), MonitorIssue> {
    match &config.scope {
        MonitorScope::Session { session_id } => validate_identifier(session_id)?,
        MonitorScope::BoundAccount {
            binding_id,
            binding_revision,
            session_id,
        } => {
            validate_identifier(binding_id)?;
            if *binding_revision == 0 {
                return Err(binding_mismatch());
            }
            if let Some(session_id) = session_id {
                validate_identifier(session_id)?;
            }
        }
    }
    if config.experimental_collector {
        if config.purpose != MonitorPurpose::ObserveOnly {
            return Err(issue(
                MonitorIssueCode::ObservationOnly,
                "experimental collection is available only to observation-only monitors",
                None,
            ));
        }
        if config.provider != jackin_protocol::usage_monitor::MonitorProvider::Claude
            || !matches!(config.scope, MonitorScope::BoundAccount { .. })
        {
            return Err(binding_required());
        }
    }
    match config.purpose {
        MonitorPurpose::ObserveOnly => {
            if config.goal_id.is_some() || config.policy_revision.is_some() {
                return Err(issue(
                    MonitorIssueCode::ObservationOnly,
                    "observation-only monitors cannot select a goal or policy revision",
                    None,
                ));
            }
        }
        MonitorPurpose::DispatchGuard => {
            if !matches!(config.scope, MonitorScope::BoundAccount { .. }) {
                return Err(binding_required());
            }
            validate_goal_id(config.goal_id.as_deref().ok_or_else(policy_required)?)?;
            if config.policy_revision.is_none_or(|revision| revision == 0) {
                return Err(policy_required());
            }
        }
    }
    if let Some(model) = config.expected_model.as_deref()
        && !valid_bounded_text(model, MAX_MODEL_LENGTH)
    {
        return Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "expected model is empty or outside its accepted bounds",
            None,
        ));
    }
    Ok(())
}

fn validate_goal_id(value: &str) -> Result<(), MonitorIssue> {
    if value.trim().is_empty()
        || value.len() > MAX_GOAL_ID_LENGTH
        || value.chars().any(char::is_control)
    {
        return Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "goal identifier is empty or outside its accepted bounds",
            None,
        ));
    }
    Ok(())
}

fn current_binding<'a>(
    state: &'a StoreState,
    binding_id: &str,
) -> Option<&'a MonitorAccountBinding> {
    state
        .bindings
        .get(binding_id)
        .and_then(|history| history.last())
}

fn scope_session_id(scope: &MonitorScope) -> Option<&str> {
    match scope {
        MonitorScope::Session { session_id } => Some(session_id),
        MonitorScope::BoundAccount { session_id, .. } => session_id.as_deref(),
    }
}

fn resolve_scope_binding<'a>(
    state: &'a StoreState,
    scope: &MonitorScope,
    provider: jackin_protocol::usage_monitor::MonitorProvider,
) -> Result<&'a MonitorAccountBinding, MonitorIssue> {
    let MonitorScope::BoundAccount {
        binding_id,
        binding_revision,
        ..
    } = scope
    else {
        return Err(binding_required());
    };
    let binding = current_binding(state, binding_id)
        .filter(|binding| binding.revision == *binding_revision)
        .ok_or_else(binding_mismatch)?;
    if binding.provider != provider {
        return Err(binding_mismatch());
    }
    Ok(binding)
}

fn current_policy<'a>(state: &'a StoreState, goal_id: &str) -> Option<&'a MonitorPolicyRecord> {
    state
        .policy_records
        .get(goal_id)
        .and_then(|history| history.last())
}

fn validate_policy_input(
    policy: MonitorPolicy,
    budget: Option<&Money>,
    acknowledge_no_sgd_cap: bool,
) -> Result<(), MonitorIssue> {
    match policy {
        MonitorPolicy::StrictSgd => {
            let Some(budget) = budget else {
                return Err(issue(
                    MonitorIssueCode::StatuslineInvalid,
                    "StrictSgd policy requires an SGD budget",
                    None,
                ));
            };
            if budget.currency != "SGD" || budget.exponent != 2 || budget.amount_minor <= 0 {
                return Err(issue(
                    MonitorIssueCode::StatuslineInvalid,
                    "StrictSgd budget must be a positive SGD amount with exponent 2",
                    None,
                ));
            }
            if acknowledge_no_sgd_cap {
                return Err(issue(
                    MonitorIssueCode::StatuslineInvalid,
                    "StrictSgd approval cannot acknowledge disabling its spend cap",
                    None,
                ));
            }
        }
        MonitorPolicy::QuotaOnly => {
            if budget.is_some() {
                return Err(issue(
                    MonitorIssueCode::StatuslineInvalid,
                    "QuotaOnly policy cannot include an SGD budget",
                    None,
                ));
            }
            if !acknowledge_no_sgd_cap {
                return Err(issue(
                    MonitorIssueCode::SgdCapAcknowledgementRequired,
                    "QuotaOnly policy requires explicit acknowledgement of no SGD cap",
                    None,
                ));
            }
        }
    }
    Ok(())
}

fn invalid_operator_label() -> MonitorIssue {
    issue(
        MonitorIssueCode::StatuslineInvalid,
        "operator label is empty or outside its accepted bounds",
        None,
    )
}

fn operator_confirmation_required() -> MonitorIssue {
    issue(
        MonitorIssueCode::OperatorConfirmationRequired,
        "the operation requires explicit operator confirmation",
        None,
    )
}

fn binding_required() -> MonitorIssue {
    issue(
        MonitorIssueCode::BindingRequired,
        "dispatch guards require a separately confirmed account binding",
        None,
    )
}

fn binding_mismatch() -> MonitorIssue {
    issue(
        MonitorIssueCode::BindingMismatch,
        "binding identifier, provider, account, or revision does not match",
        None,
    )
}

fn policy_required() -> MonitorIssue {
    issue(
        MonitorIssueCode::PolicyRequired,
        "dispatch guards require an explicit current policy approval",
        None,
    )
}

fn policy_conflict() -> MonitorIssue {
    issue(
        MonitorIssueCode::PolicyConflict,
        "policy revision conflicts with the activated goal or approved budget",
        None,
    )
}

struct MonitorReadinessContext<'a> {
    monitor: &'a DurableMonitor,
    five_hour: &'a MonitorQuotaWindowStatus,
    seven_day: &'a MonitorQuotaWindowStatus,
    issues: &'a [MonitorIssue],
    session: Option<&'a SessionObservation>,
    current_binding: Option<&'a MonitorAccountBinding>,
    current_policy: Option<&'a MonitorPolicyRecord>,
    goal: Option<&'a DurableGoalSpend>,
    spend_blocks_dispatch: bool,
}

fn monitor_readiness(context: MonitorReadinessContext<'_>) -> MonitorReadiness {
    let MonitorReadinessContext {
        monitor,
        five_hour,
        seven_day,
        issues,
        session,
        current_binding,
        current_policy,
        goal,
        spend_blocks_dispatch,
    } = context;
    let tracking =
        if session.is_some_and(|session| session.last_callback_received_at_epoch.is_some()) {
            MonitorTrackingReadiness::Ready
        } else {
            MonitorTrackingReadiness::Waiting
        };
    let quota = quota_readiness(five_hour, seven_day, issues);
    let budget = if monitor
        .policy
        .as_ref()
        .is_some_and(|policy| policy.new_policy == MonitorPolicy::QuotaOnly)
    {
        MonitorBudgetReadiness::Disabled
    } else if monitor.config.purpose == MonitorPurpose::ObserveOnly {
        MonitorBudgetReadiness::Unknown
    } else if issues
        .iter()
        .any(|item| item.code == MonitorIssueCode::SpendStale)
    {
        MonitorBudgetReadiness::Stale
    } else if issues.iter().any(|item| {
        matches!(
            item.code,
            MonitorIssueCode::SpendUnavailable
                | MonitorIssueCode::SpendUnverified
                | MonitorIssueCode::SpendRolloverUnverified
                | MonitorIssueCode::BudgetUnverifiable
        )
    }) || goal.is_none_or(|goal| {
        goal.spend_state
            .as_ref()
            .is_none_or(|spend| spend.baseline.is_none())
    }) {
        MonitorBudgetReadiness::Unknown
    } else {
        MonitorBudgetReadiness::Verified
    };
    let binding_ready = current_binding.is_some_and(|binding| {
        binding.operator_confirmed
            && monitor.account_id.as_deref() == Some(binding.account_id.as_str())
            && matches!(
                &monitor.config.scope,
                MonitorScope::BoundAccount {
                    binding_id,
                    binding_revision,
                    ..
                } if binding_id == &binding.binding_id && *binding_revision == binding.revision
            )
    });
    let policy_ready = current_policy.is_some_and(|policy| {
        policy.origin == MonitorPolicyOrigin::Operator
            && policy.operator_confirmed
            && Some(policy.revision) == monitor.config.policy_revision
            && monitor
                .policy
                .as_ref()
                .is_some_and(|stored| stored.revision == policy.revision)
            && matches!(
                &monitor.config.scope,
                MonitorScope::BoundAccount {
                    binding_id,
                    binding_revision,
                    ..
                } if policy.binding_id.as_deref() == Some(binding_id)
                    && policy.binding_revision == Some(*binding_revision)
            )
    });
    let goal_ready = goal.is_some_and(|goal| {
        monitor.account_id.as_deref() == Some(goal.account_id.as_str())
            && current_policy.is_some_and(|policy| goal.policy_revision == policy.revision)
    });
    let no_blocking_issues = !issues.iter().any(|item| {
        matches!(
            item.code,
            MonitorIssueCode::LimitGuardReached
                | MonitorIssueCode::LimitExhausted
                | MonitorIssueCode::ModelMismatch
                | MonitorIssueCode::BindingMismatch
                | MonitorIssueCode::OperatorConfirmationRequired
                | MonitorIssueCode::PolicyRequired
                | MonitorIssueCode::PolicyConflict
                | MonitorIssueCode::QuotaUnknown
                | MonitorIssueCode::QuotaStale
                | MonitorIssueCode::MissingReset
                | MonitorIssueCode::ResetDueUnverified
                | MonitorIssueCode::ModelUnknown
                | MonitorIssueCode::BudgetUnverifiable
                | MonitorIssueCode::SpendUnavailable
                | MonitorIssueCode::SpendUnverified
                | MonitorIssueCode::SpendStale
                | MonitorIssueCode::SpendRolloverUnverified
        )
    });
    let dispatch = if monitor.config.purpose == MonitorPurpose::ObserveOnly {
        MonitorDispatchReadiness::NotAuthorized
    } else if monitor.stopped_at_epoch.is_some() {
        MonitorDispatchReadiness::Blocked
    } else if binding_ready
        && policy_ready
        && goal_ready
        && !spend_blocks_dispatch
        && no_blocking_issues
        && quota == MonitorQuotaReadiness::Ready
        && budget != MonitorBudgetReadiness::Unknown
        && budget != MonitorBudgetReadiness::Stale
    {
        MonitorDispatchReadiness::Ready
    } else {
        MonitorDispatchReadiness::Blocked
    };
    MonitorReadiness {
        tracking,
        quota,
        budget,
        dispatch,
    }
}

fn quota_readiness(
    five_hour: &MonitorQuotaWindowStatus,
    seven_day: &MonitorQuotaWindowStatus,
    issues: &[MonitorIssue],
) -> MonitorQuotaReadiness {
    let exhausted = [five_hour, seven_day].into_iter().any(|window| {
        window.reset_validity != MonitorResetValidity::Due
            && window.used_percentage_basis_points.is_some_and(|used| {
                used >= 9_500
                    && window
                        .used_evidence
                        .as_ref()
                        .is_some_and(|field| field.freshness == MonitorEvidenceFreshness::Current)
            })
    }) || issues.iter().any(|item| {
        matches!(
            item.code,
            MonitorIssueCode::LimitExhausted | MonitorIssueCode::LimitGuardReached
        )
    });
    let quota_fields = [five_hour, seven_day]
        .into_iter()
        .flat_map(|window| {
            [
                window.used_evidence.as_ref(),
                window.reset_evidence.as_ref(),
            ]
        })
        .collect::<Vec<_>>();
    let has_stale_quota = [five_hour, seven_day]
        .into_iter()
        .any(|window| window.reset_validity == MonitorResetValidity::Due)
        || quota_fields.iter().any(|field| {
            field.is_some_and(|field| field.freshness == MonitorEvidenceFreshness::Stale)
        })
        || issues
            .iter()
            .any(|item| item.code == MonitorIssueCode::ResetDueUnverified);
    let all_quota_current = [five_hour, seven_day]
        .into_iter()
        .all(|window| window.reset_validity == MonitorResetValidity::Future)
        && quota_fields.iter().all(|field| {
            field.is_some_and(|field| field.freshness == MonitorEvidenceFreshness::Current)
        });
    let all_quota_present = quota_fields.iter().all(Option::is_some);
    if exhausted {
        MonitorQuotaReadiness::Exhausted
    } else if has_stale_quota {
        MonitorQuotaReadiness::Stale
    } else if all_quota_present && all_quota_current {
        MonitorQuotaReadiness::Ready
    } else {
        MonitorQuotaReadiness::Unknown
    }
}

fn monitor_issues(
    monitor: &DurableMonitor,
    account: Option<&AccountObservations>,
    session: Option<&SessionObservation>,
    current_binding: Option<&MonitorAccountBinding>,
    current_policy: Option<&MonitorPolicyRecord>,
    goal: Option<&DurableGoalSpend>,
    now_epoch: i64,
) -> Vec<MonitorIssue> {
    let mut issues = Vec::new();
    for (index, window) in [MonitorQuotaWindow::FiveHour, MonitorQuotaWindow::SevenDay]
        .into_iter()
        .enumerate()
    {
        for issue in quota_window_issues(monitor, account, window, index, now_epoch) {
            push_issue(&mut issues, issue);
        }
    }
    if monitor.config.expected_model.is_some() {
        let (_, evidence, unknown, mismatch) = model_status(monitor, account, session, now_epoch);
        if unknown {
            push_issue(
                &mut issues,
                issue(
                    MonitorIssueCode::ModelUnknown,
                    "fresh model evidence is unavailable for the configured model guard",
                    None,
                ),
            );
        }
        if mismatch {
            push_issue(
                &mut issues,
                issue(
                    MonitorIssueCode::ModelMismatch,
                    "fresh statusline model does not match the configured model guard",
                    evidence.map(|field| field.evidence_received_at_epoch),
                ),
            );
        }
    }
    if monitor.config.purpose == MonitorPurpose::DispatchGuard {
        if !current_binding.is_some_and(|binding| {
            binding.operator_confirmed
                && monitor.account_id.as_deref() == Some(binding.account_id.as_str())
                && matches!(
                    &monitor.config.scope,
                    MonitorScope::BoundAccount {
                        binding_id,
                        binding_revision,
                        ..
                    } if binding_id == &binding.binding_id && *binding_revision == binding.revision
                )
        }) {
            push_issue(
                &mut issues,
                issue(
                    if current_binding.is_some_and(|binding| !binding.operator_confirmed) {
                        MonitorIssueCode::OperatorConfirmationRequired
                    } else {
                        MonitorIssueCode::BindingMismatch
                    },
                    "a current operator-confirmed account binding is required",
                    None,
                ),
            );
        }
        match current_policy {
            None => push_issue(
                &mut issues,
                issue(
                    MonitorIssueCode::PolicyRequired,
                    "no approved policy exists for this goal",
                    None,
                ),
            ),
            Some(policy) if Some(policy.revision) != monitor.config.policy_revision => push_issue(
                &mut issues,
                issue(
                    MonitorIssueCode::PolicyConflict,
                    "the monitor is not using the current policy revision",
                    None,
                ),
            ),
            Some(policy) if policy.origin != MonitorPolicyOrigin::Operator => push_issue(
                &mut issues,
                issue(
                    MonitorIssueCode::PolicyRequired,
                    "a migrated policy must be explicitly approved before dispatch",
                    None,
                ),
            ),
            Some(policy)
                if policy.origin == MonitorPolicyOrigin::Operator
                    && (!policy.operator_confirmed
                        || matches!(
                            &monitor.config.scope,
                            MonitorScope::BoundAccount {
                                binding_id,
                                binding_revision,
                                ..
                            } if policy.binding_id.as_deref() != Some(binding_id)
                                || policy.binding_revision != Some(*binding_revision)
                        )) =>
            {
                push_issue(
                    &mut issues,
                    issue(
                        MonitorIssueCode::OperatorConfirmationRequired,
                        "the selected policy is not confirmed for this binding revision",
                        None,
                    ),
                );
            }
            _ => {}
        }
        if goal.is_none_or(|goal| {
            monitor.account_id.as_deref() != Some(goal.account_id.as_str())
                || current_policy.is_none_or(|policy| goal.policy_revision != policy.revision)
        }) {
            push_issue(
                &mut issues,
                issue(
                    MonitorIssueCode::PolicyRequired,
                    "the approved goal has not been activated",
                    None,
                ),
            );
        }
        if monitor
            .policy
            .as_ref()
            .is_some_and(|policy| policy.new_policy == MonitorPolicy::StrictSgd)
        {
            for code in spend_policy(monitor, account, now_epoch).issues {
                push_issue(&mut issues, spend_issue(code));
            }
        }
    }
    issues
}

fn quota_window_issues(
    monitor: &DurableMonitor,
    account: Option<&AccountObservations>,
    window: MonitorQuotaWindow,
    index: usize,
    now_epoch: i64,
) -> Vec<MonitorIssue> {
    let mut issues = Vec::new();
    let status = quota_window_status(monitor, account, window, index, now_epoch);
    for (value, code, missing_code, message) in [
        (
            status.used_evidence.as_ref(),
            MonitorIssueCode::QuotaStale,
            MonitorIssueCode::QuotaUnknown,
            "fresh quota utilization evidence is unavailable",
        ),
        (
            status.reset_evidence.as_ref(),
            MonitorIssueCode::QuotaStale,
            MonitorIssueCode::MissingReset,
            "fresh quota reset evidence is unavailable",
        ),
    ] {
        if !value.is_some_and(|field| field.freshness == MonitorEvidenceFreshness::Current) {
            push_issue(
                &mut issues,
                issue(
                    if value.is_some() { code } else { missing_code },
                    message,
                    None,
                ),
            );
        }
    }
    if let Some(reset_at_epoch) = status.reset_at_epoch {
        let due_at = reset_at_epoch.saturating_add(MONITOR_RESET_GRACE_SECS);
        if now_epoch >= due_at {
            push_issue(
                &mut issues,
                issue(
                    MonitorIssueCode::ResetDueUnverified,
                    "the reported reset elapsed without verified post-reset quota evidence",
                    Some(due_at),
                ),
            );
        }
    }
    if let Some(barrier) = monitor.reset_barriers[index].as_ref() {
        let due = barrier
            .prior_reset_at_epoch
            .map(|reset| reset.saturating_add(MONITOR_RESET_GRACE_SECS))
            .is_some_and(|due| now_epoch >= due);
        let active_threshold_reason = status
            .used_percentage_basis_points
            .filter(|_| {
                status
                    .used_evidence
                    .as_ref()
                    .is_some_and(|field| field.freshness == MonitorEvidenceFreshness::Current)
            })
            .filter(|used| *used >= 9_500)
            .map(quota_pause_reason);
        let code = if due {
            MonitorIssueCode::ResetDueUnverified
        } else {
            active_threshold_reason.unwrap_or(barrier.pause_reason)
        };
        push_issue(
            &mut issues,
            issue(
                code,
                if due {
                    "the old reset elapsed; waiting for fresh lower usage and an advanced reset"
                } else {
                    "quota threshold pause is sticky until a verified post-reset observation"
                },
                status.reset_at_epoch,
            ),
        );
    }
    if let Some(barrier) = account.and_then(|account| account.reset_barriers[index].as_ref()) {
        let due_at = barrier
            .prior_reset_at_epoch
            .unwrap_or(barrier.started_at_epoch)
            .saturating_add(MONITOR_RESET_GRACE_SECS)
            .max(barrier.started_at_epoch);
        let due = now_epoch >= due_at;
        let code = if due {
            MonitorIssueCode::ResetDueUnverified
        } else {
            barrier.pause_reason
        };
        push_issue(
            &mut issues,
            issue(
                code,
                if due {
                    "the account quota pause remains until a fresh paired reset is verified"
                } else {
                    "the account quota pause is sticky until a verified post-reset observation"
                },
                Some(due_at),
            ),
        );
    }
    if let Some(used) = status
        .used_percentage_basis_points
        .filter(|_| {
            status
                .used_evidence
                .as_ref()
                .is_some_and(|field| field.freshness == MonitorEvidenceFreshness::Current)
        })
        .filter(|used| *used >= 9_500)
    {
        let code = quota_pause_reason(used);
        push_issue(
            &mut issues,
            issue(
                code,
                if code == MonitorIssueCode::LimitExhausted {
                    "quota utilization reached 100 percent"
                } else {
                    "quota utilization reached the 95 percent pause threshold"
                },
                status.reset_at_epoch,
            ),
        );
    }
    issues
}

fn decision_fingerprint(
    actions: &[MonitorAction],
    issues: &[MonitorIssue],
    lifecycle: MonitorLifecycle,
    runnable: bool,
) -> String {
    let issue_codes = issues.iter().map(|item| item.code).collect::<Vec<_>>();
    serde_json::to_string(&(actions, issue_codes, lifecycle, runnable)).unwrap_or_default()
}

fn push_action(actions: &mut Vec<MonitorAction>, action: MonitorAction) {
    if !actions.contains(&action) {
        actions.push(action);
    }
}

fn push_issue(issues: &mut Vec<MonitorIssue>, issue: MonitorIssue) {
    if !issues.iter().any(|current| current.code == issue.code) {
        issues.push(issue);
    }
}

fn active_monitor_count(state: &StoreState) -> u32 {
    u32::try_from(
        state
            .monitors
            .values()
            .filter(|monitor| monitor.stopped_at_epoch.is_none())
            .count(),
    )
    .unwrap_or(u32::MAX)
}

fn next_wake_for(state: &StoreState) -> Option<i64> {
    let now_epoch = state.last_now_epoch;
    state
        .monitors
        .values()
        .filter(|monitor| monitor.stopped_at_epoch.is_none())
        .flat_map(|monitor| {
            let last_reconciled = monitor.last_reconciled_at_epoch;
            let evidence_expiries = monitor
                .evidence
                .iter()
                .flat_map(|evidence| {
                    [
                        Some(evidence.evidence_received_at_epoch),
                        evidence.evidence_at_epoch,
                    ]
                    .into_iter()
                    .flatten()
                    .map(|observed_at| {
                        observed_at
                            .saturating_add(MONITOR_EVIDENCE_TTL_SECS)
                            .saturating_add(1)
                    })
                })
                .filter(move |expiry| *expiry > last_reconciled);
            let account = monitor
                .account_id
                .as_deref()
                .and_then(|account_id| state.accounts.get(account_id));
            let reset_wakes = [
                (MonitorQuotaWindow::FiveHour, 0),
                (MonitorQuotaWindow::SevenDay, 1),
            ]
            .into_iter()
            .filter_map(move |(window, index)| {
                let status = quota_window_status(monitor, account, window, index, now_epoch);
                let reset = status.reset_at_epoch?;
                let due = reset.saturating_add(MONITOR_RESET_GRACE_SECS);
                (reset > last_reconciled)
                    .then_some(reset)
                    .into_iter()
                    .chain((due > last_reconciled).then_some(due))
                    .min()
            });
            let account_barrier_wakes = account
                .into_iter()
                .flat_map(|account| account.reset_barriers.iter().flatten())
                .flat_map(|barrier| {
                    let reset = barrier
                        .prior_reset_at_epoch
                        .unwrap_or(barrier.started_at_epoch);
                    [
                        Some(reset),
                        Some(
                            reset
                                .saturating_add(MONITOR_RESET_GRACE_SECS)
                                .max(barrier.started_at_epoch),
                        ),
                    ]
                    .into_iter()
                    .flatten()
                })
                .filter(move |wake| *wake > last_reconciled);
            evidence_expiries
                .chain(reset_wakes)
                .chain(account_barrier_wakes)
        })
        .min()
}

fn watch_snapshot(
    state: &StoreState,
    monitor_id: &str,
    after_sequence: u64,
) -> Result<(Vec<MonitorEvent>, u64), MonitorIssue> {
    let monitor = state
        .monitors
        .get(monitor_id)
        .ok_or_else(monitor_not_found)?;
    let next_sequence = monitor.next_event_sequence;
    let events = if after_sequence == 0 {
        // Sequence zero is a fresh attach, not a request to replay history.
        // The latest event was reconciled at the caller's clock above.
        monitor.events.last().cloned().into_iter().collect()
    } else {
        monitor
            .events
            .iter()
            .filter(|event| event.sequence > after_sequence)
            .cloned()
            .collect()
    };
    Ok((events, next_sequence))
}

fn observe_projection_account(
    state: &mut StoreState,
    account: &UsageAccountV1,
    now_epoch: i64,
) -> Result<bool, MonitorIssue> {
    if !valid_identifier(&account.canonical_account_id)
        || account.freshness.is_stale
        || account.freshness.phase != UsageFreshnessPhaseV1::Current
    {
        return Ok(false);
    }

    // A whole-projection publication can advance because another account
    // refreshed. Only current per-account evidence can update quota field age.
    let proposed_sequence = state.next_input_sequence.saturating_add(1);
    let windows = projection_windows(account, now_epoch, proposed_sequence);
    if windows.iter().all(Option::is_none) {
        return Ok(false);
    }
    if !state.accounts.contains_key(&account.canonical_account_id)
        && state.accounts.len() >= MAX_ACCOUNTS
    {
        return Err(issue(
            MonitorIssueCode::MonitorStoreUnavailable,
            "monitor store reached its configured account limit",
            None,
        ));
    }

    let account_state = state
        .accounts
        .entry(account.canonical_account_id.clone())
        .or_default();
    if !update_broker_windows(account_state, windows, proposed_sequence) {
        return Ok(false);
    }
    account_state.input_sequence = proposed_sequence;
    state.next_input_sequence = proposed_sequence;
    latch_broker_reset_barriers(account_state, proposed_sequence, now_epoch);
    Ok(true)
}

fn latch_broker_reset_barriers(
    account: &mut AccountObservations,
    input_sequence: u64,
    now_epoch: i64,
) {
    for index in 0..account.broker_windows.len() {
        let barrier = account.broker_windows[index]
            .used
            .as_ref()
            .filter(|used| used.input_sequence == input_sequence && used.value >= 9_500)
            .map(|used| AccountResetBarrier {
                started_at_epoch: now_epoch,
                prior_reset_at_epoch: used.reset_at_epoch,
                prior_used_percentage_basis_points: used.value,
                input_sequence_before: input_sequence,
                source: MonitorEvidenceSource::BrokerProjection,
                session_id: None,
                pause_reason: if used.value >= 10_000 {
                    MonitorIssueCode::LimitExhausted
                } else {
                    MonitorIssueCode::LimitGuardReached
                },
            });
        if let Some(barrier) = barrier {
            latch_account_reset_barrier(account, index, barrier);
        }
    }
}

fn projection_windows(
    account: &UsageAccountV1,
    received_at_epoch: i64,
    input_sequence: u64,
) -> [Option<ObservedWindow>; 2] {
    let mut windows = [None, None];
    let fallback_evidence_at = account
        .freshness
        .last_good_at_epoch
        .filter(|time| *time <= received_at_epoch);
    for (index, category) in [
        UsageWindowCategoryV1::Session,
        UsageWindowCategoryV1::LongRange,
    ]
    .into_iter()
    .enumerate()
    {
        let max_reset_ahead_secs = if category == UsageWindowCategoryV1::Session {
            5 * 60 * 60 + MONITOR_EVIDENCE_TTL_SECS
        } else {
            7 * 24 * 60 * 60 + MONITOR_EVIDENCE_TTL_SECS
        };
        let candidates = account
            .windows
            .iter()
            .filter(|window| window.category == category)
            .filter_map(|window| {
                if window.reset_at_epoch.is_some_and(|reset| {
                    reset < 0 || reset > received_at_epoch.saturating_add(max_reset_ahead_secs)
                }) {
                    return None;
                }
                let raw_used = window.used_raw_percent.or_else(|| {
                    window
                        .remaining_raw_percent
                        .map(|remaining| 100i32.saturating_sub(remaining))
                });
                let raw_used = raw_used.filter(|used| *used >= 0);
                let used = raw_used.map(|value| value.min(100));
                let used_at = raw_used.and_then(|value| {
                    metric_group_time(
                        account,
                        index,
                        Some(value),
                        window.reset_at_epoch,
                        received_at_epoch,
                    )
                    .unwrap_or(fallback_evidence_at)
                });
                let reset_at = window.reset_at_epoch.and_then(|reset| {
                    metric_group_time(account, index, raw_used, Some(reset), received_at_epoch)
                        .unwrap_or(fallback_evidence_at)
                });
                (used.is_some() || window.reset_at_epoch.is_some())
                    .then_some((window, used, used_at, reset_at))
            })
            .collect::<Vec<_>>();
        let used = candidates
            .iter()
            .filter_map(|(window, used, evidence_at, _)| {
                used.map(|used| (*window, used, *evidence_at))
            })
            .max_by_key(|(_, used, evidence_at)| (*used, *evidence_at));
        let reset = candidates
            .iter()
            .filter_map(|(window, _, _, evidence_at)| {
                window.reset_at_epoch.map(|reset| (reset, *evidence_at))
            })
            .min_by_key(|(reset, _)| *reset);
        let (used_value, used_reset, used_evidence_at) = used
            .map_or((None, None, None), |(window, used, evidence_at)| {
                (Some(used), window.reset_at_epoch, evidence_at)
            });
        let (reset_value, reset_evidence_at) = reset
            .map_or((None, None), |(reset, evidence_at)| {
                (Some(reset), evidence_at)
            });
        if used_value.is_none() && reset_value.is_none() {
            continue;
        }
        let paired = match (
            used_value,
            used_reset,
            used_evidence_at,
            reset_value,
            reset_evidence_at,
        ) {
            (Some(used), Some(used_reset), Some(used_at), Some(reset), Some(reset_at))
                if used_reset == reset =>
            {
                Some(ObservedQuotaPair {
                    used_percentage_basis_points: used.saturating_mul(100),
                    reset_at_epoch: reset,
                    evidence_at_epoch: Some(used_at.min(reset_at)),
                    received_at_epoch,
                    input_sequence,
                    claude_code_version: None,
                })
            }
            _ => None,
        };
        windows[index] = Some(ObservedWindow {
            used: used_value
                .zip(used_evidence_at)
                .map(|(value, evidence_at_epoch)| ObservedPercentage {
                    value: value.saturating_mul(100),
                    reset_at_epoch: used_reset,
                    evidence_at_epoch: Some(evidence_at_epoch),
                    received_at_epoch,
                    input_sequence,
                    claude_code_version: None,
                }),
            reset: reset_value
                .zip(reset_evidence_at)
                .map(|(value, evidence_at_epoch)| Observed {
                    value,
                    evidence_at_epoch: Some(evidence_at_epoch),
                    received_at_epoch,
                    input_sequence,
                    claude_code_version: None,
                }),
            paired,
        });
    }
    windows
}

/// `Some(None)` means a typed group identifies this field but is stale.
fn metric_group_time(
    account: &UsageAccountV1,
    window_index: usize,
    used_raw_percent: Option<i32>,
    reset_at_epoch: Option<i64>,
    now_epoch: i64,
) -> Option<Option<i64>> {
    let period_matches = |period: UsageMetricPeriodV1| match period {
        UsageMetricPeriodV1::Rolling { window_secs } => {
            (window_index == 0 && window_secs <= 86_400)
                || (window_index == 1 && window_secs > 86_400)
        }
        UsageMetricPeriodV1::Calendar { granularity } => {
            (window_index == 0
                && granularity == jackin_protocol::usage_broker::UsageCalendarPeriodV1::Daily)
                || (window_index == 1
                    && matches!(
                        granularity,
                        jackin_protocol::usage_broker::UsageCalendarPeriodV1::Weekly
                            | jackin_protocol::usage_broker::UsageCalendarPeriodV1::Monthly
                    ))
        }
        // Claude's session window has no published duration. Its typed group
        // still identifies independent per-window freshness and is authoritative.
        UsageMetricPeriodV1::ProviderDefined => window_index == 0,
        UsageMetricPeriodV1::Unknown => false,
    };
    let window_groups = account
        .metric_groups
        .iter()
        .filter(|group| group.kind == UsageMetricGroupKindV1::Window)
        .filter(|group| match &group.value {
            UsageMetricValueV1::Window { period, .. } => period_matches(*period),
            _ => false,
        })
        .collect::<Vec<_>>();
    if window_groups.is_empty() {
        return None;
    }
    let groups = window_groups
        .into_iter()
        .filter(|group| {
            let UsageMetricValueV1::Window {
                used_raw_percent: group_used,
                remaining_raw_percent: group_remaining,
                ..
            } = &group.value
            else {
                return false;
            };
            let group_used = (*group_used)
                .or_else(|| group_remaining.map(|remaining| 100i32.saturating_sub(remaining)));
            (used_raw_percent.is_none() || group_used == used_raw_percent)
                && (reset_at_epoch.is_none() || group.reset_at_epoch == reset_at_epoch)
        })
        .collect::<Vec<_>>();
    if groups.is_empty() {
        // Typed provider evidence for this quota category is authoritative.
        // A different metric group cannot lend it a fresh timestamp.
        return Some(None);
    }
    let current = groups
        .iter()
        .filter(|group| !group.is_stale && group.phase == UsageFreshnessPhaseV1::Current)
        .filter(|group| {
            group
                .observed_at_epoch
                .or(group.last_success_at_epoch)
                .unwrap_or(group.fetched_at_epoch)
                <= now_epoch.saturating_add(MAX_FUTURE_SKEW_SECS)
        })
        .max_by_key(|group| {
            group
                .observed_at_epoch
                .or(group.last_success_at_epoch)
                .unwrap_or(group.fetched_at_epoch)
        });
    current.map_or(Some(None), |group| {
        Some(Some(
            group
                .observed_at_epoch
                .or(group.last_success_at_epoch)
                .unwrap_or(group.fetched_at_epoch),
        ))
    })
}

fn update_broker_windows(
    account: &mut AccountObservations,
    input: [Option<ObservedWindow>; 2],
    input_sequence: u64,
) -> bool {
    let mut changed = false;
    for (index, maybe_window) in input.into_iter().enumerate() {
        let Some(window) = maybe_window else {
            continue;
        };
        if window.reset.as_ref().is_some_and(|reset| {
            account.latest_reset_epochs[index].is_some_and(|latest| reset.value < latest)
        }) {
            continue;
        }
        if let Some(reset) = window.reset.as_ref().filter(|reset| {
            account.latest_reset_epochs[index].is_none_or(|latest| reset.value > latest)
        }) {
            account.latest_reset_epochs[index] = Some(reset.value);
            changed = true;
        }
        let current = &mut account.broker_windows[index];
        changed |= update_broker_window(current, &window, input_sequence);
    }
    changed
}

fn update_broker_window(
    current: &mut ObservedWindow,
    next: &ObservedWindow,
    input_sequence: u64,
) -> bool {
    let mut changed = false;
    if let Some(next_reset) = &next.reset {
        let mut next_reset = next_reset.clone();
        next_reset.input_sequence = input_sequence;
        match current.reset.as_ref() {
            None => {
                current.reset = Some(next_reset);
                changed = true;
            }
            Some(previous) if next_reset.value > previous.value => {
                current.reset = Some(next_reset);
                changed = true;
            }
            Some(previous)
                if next_reset.value == previous.value
                    && source_observation_is_newer(&next_reset, previous) =>
            {
                current.reset = Some(next_reset);
                changed = true;
            }
            _ => {}
        }
    }
    if let Some(next_used) = &next.used {
        let mut next_used = next_used.clone();
        next_used.input_sequence = input_sequence;
        let next_reset = next_used.reset_at_epoch;
        let current_used = current.used.as_ref();
        let reset_is_current = current
            .reset
            .as_ref()
            .is_none_or(|reset| next_reset.is_some_and(|candidate| candidate >= reset.value));
        if reset_is_current {
            match current_used {
                None => {
                    current.used = Some(next_used);
                    changed = true;
                }
                Some(previous) if next_reset > previous.reset_at_epoch => {
                    current.used = Some(next_used);
                    changed = true;
                }
                Some(previous)
                    if next_reset == previous.reset_at_epoch
                        && next_used.value > previous.value =>
                {
                    current.used = Some(next_used);
                    changed = true;
                }
                Some(previous)
                    if next_reset == previous.reset_at_epoch
                        && next_used.value == previous.value
                        && next_used.evidence_at_epoch > previous.evidence_at_epoch =>
                {
                    current.used = Some(next_used);
                    changed = true;
                }
                _ => {}
            }
        }
    }
    if let Some(next_pair) = &next.paired {
        let pair_is_newer = current.paired.as_ref().is_none_or(|previous| {
            next_pair.used_percentage_basis_points != previous.used_percentage_basis_points
                || next_pair.reset_at_epoch != previous.reset_at_epoch
                || next_pair.evidence_at_epoch > previous.evidence_at_epoch
        });
        if pair_is_newer {
            let mut next_pair = next_pair.clone();
            next_pair.input_sequence = input_sequence;
            current.paired = Some(next_pair);
            changed = true;
        }
    }
    changed
}

fn source_observation_is_newer<T>(next: &Observed<T>, current: &Observed<T>) -> bool {
    next.evidence_at_epoch > current.evidence_at_epoch
}

fn spend_input_matches_record(input: &SpendRecordInput, record: &SpendRecord) -> bool {
    input.account_id == record.account_id
        && input.billing_period_start_epoch == record.billing_period_start_epoch
        && input.billing_period_end_epoch == record.billing_period_end_epoch
        && input.amount == record.amount
        && input.evidence_at_epoch == record.evidence_at_epoch
        && input.source == record.source
        && input.verified
            == matches!(
                record.verification,
                SpendVerification::Verified | SpendVerification::Stale
            )
}

fn issue(code: MonitorIssueCode, message: &str, retry_at_epoch: Option<i64>) -> MonitorIssue {
    MonitorIssue {
        code,
        message: message.to_owned(),
        retry_at_epoch,
    }
}

fn effective_now(previous: i64, supplied: i64) -> i64 {
    previous.max(supplied)
}

fn monitor_not_found() -> MonitorIssue {
    issue(
        MonitorIssueCode::MonitorNotFound,
        "monitor does not exist",
        None,
    )
}

fn store_unavailable() -> MonitorIssue {
    issue(
        MonitorIssueCode::MonitorStoreUnavailable,
        "monitor state is unavailable or invalid",
        None,
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "monitor/session_capacity_tests.rs"]
mod session_capacity_tests;

#[cfg(test)]
#[path = "monitor/watch_restart_tests.rs"]
mod watch_restart_tests;

#[cfg(test)]
#[path = "monitor/tests/policy_regression_tests.rs"]
mod policy_regression_tests;

#[cfg(test)]
#[path = "monitor/v2_tests.rs"]
mod v2_tests;
