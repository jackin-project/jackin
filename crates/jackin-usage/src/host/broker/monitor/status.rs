// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::evaluation::{
    quota_pause_reason, spend_decision_blocks_dispatch, spend_issue, spend_policy,
};
use super::quota::{evidence_is_relevant, field_age, model_status, quota_window_status};
use super::validation::{current_binding, current_policy};
use super::*;

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

pub(super) fn status_session_id(
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

pub(super) fn status_lifecycle(
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

pub(super) struct MonitorReadinessContext<'a> {
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

pub(super) fn monitor_readiness(context: MonitorReadinessContext<'_>) -> MonitorReadiness {
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

pub(super) fn quota_readiness(
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

pub(super) fn monitor_issues(
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
                    "model evidence is unavailable or its session context has expired",
                    None,
                ),
            );
        }
        if mismatch {
            push_issue(
                &mut issues,
                issue(
                    MonitorIssueCode::ModelMismatch,
                    "statusline model does not match the configured model guard",
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

pub(super) fn quota_window_issues(
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

pub(super) fn decision_fingerprint(
    actions: &[MonitorAction],
    issues: &[MonitorIssue],
    lifecycle: MonitorLifecycle,
    runnable: bool,
) -> String {
    let issue_codes = issues.iter().map(|item| item.code).collect::<Vec<_>>();
    serde_json::to_string(&(actions, issue_codes, lifecycle, runnable)).unwrap_or_default()
}

pub(super) fn push_action(actions: &mut Vec<MonitorAction>, action: MonitorAction) {
    if !actions.contains(&action) {
        actions.push(action);
    }
}

pub(super) fn push_issue(issues: &mut Vec<MonitorIssue>, issue: MonitorIssue) {
    if !issues.iter().any(|current| current.code == issue.code) {
        issues.push(issue);
    }
}
