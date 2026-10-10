// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

pub(super) fn validate_store_state(state: &StoreState) -> Result<(), MonitorIssue> {
    validate_store_header(state)?;
    validate_accounts(state)?;
    validate_unbound_sessions(state)?;
    validate_bindings(state)?;
    validate_policy_records(state)?;
    validate_monitors(state)?;
    validate_goals(state)
}

fn validate_store_header(state: &StoreState) -> Result<(), MonitorIssue> {
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

fn validate_accounts(state: &StoreState) -> Result<(), MonitorIssue> {
    for (account_id, account) in &state.accounts {
        if !valid_identifier(account_id) || account.sessions.len() > MAX_SESSIONS_PER_ACCOUNT {
            return Err(store_unavailable());
        }
        if account.input_sequence > state.next_input_sequence
            || !account_spend_matches_account(&account.spend, account_id)
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

fn validate_unbound_sessions(state: &StoreState) -> Result<(), MonitorIssue> {
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

fn validate_bindings(state: &StoreState) -> Result<(), MonitorIssue> {
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
                    .is_some_and(|source_id| !valid_source_capability_id(source_id))
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

fn validate_policy_records(state: &StoreState) -> Result<(), MonitorIssue> {
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

fn policy_binding_exists(
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

fn validate_monitors(state: &StoreState) -> Result<(), MonitorIssue> {
    let mut idempotency_keys = BTreeSet::<&str>::new();
    for (monitor_id, monitor) in &state.monitors {
        let expected_goal = monitor.config.goal_id.as_deref();
        let mut evidence_sequences = BTreeSet::new();
        if parse_counter_id(monitor_id, "monitor-").is_none()
            || monitor.evidence.len() > MAX_EVIDENCE_PER_MONITOR
            || monitor.evidence_fingerprints.len() > MAX_EVIDENCE_FINGERPRINTS
            || monitor
                .evidence_fingerprints
                .iter()
                .any(|(key, value)| !valid_evidence_fingerprint(key, value))
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

fn validate_goals(state: &StoreState) -> Result<(), MonitorIssue> {
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

fn monitor_decision_is_valid(
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

fn monitor_status_evidence_is_valid(evidence: &[MonitorEvidence], next_sequence: u64) -> bool {
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

fn account_spend_matches_account(state: &SpendAccountState, account_id: &str) -> bool {
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
