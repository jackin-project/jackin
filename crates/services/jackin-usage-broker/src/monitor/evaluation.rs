// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

pub(super) fn refresh_all_goal_spend(state: &mut StoreState, now_epoch: i64) {
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

pub(super) fn reconcile_all_monitors(state: &mut StoreState, now_epoch: i64) -> bool {
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

pub(super) fn upsert_evidence(monitor: &mut DurableMonitor, input: EvidenceInput<'_>) {
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

pub(super) fn quota_pause_reason(used: i32) -> MonitorIssueCode {
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

pub(super) fn quota_window_status(
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

pub(super) fn evidence_is_relevant(evidence: &MonitorEvidence, monitor: &DurableMonitor) -> bool {
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

pub(super) fn field_age(
    evidence_at_epoch: Option<i64>,
    received_at_epoch: i64,
    now_epoch: i64,
) -> u64 {
    let nonnegative_age =
        |timestamp| u64::try_from(now_epoch.saturating_sub(timestamp).max(0)).unwrap_or(u64::MAX);
    let received_age = nonnegative_age(received_at_epoch);
    evidence_at_epoch.map_or(received_age, |time| received_age.max(nonnegative_age(time)))
}

pub(super) fn is_current(evidence: &MonitorEvidence, now_epoch: i64) -> bool {
    field_evidence(evidence, now_epoch).freshness == MonitorEvidenceFreshness::Current
}

pub(super) fn model_status(
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
