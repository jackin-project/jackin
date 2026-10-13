// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

pub(super) fn session_is_active(session: &SessionObservation, now_epoch: i64) -> bool {
    session
        .last_observation_received_at_epoch
        .is_some_and(|received| {
            received <= now_epoch && now_epoch.saturating_sub(received) <= MONITOR_EVIDENCE_TTL_SECS
        })
}

pub(super) fn prune_inactive_sessions(
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

pub(super) fn prune_inactive_unbound_sessions(
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

pub(super) fn update_reset_barrier(
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

pub(super) fn append_event_if_changed(
    state: &mut StoreState,
    monitor_id: &str,
    now_epoch: i64,
) -> bool {
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

pub(super) fn append_event(monitor: &mut DurableMonitor, status: MonitorStatus, now_epoch: i64) {
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

pub(super) fn status_for(
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
