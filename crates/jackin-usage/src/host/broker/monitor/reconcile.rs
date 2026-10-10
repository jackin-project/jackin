// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::evaluation::{MonitorEvaluationContext, evaluate_monitor};
use super::evidence::{
    advance_account_reset_barriers, sync_monitor_from_account, sync_monitor_from_session,
};
use super::status::append_event_if_changed;
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

pub(super) fn budget_is_same_or_tighter(previous: Option<&Money>, next: Option<&Money>) -> bool {
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

pub(super) fn is_migrated_zero_sgd_budget_repair(policy: &MonitorPolicyRecord) -> bool {
    policy.origin == MonitorPolicyOrigin::MigratedV1
        && policy.new_policy == MonitorPolicy::StrictSgd
        && policy.budget.as_ref().is_some_and(|budget| {
            budget.currency == "SGD" && budget.exponent == 2 && budget.amount_minor == 0
        })
}
