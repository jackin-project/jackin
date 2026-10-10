// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Durable, secret-free monitor state and local decision engine.

mod legacy;
mod operations;
mod spend;
mod statusline;
mod storage;
mod validation;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::sync::{Condvar, Mutex};

use jackin_protocol::control::Money;
use validation::{
    spend_snapshot_preserves_history, spend_state_matches_account, validate_store_state,
};

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

fn refresh_all_goal_spend(state: &mut StoreState, now_epoch: i64) {
    let spend_by_account = state
        .accounts
        .iter()
        .map(|(account_id, account)| (account_id.clone(), account.spend.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut updates = BTreeMap::new();
    for (goal_id, goal) in &mut state.goals {
        if goal.policy == MonitorPolicy::StrictSgd
            && let (Some(account), Some(spend_state)) = (
                spend_by_account.get(&goal.account_id),
                goal.spend_state.as_mut(),
            )
        {
            advance_goal_spend(spend_state, account, goal.budget.as_ref(), now_epoch);
        }
        updates.insert(
            goal_id.clone(),
            (
                goal.account_id.clone(),
                goal.binding_id.clone(),
                goal.binding_revision,
                goal.policy_revision,
                goal.policy,
                goal.budget.clone(),
                goal.spend_state.clone(),
            ),
        );
    }
    for monitor in state.monitors.values_mut() {
        if let Some(goal_id) = monitor.config.goal_id.as_deref()
            && let Some((
                account_id,
                binding_id,
                binding_revision,
                policy_revision,
                _,
                _,
                spend_state,
            )) = updates.get(goal_id)
            && monitor.account_id.as_ref() == Some(account_id)
            && monitor.config.policy_revision == Some(*policy_revision)
            && matches!(
                &monitor.config.scope,
                MonitorScope::BoundAccount {
                    binding_id: configured,
                    binding_revision: configured_revision,
                    ..
                } if configured == binding_id && configured_revision == binding_revision
            )
        {
            monitor.spend_state = spend_state.clone();
        }
    }
}

fn reconcile_all_monitors(state: &mut StoreState, now_epoch: i64) -> bool {
    let mut changed = false;
    for account in state.accounts.values_mut() {
        changed |= advance_account_reset_barriers(account, now_epoch);
    }
    refresh_all_goal_spend(state, now_epoch);
    let accounts = state.accounts.clone();
    let unbound_sessions = state.unbound_sessions.clone();
    let bindings = state.bindings.clone();
    let policy_records = state.policy_records.clone();
    let goals = state.goals.clone();
    let ids = state
        .monitors
        .iter()
        .filter_map(|(id, monitor)| monitor.stopped_at_epoch.is_none().then_some(id.clone()))
        .collect::<Vec<_>>();
    for id in ids {
        if let Some(monitor) = state.monitors.get_mut(&id) {
            let account_id = monitor.account_id.clone();
            let scope = monitor.config.scope.clone();
            if let Some(account_id) = account_id.as_deref() {
                sync_monitor_from_account(&accounts, account_id, monitor, now_epoch);
            } else if let MonitorScope::Session { session_id } = scope
                && let Some(session) = unbound_sessions.get(&session_id)
            {
                sync_monitor_from_session(None, &session_id, session, monitor);
            }
            let current_binding = match &monitor.config.scope {
                MonitorScope::BoundAccount { binding_id, .. } => bindings
                    .get(binding_id)
                    .and_then(|history| history.last())
                    .cloned(),
                MonitorScope::Session { .. } => None,
            };
            let current_policy = monitor
                .config
                .goal_id
                .as_ref()
                .and_then(|goal_id| policy_records.get(goal_id))
                .and_then(|history| history.last())
                .cloned();
            let goal = monitor
                .config
                .goal_id
                .as_ref()
                .and_then(|goal_id| goals.get(goal_id))
                .cloned();
            changed |= evaluate_monitor(
                MonitorEvaluationContext {
                    accounts: &accounts,
                    unbound_sessions: &unbound_sessions,
                    current_binding: current_binding.as_ref(),
                    current_policy: current_policy.as_ref(),
                    goal: goal.as_ref(),
                },
                monitor,
                now_epoch,
            );
        }
        changed |= append_event_if_changed(state, &id, now_epoch);
    }
    changed
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

fn validate_observation(
    observation: &StatuslineObservation,
    now_epoch: i64,
) -> Result<(), MonitorIssue> {
    if observation.schema_version != USAGE_STATUSLINE_INPUT_SCHEMA_VERSION {
        return Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "statusline schema version is unsupported",
            None,
        ));
    }
    validate_identifier(&observation.session_id)?;
    if let Some(model) = observation.model.as_deref()
        && !valid_bounded_text(model, MAX_MODEL_LENGTH)
    {
        return Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "statusline model is outside its accepted bounds",
            None,
        ));
    }
    if let Some(version) = observation.claude_code_version.as_deref()
        && !valid_bounded_text(version, 64)
    {
        return Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "Claude Code version is outside its accepted bounds",
            None,
        ));
    }
    for (index, window) in [
        observation.rate_limits.five_hour.as_ref(),
        observation.rate_limits.seven_day.as_ref(),
    ]
    .into_iter()
    .enumerate()
    {
        let Some(window) = window else {
            continue;
        };
        let max_future = if index == 0 {
            5 * 60 * 60 + MONITOR_EVIDENCE_TTL_SECS
        } else {
            7 * 24 * 60 * 60 + MONITOR_EVIDENCE_TTL_SECS
        };
        if window
            .used_percentage_basis_points
            .is_some_and(|value| !(0..=10_000).contains(&value))
            || window
                .reset_at_epoch
                .is_some_and(|value| value < 0 || value > now_epoch.saturating_add(max_future))
        {
            return Err(issue(
                MonitorIssueCode::StatuslineInvalid,
                "statusline quota value is outside its accepted bounds",
                None,
            ));
        }
    }
    Ok(())
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_LENGTH
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn valid_source_capability_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_evidence_fingerprint(key: &str, value: &str) -> bool {
    if !valid_bounded_text(key, MAX_ID_LENGTH + 32) || !valid_bounded_text(value, 2_048) {
        return false;
    }
    if key == "spend:account" {
        return true;
    }
    if let Some(session_id) = key.strip_prefix("model:") {
        return valid_identifier(session_id);
    }
    let Some((window, scope)) = key
        .strip_prefix("used:")
        .or_else(|| key.strip_prefix("reset:"))
        .and_then(|rest| rest.split_once(':'))
    else {
        return false;
    };
    matches!(window, "five_hour" | "seven_day") && (scope == "account" || valid_identifier(scope))
}

fn parse_counter_id(value: &str, prefix: &str) -> Option<u64> {
    let suffix = value.strip_prefix(prefix)?;
    if !(8..=20).contains(&suffix.len()) || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let parsed = suffix.parse::<u64>().ok()?;
    (parsed > 0).then_some(parsed)
}

fn validate_identifier(value: &str) -> Result<(), MonitorIssue> {
    if valid_identifier(value) {
        Ok(())
    } else {
        Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "identifier is empty or outside its accepted bounds",
            None,
        ))
    }
}

fn valid_bounded_text(value: &str, max_len: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max_len
        && value.is_ascii()
        && !value.chars().any(char::is_control)
}

fn apply_statusline(
    account: &mut AccountObservations,
    observation: &StatuslineObservation,
    now_epoch: i64,
    input_sequence: u64,
) -> (bool, bool) {
    let mut latest_resets = account.latest_reset_epochs;
    let new_session = !account.sessions.contains_key(&observation.session_id);
    let (session_changed, fields_changed, resets) = apply_statusline_session(
        account
            .sessions
            .entry(observation.session_id.clone())
            .or_default(),
        observation,
        now_epoch,
        input_sequence,
        latest_resets,
        new_session,
    );
    let changed = session_changed || new_session;
    let [five_hour_reset, seven_day_reset] = resets;
    latest_resets[0] = max_option(latest_resets[0], five_hour_reset);
    latest_resets[1] = max_option(latest_resets[1], seven_day_reset);
    account.latest_reset_epochs = latest_resets;
    if changed {
        let new_barriers = account
            .sessions
            .get(&observation.session_id)
            .into_iter()
            .flat_map(|session| session.windows.iter().enumerate())
            .filter_map(|(index, window)| {
                let used = window
                    .used
                    .as_ref()
                    .filter(|used| used.input_sequence == input_sequence && used.value >= 9_500)?;
                Some((
                    index,
                    AccountResetBarrier {
                        started_at_epoch: now_epoch,
                        prior_reset_at_epoch: used.reset_at_epoch,
                        prior_used_percentage_basis_points: used.value,
                        input_sequence_before: input_sequence,
                        source: MonitorEvidenceSource::Statusline,
                        session_id: Some(observation.session_id.clone()),
                        pause_reason: if used.value >= 10_000 {
                            MonitorIssueCode::LimitExhausted
                        } else {
                            MonitorIssueCode::LimitGuardReached
                        },
                    },
                ))
            })
            .collect::<Vec<_>>();
        for (index, barrier) in new_barriers {
            latch_account_reset_barrier(account, index, barrier);
        }
    }
    (changed, fields_changed)
}

fn apply_statusline_session(
    session: &mut SessionObservation,
    observation: &StatuslineObservation,
    now_epoch: i64,
    input_sequence: u64,
    watermarks: [Option<i64>; 2],
    new_session: bool,
) -> (bool, bool, [Option<i64>; 2]) {
    let mut changed = new_session;
    let mut fields_changed = false;
    if let Some(model) = observation.model.as_ref() {
        let model_changed = update_observed(
            &mut session.model,
            model.clone(),
            None,
            now_epoch,
            input_sequence,
            observation.claude_code_version.as_deref(),
        );
        changed |= model_changed;
        fields_changed |= model_changed;
    }
    let (five_hour_changed, five_hour_reset) = apply_session_window(
        &mut session.windows[0],
        observation.rate_limits.five_hour.as_ref(),
        watermarks[0],
        now_epoch,
        input_sequence,
        observation.claude_code_version.as_deref(),
    );
    let (seven_day_changed, seven_day_reset) = apply_session_window(
        &mut session.windows[1],
        observation.rate_limits.seven_day.as_ref(),
        watermarks[1],
        now_epoch,
        input_sequence,
        observation.claude_code_version.as_deref(),
    );
    changed |= five_hour_changed || seven_day_changed;
    fields_changed |= five_hour_changed || seven_day_changed;
    if fields_changed {
        session.last_observation_received_at_epoch = Some(now_epoch);
    }
    if let Some(version) = observation.claude_code_version.as_ref()
        && session.claude_code_version.as_ref() != Some(version)
    {
        session.claude_code_version = Some(version.clone());
        changed = true;
    }
    if session.last_callback_received_at_epoch != Some(now_epoch) {
        session.last_callback_received_at_epoch = Some(now_epoch);
        changed = true;
    }
    (changed, fields_changed, [five_hour_reset, seven_day_reset])
}

fn apply_unbound_statusline(
    session: &mut SessionObservation,
    observation: &StatuslineObservation,
    now_epoch: i64,
    input_sequence: u64,
    new_session: bool,
) -> (bool, bool) {
    let watermarks = std::array::from_fn(|index| {
        session.windows[index]
            .reset
            .as_ref()
            .map(|reset| reset.value)
    });
    let (changed, fields_changed, _) = apply_statusline_session(
        session,
        observation,
        now_epoch,
        input_sequence,
        watermarks,
        new_session,
    );
    (changed, fields_changed)
}

fn latch_account_reset_barrier(
    account: &mut AccountObservations,
    index: usize,
    incoming: AccountResetBarrier,
) {
    let Some(slot) = account.reset_barriers.get_mut(index) else {
        return;
    };
    let Some(existing) = slot.as_ref() else {
        *slot = Some(incoming);
        return;
    };
    let reset_advanced = incoming.prior_reset_at_epoch.is_some_and(|reset| {
        existing
            .prior_reset_at_epoch
            .is_none_or(|previous| reset > previous)
    });
    if incoming.prior_used_percentage_basis_points > existing.prior_used_percentage_basis_points
        || reset_advanced
    {
        let mut incoming = incoming;
        incoming.prior_reset_at_epoch =
            max_option(existing.prior_reset_at_epoch, incoming.prior_reset_at_epoch);
        *slot = Some(incoming);
    }
}

fn account_reset_barrier_satisfied(
    account: &AccountObservations,
    index: usize,
    barrier: &AccountResetBarrier,
    now_epoch: i64,
) -> bool {
    let required_at = barrier
        .prior_reset_at_epoch
        .unwrap_or(barrier.started_at_epoch)
        .saturating_add(MONITOR_RESET_GRACE_SECS)
        .max(barrier.started_at_epoch);
    if now_epoch < required_at {
        return false;
    }
    let pair_satisfies = |window: &ObservedWindow| {
        let (Some(used), Some(reset), Some(pair)) = (
            window.used.as_ref(),
            window.reset.as_ref(),
            window.paired.as_ref(),
        ) else {
            return false;
        };
        let reset_advanced = barrier
            .prior_reset_at_epoch
            .is_none_or(|previous| pair.reset_at_epoch > previous);
        pair.input_sequence > barrier.input_sequence_before
            && pair.received_at_epoch >= required_at
            && pair.received_at_epoch <= now_epoch
            && pair
                .evidence_at_epoch
                .is_none_or(|evidence_at| evidence_at >= required_at)
            && field_age(used.evidence_at_epoch, used.received_at_epoch, now_epoch)
                <= MONITOR_EVIDENCE_TTL_SECS as u64
            && field_age(reset.evidence_at_epoch, reset.received_at_epoch, now_epoch)
                <= MONITOR_EVIDENCE_TTL_SECS as u64
            && used.value == pair.used_percentage_basis_points
            && reset.value == pair.reset_at_epoch
            && used.reset_at_epoch == Some(reset.value)
            && pair.used_percentage_basis_points < barrier.prior_used_percentage_basis_points
            && reset_advanced
    };

    // The quota guard belongs to the account, so any source may confirm the
    // reset. Each candidate is one stored full-window observation, preserving
    // the source/session pairing and preventing fields from separate callbacks
    // from being combined to release the guard.
    account
        .sessions
        .values()
        .filter_map(|session| session.windows.get(index))
        .chain(account.broker_windows.get(index))
        .any(pair_satisfies)
}

fn advance_account_reset_barriers(account: &mut AccountObservations, now_epoch: i64) -> bool {
    let mut changed = false;
    for index in 0..account.reset_barriers.len() {
        let Some(barrier) = account.reset_barriers[index].as_ref() else {
            continue;
        };
        if account_reset_barrier_satisfied(account, index, barrier, now_epoch) {
            account.reset_barriers[index] = None;
            changed = true;
        }
    }
    changed
}

fn apply_session_window(
    stored: &mut ObservedWindow,
    input: Option<&StatuslineQuotaWindow>,
    watermark: Option<i64>,
    now_epoch: i64,
    input_sequence: u64,
    claude_code_version: Option<&str>,
) -> (bool, Option<i64>) {
    let Some(input) = input else {
        return (false, None);
    };
    if input
        .reset_at_epoch
        .is_some_and(|reset| watermark.is_some_and(|latest| reset < latest))
    {
        // A callback from an older overlapping session cannot replace the
        // current reset or lower its utilization.
        return (false, None);
    }
    let mut changed = false;
    let mut effective_reset = stored.reset.as_ref().map(|reset| reset.value);
    if let Some(candidate_reset) = input.reset_at_epoch {
        if stored
            .reset
            .as_ref()
            .is_none_or(|reset| candidate_reset > reset.value)
        {
            stored.reset = Some(Observed {
                value: candidate_reset,
                evidence_at_epoch: None,
                received_at_epoch: now_epoch,
                input_sequence,
                claude_code_version: claude_code_version.map(str::to_owned),
            });
            effective_reset = Some(candidate_reset);
            changed = true;
        } else if stored
            .reset
            .as_ref()
            .is_some_and(|reset| reset.value == candidate_reset)
        {
            effective_reset = Some(candidate_reset);
        }
    }
    if let Some(candidate_used) = input.used_percentage_basis_points {
        let current = stored.used.as_ref();
        let same_window = current.is_some_and(|used| used.reset_at_epoch == effective_reset);
        let reset_advanced = effective_reset.is_some_and(|reset| {
            current.is_some_and(|used| used.reset_at_epoch.is_none_or(|old| reset > old))
        });
        let watermark_allows_used =
            watermark.is_none_or(|latest| effective_reset.is_some_and(|reset| reset >= latest));
        let should_accept = current.is_none()
            || reset_advanced
            || current.is_some_and(|used| same_window && candidate_used > used.value);
        if watermark_allows_used && should_accept {
            stored.used = Some(ObservedPercentage {
                value: candidate_used,
                reset_at_epoch: effective_reset,
                evidence_at_epoch: None,
                received_at_epoch: now_epoch,
                input_sequence,
                claude_code_version: claude_code_version.map(str::to_owned),
            });
            changed = true;
        }
    }
    if let (Some(candidate_used), Some(candidate_reset)) =
        (input.used_percentage_basis_points, input.reset_at_epoch)
        && stored.used.as_ref().is_some_and(|used| {
            used.value == candidate_used && used.reset_at_epoch == Some(candidate_reset)
        })
        && stored
            .reset
            .as_ref()
            .is_some_and(|reset| reset.value == candidate_reset)
        && stored.paired.as_ref().is_none_or(|pair| {
            pair.used_percentage_basis_points != candidate_used
                || pair.reset_at_epoch != candidate_reset
        })
    {
        stored.paired = Some(ObservedQuotaPair {
            used_percentage_basis_points: candidate_used,
            reset_at_epoch: candidate_reset,
            evidence_at_epoch: None,
            received_at_epoch: now_epoch,
            input_sequence,
            claude_code_version: claude_code_version.map(str::to_owned),
        });
        changed = true;
    }
    (changed, stored.reset.as_ref().map(|reset| reset.value))
}

fn max_option(left: Option<i64>, right: Option<i64>) -> Option<i64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn update_observed<T: Clone + PartialEq>(
    stored: &mut Option<Observed<T>>,
    value: T,
    evidence_at_epoch: Option<i64>,
    received_at_epoch: i64,
    input_sequence: u64,
    claude_code_version: Option<&str>,
) -> bool {
    if stored
        .as_ref()
        .is_some_and(|current| current.value == value)
    {
        return false;
    }
    *stored = Some(Observed {
        value,
        evidence_at_epoch,
        received_at_epoch,
        input_sequence,
        claude_code_version: claude_code_version.map(str::to_owned),
    });
    true
}

fn sync_monitor_from_account(
    accounts: &BTreeMap<String, AccountObservations>,
    account_id: &str,
    monitor: &mut DurableMonitor,
    now_epoch: i64,
) {
    let Some(account) = accounts.get(account_id) else {
        return;
    };
    for (session_id, session) in &account.sessions {
        if scope_session_id(&monitor.config.scope).is_some_and(|expected| expected != session_id) {
            continue;
        }
        if let Some(model) = &session.model {
            upsert_evidence(
                monitor,
                EvidenceInput {
                    account_id: Some(account_id),
                    session_id: Some(session_id),
                    source: MonitorEvidenceSource::Statusline,
                    evidence_at_epoch: model.evidence_at_epoch,
                    received_at_epoch: model.received_at_epoch,
                    claude_code_version: model.claude_code_version.as_deref(),
                    value: MonitorEvidenceValue::Model {
                        model: model.value.clone(),
                    },
                    fingerprint_key: format!("model:{session_id}"),
                },
            );
        }
        for index in 0..2 {
            sync_window(
                monitor,
                Some(account_id),
                Some(session_id),
                MonitorEvidenceSource::Statusline,
                index,
                &session.windows[index],
            );
        }
    }
    for index in 0..2 {
        sync_window(
            monitor,
            Some(account_id),
            None,
            MonitorEvidenceSource::BrokerProjection,
            index,
            &account.broker_windows[index],
        );
    }
    if let Some(record) = &account.spend.latest_record {
        upsert_evidence(
            monitor,
            EvidenceInput {
                account_id: Some(account_id),
                session_id: None,
                source: MonitorEvidenceSource::Operator,
                evidence_at_epoch: record.evidence_at_epoch,
                received_at_epoch: record.evidence_received_at_epoch,
                claude_code_version: None,
                value: MonitorEvidenceValue::Spend {
                    amount: record.amount.clone(),
                    billing_period_start_epoch: record.billing_period_start_epoch,
                    billing_period_end_epoch: record.billing_period_end_epoch,
                    verification: record.verification,
                },
                fingerprint_key: "spend:account".to_owned(),
            },
        );
    }
    if monitor.evidence.len() > MAX_EVIDENCE_PER_MONITOR {
        monitor
            .evidence
            .drain(0..monitor.evidence.len() - MAX_EVIDENCE_PER_MONITOR);
    }
    let _ = now_epoch;
}

fn sync_monitor_from_session(
    account_id: Option<&str>,
    session_id: &str,
    session: &SessionObservation,
    monitor: &mut DurableMonitor,
) {
    if let Some(model) = &session.model {
        upsert_evidence(
            monitor,
            EvidenceInput {
                account_id,
                session_id: Some(session_id),
                source: MonitorEvidenceSource::Statusline,
                evidence_at_epoch: model.evidence_at_epoch,
                received_at_epoch: model.received_at_epoch,
                claude_code_version: model.claude_code_version.as_deref(),
                value: MonitorEvidenceValue::Model {
                    model: model.value.clone(),
                },
                fingerprint_key: format!("model:{session_id}"),
            },
        );
    }
    for index in 0..2 {
        sync_window(
            monitor,
            account_id,
            Some(session_id),
            MonitorEvidenceSource::Statusline,
            index,
            &session.windows[index],
        );
    }
    if monitor.evidence.len() > MAX_EVIDENCE_PER_MONITOR {
        monitor
            .evidence
            .drain(0..monitor.evidence.len() - MAX_EVIDENCE_PER_MONITOR);
    }
}

fn sync_window(
    monitor: &mut DurableMonitor,
    account_id: Option<&str>,
    session_id: Option<&str>,
    source: MonitorEvidenceSource,
    index: usize,
    window: &ObservedWindow,
) {
    let name = if index == 0 { "five_hour" } else { "seven_day" };
    let kind = if index == 0 {
        MonitorQuotaWindow::FiveHour
    } else {
        MonitorQuotaWindow::SevenDay
    };
    if let Some(used) = &window.used {
        upsert_evidence(
            monitor,
            EvidenceInput {
                account_id,
                session_id,
                source,
                evidence_at_epoch: used.evidence_at_epoch,
                received_at_epoch: used.received_at_epoch,
                claude_code_version: used.claude_code_version.as_deref(),
                value: MonitorEvidenceValue::QuotaUsedPercentage {
                    window: kind,
                    used_percentage_basis_points: used.value,
                },
                fingerprint_key: format!("used:{name}:{}", session_id.unwrap_or("account")),
            },
        );
    }
    if let Some(reset) = &window.reset {
        upsert_evidence(
            monitor,
            EvidenceInput {
                account_id,
                session_id,
                source,
                evidence_at_epoch: reset.evidence_at_epoch,
                received_at_epoch: reset.received_at_epoch,
                claude_code_version: reset.claude_code_version.as_deref(),
                value: MonitorEvidenceValue::QuotaReset {
                    window: kind,
                    reset_at_epoch: reset.value,
                },
                fingerprint_key: format!("reset:{name}:{}", session_id.unwrap_or("account")),
            },
        );
    }
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

fn upsert_evidence(monitor: &mut DurableMonitor, input: EvidenceInput<'_>) {
    let EvidenceInput {
        account_id,
        session_id,
        source,
        evidence_at_epoch,
        received_at_epoch,
        claude_code_version,
        value,
        fingerprint_key,
    } = input;
    let fingerprint = serde_json::to_string(&(
        source,
        session_id,
        evidence_at_epoch,
        received_at_epoch,
        claude_code_version,
        &value,
    ))
    .unwrap_or_default();
    if monitor
        .evidence_fingerprints
        .get(&fingerprint_key)
        .is_some_and(|current| current == &fingerprint)
    {
        return;
    }
    monitor.next_evidence_sequence = monitor.next_evidence_sequence.saturating_add(1);
    let evidence = MonitorEvidence {
        sequence: monitor.next_evidence_sequence,
        account_id: account_id.map(str::to_owned),
        session_id: session_id.map(str::to_owned),
        source,
        evidence_at_epoch,
        evidence_received_at_epoch: received_at_epoch,
        age_seconds: 0,
        claude_code_version: claude_code_version.map(str::to_owned),
        value,
    };
    let key = evidence_key(&evidence);
    if let Some(position) = monitor
        .evidence
        .iter()
        .position(|item| evidence_key(item) == key)
    {
        monitor.evidence[position] = evidence;
    } else if monitor.evidence.len() < MAX_EVIDENCE_PER_MONITOR {
        monitor.evidence.push(evidence);
    }
    monitor
        .evidence_fingerprints
        .insert(fingerprint_key, fingerprint);
}

fn evidence_key(evidence: &MonitorEvidence) -> String {
    let field = match &evidence.value {
        MonitorEvidenceValue::QuotaUsedPercentage { window, .. } => {
            format!("used:{window:?}")
        }
        MonitorEvidenceValue::QuotaReset { window, .. } => {
            format!("reset:{window:?}")
        }
        MonitorEvidenceValue::Model { .. } => "model".to_owned(),
        MonitorEvidenceValue::Spend { .. } => "spend".to_owned(),
        MonitorEvidenceValue::QuotaWindow { window_id, .. } => format!("quota:{window_id}"),
        MonitorEvidenceValue::Budget { .. } => "budget".to_owned(),
    };
    format!(
        "{:?}:{}:{field}",
        evidence.source,
        evidence.session_id.as_deref().unwrap_or("account")
    )
}

#[derive(Default)]
struct MonitorEvaluation {
    issues: Vec<MonitorIssue>,
    actions: Vec<MonitorAction>,
    any_unknown: bool,
    blocked: bool,
}

struct MonitorEvaluationContext<'a> {
    accounts: &'a BTreeMap<String, AccountObservations>,
    unbound_sessions: &'a BTreeMap<String, SessionObservation>,
    current_binding: Option<&'a MonitorAccountBinding>,
    current_policy: Option<&'a MonitorPolicyRecord>,
    goal: Option<&'a DurableGoalSpend>,
}

fn evaluate_monitor(
    context: MonitorEvaluationContext<'_>,
    monitor: &mut DurableMonitor,
    now_epoch: i64,
) -> bool {
    let MonitorEvaluationContext {
        accounts,
        unbound_sessions,
        current_binding,
        current_policy,
        goal,
    } = context;
    if monitor.stopped_at_epoch.is_some() {
        return false;
    }
    let before = monitor.decision_fingerprint.clone();
    let account = monitor
        .account_id
        .as_deref()
        .and_then(|account_id| accounts.get(account_id));
    let session = match &monitor.config.scope {
        MonitorScope::Session { session_id } => unbound_sessions.get(session_id),
        MonitorScope::BoundAccount { session_id, .. } => session_id
            .as_deref()
            .and_then(|session_id| account?.sessions.get(session_id)),
    };
    let mut evaluation = MonitorEvaluation::default();
    evaluate_quota_windows(monitor, account, now_epoch, &mut evaluation);
    if monitor.config.purpose == MonitorPurpose::ObserveOnly {
        evaluation.actions.clear();
        evaluation.issues.clear();
        for (window, index) in [MonitorQuotaWindow::FiveHour, MonitorQuotaWindow::SevenDay]
            .into_iter()
            .zip(0..2)
        {
            for item in quota_window_issues(monitor, account, window, index, now_epoch) {
                push_issue(&mut evaluation.issues, item);
            }
        }
        let (_, _, model_unknown, model_mismatch) =
            model_status(monitor, account, session, now_epoch);
        if model_unknown {
            push_issue(
                &mut evaluation.issues,
                issue(
                    MonitorIssueCode::ModelUnknown,
                    "fresh model evidence is unavailable for the configured model guard",
                    None,
                ),
            );
        }
        if model_mismatch {
            push_issue(
                &mut evaluation.issues,
                issue(
                    MonitorIssueCode::ModelMismatch,
                    "fresh statusline model does not match the configured model guard",
                    None,
                ),
            );
        }
        evaluation.any_unknown = evaluation.issues.iter().any(|item| {
            matches!(
                item.code,
                MonitorIssueCode::QuotaUnknown
                    | MonitorIssueCode::QuotaStale
                    | MonitorIssueCode::MissingReset
                    | MonitorIssueCode::ModelUnknown
            )
        });
        evaluation.blocked = evaluation.issues.iter().any(|item| {
            matches!(
                item.code,
                MonitorIssueCode::LimitExhausted
                    | MonitorIssueCode::LimitGuardReached
                    | MonitorIssueCode::ModelMismatch
            )
        });
    } else {
        append_dispatch_authorization_issues(
            monitor,
            current_binding,
            current_policy,
            goal,
            &mut evaluation,
        );
        evaluate_model_guard(monitor, account, session, now_epoch, &mut evaluation);
        if monitor
            .policy
            .as_ref()
            .is_some_and(|policy| policy.new_policy == MonitorPolicy::StrictSgd)
        {
            evaluate_spend_guard(monitor, account, now_epoch, &mut evaluation);
        }
        append_unknown_pause_actions(monitor, &mut evaluation);
    }

    let runnable = monitor.config.purpose == MonitorPurpose::DispatchGuard
        && !evaluation.blocked
        && !evaluation.any_unknown;
    let lifecycle = if evaluation.blocked {
        MonitorLifecycle::Paused
    } else if evaluation.any_unknown {
        MonitorLifecycle::NeedsEvidence
    } else if evaluation
        .actions
        .iter()
        .any(|action| matches!(action, MonitorAction::Wait { .. }))
    {
        MonitorLifecycle::Waiting
    } else {
        MonitorLifecycle::Active
    };
    let fingerprint =
        decision_fingerprint(&evaluation.actions, &evaluation.issues, lifecycle, runnable);
    if monitor.decision_fingerprint.as_ref() != Some(&fingerprint) {
        monitor.next_decision_sequence = monitor.next_decision_sequence.saturating_add(1);
        let evidence_sequences = monitor
            .evidence
            .iter()
            .filter(|evidence| evidence_is_relevant(evidence, monitor))
            .map(|evidence| evidence.sequence)
            .collect::<Vec<_>>();
        monitor.latest_decision = Some(MonitorDecision {
            sequence: monitor.next_decision_sequence,
            decided_at_epoch: now_epoch,
            evidence_sequences,
            actions: evaluation.actions,
        });
        monitor.decision_fingerprint = Some(fingerprint);
        monitor.updated_at_epoch = now_epoch;
    }
    monitor.last_reconciled_at_epoch = now_epoch;
    before != monitor.decision_fingerprint
}

fn append_dispatch_authorization_issues(
    monitor: &DurableMonitor,
    current_binding: Option<&MonitorAccountBinding>,
    current_policy: Option<&MonitorPolicyRecord>,
    goal: Option<&DurableGoalSpend>,
    evaluation: &mut MonitorEvaluation,
) {
    let binding_matches = current_binding.is_some_and(|binding| {
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
    if !binding_matches {
        let code = if current_binding.is_some_and(|binding| !binding.operator_confirmed) {
            MonitorIssueCode::OperatorConfirmationRequired
        } else {
            MonitorIssueCode::BindingMismatch
        };
        push_issue(
            &mut evaluation.issues,
            issue(
                code,
                "a current operator-confirmed account binding is required",
                None,
            ),
        );
        push_action(
            &mut evaluation.actions,
            MonitorAction::Pause { reason: code },
        );
        evaluation.blocked = true;
    }

    let current_revision = current_policy.map(|policy| policy.revision);
    if current_revision != monitor.config.policy_revision {
        let code = if current_policy.is_some() {
            MonitorIssueCode::PolicyConflict
        } else {
            MonitorIssueCode::PolicyRequired
        };
        push_issue(
            &mut evaluation.issues,
            issue(
                code,
                "the monitor is not using the current approved policy revision",
                None,
            ),
        );
        push_action(
            &mut evaluation.actions,
            MonitorAction::Pause { reason: code },
        );
        evaluation.blocked = true;
    }
    if let Some(policy) = current_policy {
        if policy.origin != MonitorPolicyOrigin::Operator {
            push_issue(
                &mut evaluation.issues,
                issue(
                    MonitorIssueCode::PolicyRequired,
                    "a migrated policy must be explicitly approved before dispatch",
                    None,
                ),
            );
            evaluation.blocked = true;
        } else if !policy.operator_confirmed
            || matches!(
                &monitor.config.scope,
                MonitorScope::BoundAccount {
                    binding_id,
                    binding_revision,
                    ..
                } if policy.binding_id.as_deref() != Some(binding_id)
                    || policy.binding_revision != Some(*binding_revision)
            )
        {
            push_issue(
                &mut evaluation.issues,
                issue(
                    MonitorIssueCode::OperatorConfirmationRequired,
                    "the selected policy lacks confirmation for this account binding",
                    None,
                ),
            );
            evaluation.blocked = true;
        }
    } else {
        push_issue(
            &mut evaluation.issues,
            issue(
                MonitorIssueCode::PolicyRequired,
                "no approved policy exists for this goal",
                None,
            ),
        );
        evaluation.blocked = true;
    }
    if let (Some(goal_id), Some(goal)) = (monitor.config.goal_id.as_deref(), goal) {
        if monitor.account_id.as_deref() != Some(goal.account_id.as_str())
            || monitor.policy.as_ref().is_none_or(|policy| {
                policy.revision != goal.policy_revision || policy.goal_id != goal_id
            })
        {
            push_issue(
                &mut evaluation.issues,
                issue(
                    MonitorIssueCode::PolicyConflict,
                    "goal state does not match its approved policy",
                    None,
                ),
            );
            evaluation.blocked = true;
        }
    } else {
        push_issue(
            &mut evaluation.issues,
            issue(
                MonitorIssueCode::PolicyRequired,
                "the approved goal has not been activated",
                None,
            ),
        );
        evaluation.blocked = true;
    }
    if evaluation.blocked {
        let reason = evaluation
            .issues
            .last()
            .map_or(MonitorIssueCode::PolicyRequired, |item| item.code);
        push_action(
            &mut evaluation.actions,
            MonitorAction::Checkpoint {
                goal_id: monitor.config.goal_id.clone().unwrap_or_default(),
            },
        );
        push_action(&mut evaluation.actions, MonitorAction::Pause { reason });
    }
}

fn evaluate_quota_windows(
    monitor: &mut DurableMonitor,
    account: Option<&AccountObservations>,
    now_epoch: i64,
    evaluation: &mut MonitorEvaluation,
) {
    for (index, window) in [MonitorQuotaWindow::FiveHour, MonitorQuotaWindow::SevenDay]
        .into_iter()
        .enumerate()
    {
        evaluate_quota_window(monitor, account, window, index, now_epoch, evaluation);
    }
}

fn evaluate_quota_window(
    monitor: &mut DurableMonitor,
    account: Option<&AccountObservations>,
    window: MonitorQuotaWindow,
    index: usize,
    now_epoch: i64,
    evaluation: &mut MonitorEvaluation,
) {
    let status = quota_window_status(monitor, account, window, index, now_epoch);
    let used_fresh = append_quota_freshness_issues(&status, now_epoch, evaluation);
    append_quota_usage_actions(
        monitor, account, index, &status, used_fresh, now_epoch, evaluation,
    );
    append_quota_barrier_actions(
        monitor, account, index, &status, used_fresh, now_epoch, evaluation,
    );
}

fn append_quota_freshness_issues(
    status: &MonitorQuotaWindowStatus,
    now_epoch: i64,
    evaluation: &mut MonitorEvaluation,
) -> bool {
    let reset_elapsed = status
        .reset_at_epoch
        .is_some_and(|reset| reset <= now_epoch);
    let used_fresh = !reset_elapsed
        && status
            .used_evidence
            .as_ref()
            .is_some_and(|field| field.freshness == MonitorEvidenceFreshness::Current);
    let reset_fresh = status
        .reset_evidence
        .as_ref()
        .is_some_and(|field| field.freshness == MonitorEvidenceFreshness::Current);
    if !used_fresh {
        evaluation.any_unknown = true;
        push_issue(
            &mut evaluation.issues,
            issue(
                if status.used_evidence.is_some() || reset_elapsed {
                    MonitorIssueCode::QuotaStale
                } else {
                    MonitorIssueCode::QuotaUnknown
                },
                "fresh quota utilization evidence is unavailable",
                None,
            ),
        );
    }
    if !reset_fresh {
        evaluation.any_unknown = true;
        push_issue(
            &mut evaluation.issues,
            issue(
                if status.reset_evidence.is_some() {
                    MonitorIssueCode::QuotaStale
                } else {
                    MonitorIssueCode::MissingReset
                },
                "fresh quota reset evidence is unavailable",
                None,
            ),
        );
    }
    used_fresh
}

fn append_quota_usage_actions(
    monitor: &mut DurableMonitor,
    account: Option<&AccountObservations>,
    index: usize,
    status: &MonitorQuotaWindowStatus,
    used_fresh: bool,
    now_epoch: i64,
    evaluation: &mut MonitorEvaluation,
) {
    let Some(used) = status.used_percentage_basis_points.filter(|_| used_fresh) else {
        return;
    };
    if used >= 9_000 {
        push_action(
            &mut evaluation.actions,
            MonitorAction::Checkpoint {
                goal_id: monitor.config.goal_id.clone().unwrap_or_default(),
            },
        );
    }
    if used >= 9_100 {
        push_action(
            &mut evaluation.actions,
            MonitorAction::ReduceDispatch {
                max_parallel: Some(DECISION_MAX_PARALLEL),
            },
        );
    }
    if used < 9_500 {
        return;
    }

    let reason = quota_pause_reason(used);
    let used_reset = observed_reset_for_used(monitor, account, status, index);
    latch_monitor_reset_barrier(monitor, index, used, used_reset, status, reason, now_epoch);
    push_action(&mut evaluation.actions, MonitorAction::Pause { reason });
    push_issue(
        &mut evaluation.issues,
        issue(
            reason,
            if reason == MonitorIssueCode::LimitExhausted {
                "quota utilization reached 100 percent"
            } else {
                "quota utilization reached the 95 percent pause threshold"
            },
            status.reset_at_epoch,
        ),
    );
    evaluation.blocked = true;
}

fn quota_pause_reason(used: i32) -> MonitorIssueCode {
    if used >= 10_000 {
        MonitorIssueCode::LimitExhausted
    } else {
        MonitorIssueCode::LimitGuardReached
    }
}

fn latch_monitor_reset_barrier(
    monitor: &mut DurableMonitor,
    index: usize,
    used: i32,
    used_reset: Option<i64>,
    status: &MonitorQuotaWindowStatus,
    reason: MonitorIssueCode,
    now_epoch: i64,
) {
    let dependency_sequence = status
        .used_evidence
        .as_ref()
        .map(|evidence| evidence.evidence_sequence);
    if let Some(barrier) = monitor.reset_barriers[index].as_mut() {
        let should_rebase = used > barrier.prior_used_percentage_basis_points
            || used_reset
                .is_some_and(|reset| barrier.prior_reset_at_epoch.is_none_or(|old| reset > old));
        if !should_rebase {
            return;
        }
        barrier.started_at_epoch = now_epoch;
        barrier.prior_reset_at_epoch = max_option(barrier.prior_reset_at_epoch, used_reset);
        barrier.prior_used_percentage_basis_points =
            barrier.prior_used_percentage_basis_points.max(used);
        barrier.evidence_sequence_before = monitor.next_evidence_sequence;
        barrier.dependency_evidence_sequence = dependency_sequence;
        barrier.pause_reason = reason;
    } else {
        monitor.reset_barriers[index] = Some(ResetBarrier {
            started_at_epoch: now_epoch,
            prior_reset_at_epoch: used_reset,
            prior_used_percentage_basis_points: used,
            evidence_sequence_before: monitor.next_evidence_sequence,
            dependency_evidence_sequence: dependency_sequence,
            pause_reason: reason,
        });
    }
    monitor.updated_at_epoch = now_epoch;
}

fn append_quota_barrier_actions(
    monitor: &mut DurableMonitor,
    account: Option<&AccountObservations>,
    index: usize,
    status: &MonitorQuotaWindowStatus,
    used_fresh: bool,
    now_epoch: i64,
    evaluation: &mut MonitorEvaluation,
) {
    let barrier_changed = update_reset_barrier(
        monitor,
        account,
        index,
        status,
        now_epoch,
        &mut evaluation.issues,
    );
    if let Some(barrier) = monitor.reset_barriers[index].as_ref() {
        let due = barrier
            .prior_reset_at_epoch
            .map(|reset| reset.saturating_add(MONITOR_RESET_GRACE_SECS))
            .is_some_and(|due| now_epoch >= due);
        let active_threshold_reason = status
            .used_percentage_basis_points
            .filter(|used| used_fresh && *used >= 9_500)
            .map(quota_pause_reason);
        push_action(
            &mut evaluation.actions,
            MonitorAction::Checkpoint {
                goal_id: monitor.config.goal_id.clone().unwrap_or_default(),
            },
        );
        push_action(
            &mut evaluation.actions,
            MonitorAction::Pause {
                reason: if due {
                    MonitorIssueCode::ResetDueUnverified
                } else {
                    active_threshold_reason.unwrap_or(barrier.pause_reason)
                },
            },
        );
        evaluation.blocked = true;
        evaluation.any_unknown |= due;
    } else if let Some(reset) = status.reset_at_epoch {
        let wait_until = reset.saturating_add(MONITOR_RESET_GRACE_SECS);
        if wait_until > now_epoch {
            push_action(
                &mut evaluation.actions,
                MonitorAction::Wait {
                    until_epoch: Some(wait_until),
                },
            );
        }
    }
    append_account_barrier_actions(account, index, now_epoch, monitor, evaluation);
    if barrier_changed {
        monitor.updated_at_epoch = now_epoch;
    }
}

fn append_account_barrier_actions(
    account: Option<&AccountObservations>,
    index: usize,
    now_epoch: i64,
    monitor: &DurableMonitor,
    evaluation: &mut MonitorEvaluation,
) {
    let Some(barrier) = account.and_then(|account| account.reset_barriers[index].as_ref()) else {
        return;
    };
    let due_at = barrier
        .prior_reset_at_epoch
        .unwrap_or(barrier.started_at_epoch)
        .saturating_add(MONITOR_RESET_GRACE_SECS)
        .max(barrier.started_at_epoch);
    let due = now_epoch >= due_at;
    let reason = if due {
        MonitorIssueCode::ResetDueUnverified
    } else {
        barrier.pause_reason
    };
    push_action(
        &mut evaluation.actions,
        MonitorAction::Checkpoint {
            goal_id: monitor.config.goal_id.clone().unwrap_or_default(),
        },
    );
    push_action(&mut evaluation.actions, MonitorAction::Pause { reason });
    push_issue(
        &mut evaluation.issues,
        issue(
            reason,
            if due {
                "the account quota pause remains until a fresh paired reset is verified"
            } else {
                "the account quota pause is sticky until a verified post-reset observation"
            },
            Some(due_at),
        ),
    );
    evaluation.blocked = true;
    evaluation.any_unknown |= due;
}

fn evaluate_model_guard(
    monitor: &DurableMonitor,
    account: Option<&AccountObservations>,
    session: Option<&SessionObservation>,
    now_epoch: i64,
    evaluation: &mut MonitorEvaluation,
) {
    if monitor.config.expected_model.is_none() {
        return;
    }
    let (_, _, model_unknown, model_mismatch) = model_status(monitor, account, session, now_epoch);
    if model_unknown {
        push_issue(
            &mut evaluation.issues,
            issue(
                MonitorIssueCode::ModelUnknown,
                "fresh model evidence is unavailable for the configured model guard",
                None,
            ),
        );
        evaluation.any_unknown = true;
    }
    if model_mismatch {
        push_issue(
            &mut evaluation.issues,
            issue(
                MonitorIssueCode::ModelMismatch,
                "fresh statusline model does not match the configured model guard",
                None,
            ),
        );
        evaluation.blocked = true;
    }
    if model_unknown || model_mismatch {
        push_action(
            &mut evaluation.actions,
            MonitorAction::Checkpoint {
                goal_id: monitor.config.goal_id.clone().unwrap_or_default(),
            },
        );
        if model_unknown {
            push_action(
                &mut evaluation.actions,
                MonitorAction::Pause {
                    reason: MonitorIssueCode::ModelUnknown,
                },
            );
        }
        if model_mismatch {
            push_action(
                &mut evaluation.actions,
                MonitorAction::Pause {
                    reason: MonitorIssueCode::ModelMismatch,
                },
            );
        }
        evaluation.blocked = true;
    }
}

fn evaluate_spend_guard(
    monitor: &DurableMonitor,
    account: Option<&AccountObservations>,
    now_epoch: i64,
    evaluation: &mut MonitorEvaluation,
) {
    let spend = spend_policy(monitor, account, now_epoch);
    evaluation.blocked |= spend_decision_blocks_dispatch(&spend);
    for action in spend.actions {
        push_action(&mut evaluation.actions, action);
    }
    for code in spend.issues {
        push_issue(&mut evaluation.issues, spend_issue(code));
    }
    if spend.budget_unverifiable || spend.rollover_unknown {
        evaluation.any_unknown = true;
    }
}

fn spend_decision_blocks_dispatch(spend: &SpendDecision) -> bool {
    spend.budget_unverifiable
        || spend.rollover_unknown
        || spend.actions.iter().any(|action| {
            matches!(
                action,
                MonitorAction::ReduceDispatch {
                    max_parallel: Some(0)
                } | MonitorAction::Pause { .. }
            )
        })
}

fn spend_issue(code: MonitorIssueCode) -> MonitorIssue {
    let message = match code {
        MonitorIssueCode::BudgetUnverifiable => {
            "spend cannot be verified against the configured SGD budget"
        }
        MonitorIssueCode::SpendStale => "spend evidence is older than 300 seconds",
        MonitorIssueCode::SpendUnavailable => "no verified spend baseline is available",
        MonitorIssueCode::SpendUnverified => {
            "spend record is not verified for this account and billing period"
        }
        MonitorIssueCode::SpendRolloverUnverified => {
            "billing-period rollover lacks a verified closing total"
        }
        MonitorIssueCode::BudgetWarn => "goal spend reached the warning threshold",
        MonitorIssueCode::BudgetCheckpoint => "goal spend reached the checkpoint threshold",
        MonitorIssueCode::BudgetPause => "goal spend reached the pause threshold",
        _ => "spend policy requires operator attention",
    };
    issue(code, message, None)
}

fn append_unknown_pause_actions(monitor: &DurableMonitor, evaluation: &mut MonitorEvaluation) {
    if !evaluation.any_unknown {
        return;
    }
    for code in evaluation.issues.iter().map(|item| item.code) {
        if !matches!(
            code,
            MonitorIssueCode::QuotaUnknown
                | MonitorIssueCode::QuotaStale
                | MonitorIssueCode::MissingReset
                | MonitorIssueCode::ResetDueUnverified
                | MonitorIssueCode::ModelUnknown
                | MonitorIssueCode::BudgetUnverifiable
                | MonitorIssueCode::SpendUnavailable
                | MonitorIssueCode::SpendUnverified
                | MonitorIssueCode::SpendStale
                | MonitorIssueCode::SpendRolloverUnverified
        ) {
            continue;
        }
        push_action(
            &mut evaluation.actions,
            MonitorAction::Checkpoint {
                goal_id: monitor.config.goal_id.clone().unwrap_or_default(),
            },
        );
        push_action(
            &mut evaluation.actions,
            MonitorAction::Pause { reason: code },
        );
    }
}

fn spend_policy(
    monitor: &DurableMonitor,
    account: Option<&AccountObservations>,
    now_epoch: i64,
) -> SpendDecision {
    let unavailable = SpendAccountState::default();
    let account_spend = account.map_or(&unavailable, |account| &account.spend);
    let unavailable_state = SpendState::default();
    let spend_state = monitor.spend_state.as_ref().unwrap_or(&unavailable_state);
    let goal_id = monitor.config.goal_id.as_deref().unwrap_or("");
    evaluate_spend_policy(
        monitor.account_id.as_deref().unwrap_or(""),
        goal_id,
        monitor
            .policy
            .as_ref()
            .and_then(|policy| policy.budget.as_ref()),
        account_spend,
        spend_state,
        now_epoch,
    )
}

fn quota_window_status(
    monitor: &DurableMonitor,
    account: Option<&AccountObservations>,
    window: MonitorQuotaWindow,
    index: usize,
    now_epoch: i64,
) -> MonitorQuotaWindowStatus {
    let mut used_candidates = Vec::<(&MonitorEvidence, i32)>::new();
    let mut reset_candidates = Vec::<(&MonitorEvidence, i64)>::new();
    for evidence in &monitor.evidence {
        if !evidence_is_relevant(evidence, monitor) {
            continue;
        }
        match evidence.value {
            MonitorEvidenceValue::QuotaUsedPercentage {
                window: evidence_window,
                used_percentage_basis_points,
            } if evidence_window == window => {
                if evidence_is_effective_used(evidence, account, index) {
                    used_candidates.push((evidence, used_percentage_basis_points));
                }
            }
            MonitorEvidenceValue::QuotaReset {
                window: evidence_window,
                reset_at_epoch,
            } if evidence_window == window
                && evidence_is_effective_reset(evidence, account, index) =>
            {
                reset_candidates.push((evidence, reset_at_epoch));
            }
            _ => {}
        }
    }
    let used = choose_used_candidate(used_candidates, now_epoch);
    let reset = choose_reset_candidate(reset_candidates, now_epoch);
    let reset_at_epoch = reset.map(|(_, value)| value);
    MonitorQuotaWindowStatus {
        used_percentage_basis_points: used.map(|(_, value)| value),
        used_evidence: used.map(|(evidence, _)| field_evidence(evidence, now_epoch)),
        reset_at_epoch,
        reset_validity: match reset_at_epoch {
            None => MonitorResetValidity::Unknown,
            Some(reset) if reset <= now_epoch => MonitorResetValidity::Due,
            Some(_) => MonitorResetValidity::Future,
        },
        reset_evidence: reset.map(|(evidence, _)| field_evidence(evidence, now_epoch)),
    }
}

fn observed_reset_for_used(
    monitor: &DurableMonitor,
    account: Option<&AccountObservations>,
    status: &MonitorQuotaWindowStatus,
    index: usize,
) -> Option<i64> {
    let sequence = status.used_evidence.as_ref()?.evidence_sequence;
    let evidence = monitor
        .evidence
        .iter()
        .find(|evidence| evidence.sequence == sequence)?;
    match (evidence.source, account) {
        (MonitorEvidenceSource::Statusline, Some(account)) => evidence
            .session_id
            .as_deref()
            .and_then(|session_id| account.sessions.get(session_id))
            .and_then(|session| session.windows[index].used.as_ref())
            .and_then(|used| used.reset_at_epoch),
        (MonitorEvidenceSource::BrokerProjection, Some(account)) => account.broker_windows[index]
            .used
            .as_ref()
            .and_then(|used| used.reset_at_epoch),
        _ => None,
    }
}

fn choose_used_candidate(
    candidates: Vec<(&MonitorEvidence, i32)>,
    now_epoch: i64,
) -> Option<(&MonitorEvidence, i32)> {
    let current = candidates
        .iter()
        .copied()
        .filter(|(evidence, _)| is_current(evidence, now_epoch))
        .max_by_key(|(evidence, value)| (*value, evidence.evidence_received_at_epoch));
    current.or_else(|| {
        candidates
            .into_iter()
            .max_by_key(|(evidence, value)| (*value, evidence.evidence_received_at_epoch))
    })
}

fn choose_reset_candidate(
    candidates: Vec<(&MonitorEvidence, i64)>,
    now_epoch: i64,
) -> Option<(&MonitorEvidence, i64)> {
    let current = candidates
        .iter()
        .copied()
        .filter(|(evidence, _)| is_current(evidence, now_epoch))
        .min_by_key(|(evidence, value)| {
            (
                *value,
                std::cmp::Reverse(evidence.evidence_received_at_epoch),
            )
        });
    current.or_else(|| {
        candidates.into_iter().min_by_key(|(evidence, value)| {
            (
                *value,
                std::cmp::Reverse(evidence.evidence_received_at_epoch),
            )
        })
    })
}

fn evidence_is_effective_used(
    evidence: &MonitorEvidence,
    account: Option<&AccountObservations>,
    index: usize,
) -> bool {
    let Some(account) = account else {
        return true;
    };
    let Some(latest_reset) = account.latest_reset_epochs[index] else {
        return true;
    };
    if evidence.source == MonitorEvidenceSource::BrokerProjection {
        return account.broker_windows[index]
            .used
            .as_ref()
            .is_some_and(|used| {
                used.reset_at_epoch
                    .is_some_and(|reset| reset >= latest_reset)
            });
    }
    let Some(session_id) = evidence.session_id.as_deref() else {
        return true;
    };
    account
        .sessions
        .get(session_id)
        .and_then(|session| session.windows[index].used.as_ref())
        .is_some_and(|used| {
            used.reset_at_epoch
                .is_some_and(|reset| reset >= latest_reset)
        })
}

fn evidence_is_effective_reset(
    evidence: &MonitorEvidence,
    account: Option<&AccountObservations>,
    index: usize,
) -> bool {
    let Some(account) = account else {
        return true;
    };
    let Some(latest_reset) = account.latest_reset_epochs[index] else {
        return true;
    };
    if evidence.source == MonitorEvidenceSource::BrokerProjection {
        return account.broker_windows[index]
            .reset
            .as_ref()
            .is_some_and(|reset| reset.value >= latest_reset);
    }
    let Some(session_id) = evidence.session_id.as_deref() else {
        return true;
    };
    account
        .sessions
        .get(session_id)
        .and_then(|session| session.windows[index].reset.as_ref())
        .is_some_and(|reset| reset.value >= latest_reset)
}

fn evidence_is_relevant(evidence: &MonitorEvidence, monitor: &DurableMonitor) -> bool {
    match &monitor.config.scope {
        MonitorScope::Session { session_id } => {
            evidence.account_id.is_none() && evidence.session_id.as_ref() == Some(session_id)
        }
        MonitorScope::BoundAccount { session_id, .. } => {
            evidence.account_id == monitor.account_id
                && (session_id
                    .as_ref()
                    .is_none_or(|session| evidence.session_id.as_ref() == Some(session))
                    || (evidence.session_id.is_none()
                        && matches!(&evidence.value, MonitorEvidenceValue::Spend { .. })))
        }
    }
}

fn field_evidence(evidence: &MonitorEvidence, now_epoch: i64) -> MonitorFieldEvidence {
    let age = field_age(
        evidence.evidence_at_epoch,
        evidence.evidence_received_at_epoch,
        now_epoch,
    );
    MonitorFieldEvidence {
        evidence_sequence: evidence.sequence,
        evidence_at_epoch: evidence.evidence_at_epoch,
        evidence_received_at_epoch: evidence.evidence_received_at_epoch,
        age_seconds: age,
        freshness: if age <= MONITOR_EVIDENCE_TTL_SECS as u64
            && evidence.evidence_received_at_epoch <= now_epoch
            && evidence
                .evidence_at_epoch
                .is_none_or(|time| time <= now_epoch.saturating_add(MAX_FUTURE_SKEW_SECS))
        {
            MonitorEvidenceFreshness::Current
        } else {
            MonitorEvidenceFreshness::Stale
        },
    }
}

fn field_age(evidence_at_epoch: Option<i64>, received_at_epoch: i64, now_epoch: i64) -> u64 {
    let nonnegative_age =
        |timestamp| u64::try_from(now_epoch.saturating_sub(timestamp).max(0)).unwrap_or(u64::MAX);
    let received_age = nonnegative_age(received_at_epoch);
    evidence_at_epoch.map_or(received_age, |time| received_age.max(nonnegative_age(time)))
}

fn is_current(evidence: &MonitorEvidence, now_epoch: i64) -> bool {
    field_evidence(evidence, now_epoch).freshness == MonitorEvidenceFreshness::Current
}

fn model_status(
    monitor: &DurableMonitor,
    account: Option<&AccountObservations>,
    unbound_session: Option<&SessionObservation>,
    now_epoch: i64,
) -> (Option<String>, Option<MonitorFieldEvidence>, bool, bool) {
    let sessions = match &monitor.config.scope {
        MonitorScope::Session { session_id } => unbound_session
            .map(|session| vec![(session_id.as_str(), session)])
            .unwrap_or_default(),
        MonitorScope::BoundAccount {
            session_id: Some(session_id),
            ..
        } => account
            .and_then(|account| account.sessions.get(session_id))
            .map(|session| vec![(session_id.as_str(), session)])
            .unwrap_or_default(),
        MonitorScope::BoundAccount {
            session_id: None, ..
        } => account
            .map(|account| {
                account
                    .sessions
                    .iter()
                    .map(|(session_id, session)| (session_id.as_str(), session))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
    };
    if sessions.is_empty() {
        return (None, None, true, false);
    }

    let mut latest: Option<(&MonitorEvidence, &str)> = None;
    let mut unknown = false;
    let mut mismatch = false;
    for (session_id, session) in sessions {
        let Some(model) = session.model.as_ref() else {
            unknown = true;
            continue;
        };
        let Some(evidence) = monitor.evidence.iter().find(|evidence| {
            evidence.source == MonitorEvidenceSource::Statusline
                && evidence.session_id.as_deref() == Some(session_id)
                && matches!(evidence.value, MonitorEvidenceValue::Model { .. })
        }) else {
            unknown = true;
            continue;
        };
        let current = session_is_active(session, now_epoch) && is_current(evidence, now_epoch);
        if current {
            mismatch |= monitor
                .config
                .expected_model
                .as_ref()
                .is_some_and(|expected| expected != &model.value);
        } else {
            unknown = true;
        }
        if latest.is_none_or(|(current, _)| {
            evidence.evidence_received_at_epoch > current.evidence_received_at_epoch
        }) {
            latest = Some((evidence, model.value.as_str()));
        }
    }
    let metadata = latest.map(|(evidence, _)| field_evidence(evidence, now_epoch));
    let model = latest.map(|(_, value)| value.to_owned());
    (model, metadata, unknown, mismatch)
}

fn session_is_active(session: &SessionObservation, now_epoch: i64) -> bool {
    session
        .last_observation_received_at_epoch
        .is_some_and(|received| {
            received <= now_epoch && now_epoch.saturating_sub(received) <= MONITOR_EVIDENCE_TTL_SECS
        })
}

fn prune_inactive_sessions(
    state: &mut StoreState,
    account_id: &str,
    incoming_session_id: Option<&str>,
    now_epoch: i64,
) {
    let mut protected = Vec::<String>::new();
    if let Some(account) = state.accounts.get(account_id) {
        protected.extend(
            account
                .reset_barriers
                .iter()
                .flatten()
                .filter_map(|barrier| barrier.session_id.clone()),
        );
    }
    for monitor in state.monitors.values().filter(|monitor| {
        monitor.stopped_at_epoch.is_none() && monitor.account_id.as_deref() == Some(account_id)
    }) {
        if let Some(session_id) = scope_session_id(&monitor.config.scope) {
            protected.push(session_id.to_owned());
        }
        for barrier in monitor.reset_barriers.iter().flatten() {
            let Some(dependency_sequence) = barrier.dependency_evidence_sequence else {
                continue;
            };
            let dependency = monitor.evidence.iter().find(|evidence| {
                evidence.sequence == dependency_sequence
                    && evidence.source == MonitorEvidenceSource::Statusline
                    && evidence.account_id.as_deref() == Some(account_id)
                    && matches!(
                        &evidence.value,
                        MonitorEvidenceValue::QuotaUsedPercentage { .. }
                    )
            });
            if let Some(session_id) = dependency.and_then(|evidence| evidence.session_id.as_ref()) {
                protected.push(session_id.clone());
            }
        }
    }
    if let Some(account) = state.accounts.get_mut(account_id) {
        let removed = account
            .sessions
            .iter()
            .filter_map(|(session_id, session)| {
                (!session_is_active(session, now_epoch)
                    && !protected.contains(session_id)
                    && Some(session_id.as_str()) != incoming_session_id)
                    .then_some(session_id.clone())
            })
            .collect::<Vec<_>>();
        account.sessions.retain(|session_id, session| {
            session_is_active(session, now_epoch)
                || protected.contains(session_id)
                || Some(session_id.as_str()) == incoming_session_id
        });
        if !removed.is_empty() {
            for monitor in state
                .monitors
                .values_mut()
                .filter(|monitor| monitor.account_id.as_deref() == Some(account_id))
            {
                prune_monitor_session_evidence(monitor, &removed);
            }
        }
    }
}

fn prune_inactive_unbound_sessions(
    state: &mut StoreState,
    incoming_session_id: &str,
    now_epoch: i64,
) {
    let protected = state
        .monitors
        .values()
        .filter(|monitor| monitor.stopped_at_epoch.is_none())
        .filter_map(|monitor| match &monitor.config.scope {
            MonitorScope::Session { session_id } => Some(session_id.clone()),
            MonitorScope::BoundAccount { .. } => None,
        })
        .collect::<Vec<_>>();
    let removed = state
        .unbound_sessions
        .iter()
        .filter_map(|(session_id, session)| {
            (!session_is_active(session, now_epoch)
                && !protected.contains(session_id)
                && session_id != incoming_session_id)
                .then_some(session_id.clone())
        })
        .collect::<Vec<_>>();
    state.unbound_sessions.retain(|session_id, session| {
        session_is_active(session, now_epoch)
            || protected.contains(session_id)
            || session_id == incoming_session_id
    });
    if !removed.is_empty() {
        for monitor in state.monitors.values_mut() {
            if matches!(monitor.config.scope, MonitorScope::Session { .. }) {
                prune_monitor_session_evidence(monitor, &removed);
            }
        }
    }
}

fn prune_monitor_session_evidence(monitor: &mut DurableMonitor, removed: &[String]) {
    monitor
        .evidence_fingerprints
        .retain(|key, _| !fingerprint_key_references_removed_session(key, removed));
    if monitor.stopped_at_epoch.is_none() {
        monitor
            .evidence
            .retain(|evidence| !evidence_references_removed_session(evidence, removed));
    }
}

fn fingerprint_key_references_removed_session(key: &str, removed: &[String]) -> bool {
    if let Some(session_id) = key.strip_prefix("model:") {
        return removed.iter().any(|removed| removed == session_id);
    }
    let Some((_, session_id)) = key
        .strip_prefix("used:")
        .or_else(|| key.strip_prefix("reset:"))
        .and_then(|rest| rest.rsplit_once(':'))
    else {
        return false;
    };
    session_id != "account" && removed.iter().any(|removed| removed == session_id)
}

fn evidence_references_removed_session(evidence: &MonitorEvidence, removed: &[String]) -> bool {
    evidence
        .session_id
        .as_ref()
        .is_some_and(|session_id| removed.contains(session_id))
}

fn update_reset_barrier(
    monitor: &mut DurableMonitor,
    account: Option<&AccountObservations>,
    index: usize,
    status: &MonitorQuotaWindowStatus,
    now_epoch: i64,
    issues: &mut Vec<MonitorIssue>,
) -> bool {
    if let Some(barrier) = monitor.reset_barriers[index].as_ref() {
        let satisfied = reset_barrier_satisfied(monitor, account, index, barrier, now_epoch);
        if satisfied {
            monitor.reset_barriers[index] = None;
        } else {
            let due = barrier
                .prior_reset_at_epoch
                .map(|reset| reset.saturating_add(MONITOR_RESET_GRACE_SECS))
                .is_some_and(|due| now_epoch >= due);
            if due {
                push_issue(
                    issues,
                    issue(
                        MonitorIssueCode::ResetDueUnverified,
                        "the old reset elapsed; waiting for fresh lower usage and an advanced reset",
                        status.reset_at_epoch,
                    ),
                );
            }
            return false;
        }
        return true;
    }
    let Some(reset_at_epoch) = status.reset_at_epoch else {
        return false;
    };
    let due_at = reset_at_epoch.saturating_add(MONITOR_RESET_GRACE_SECS);
    if now_epoch < due_at {
        return false;
    }
    let used = status.used_percentage_basis_points.unwrap_or(10_000);
    monitor.reset_barriers[index] = Some(ResetBarrier {
        started_at_epoch: now_epoch,
        prior_reset_at_epoch: Some(reset_at_epoch),
        prior_used_percentage_basis_points: used,
        evidence_sequence_before: monitor.next_evidence_sequence,
        dependency_evidence_sequence: status
            .used_evidence
            .as_ref()
            .map(|evidence| evidence.evidence_sequence),
        pause_reason: MonitorIssueCode::ResetDueUnverified,
    });
    push_issue(
        issues,
        issue(
            MonitorIssueCode::ResetDueUnverified,
            "the old reset elapsed; waiting for fresh lower usage and an advanced reset",
            Some(due_at),
        ),
    );
    true
}

fn reset_barrier_satisfied(
    monitor: &DurableMonitor,
    account: Option<&AccountObservations>,
    index: usize,
    barrier: &ResetBarrier,
    now_epoch: i64,
) -> bool {
    let window = if index == 0 {
        MonitorQuotaWindow::FiveHour
    } else {
        MonitorQuotaWindow::SevenDay
    };
    let Some(prior_reset) = barrier.prior_reset_at_epoch else {
        return false;
    };
    let due_at = prior_reset.saturating_add(MONITOR_RESET_GRACE_SECS);
    let required_at = due_at.max(barrier.started_at_epoch);
    if now_epoch < required_at {
        return false;
    }
    let mut resets = Vec::<(&MonitorEvidence, i64)>::new();
    let mut usages = Vec::<(&MonitorEvidence, i32)>::new();
    for evidence in &monitor.evidence {
        if evidence.sequence <= barrier.evidence_sequence_before
            || !is_current(evidence, now_epoch)
            || !evidence_is_relevant(evidence, monitor)
            || evidence.evidence_received_at_epoch < required_at
            || evidence
                .evidence_at_epoch
                .is_some_and(|observed| observed < required_at)
        {
            continue;
        }
        match evidence.value {
            MonitorEvidenceValue::QuotaReset {
                window: found,
                reset_at_epoch,
            } if found == window && reset_at_epoch > prior_reset => {
                resets.push((evidence, reset_at_epoch));
            }
            MonitorEvidenceValue::QuotaUsedPercentage {
                window: found,
                used_percentage_basis_points,
            } if found == window
                && used_percentage_basis_points < barrier.prior_used_percentage_basis_points =>
            {
                usages.push((evidence, used_percentage_basis_points));
            }
            _ => {}
        }
    }
    usages.into_iter().any(|(evidence, _)| {
        let stored_reset = match (evidence.source, account) {
            (MonitorEvidenceSource::Statusline, Some(account)) => evidence
                .session_id
                .as_deref()
                .and_then(|session_id| account.sessions.get(session_id))
                .and_then(|session| session.windows[index].used.as_ref())
                .and_then(|used| used.reset_at_epoch),
            (MonitorEvidenceSource::BrokerProjection, Some(account)) => account.broker_windows
                [index]
                .used
                .as_ref()
                .and_then(|used| used.reset_at_epoch),
            _ => None,
        };
        stored_reset.is_some_and(|reset| {
            reset > prior_reset
                && resets.iter().any(|(reset_evidence, new_reset)| {
                    reset_evidence.source == evidence.source
                        && reset_evidence.session_id == evidence.session_id
                        && *new_reset == reset
                })
        })
    })
}

fn append_event_if_changed(state: &mut StoreState, monitor_id: &str, now_epoch: i64) -> bool {
    let status = {
        let Some(monitor) = state.monitors.get(monitor_id) else {
            return false;
        };
        status_for(state, monitor_id, monitor, now_epoch)
    };
    let Some(monitor) = state.monitors.get_mut(monitor_id) else {
        return false;
    };
    if monitor.events.last().is_some_and(|event| {
        event.status.lifecycle == status.lifecycle
            && event.status.runnable == status.runnable
            && event.status.latest_decision == status.latest_decision
            && event.status.expected_model == status.expected_model
            && event.status.model_guard_validity == status.model_guard_validity
            && event.status.budget == status.budget
            && event.status.cumulative_goal_spend == status.cumulative_goal_spend
            && event.status.spend_period_baseline == status.spend_period_baseline
            && event.status.five_hour == status.five_hour
            && event.status.seven_day == status.seven_day
            && event.status.evidence == status.evidence
            && event.status.issues == status.issues
            && event.status.readiness == status.readiness
            && event.status.claude_code_version == status.claude_code_version
    }) {
        return false;
    }
    append_event(monitor, status, now_epoch);
    true
}

fn append_event(monitor: &mut DurableMonitor, status: MonitorStatus, now_epoch: i64) {
    monitor.next_event_sequence = monitor.next_event_sequence.saturating_add(1);
    monitor.events.push(MonitorEvent {
        sequence: monitor.next_event_sequence,
        occurred_at_epoch: now_epoch,
        status,
    });
    if monitor.events.len() > MAX_EVENTS_PER_MONITOR {
        monitor.events.remove(0);
    }
}

fn status_for(
    state: &StoreState,
    monitor_id: &str,
    monitor: &DurableMonitor,
    now_epoch: i64,
) -> MonitorStatus {
    let account = monitor
        .account_id
        .as_deref()
        .and_then(|account_id| state.accounts.get(account_id));
    let unbound_session = match &monitor.config.scope {
        MonitorScope::Session { session_id } => state.unbound_sessions.get(session_id),
        MonitorScope::BoundAccount { .. } => None,
    };
    let session = match &monitor.config.scope {
        MonitorScope::Session { .. } => unbound_session,
        MonitorScope::BoundAccount { session_id, .. } => match session_id.as_deref() {
            Some(session_id) => account.and_then(|account| account.sessions.get(session_id)),
            None => account.and_then(|account| {
                account
                    .sessions
                    .values()
                    .filter(|session| session.last_callback_received_at_epoch.is_some())
                    .max_by_key(|session| session.last_callback_received_at_epoch)
            }),
        },
    };
    let five_hour =
        quota_window_status(monitor, account, MonitorQuotaWindow::FiveHour, 0, now_epoch);
    let seven_day =
        quota_window_status(monitor, account, MonitorQuotaWindow::SevenDay, 1, now_epoch);
    let (model, model_evidence, model_unknown, model_mismatch) =
        model_status(monitor, account, session, now_epoch);
    let model_guard_validity = if monitor.config.expected_model.is_none() {
        MonitorModelGuardValidity::NotConfigured
    } else if model_mismatch {
        MonitorModelGuardValidity::Mismatch
    } else if model_unknown {
        MonitorModelGuardValidity::Unknown
    } else {
        MonitorModelGuardValidity::Match
    };
    let current_binding = match &monitor.config.scope {
        MonitorScope::BoundAccount { binding_id, .. } => current_binding(state, binding_id),
        MonitorScope::Session { .. } => None,
    };
    let current_policy = monitor
        .config
        .goal_id
        .as_deref()
        .and_then(|goal_id| current_policy(state, goal_id));
    let goal = monitor
        .config
        .goal_id
        .as_deref()
        .and_then(|goal_id| state.goals.get(goal_id));
    let issues = monitor_issues(
        monitor,
        account,
        session,
        current_binding,
        current_policy,
        goal,
        now_epoch,
    );
    let spend_blocks_dispatch = monitor
        .policy
        .as_ref()
        .is_some_and(|policy| policy.new_policy == MonitorPolicy::StrictSgd)
        && spend_decision_blocks_dispatch(&spend_policy(monitor, account, now_epoch));
    let readiness = monitor_readiness(MonitorReadinessContext {
        monitor,
        five_hour: &five_hour,
        seven_day: &seven_day,
        issues: &issues,
        session,
        current_binding,
        current_policy,
        goal,
        spend_blocks_dispatch,
    });
    let latest_decision = monitor.latest_decision.clone();
    let (lifecycle, runnable, readiness) = status_lifecycle(
        monitor,
        &issues,
        latest_decision.as_ref(),
        readiness,
        spend_blocks_dispatch,
    );
    let version = session.and_then(|session| session.claude_code_version.clone());
    let budget = monitor
        .policy
        .as_ref()
        .filter(|policy| policy.new_policy == MonitorPolicy::StrictSgd)
        .and_then(|policy| policy.budget.clone());
    let selected_session_id = status_session_id(monitor, account);
    MonitorStatus {
        schema_version: USAGE_MONITOR_SCHEMA_VERSION,
        monitor_id: monitor_id.to_owned(),
        provider: monitor.config.provider,
        purpose: monitor.config.purpose,
        scope: monitor.config.scope.clone(),
        account_id: monitor.account_id.clone(),
        goal_id: monitor.config.goal_id.clone(),
        session_id: selected_session_id,
        claude_code_version: version,
        policy: monitor.policy.clone(),
        expected_model: monitor.config.expected_model.clone(),
        model,
        model_evidence,
        model_guard_validity,
        lifecycle,
        readiness,
        runnable,
        five_hour,
        seven_day,
        budget,
        cumulative_goal_spend: monitor
            .spend_state
            .as_ref()
            .and_then(|state| state.cumulative_goal_spend.clone()),
        spend_period_baseline: monitor
            .spend_state
            .as_ref()
            .and_then(|state| state.period_anchor.clone()),
        evidence: monitor
            .evidence
            .iter()
            .filter(|evidence| evidence_is_relevant(evidence, monitor))
            .map(|evidence| {
                let mut evidence = evidence.clone();
                evidence.age_seconds = field_age(
                    evidence.evidence_at_epoch,
                    evidence.evidence_received_at_epoch,
                    now_epoch,
                );
                evidence
            })
            .collect(),
        latest_decision,
        updated_at_epoch: monitor.updated_at_epoch,
        issues,
    }
}

fn status_session_id(
    monitor: &DurableMonitor,
    account: Option<&AccountObservations>,
) -> Option<String> {
    match &monitor.config.scope {
        MonitorScope::Session { session_id } => Some(session_id.clone()),
        MonitorScope::BoundAccount {
            session_id: Some(session_id),
            ..
        } => Some(session_id.clone()),
        MonitorScope::BoundAccount {
            session_id: None, ..
        } => account.and_then(|account| {
            account
                .sessions
                .iter()
                .max_by_key(|(_, session)| session.last_callback_received_at_epoch)
                .map(|(session_id, _)| session_id.clone())
        }),
    }
}

fn status_lifecycle(
    monitor: &DurableMonitor,
    issues: &[MonitorIssue],
    latest_decision: Option<&MonitorDecision>,
    mut readiness: MonitorReadiness,
    spend_blocks_dispatch: bool,
) -> (MonitorLifecycle, bool, MonitorReadiness) {
    let blocked_issue = issues.iter().any(|item| {
        matches!(
            item.code,
            MonitorIssueCode::LimitGuardReached
                | MonitorIssueCode::LimitExhausted
                | MonitorIssueCode::ModelMismatch
                | MonitorIssueCode::BindingMismatch
                | MonitorIssueCode::OperatorConfirmationRequired
                | MonitorIssueCode::PolicyRequired
                | MonitorIssueCode::PolicyConflict
        )
    });
    let unknown_issue = issues.iter().any(|item| {
        matches!(
            item.code,
            MonitorIssueCode::QuotaUnknown
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
    let lifecycle = if monitor.stopped_at_epoch.is_some() {
        MonitorLifecycle::Stopped
    } else if blocked_issue
        || spend_blocks_dispatch
        || latest_decision.is_some_and(|decision| {
            decision
                .actions
                .iter()
                .any(|action| matches!(action, MonitorAction::Pause { .. }))
        })
    {
        MonitorLifecycle::Paused
    } else if unknown_issue {
        MonitorLifecycle::NeedsEvidence
    } else if latest_decision.is_some_and(|decision| {
        decision
            .actions
            .iter()
            .any(|action| matches!(action, MonitorAction::Wait { .. }))
    }) {
        MonitorLifecycle::Waiting
    } else {
        MonitorLifecycle::Active
    };
    let runnable = monitor.config.purpose == MonitorPurpose::DispatchGuard
        && !blocked_issue
        && !spend_blocks_dispatch
        && !unknown_issue
        && matches!(
            lifecycle,
            MonitorLifecycle::Active | MonitorLifecycle::Waiting
        )
        && readiness.dispatch == MonitorDispatchReadiness::Ready;
    readiness.dispatch = if monitor.config.purpose == MonitorPurpose::ObserveOnly {
        MonitorDispatchReadiness::NotAuthorized
    } else if runnable {
        MonitorDispatchReadiness::Ready
    } else {
        MonitorDispatchReadiness::Blocked
    };
    (lifecycle, runnable, readiness)
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
#[path = "monitor/policy_regression_tests.rs"]
mod policy_regression_tests;

#[cfg(test)]
#[path = "monitor/v2_tests.rs"]
mod v2_tests;
