// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::evidence::max_option;
use super::quota::{
    evidence_is_relevant, model_status, observed_reset_for_used, quota_window_status,
    update_reset_barrier,
};
use super::spend::evaluate_spend_policy;
use super::status::{decision_fingerprint, push_action, push_issue, quota_window_issues};
use super::{
    AccountObservations, DECISION_MAX_PARALLEL, DurableGoalSpend, DurableMonitor,
    MONITOR_RESET_GRACE_SECS, MonitorAccountBinding, MonitorAction, MonitorDecision,
    MonitorEvaluation, MonitorEvidenceFreshness, MonitorIssue, MonitorIssueCode, MonitorLifecycle,
    MonitorPolicy, MonitorPolicyOrigin, MonitorPolicyRecord, MonitorPurpose, MonitorQuotaWindow,
    MonitorQuotaWindowStatus, MonitorScope, ResetBarrier, SessionObservation, SpendAccountState,
    SpendDecision, SpendState, issue,
};
use std::collections::BTreeMap;

pub(super) struct MonitorEvaluationContext<'a> {
    pub(super) accounts: &'a BTreeMap<String, AccountObservations>,
    pub(super) unbound_sessions: &'a BTreeMap<String, SessionObservation>,
    pub(super) current_binding: Option<&'a MonitorAccountBinding>,
    pub(super) current_policy: Option<&'a MonitorPolicyRecord>,
    pub(super) goal: Option<&'a DurableGoalSpend>,
}

pub(super) fn evaluate_monitor(
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
                    "model evidence is unavailable or its session context has expired",
                    None,
                ),
            );
        }
        if model_mismatch {
            push_issue(
                &mut evaluation.issues,
                issue(
                    MonitorIssueCode::ModelMismatch,
                    "statusline model does not match the configured model guard",
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

pub(super) fn append_dispatch_authorization_issues(
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

pub(super) fn evaluate_quota_windows(
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

pub(super) fn evaluate_quota_window(
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

pub(super) fn append_quota_freshness_issues(
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

pub(super) fn append_quota_usage_actions(
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

pub(super) fn quota_pause_reason(used: i32) -> MonitorIssueCode {
    if used >= 10_000 {
        MonitorIssueCode::LimitExhausted
    } else {
        MonitorIssueCode::LimitGuardReached
    }
}

pub(super) fn latch_monitor_reset_barrier(
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

pub(super) fn append_quota_barrier_actions(
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

pub(super) fn append_account_barrier_actions(
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

pub(super) fn evaluate_model_guard(
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
                "model evidence is unavailable or its session context has expired",
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
                "statusline model does not match the configured model guard",
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

pub(super) fn evaluate_spend_guard(
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

pub(super) fn spend_decision_blocks_dispatch(spend: &SpendDecision) -> bool {
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

pub(super) fn spend_issue(code: MonitorIssueCode) -> MonitorIssue {
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

pub(super) fn append_unknown_pause_actions(
    monitor: &DurableMonitor,
    evaluation: &mut MonitorEvaluation,
) {
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

pub(super) fn spend_policy(
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
