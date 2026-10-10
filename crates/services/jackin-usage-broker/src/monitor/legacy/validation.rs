// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

pub(super) fn validate_v1(state: &V1StoreState) -> Result<(), MonitorIssue> {
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
