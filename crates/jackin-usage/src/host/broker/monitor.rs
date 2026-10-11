// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Durable, secret-free monitor state and local decision engine.

mod evaluation;
mod evidence;
mod legacy;
mod projection;
mod quota;
mod reconcile;
mod spend;
mod status;
mod statusline;
mod storage;
mod validation;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::path::Path;
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use jackin_protocol::control::Money;
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
    MonitorPolicyApprovalInput, MonitorPolicyOrigin, MonitorPolicyRecord, MonitorProviderReadiness,
    MonitorPurpose, MonitorQuotaReadiness, MonitorQuotaWindow, MonitorQuotaWindowStatus,
    MonitorReadiness, MonitorReply, MonitorResetValidity, MonitorScope, MonitorServiceStatus,
    MonitorStatus, MonitorTrackingReadiness, SpendRecord, SpendRecordInput, SpendVerification,
    StatuslineObservation, USAGE_MONITOR_SCHEMA_VERSION, USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};

use super::super::HostSurfaceId;

use self::spend::{SpendAccountState, SpendDecision, SpendState, record_account_spend};

use self::evidence::{apply_statusline, apply_unbound_statusline};
use self::projection::observe_projection_account;
use self::quota::{prune_inactive_sessions, prune_inactive_unbound_sessions, quota_window_status};
use self::reconcile::{
    budget_is_same_or_tighter, is_migrated_zero_sgd_budget_repair, reconcile_all_monitors,
    refresh_all_goal_spend,
};
use self::status::{append_event, status_for};
use self::validation::{
    StartAuthority, binding_mismatch, binding_required, current_binding, current_policy,
    invalid_operator_label, operator_confirmation_required, policy_conflict,
    prepare_start_authority, valid_bounded_text, valid_legacy_evidence_fingerprint,
    validate_config, validate_goal_id, validate_identifier, validate_observation,
    validate_policy_input, validate_store_state, validate_store_state_before_fingerprint_migration,
};

#[cfg(test)]
use self::evaluation::evaluate_model_guard;
#[cfg(test)]
use self::quota::model_status;
#[cfg(test)]
use self::status::append_event_if_changed;
#[cfg(test)]
use self::validation::valid_evidence_fingerprint;
#[cfg(test)]
use self::validation::{spend_snapshot_preserves_history, spend_state_matches_account};

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
    #[serde(default)]
    provider_observation: Option<ProviderObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ProviderObservation {
    generation: u64,
    broker_instance_id: String,
    broker_generation: u64,
    source_account_id: String,
    binding_id: Option<String>,
    binding_revision: Option<u64>,
    readiness: MonitorProviderReadiness,
    last_good_at_epoch: Option<i64>,
    retry_at_epoch: Option<i64>,
    issue_code: Option<MonitorIssueCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ProjectionBindingIdentity {
    account_id: String,
    binding_id: String,
    binding_revision: u64,
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

#[derive(Default)]
struct MonitorEvaluation {
    issues: Vec<MonitorIssue>,
    actions: Vec<MonitorAction>,
    any_unknown: bool,
    blocked: bool,
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

impl MonitorStore {
    /// Open the private host-broker monitor store below `data_dir`.
    pub(crate) fn open(data_dir: &Path) -> Result<Self, MonitorIssue> {
        let directory = storage::open_store_dir(data_dir)?;
        let state = if let Some(state) = storage::load(&directory)? {
            validate_store_state(&state)?;
            state
        } else {
            let state = StoreState::default();
            storage::save(&directory, &state)?;
            state
        };
        Ok(Self {
            inner: std::sync::Arc::new(MonitorStoreInner {
                directory,
                state: Mutex::new(state),
                experimental_collector_source: Mutex::new(None),
                changed: Condvar::new(),
            }),
        })
    }

    /// Apply one monitor operation using the caller's clock value.
    pub(crate) fn operate(
        &self,
        operation: MonitorOperation,
        now_epoch: i64,
    ) -> Result<MonitorReply, MonitorIssue> {
        match operation {
            MonitorOperation::Start {
                config,
                idempotency_key,
            } => self.start(config, &idempotency_key, now_epoch),
            MonitorOperation::BindAccount { binding } => self.bind_account(binding, now_epoch),
            MonitorOperation::ApprovePolicy { approval } => {
                self.approve_policy(approval, now_epoch)
            }
            MonitorOperation::Stop { monitor_id } => self.stop(&monitor_id, now_epoch),
            MonitorOperation::Status { monitor_id } => self.status(&monitor_id, now_epoch),
            MonitorOperation::Doctor { provider } => Ok(MonitorReply::Doctor {
                report: MonitorDoctorReport {
                    provider,
                    broker_available: true,
                    statusline_ingress_supported: true,
                    auth_state: MonitorAuthState::Unknown,
                    issues: vec![issue(
                        MonitorIssueCode::AuthStatusUnknown,
                        "authentication was not inspected by this passive check",
                        None,
                    )],
                },
            }),
            MonitorOperation::Ingest { scope, observation } => {
                self.ingest_statusline(scope, observation, now_epoch)
            }
            MonitorOperation::RecordSpend { record } => self.record_spend(record, now_epoch),
            MonitorOperation::Refresh { monitor_id } => self.refresh(&monitor_id, now_epoch),
            MonitorOperation::ServiceStatus => {
                self.tick(now_epoch)?;
                let experimental_collector_source = self.experimental_collector_source();
                let state = self.lock();
                let active_monitors = active_monitor_count(&state);
                Ok(MonitorReply::ServiceStatus {
                    status: MonitorServiceStatus {
                        running: true,
                        active_monitors,
                        next_wake_epoch: next_wake_for(&state),
                        experimental_collector_source,
                    },
                })
            }
            MonitorOperation::ServiceStop => Ok(MonitorReply::ServiceStopped),
            MonitorOperation::Watch {
                monitor_id,
                after_sequence,
                timeout_ms,
            } => self.watch(&monitor_id, after_sequence, timeout_ms, now_epoch),
        }
    }

    /// Apply one canonical provider projection without performing a refresh.
    pub(crate) fn observe_projection(
        &self,
        projection: &UsageProjectionV1,
        now_epoch: i64,
    ) -> Result<(), MonitorIssue> {
        projection.validate().map_err(|_| {
            issue(
                MonitorIssueCode::MonitorStoreUnavailable,
                "canonical usage projection is invalid",
                None,
            )
        })?;
        // Read the ephemeral source before locking durable state; collector
        // admission uses the same source-then-state lock order.
        let configured_source = self.experimental_collector_source();
        let mut guard = self.lock();
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);
        let clock_advanced = now_epoch > guard.last_now_epoch;
        staged.last_now_epoch = now_epoch;
        let mut observations_changed = false;
        for account in projection
            .providers
            .iter()
            .filter(|provider| provider.provider_id == HostSurfaceId::Claude.provider_id())
            .flat_map(|provider| &provider.accounts)
        {
            let Some(binding) = projection_binding_for_source(
                &staged,
                configured_source.as_deref(),
                &account.canonical_account_id,
            ) else {
                continue;
            };
            observations_changed |= observe_projection_account(
                &mut staged,
                account,
                projection,
                &binding.account_id,
                Some(&binding),
                now_epoch,
            )?;
        }
        let monitors_changed = reconcile_all_monitors(&mut staged, now_epoch);
        if observations_changed || monitors_changed || clock_advanced {
            self.commit(&mut guard, staged)?;
        }
        if monitors_changed {
            self.inner.changed.notify_all();
        }
        Ok(())
    }

    /// Reconcile active monitor decisions against current local evidence.
    pub(crate) fn tick(&self, now_epoch: i64) -> Result<(), MonitorIssue> {
        let mut guard = self.lock();
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);
        let clock_advanced = now_epoch > guard.last_now_epoch;
        staged.last_now_epoch = now_epoch;
        let changed = reconcile_all_monitors(&mut staged, now_epoch);
        if !changed && !clock_advanced {
            return Ok(());
        }
        self.commit(&mut guard, staged)?;
        if changed {
            self.inner.changed.notify_all();
        }
        Ok(())
    }

    /// Whether the service still owns any non-stopped monitor.
    #[must_use]
    pub(crate) fn has_active(&self) -> bool {
        active_monitor_count(&self.lock()) > 0
    }

    /// Source account IDs authorized for the experimental Claude collector.
    /// Only current, confirmed bindings attached to active opted-in monitors
    /// whose source matches this process's configured foreground source grant
    /// collection. Unmapped, stale, session-only, and stopped monitors never
    /// enter this set.
    #[must_use]
    pub(crate) fn collection_accounts(&self) -> Vec<String> {
        let Some(configured_source) = self.experimental_collector_source() else {
            return Vec::new();
        };
        let state = self.lock();
        let accounts = state
            .monitors
            .values()
            .filter(|monitor| {
                monitor.stopped_at_epoch.is_none()
                    && monitor.config.experimental_collector
                    && monitor.config.purpose == MonitorPurpose::ObserveOnly
                    && monitor.config.provider
                        == jackin_protocol::usage_monitor::MonitorProvider::Claude
            })
            .filter_map(|monitor| {
                let MonitorScope::BoundAccount {
                    binding_id,
                    binding_revision,
                    ..
                } = &monitor.config.scope
                else {
                    return None;
                };
                let binding = current_binding(&state, binding_id)?;
                (binding.revision == *binding_revision
                    && binding.provider == monitor.config.provider
                    && binding.operator_confirmed
                    && binding.experimental_collector_approved
                    && Some(binding.account_id.as_str()) == monitor.account_id.as_deref()
                    && binding.provider_account_id.as_deref() == Some(configured_source.as_str()))
                .then(|| binding.provider_account_id.clone())
                .flatten()
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|source| {
                projection_binding_for_source(&state, Some(&configured_source), source).is_some()
            })
            .collect::<BTreeSet<_>>();
        accounts.into_iter().collect()
    }

    /// Configure the selected foreground source for this broker process.
    /// This value is intentionally ephemeral and does not assert that the
    /// source's credential is currently valid or that a provider call worked.
    pub(super) fn set_experimental_collector_source(&self, source: Option<String>) {
        *self
            .inner
            .experimental_collector_source
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = source;
    }

    fn experimental_collector_source(&self) -> Option<String> {
        self.inner
            .experimental_collector_source
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Earliest persisted evidence-expiry or reset-grace wake.
    #[must_use]
    pub(crate) fn next_wake(&self) -> Option<i64> {
        next_wake_for(&self.lock())
    }

    fn bind_account(
        &self,
        input: MonitorAccountBindingInput,
        now_epoch: i64,
    ) -> Result<MonitorReply, MonitorIssue> {
        validate_identifier(&input.account_id)?;
        if let Some(provider_account_id) = input.provider_account_id.as_deref() {
            validate_identifier(provider_account_id)?;
        }
        if !valid_bounded_text(&input.operator_label, MAX_OPERATOR_LABEL_LENGTH) {
            return Err(invalid_operator_label());
        }
        if !input.operator_confirmed {
            return Err(operator_confirmation_required());
        }
        if input.experimental_collector_approved && input.provider_account_id.is_none() {
            return Err(binding_required());
        }

        let mut guard = self.lock();
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);
        staged.last_now_epoch = now_epoch;

        let existing = staged.bindings.iter().find_map(|(binding_id, history)| {
            history.last().and_then(|latest| {
                (latest.provider == input.provider && latest.account_id == input.account_id)
                    .then_some((binding_id.clone(), latest.clone()))
            })
        });
        let binding = if let Some((binding_id, previous)) = existing {
            if previous.operator_confirmed
                && previous.operator_label == input.operator_label
                && previous.provider_account_id == input.provider_account_id
                && previous.experimental_collector_approved == input.experimental_collector_approved
            {
                previous
            } else {
                let source_mapping_changed =
                    previous.provider_account_id != input.provider_account_id;
                let has_goal_history =
                    staged
                        .goals
                        .values()
                        .any(|goal| goal.binding_id == binding_id)
                        || staged.policy_records.values().flatten().any(|policy| {
                            policy.binding_id.as_deref() == Some(binding_id.as_str())
                        });
                if source_mapping_changed && has_goal_history {
                    return Err(issue(
                        MonitorIssueCode::AccountMismatch,
                        "a source account mapping cannot change after goal history exists",
                        None,
                    ));
                }
                let revision = previous
                    .revision
                    .checked_add(1)
                    .ok_or_else(store_unavailable)?;
                // Evidence from a prior binding revision must never flow into
                // the newly selected local/source mapping.
                clear_broker_projection(&mut staged, &previous.account_id);
                let next = MonitorAccountBinding {
                    binding_id: binding_id.clone(),
                    provider: input.provider,
                    account_id: input.account_id,
                    provider_account_id: input.provider_account_id,
                    experimental_collector_approved: input.experimental_collector_approved,
                    operator_label: input.operator_label,
                    revision,
                    operator_confirmed: true,
                    confirmed_at_epoch: Some(now_epoch),
                };
                staged
                    .bindings
                    .get_mut(&binding_id)
                    .ok_or_else(store_unavailable)?
                    .push(next.clone());
                next
            }
        } else {
            if staged.bindings.len() >= MAX_BINDINGS {
                return Err(store_unavailable());
            }
            let next_binding_id = staged
                .next_binding_id
                .checked_add(1)
                .ok_or_else(store_unavailable)?;
            let binding_id = format!("binding-{:08}", staged.next_binding_id);
            staged.next_binding_id = next_binding_id;
            let binding = MonitorAccountBinding {
                binding_id: binding_id.clone(),
                provider: input.provider,
                account_id: input.account_id,
                provider_account_id: input.provider_account_id,
                experimental_collector_approved: input.experimental_collector_approved,
                operator_label: input.operator_label,
                revision: 1,
                operator_confirmed: true,
                confirmed_at_epoch: Some(now_epoch),
            };
            staged.bindings.insert(binding_id, vec![binding.clone()]);
            binding
        };

        self.commit(&mut guard, staged)?;
        self.inner.changed.notify_all();
        Ok(MonitorReply::AccountBound { binding })
    }

    fn approve_policy(
        &self,
        approval: MonitorPolicyApprovalInput,
        now_epoch: i64,
    ) -> Result<MonitorReply, MonitorIssue> {
        validate_identifier(&approval.binding_id)?;
        validate_goal_id(&approval.goal_id)?;
        if !valid_bounded_text(&approval.operator_label, MAX_OPERATOR_LABEL_LENGTH) {
            return Err(invalid_operator_label());
        }
        if !approval.operator_confirmed {
            return Err(operator_confirmation_required());
        }
        validate_policy_input(
            approval.new_policy,
            approval.budget.as_ref(),
            approval.acknowledge_no_sgd_cap,
        )?;

        let mut guard = self.lock();
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);
        staged.last_now_epoch = now_epoch;
        let binding = current_binding(&staged, &approval.binding_id)
            .filter(|binding| binding.revision == approval.binding_revision)
            .cloned()
            .ok_or_else(binding_mismatch)?;
        if !binding.operator_confirmed {
            return Err(operator_confirmation_required());
        }
        if staged.policy_records.values().map(Vec::len).sum::<usize>() >= MAX_POLICY_REVISIONS {
            return Err(store_unavailable());
        }

        let previous = current_policy(&staged, &approval.goal_id).cloned();
        if previous.as_ref().is_some_and(|record| {
            record.provider != binding.provider || record.account_id != binding.account_id
        }) {
            return Err(issue(
                MonitorIssueCode::AccountMismatch,
                "a goal policy cannot move to a different provider account",
                None,
            ));
        }
        if let Some(expected) = approval.expected_revision
            && previous.as_ref().map(|record| record.revision) != Some(expected)
        {
            return Err(policy_conflict());
        }
        if let (Some(previous), Some(_goal)) =
            (previous.as_ref(), staged.goals.get(&approval.goal_id))
            && previous.new_policy != approval.new_policy
        {
            return Err(policy_conflict());
        }
        if previous.as_ref().is_some_and(|record| {
            record.new_policy == MonitorPolicy::StrictSgd
                && approval.new_policy == MonitorPolicy::QuotaOnly
        }) {
            return Err(policy_conflict());
        }
        if previous.as_ref().is_some_and(|record| {
            record.new_policy == MonitorPolicy::StrictSgd
                && approval.new_policy == MonitorPolicy::StrictSgd
                && !budget_is_same_or_tighter(record.budget.as_ref(), approval.budget.as_ref())
                && !is_migrated_zero_sgd_budget_repair(record)
        }) {
            return Err(policy_conflict());
        }

        let revision = previous
            .as_ref()
            .map(|record| record.revision.checked_add(1).ok_or_else(store_unavailable))
            .transpose()?
            .unwrap_or(1);
        let record = MonitorPolicyRecord {
            provider: binding.provider,
            account_id: binding.account_id.clone(),
            binding_id: Some(binding.binding_id.clone()),
            binding_revision: Some(binding.revision),
            goal_id: approval.goal_id.clone(),
            previous_policy: previous.as_ref().map(|record| record.new_policy),
            new_policy: approval.new_policy,
            budget: approval.budget,
            operator_label: Some(approval.operator_label),
            operator_confirmed: true,
            acknowledge_no_sgd_cap: approval.acknowledge_no_sgd_cap,
            recorded_at_epoch: Some(now_epoch),
            revision,
            origin: MonitorPolicyOrigin::Operator,
        };
        staged
            .policy_records
            .entry(record.goal_id.clone())
            .or_default()
            .push(record.clone());
        if let Some(goal) = staged.goals.get_mut(&record.goal_id) {
            goal.binding_id = binding.binding_id.clone();
            goal.binding_revision = binding.revision;
            goal.policy_revision = record.revision;
            goal.policy = record.new_policy;
            goal.budget = record.budget.clone();
        }
        refresh_all_goal_spend(&mut staged, now_epoch);
        let changed = reconcile_all_monitors(&mut staged, now_epoch);
        self.commit(&mut guard, staged)?;
        if changed {
            self.inner.changed.notify_all();
        }
        Ok(MonitorReply::PolicyApproved { policy: record })
    }

    fn start(
        &self,
        config: MonitorConfig,
        idempotency_key: &str,
        now_epoch: i64,
    ) -> Result<MonitorReply, MonitorIssue> {
        validate_config(&config)?;
        if !valid_bounded_text(idempotency_key, MAX_IDEMPOTENCY_KEY_LENGTH)
            || idempotency_key.starts_with(MIGRATED_IDEMPOTENCY_KEY_PREFIX)
        {
            return Err(issue(
                MonitorIssueCode::StatuslineInvalid,
                "idempotency key is empty, reserved, or outside its accepted bounds",
                None,
            ));
        }
        let mut guard = self.lock();
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);

        // Revalidate the current binding before returning an idempotent replay.
        // Approval can be revoked by a later binding revision while the start
        // key itself remains unchanged.
        if config.experimental_collector {
            let binding =
                validation::resolve_scope_binding(&staged, &config.scope, config.provider)?;
            if !binding.operator_confirmed {
                return Err(operator_confirmation_required());
            }
            validation::validate_collection_binding(&config, binding)?;
            let configured_source = self.experimental_collector_source();
            if configured_source
                .as_deref()
                .is_none_or(|source| binding.provider_account_id.as_deref() != Some(source))
            {
                return Err(issue(
                    MonitorIssueCode::CollectorAuthRequired,
                    "experimental collection requires a configured foreground source matching the approved binding",
                    None,
                ));
            }
        }

        if let Some((monitor_id, existing_config)) = staged
            .monitors
            .iter()
            .find(|(_, monitor)| monitor.idempotency_key == idempotency_key)
            .map(|(monitor_id, monitor)| (monitor_id.clone(), monitor.config.clone()))
        {
            if existing_config != config {
                return Err(issue(
                    MonitorIssueCode::IdempotencyConflict,
                    "the start key is already associated with a different monitor configuration",
                    None,
                ));
            }
            staged.last_now_epoch = now_epoch;
            let changed = reconcile_all_monitors(&mut staged, now_epoch);
            let monitor = staged
                .monitors
                .get(&monitor_id)
                .ok_or_else(store_unavailable)?;
            let status = status_for(&staged, &monitor_id, monitor, now_epoch);
            if changed || now_epoch > guard.last_now_epoch {
                self.commit(&mut guard, staged)?;
            }
            return Ok(MonitorReply::Started {
                status: Box::new(status),
            });
        }

        if staged.monitors.len() >= MAX_MONITORS {
            return Err(issue(
                MonitorIssueCode::MonitorStoreUnavailable,
                "monitor store reached its configured monitor limit",
                None,
            ));
        }
        staged.last_now_epoch = now_epoch;

        let StartAuthority {
            account_id,
            policy,
            goal_id,
        } = prepare_start_authority(&mut staged, &config, now_epoch)?;

        refresh_all_goal_spend(&mut staged, now_epoch);
        let next_monitor_id = staged
            .next_monitor_id
            .checked_add(1)
            .ok_or_else(store_unavailable)?;
        let monitor_id = format!("monitor-{:08}", staged.next_monitor_id);
        staged.next_monitor_id = next_monitor_id;
        let spend_state = goal_id
            .as_ref()
            .and_then(|goal_id| staged.goals.get(goal_id))
            .and_then(|goal| goal.spend_state.clone());
        let monitor = DurableMonitor {
            config: config.clone(),
            account_id,
            idempotency_key: idempotency_key.to_owned(),
            policy,
            created_at_epoch: now_epoch,
            stopped_at_epoch: None,
            updated_at_epoch: now_epoch,
            last_reconciled_at_epoch: now_epoch,
            next_evidence_sequence: 0,
            next_decision_sequence: 0,
            next_event_sequence: 0,
            evidence: Vec::new(),
            evidence_fingerprints: BTreeMap::new(),
            reset_barriers: [None, None],
            latest_decision: None,
            decision_fingerprint: None,
            events: Vec::new(),
            spend_state,
        };
        staged.monitors.insert(monitor_id.clone(), monitor);
        let _monitor_decisions_changed = reconcile_all_monitors(&mut staged, now_epoch);
        let monitor = staged
            .monitors
            .get(&monitor_id)
            .ok_or_else(store_unavailable)?;
        let status = status_for(&staged, &monitor_id, monitor, now_epoch);
        self.commit(&mut guard, staged)?;
        self.inner.changed.notify_all();
        Ok(MonitorReply::Started {
            status: Box::new(status),
        })
    }

    fn stop(&self, monitor_id: &str, now_epoch: i64) -> Result<MonitorReply, MonitorIssue> {
        let mut guard = self.lock();
        if let Some(monitor) = guard.monitors.get(monitor_id)
            && monitor.stopped_at_epoch.is_some()
        {
            let status = status_for(&guard, monitor_id, monitor, guard.last_now_epoch);
            return Ok(MonitorReply::Stopped {
                status: Box::new(status),
            });
        }
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);
        staged.last_now_epoch = now_epoch;
        if !staged.monitors.contains_key(monitor_id) {
            return Err(monitor_not_found());
        }
        let monitor = staged
            .monitors
            .get_mut(monitor_id)
            .ok_or_else(monitor_not_found)?;
        monitor.stopped_at_epoch = Some(now_epoch);
        monitor.updated_at_epoch = now_epoch;
        monitor.latest_decision = None;
        monitor.decision_fingerprint = None;
        let _other_monitors_changed = reconcile_all_monitors(&mut staged, now_epoch);
        let status = {
            let monitor = staged
                .monitors
                .get(monitor_id)
                .ok_or_else(monitor_not_found)?;
            status_for(&staged, monitor_id, monitor, now_epoch)
        };
        append_event(
            staged
                .monitors
                .get_mut(monitor_id)
                .ok_or_else(monitor_not_found)?,
            status.clone(),
            now_epoch,
        );
        self.commit(&mut guard, staged)?;
        self.inner.changed.notify_all();
        Ok(MonitorReply::Stopped {
            status: Box::new(status),
        })
    }

    fn ingest_statusline(
        &self,
        scope: MonitorScope,
        observation: StatuslineObservation,
        now_epoch: i64,
    ) -> Result<MonitorReply, MonitorIssue> {
        let mut guard = self.lock();
        let account_id = match &scope {
            MonitorScope::Session { session_id } => {
                validate_identifier(session_id)?;
                if session_id != &observation.session_id {
                    return Err(issue(
                        MonitorIssueCode::BindingMismatch,
                        "session evidence does not match the selected session scope",
                        None,
                    ));
                }
                None
            }
            MonitorScope::BoundAccount {
                binding_id,
                binding_revision,
                session_id,
            } => {
                validate_identifier(binding_id)?;
                let binding = current_binding(&guard, binding_id)
                    .filter(|binding| binding.revision == *binding_revision)
                    .cloned()
                    .ok_or_else(binding_mismatch)?;
                if !binding.operator_confirmed {
                    return Err(operator_confirmation_required());
                }
                if session_id
                    .as_deref()
                    .is_some_and(|expected| expected != observation.session_id)
                {
                    return Err(binding_mismatch());
                }
                Some(binding.account_id)
            }
        };
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);
        staged.last_now_epoch = now_epoch;
        validate_observation(&observation, now_epoch)?;
        if account_id.is_none() {
            prune_inactive_unbound_sessions(&mut staged, &observation.session_id, now_epoch);
        }
        let proposed_sequence = staged.next_input_sequence.saturating_add(1);
        let evidence_sequence = if let Some(account_id) = account_id.as_deref() {
            prune_inactive_sessions(
                &mut staged,
                account_id,
                Some(&observation.session_id),
                now_epoch,
            );
            if !staged.accounts.contains_key(account_id) && staged.accounts.len() >= MAX_ACCOUNTS {
                return Err(store_unavailable());
            }
            let session_exists = staged
                .accounts
                .get(account_id)
                .is_some_and(|account| account.sessions.contains_key(&observation.session_id));
            if !session_exists
                && staged
                    .accounts
                    .get(account_id)
                    .is_some_and(|account| account.sessions.len() >= MAX_SESSIONS_PER_ACCOUNT)
            {
                return Err(store_unavailable());
            }
            let (account_sequence, fields_changed) = {
                let account = staged.accounts.entry(account_id.to_owned()).or_default();
                let (changed, fields_changed) =
                    apply_statusline(account, &observation, now_epoch, proposed_sequence);
                let _ = changed;
                if fields_changed {
                    account.input_sequence = proposed_sequence;
                }
                (account.input_sequence, fields_changed)
            };
            if fields_changed {
                staged.next_input_sequence = proposed_sequence;
            }
            account_sequence
        } else {
            let session_exists = staged
                .unbound_sessions
                .contains_key(&observation.session_id);
            if !session_exists && staged.unbound_sessions.len() >= MAX_UNBOUND_SESSIONS {
                return Err(store_unavailable());
            }
            let session = staged
                .unbound_sessions
                .entry(observation.session_id.clone())
                .or_default();
            let (changed, fields_changed) = apply_unbound_statusline(
                session,
                &observation,
                now_epoch,
                proposed_sequence,
                !session_exists,
            );
            let _ = changed;
            if fields_changed {
                staged.next_input_sequence = proposed_sequence;
                proposed_sequence
            } else {
                staged.next_input_sequence
            }
        };
        let monitors_changed = reconcile_all_monitors(&mut staged, now_epoch);
        self.commit(&mut guard, staged)?;
        if monitors_changed {
            self.inner.changed.notify_all();
        }
        Ok(MonitorReply::Ingested {
            scope,
            account_id,
            session_id: observation.session_id,
            evidence_sequence,
        })
    }

    fn record_spend(
        &self,
        record: SpendRecordInput,
        now_epoch: i64,
    ) -> Result<MonitorReply, MonitorIssue> {
        validate_identifier(&record.account_id)?;
        if record.amount.currency.is_empty()
            || record.amount.currency.len() > 16
            || !record.amount.currency.is_ascii()
            || record.amount.exponent > 9
        {
            return Err(issue(
                MonitorIssueCode::StatuslineInvalid,
                "spend currency or exponent is outside its accepted bounds",
                None,
            ));
        }
        let mut guard = self.lock();
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);
        staged.last_now_epoch = now_epoch;
        if !staged.accounts.contains_key(&record.account_id)
            && staged.accounts.len() >= MAX_ACCOUNTS
        {
            return Err(issue(
                MonitorIssueCode::MonitorStoreUnavailable,
                "monitor store reached its configured account limit",
                None,
            ));
        }
        let account_id = record.account_id.clone();
        if let Some(current) = staged
            .accounts
            .get(&account_id)
            .and_then(|account| account.spend.latest_record.as_ref())
            .filter(|current| spend_input_matches_record(&record, current))
            .cloned()
        {
            let monitors_changed = reconcile_all_monitors(&mut staged, now_epoch);
            self.commit(&mut guard, staged)?;
            if monitors_changed {
                self.inner.changed.notify_all();
            }
            return Ok(MonitorReply::SpendRecorded { record: current });
        }
        let accepted = {
            let account = staged.accounts.entry(account_id.clone()).or_default();
            let (next, accepted) =
                record_account_spend(&account.spend, &account_id, record, now_epoch).map_err(
                    |error| match error {
                        spend::SpendRecordReject::AccountMismatch => issue(
                            MonitorIssueCode::AccountMismatch,
                            "spend record account does not match the selected account",
                            None,
                        ),
                        _ => issue(
                            MonitorIssueCode::StatuslineInvalid,
                            "spend record failed account, period, amount, or source validation",
                            None,
                        ),
                    },
                )?;
            account.spend = next;
            accepted
        };
        let monitors_changed = reconcile_all_monitors(&mut staged, now_epoch);
        self.commit(&mut guard, staged)?;
        if monitors_changed {
            self.inner.changed.notify_all();
        }
        Ok(MonitorReply::SpendRecorded { record: accepted })
    }

    fn refresh(&self, monitor_id: &str, now_epoch: i64) -> Result<MonitorReply, MonitorIssue> {
        let mut guard = self.lock();
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);
        staged.last_now_epoch = now_epoch;
        if !staged.monitors.contains_key(monitor_id) {
            return Err(monitor_not_found());
        }
        let monitors_changed = reconcile_all_monitors(&mut staged, now_epoch);
        let monitor = staged
            .monitors
            .get(monitor_id)
            .ok_or_else(monitor_not_found)?;
        let status = status_for(&staged, monitor_id, monitor, now_epoch);
        self.commit(&mut guard, staged)?;
        if monitors_changed {
            self.inner.changed.notify_all();
        }
        Ok(MonitorReply::Refreshed {
            status: Box::new(status),
        })
    }

    fn status(&self, monitor_id: &str, now_epoch: i64) -> Result<MonitorReply, MonitorIssue> {
        let mut guard = self.lock();
        let mut staged = guard.clone();
        let now_epoch = effective_now(staged.last_now_epoch, now_epoch);
        staged.last_now_epoch = now_epoch;
        if !staged.monitors.contains_key(monitor_id) {
            return Err(monitor_not_found());
        }
        let monitors_changed = reconcile_all_monitors(&mut staged, now_epoch);
        let monitor = staged
            .monitors
            .get(monitor_id)
            .ok_or_else(monitor_not_found)?;
        let status = status_for(&staged, monitor_id, monitor, now_epoch);
        self.commit(&mut guard, staged)?;
        if monitors_changed {
            self.inner.changed.notify_all();
        }
        Ok(MonitorReply::Status {
            status: Box::new(status),
        })
    }

    fn watch(
        &self,
        monitor_id: &str,
        after_sequence: u64,
        timeout_ms: u64,
        now_epoch: i64,
    ) -> Result<MonitorReply, MonitorIssue> {
        // Reconcile durable evidence ages before a fresh attach can observe the
        // event log. In particular, a broker restart past the evidence TTL must
        // persist a blocked decision before returning the prior runnable event.
        self.tick(now_epoch)?;
        let timeout_ms = timeout_ms.min(MONITOR_WATCH_TIMEOUT_CAP_MS);
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut state = self.lock();
        if !state.monitors.contains_key(monitor_id) {
            return Err(monitor_not_found());
        }
        loop {
            let (events, next_sequence) = watch_snapshot(&state, monitor_id, after_sequence)?;
            if !events.is_empty() {
                return Ok(MonitorReply::Watch {
                    events,
                    next_sequence,
                    timed_out: false,
                });
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(MonitorReply::Watch {
                    events,
                    next_sequence,
                    timed_out: true,
                });
            }
            let result = self.inner.changed.wait_timeout(state, remaining);
            let (next_state, wait) = result.unwrap_or_else(std::sync::PoisonError::into_inner);
            state = next_state;
            if wait.timed_out() {
                let (events, next_sequence) = watch_snapshot(&state, monitor_id, after_sequence)?;
                return Ok(MonitorReply::Watch {
                    timed_out: events.is_empty(),
                    events,
                    next_sequence,
                });
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, StoreState> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn commit(
        &self,
        guard: &mut MutexGuard<'_, StoreState>,
        staged: StoreState,
    ) -> Result<(), MonitorIssue> {
        validate_store_state(&staged)?;
        storage::save(&self.inner.directory, &staged)?;
        **guard = staged;
        Ok(())
    }
}

fn projection_binding_for_source(
    state: &StoreState,
    configured_source: Option<&str>,
    source_account_id: &str,
) -> Option<ProjectionBindingIdentity> {
    if configured_source != Some(source_account_id) {
        return None;
    }
    let candidates = state
        .monitors
        .values()
        .filter(|monitor| {
            monitor.stopped_at_epoch.is_none()
                && monitor.config.experimental_collector
                && monitor.config.purpose == MonitorPurpose::ObserveOnly
                && monitor.config.provider
                    == jackin_protocol::usage_monitor::MonitorProvider::Claude
        })
        .filter_map(|monitor| {
            let MonitorScope::BoundAccount {
                binding_id,
                binding_revision,
                ..
            } = &monitor.config.scope
            else {
                return None;
            };
            let binding = current_binding(state, binding_id)?;
            (binding.revision == *binding_revision
                && binding.provider == monitor.config.provider
                && binding.operator_confirmed
                && binding.experimental_collector_approved
                && monitor.account_id.as_deref() == Some(binding.account_id.as_str())
                && binding.provider_account_id.as_deref() == Some(source_account_id))
            .then(|| ProjectionBindingIdentity {
                account_id: binding.account_id.clone(),
                binding_id: binding.binding_id.clone(),
                binding_revision: binding.revision,
            })
        })
        .collect::<BTreeSet<_>>();
    (candidates.len() == 1)
        .then(|| candidates.into_iter().next())
        .flatten()
}

fn clear_broker_projection(state: &mut StoreState, account_id: &str) {
    let Some(account) = state.accounts.get_mut(account_id) else {
        return;
    };
    clear_account_broker_projection(account);
}

fn clear_account_broker_projection(account: &mut AccountObservations) {
    account.broker_windows = Default::default();
    account.latest_reset_epochs = std::array::from_fn(|index| {
        account
            .sessions
            .values()
            .filter_map(|session| {
                session.windows[index]
                    .reset
                    .as_ref()
                    .map(|reset| reset.value)
            })
            .max()
    });
    for barrier in &mut account.reset_barriers {
        if barrier
            .as_ref()
            .is_some_and(|barrier| barrier.source == MonitorEvidenceSource::BrokerProjection)
        {
            *barrier = None;
        }
    }
    account.provider_observation = None;
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
