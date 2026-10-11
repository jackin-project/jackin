// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::quota::evidence_is_relevant;
use super::reconcile::{budget_is_same_or_tighter, is_migrated_zero_sgd_budget_repair};
use super::spend::{self, capture_goal_baseline};
use super::{
    DurableGoalSpend, MAX_ACCOUNTS, MAX_BINDINGS, MAX_EVENTS_PER_MONITOR,
    MAX_EVIDENCE_FINGERPRINTS, MAX_EVIDENCE_PER_MONITOR, MAX_FUTURE_SKEW_SECS, MAX_GOAL_ID_LENGTH,
    MAX_GOALS, MAX_ID_LENGTH, MAX_IDEMPOTENCY_KEY_LENGTH, MAX_MODEL_LENGTH, MAX_MONITORS,
    MAX_OPERATOR_LABEL_LENGTH, MAX_POLICY_REVISIONS, MAX_SESSIONS_PER_ACCOUNT,
    MAX_UNBOUND_SESSIONS, MONITOR_EVIDENCE_TTL_SECS, MonitorAccountBinding, MonitorConfig,
    MonitorDecision, MonitorDispatchReadiness, MonitorEvidence, MonitorEvidenceSource,
    MonitorIssue, MonitorIssueCode, MonitorPolicy, MonitorPolicyOrigin, MonitorPolicyRecord,
    MonitorProviderReadiness, MonitorPurpose, MonitorScope, ProviderObservation, SpendAccountState,
    SpendState, StatuslineObservation, StoreState, USAGE_MONITOR_SCHEMA_VERSION,
    USAGE_STATUSLINE_INPUT_SCHEMA_VERSION, issue, store_unavailable,
};
use jackin_protocol::control::Money;
use std::collections::BTreeSet;

pub(super) struct StartAuthority {
    pub(super) account_id: Option<String>,
    pub(super) policy: Option<MonitorPolicyRecord>,
    pub(super) goal_id: Option<String>,
}

pub(super) fn prepare_start_authority(
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

pub(super) fn prepare_dispatch_guard_authority(
    state: &mut StoreState,
    config: &MonitorConfig,
    now_epoch: i64,
) -> Result<StartAuthority, MonitorIssue> {
    let binding = resolve_scope_binding(state, &config.scope, config.provider)?.clone();
    if !binding.operator_confirmed {
        return Err(operator_confirmation_required());
    }
    validate_collection_binding(config, &binding)?;
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

pub(super) fn validate_start_policy_for_binding(
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

pub(super) fn validate_collection_binding(
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

pub(super) fn prepare_goal_activation(
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

pub(super) fn validate_store_state(state: &StoreState) -> Result<(), MonitorIssue> {
    validate_store_state_with_legacy_fingerprints(state, false)
}

pub(super) fn validate_store_state_before_fingerprint_migration(
    state: &StoreState,
) -> Result<(), MonitorIssue> {
    validate_store_state_with_legacy_fingerprints(state, true)
}

fn validate_store_state_with_legacy_fingerprints(
    state: &StoreState,
    allow_legacy_fingerprints: bool,
) -> Result<(), MonitorIssue> {
    validate_store_header(state)?;
    validate_accounts(state)?;
    validate_unbound_sessions(state)?;
    validate_bindings(state)?;
    validate_policy_records(state)?;
    validate_monitors_with_fingerprint_version(state, allow_legacy_fingerprints)?;
    validate_goals(state)
}

pub(super) fn validate_store_header(state: &StoreState) -> Result<(), MonitorIssue> {
    if state.schema_version != USAGE_MONITOR_SCHEMA_VERSION
        || state.next_monitor_id == 0
        || state.next_binding_id == 0
        || state.next_input_sequence == u64::MAX
        || state.monitors.len() > MAX_MONITORS
        || state.accounts.len() > MAX_ACCOUNTS
        || state.unbound_sessions.len() > MAX_UNBOUND_SESSIONS
        || state.bindings.len() > MAX_BINDINGS
        || state.policy_records.values().map(Vec::len).sum::<usize>() > MAX_POLICY_REVISIONS
        || state.last_now_epoch < 0
    {
        return Err(store_unavailable());
    }
    let max_monitor_id = state
        .monitors
        .keys()
        .map(|monitor_id| parse_counter_id(monitor_id, "monitor-"))
        .collect::<Option<Vec<_>>>()
        .and_then(|ids| ids.into_iter().max())
        .unwrap_or(0);
    let max_binding_id = state
        .bindings
        .keys()
        .map(|binding_id| parse_counter_id(binding_id, "binding-"))
        .collect::<Option<Vec<_>>>()
        .and_then(|ids| ids.into_iter().max())
        .unwrap_or(0);
    if state.next_monitor_id <= max_monitor_id || state.next_binding_id <= max_binding_id {
        return Err(store_unavailable());
    }
    Ok(())
}

pub(super) fn validate_accounts(state: &StoreState) -> Result<(), MonitorIssue> {
    for (account_id, account) in &state.accounts {
        if !valid_identifier(account_id) || account.sessions.len() > MAX_SESSIONS_PER_ACCOUNT {
            return Err(store_unavailable());
        }
        if account.input_sequence > state.next_input_sequence
            || !account_spend_matches_account(&account.spend, account_id)
            || account
                .spend
                .historical_correction_horizon_epoch
                .is_some_and(|epoch| epoch > state.last_now_epoch)
            || account
                .provider_observation
                .as_ref()
                .is_some_and(|observation| {
                    !provider_observation_is_valid(observation, state.last_now_epoch)
                        || !provider_observation_matches_account(state, account_id, observation)
                })
        {
            return Err(store_unavailable());
        }
        for barrier in account.reset_barriers.iter().flatten() {
            if !(9_500..=10_000).contains(&barrier.prior_used_percentage_basis_points)
                || barrier.started_at_epoch < 0
                || barrier.input_sequence_before > account.input_sequence
                || match barrier.source {
                    MonitorEvidenceSource::Statusline => {
                        barrier.session_id.as_deref().is_none_or(|session_id| {
                            !valid_identifier(session_id)
                                || !account.sessions.contains_key(session_id)
                        })
                    }
                    MonitorEvidenceSource::BrokerProjection => barrier.session_id.is_some(),
                    _ => true,
                }
            {
                return Err(store_unavailable());
            }
        }
        for session_id in account.sessions.keys() {
            if !valid_identifier(session_id) {
                return Err(store_unavailable());
            }
            let session = &account.sessions[session_id];
            if session
                .last_observation_received_at_epoch
                .is_some_and(|time| time < 0 || time > state.last_now_epoch)
                || session
                    .last_callback_received_at_epoch
                    .is_some_and(|time| time < 0 || time > state.last_now_epoch)
                || session
                    .claude_code_version
                    .as_deref()
                    .is_some_and(|version| !valid_bounded_text(version, 64))
            {
                return Err(store_unavailable());
            }
            if session
                .model
                .as_ref()
                .is_some_and(|model| model.input_sequence > account.input_sequence)
            {
                return Err(store_unavailable());
            }
            for window in &session.windows {
                if window
                    .used
                    .as_ref()
                    .is_some_and(|used| used.input_sequence > account.input_sequence)
                    || window
                        .reset
                        .as_ref()
                        .is_some_and(|reset| reset.input_sequence > account.input_sequence)
                {
                    return Err(store_unavailable());
                }
            }
        }
        for window in &account.broker_windows {
            if window
                .used
                .as_ref()
                .is_some_and(|used| used.input_sequence > account.input_sequence)
                || window
                    .reset
                    .as_ref()
                    .is_some_and(|reset| reset.input_sequence > account.input_sequence)
            {
                return Err(store_unavailable());
            }
        }
    }
    Ok(())
}

fn provider_observation_is_valid(observation: &ProviderObservation, last_now_epoch: i64) -> bool {
    let issue_code_is_supported = observation.issue_code.is_none_or(|code| {
        matches!(
            code,
            MonitorIssueCode::ProviderRateLimited
                | MonitorIssueCode::ProviderUnauthorized
                | MonitorIssueCode::ProviderTimeout
                | MonitorIssueCode::ProviderUnavailable
                | MonitorIssueCode::ProviderNeedsSecret
        )
    });
    valid_identifier(&observation.source_account_id)
        && valid_bounded_text(&observation.broker_instance_id, MAX_ID_LENGTH)
        && match (&observation.binding_id, observation.binding_revision) {
            (Some(binding_id), Some(revision)) => valid_identifier(binding_id) && revision > 0,
            (None, None) => true,
            _ => false,
        }
        && observation.last_good_at_epoch.is_none_or(|epoch| {
            epoch >= 0 && epoch <= last_now_epoch.saturating_add(MAX_FUTURE_SKEW_SECS)
        })
        && observation.retry_at_epoch.is_none_or(|epoch| epoch >= 0)
        && issue_code_is_supported
        && (observation.readiness == MonitorProviderReadiness::RateLimited)
            == (observation.issue_code == Some(MonitorIssueCode::ProviderRateLimited))
}

fn provider_observation_matches_account(
    state: &StoreState,
    local_account_id: &str,
    observation: &ProviderObservation,
) -> bool {
    let (Some(binding_id), Some(binding_revision)) = (
        observation.binding_id.as_deref(),
        observation.binding_revision,
    ) else {
        return false;
    };
    let Some(binding) = current_binding(state, binding_id) else {
        return false;
    };
    binding.revision == binding_revision
        && binding.account_id == local_account_id
        && binding.provider == jackin_protocol::usage_monitor::MonitorProvider::Claude
        && binding.operator_confirmed
        && binding.experimental_collector_approved
        && binding.provider_account_id.as_deref() == Some(observation.source_account_id.as_str())
}

pub(super) fn validate_unbound_sessions(state: &StoreState) -> Result<(), MonitorIssue> {
    for (session_id, session) in &state.unbound_sessions {
        if !valid_identifier(session_id)
            || session
                .last_observation_received_at_epoch
                .is_some_and(|time| time < 0 || time > state.last_now_epoch)
            || session
                .last_callback_received_at_epoch
                .is_some_and(|time| time < 0 || time > state.last_now_epoch)
            || session
                .claude_code_version
                .as_deref()
                .is_some_and(|version| !valid_bounded_text(version, 64))
            || session.model.as_ref().is_some_and(|model| {
                model.input_sequence > state.next_input_sequence
                    || !valid_bounded_text(&model.value, MAX_MODEL_LENGTH)
            })
            || session.windows.iter().any(|window| {
                window
                    .used
                    .as_ref()
                    .is_some_and(|used| used.input_sequence > state.next_input_sequence)
                    || window
                        .reset
                        .as_ref()
                        .is_some_and(|reset| reset.input_sequence > state.next_input_sequence)
            })
        {
            return Err(store_unavailable());
        }
    }
    Ok(())
}

pub(super) fn validate_bindings(state: &StoreState) -> Result<(), MonitorIssue> {
    for (binding_id, history) in &state.bindings {
        if parse_counter_id(binding_id, "binding-").is_none()
            || history.is_empty()
            || history.len() > MAX_BINDINGS
        {
            return Err(store_unavailable());
        }
        let mut previous_revision = 0;
        for binding in history {
            if binding.binding_id != *binding_id
                || !valid_identifier(&binding.account_id)
                || binding
                    .provider_account_id
                    .as_deref()
                    .is_some_and(|account_id| !valid_identifier(account_id))
                || (binding.experimental_collector_approved
                    && binding.provider_account_id.is_none())
                || !valid_bounded_text(&binding.operator_label, MAX_OPERATOR_LABEL_LENGTH)
                || binding.revision <= previous_revision
                || match (binding.operator_confirmed, binding.confirmed_at_epoch) {
                    (true, Some(time)) => time < 0 || time > state.last_now_epoch,
                    (false, None) => false,
                    _ => true,
                }
            {
                return Err(store_unavailable());
            }
            previous_revision = binding.revision;
        }
    }
    Ok(())
}

pub(super) fn validate_policy_records(state: &StoreState) -> Result<(), MonitorIssue> {
    for (goal_id, history) in &state.policy_records {
        if validate_goal_id(goal_id).is_err()
            || history.is_empty()
            || history.len() > MAX_POLICY_REVISIONS
        {
            return Err(store_unavailable());
        }
        let mut previous_revision = 0;
        let mut previous_policy = None;
        let mut previous_record: Option<&MonitorPolicyRecord> = None;
        for policy in history {
            if policy.goal_id != *goal_id
                || !valid_identifier(&policy.account_id)
                || policy.revision <= previous_revision
                || match policy.origin {
                    MonitorPolicyOrigin::Operator => policy
                        .recorded_at_epoch
                        .is_none_or(|time| time < 0 || time > state.last_now_epoch),
                    MonitorPolicyOrigin::MigratedV1 => policy.recorded_at_epoch.is_some(),
                }
                || policy.previous_policy != previous_policy
                || (policy.origin == MonitorPolicyOrigin::Operator
                    && validate_policy_input(
                        policy.new_policy,
                        policy.budget.as_ref(),
                        policy.acknowledge_no_sgd_cap,
                    )
                    .is_err())
                || previous_record
                    .is_some_and(|previous| !policy_transition_is_valid(previous, policy))
            {
                return Err(store_unavailable());
            }
            match policy.origin {
                MonitorPolicyOrigin::MigratedV1 => {
                    if policy.binding_id.is_some()
                        || policy.binding_revision.is_some()
                        || policy.operator_label.is_some()
                        || policy.operator_confirmed
                        || policy.acknowledge_no_sgd_cap
                        || policy.new_policy != MonitorPolicy::StrictSgd
                    {
                        return Err(store_unavailable());
                    }
                }
                MonitorPolicyOrigin::Operator => {
                    let Some(binding_id) = policy.binding_id.as_deref() else {
                        return Err(store_unavailable());
                    };
                    let Some(binding_revision) = policy.binding_revision else {
                        return Err(store_unavailable());
                    };
                    let binding_exists =
                        policy_binding_exists(state, policy, binding_id, binding_revision);
                    if !binding_exists
                        || !policy.operator_confirmed
                        || policy.operator_label.as_deref().is_none_or(|label| {
                            !valid_bounded_text(label, MAX_OPERATOR_LABEL_LENGTH)
                        })
                    {
                        return Err(store_unavailable());
                    }
                }
            }
            previous_revision = policy.revision;
            previous_policy = Some(policy.new_policy);
            previous_record = Some(policy);
        }
    }
    Ok(())
}

fn policy_transition_is_valid(previous: &MonitorPolicyRecord, next: &MonitorPolicyRecord) -> bool {
    if previous.provider != next.provider || previous.account_id != next.account_id {
        return false;
    }

    match (previous.new_policy, next.new_policy) {
        (MonitorPolicy::StrictSgd, MonitorPolicy::QuotaOnly) => false,
        (MonitorPolicy::StrictSgd, MonitorPolicy::StrictSgd) => {
            budget_is_same_or_tighter(previous.budget.as_ref(), next.budget.as_ref())
                || is_migrated_zero_sgd_budget_repair(previous)
        }
        _ => true,
    }
}

pub(super) fn policy_binding_exists(
    state: &StoreState,
    policy: &MonitorPolicyRecord,
    binding_id: &str,
    binding_revision: u64,
) -> bool {
    state.bindings.get(binding_id).is_some_and(|history| {
        history.iter().any(|binding| {
            binding.revision == binding_revision
                && binding.account_id == policy.account_id
                && binding.provider == policy.provider
                && binding.operator_confirmed
        })
    })
}

fn validate_monitors_with_fingerprint_version(
    state: &StoreState,
    allow_legacy_fingerprints: bool,
) -> Result<(), MonitorIssue> {
    let mut idempotency_keys = BTreeSet::<&str>::new();
    for (monitor_id, monitor) in &state.monitors {
        let expected_goal = monitor.config.goal_id.as_deref();
        let mut evidence_sequences = BTreeSet::new();
        if parse_counter_id(monitor_id, "monitor-").is_none()
            || monitor.evidence.len() > MAX_EVIDENCE_PER_MONITOR
            || monitor.evidence_fingerprints.len() > MAX_EVIDENCE_FINGERPRINTS
            || monitor.evidence_fingerprints.iter().any(|(key, value)| {
                !(if allow_legacy_fingerprints {
                    valid_legacy_evidence_fingerprint(key, value)
                } else {
                    valid_evidence_fingerprint(key, value)
                })
            })
            || monitor.events.len() > MAX_EVENTS_PER_MONITOR
            || monitor.events.len() as u64 > monitor.next_event_sequence
            || monitor.next_evidence_sequence == u64::MAX
            || monitor.next_decision_sequence == u64::MAX
            || monitor.next_event_sequence == u64::MAX
            || validate_config(&monitor.config).is_err()
            || !valid_bounded_text(&monitor.idempotency_key, MAX_IDEMPOTENCY_KEY_LENGTH)
            || monitor.created_at_epoch < 0
            || monitor.last_reconciled_at_epoch < monitor.created_at_epoch
            || monitor.last_reconciled_at_epoch > state.last_now_epoch
            || monitor
                .stopped_at_epoch
                .is_some_and(|time| time < monitor.created_at_epoch || time > state.last_now_epoch)
            || match (&monitor.config.scope, monitor.account_id.as_deref()) {
                (MonitorScope::Session { .. }, None) => {
                    monitor.config.purpose != MonitorPurpose::ObserveOnly
                        || monitor.policy.is_some()
                        || monitor.spend_state.is_some()
                }
                (
                    MonitorScope::BoundAccount {
                        binding_id,
                        binding_revision,
                        ..
                    },
                    Some(account_id),
                ) => !state.bindings.get(binding_id).is_some_and(|history| {
                    history.iter().any(|binding| {
                        binding.revision == *binding_revision
                            && binding.account_id == account_id
                            && binding.provider == monitor.config.provider
                    })
                }),
                _ => true,
            }
            || (monitor.config.purpose == MonitorPurpose::ObserveOnly
                && (expected_goal.is_some()
                    || monitor.config.policy_revision.is_some()
                    || monitor.policy.is_some()
                    || monitor.spend_state.is_some()))
            || (monitor.config.purpose == MonitorPurpose::DispatchGuard
                && monitor.policy.as_ref().is_none_or(|policy| {
                    Some(policy.goal_id.as_str()) != expected_goal
                        || Some(policy.revision) != monitor.config.policy_revision
                        || !state
                            .policy_records
                            .get(&policy.goal_id)
                            .is_some_and(|history| history.iter().any(|record| record == policy))
                }))
            || monitor.policy.as_ref().is_some_and(|policy| {
                policy.new_policy == MonitorPolicy::StrictSgd && monitor.spend_state.is_none()
            })
            || monitor.spend_state.as_ref().is_some_and(|spend_state| {
                monitor
                    .account_id
                    .as_deref()
                    .is_none_or(|account_id| !spend_state_matches_account(spend_state, account_id))
            })
            || monitor.evidence.iter().any(|evidence| {
                evidence.sequence == 0
                    || evidence.sequence > monitor.next_evidence_sequence
                    || !evidence_sequences.insert(evidence.sequence)
                    || !evidence_is_relevant(evidence, monitor)
                    || evidence.evidence_received_at_epoch < 0
                    || evidence.evidence_received_at_epoch > state.last_now_epoch
            })
            || monitor
                .events
                .windows(2)
                .any(|events| events[0].sequence >= events[1].sequence)
            || monitor.latest_decision.as_ref().is_some_and(|decision| {
                !monitor_decision_is_valid(
                    decision,
                    monitor.next_evidence_sequence,
                    monitor.next_decision_sequence,
                    state.last_now_epoch,
                )
            })
            || monitor.events.iter().any(|event| {
                event.sequence == 0
                    || event.sequence > monitor.next_event_sequence
                    || event.status.schema_version != USAGE_MONITOR_SCHEMA_VERSION
                    || (event.status.runnable
                        != (event.status.readiness.dispatch == MonitorDispatchReadiness::Ready))
                    || event.status.monitor_id != *monitor_id
                    || event.status.provider != monitor.config.provider
                    || event.status.purpose != monitor.config.purpose
                    || event.status.scope != monitor.config.scope
                    || event.status.account_id != monitor.account_id
                    || event.status.goal_id != monitor.config.goal_id
                    || event.status.expected_model != monitor.config.expected_model
                    || !monitor_status_evidence_is_valid(
                        &event.status.evidence,
                        monitor.next_evidence_sequence,
                    )
                    || event
                        .status
                        .latest_decision
                        .as_ref()
                        .is_some_and(|decision| {
                            !monitor_decision_is_valid(
                                decision,
                                monitor.next_evidence_sequence,
                                monitor.next_decision_sequence,
                                state.last_now_epoch,
                            )
                        })
                    || event.occurred_at_epoch < monitor.created_at_epoch
                    || event.occurred_at_epoch > state.last_now_epoch
            })
        {
            return Err(store_unavailable());
        }
        if !idempotency_keys.insert(&monitor.idempotency_key) {
            return Err(store_unavailable());
        }
    }
    Ok(())
}

pub(super) fn validate_goals(state: &StoreState) -> Result<(), MonitorIssue> {
    if state.goals.len() > MAX_GOALS {
        return Err(store_unavailable());
    }
    for (goal_id, goal) in &state.goals {
        if validate_goal_id(goal_id).is_err()
            || !valid_identifier(&goal.account_id)
            || !state.bindings.get(&goal.binding_id).is_some_and(|history| {
                history.iter().any(|binding| {
                    binding.revision == goal.binding_revision
                        && binding.account_id == goal.account_id
                })
            })
            || current_policy(state, goal_id).is_none_or(|policy| {
                policy.account_id != goal.account_id
                    || policy.revision != goal.policy_revision
                    || policy.new_policy != goal.policy
                    || policy.budget != goal.budget
            })
            || goal.spend_state.as_ref().is_some_and(|spend_state| {
                !spend_state_matches_account(spend_state, &goal.account_id)
            })
            || (goal.policy == MonitorPolicy::StrictSgd && goal.spend_state.is_none())
            || (goal.policy == MonitorPolicy::QuotaOnly && goal.spend_state.is_some())
        {
            return Err(store_unavailable());
        }
        for monitor in state
            .monitors
            .values()
            .filter(|monitor| monitor.config.goal_id.as_deref() == Some(goal_id))
        {
            let Some(snapshot) = monitor.spend_state.as_ref() else {
                continue;
            };
            let Some(canonical) = goal.spend_state.as_ref() else {
                return Err(store_unavailable());
            };
            if !spend_snapshot_preserves_history(snapshot, canonical)
                || (monitor.config.policy_revision == Some(goal.policy_revision)
                    && snapshot != canonical)
            {
                return Err(store_unavailable());
            }
        }
    }
    Ok(())
}

pub(super) fn monitor_decision_is_valid(
    decision: &MonitorDecision,
    next_evidence_sequence: u64,
    next_decision_sequence: u64,
    last_now_epoch: i64,
) -> bool {
    let mut evidence_sequences = BTreeSet::new();
    decision.sequence > 0
        && decision.sequence <= next_decision_sequence
        && decision.decided_at_epoch >= 0
        && decision.decided_at_epoch <= last_now_epoch
        && decision.evidence_sequences.iter().all(|sequence| {
            *sequence > 0
                && *sequence <= next_evidence_sequence
                && evidence_sequences.insert(*sequence)
        })
}

pub(super) fn monitor_status_evidence_is_valid(
    evidence: &[MonitorEvidence],
    next_sequence: u64,
) -> bool {
    if evidence.len() > MAX_EVIDENCE_PER_MONITOR {
        return false;
    }
    let mut sequences = BTreeSet::new();
    evidence.iter().all(|item| {
        item.sequence > 0 && item.sequence <= next_sequence && sequences.insert(item.sequence)
    })
}

pub(super) fn spend_snapshot_preserves_history(
    snapshot: &SpendState,
    canonical: &SpendState,
) -> bool {
    if snapshot.baseline != canonical.baseline
        || (!snapshot.cumulative_complete && canonical.cumulative_complete)
    {
        return false;
    }
    let closed_period_history_preserved = match (
        snapshot.closed_period_anchor.as_ref(),
        canonical.closed_period_anchor.as_ref(),
    ) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(snapshot), Some(canonical)) => {
            if snapshot.billing_period_start_epoch == canonical.billing_period_start_epoch
                && snapshot.billing_period_end_epoch == canonical.billing_period_end_epoch
            {
                canonical.amount.currency == snapshot.amount.currency
                    && canonical.amount.exponent == snapshot.amount.exponent
                    && canonical.amount.amount_minor >= snapshot.amount.amount_minor
            } else {
                canonical.billing_period_start_epoch >= snapshot.billing_period_end_epoch
            }
        }
    };
    if !closed_period_history_preserved {
        return false;
    }
    let Some(snapshot_cumulative) = snapshot.cumulative_goal_spend.as_ref() else {
        return true;
    };
    canonical
        .cumulative_goal_spend
        .as_ref()
        .is_some_and(|canonical_cumulative| {
            canonical_cumulative.currency == snapshot_cumulative.currency
                && canonical_cumulative.exponent == snapshot_cumulative.exponent
                && canonical_cumulative.amount_minor >= snapshot_cumulative.amount_minor
        })
}

pub(super) fn account_spend_matches_account(state: &SpendAccountState, account_id: &str) -> bool {
    spend::validate_account_spend_state(state)
        && [
            state.latest_record.as_ref(),
            state.current_period_record.as_ref(),
            state.previous_period_record.as_ref(),
        ]
        .into_iter()
        .flatten()
        .all(|record| record.account_id == account_id)
}

pub(super) fn spend_state_matches_account(state: &SpendState, account_id: &str) -> bool {
    spend::validate_spend_state(state)
        && state
            .baseline
            .as_ref()
            .is_none_or(|record| record.account_id == account_id)
        && state
            .period_anchor
            .as_ref()
            .is_none_or(|record| record.account_id == account_id)
        && state
            .closed_period_anchor
            .as_ref()
            .is_none_or(|record| record.account_id == account_id)
}

pub(super) fn validate_config(config: &MonitorConfig) -> Result<(), MonitorIssue> {
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
                "the experimental collector is available only to observation-only monitors",
                None,
            ));
        }
        if config.provider != jackin_protocol::usage_monitor::MonitorProvider::Claude
            || !matches!(&config.scope, MonitorScope::BoundAccount { .. })
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

pub(super) fn validate_goal_id(value: &str) -> Result<(), MonitorIssue> {
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

pub(super) fn current_binding<'a>(
    state: &'a StoreState,
    binding_id: &str,
) -> Option<&'a MonitorAccountBinding> {
    state
        .bindings
        .get(binding_id)
        .and_then(|history| history.last())
}

pub(super) fn scope_session_id(scope: &MonitorScope) -> Option<&str> {
    match scope {
        MonitorScope::Session { session_id } => Some(session_id),
        MonitorScope::BoundAccount { session_id, .. } => session_id.as_deref(),
    }
}

pub(super) fn resolve_scope_binding<'a>(
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

pub(super) fn current_policy<'a>(
    state: &'a StoreState,
    goal_id: &str,
) -> Option<&'a MonitorPolicyRecord> {
    state
        .policy_records
        .get(goal_id)
        .and_then(|history| history.last())
}

#[cfg(test)]
#[path = "validation/policy_history_tests.rs"]
mod policy_history_tests;

pub(super) fn validate_policy_input(
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

pub(super) fn invalid_operator_label() -> MonitorIssue {
    issue(
        MonitorIssueCode::StatuslineInvalid,
        "operator label is empty or outside its accepted bounds",
        None,
    )
}

pub(super) fn operator_confirmation_required() -> MonitorIssue {
    issue(
        MonitorIssueCode::OperatorConfirmationRequired,
        "the operation requires explicit operator confirmation",
        None,
    )
}

pub(super) fn binding_required() -> MonitorIssue {
    issue(
        MonitorIssueCode::BindingRequired,
        "dispatch guards require a separately confirmed account binding",
        None,
    )
}

pub(super) fn binding_mismatch() -> MonitorIssue {
    issue(
        MonitorIssueCode::BindingMismatch,
        "binding identifier, provider, account, or revision does not match",
        None,
    )
}

pub(super) fn policy_required() -> MonitorIssue {
    issue(
        MonitorIssueCode::PolicyRequired,
        "dispatch guards require an explicit current policy approval",
        None,
    )
}

pub(super) fn policy_conflict() -> MonitorIssue {
    issue(
        MonitorIssueCode::PolicyConflict,
        "policy revision conflicts with the activated goal or approved budget",
        None,
    )
}

pub(super) fn validate_observation(
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

pub(super) fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_LENGTH
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

pub(super) fn valid_evidence_fingerprint(key: &str, value: &str) -> bool {
    if !valid_bounded_text(key, MAX_ID_LENGTH + 64) || !valid_bounded_text(value, 2_048) {
        return false;
    }
    if key == "spend:account" {
        return true;
    }
    if let Some(session_id) = key.strip_prefix("model:") {
        return valid_identifier(session_id);
    }
    let Some((source, rest)) = key.split_once(':') else {
        return false;
    };
    let Some((field, rest)) = rest.split_once(':') else {
        return false;
    };
    let Some((window, scope)) = rest.split_once(':') else {
        return false;
    };
    valid_fingerprint_source(source)
        && matches!(field, "used" | "reset")
        && matches!(window, "five_hour" | "seven_day")
        && (scope == "account" || scope.strip_prefix("session:").is_some_and(valid_identifier))
}

pub(super) fn valid_legacy_evidence_fingerprint(key: &str, value: &str) -> bool {
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

fn valid_fingerprint_source(source: &str) -> bool {
    matches!(
        source,
        "broker_projection" | "statusline" | "provider_spend" | "operator" | "local_session_log"
    )
}

pub(super) fn parse_counter_id(value: &str, prefix: &str) -> Option<u64> {
    let suffix = value.strip_prefix(prefix)?;
    if !(8..=20).contains(&suffix.len()) || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let parsed = suffix.parse::<u64>().ok()?;
    (parsed > 0).then_some(parsed)
}

pub(super) fn validate_identifier(value: &str) -> Result<(), MonitorIssue> {
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

pub(super) fn valid_bounded_text(value: &str, max_len: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max_len
        && value.is_ascii()
        && !value.chars().any(char::is_control)
}
