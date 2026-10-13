// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Probe completion.

use std::sync::Arc;

use jackin_protocol::control::FocusedUsageView;
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageCoordinationErrorKind, UsageRefreshPhase,
};

use super::state::sanitize_usage_view;
use super::{CoordinatorState, ProbeJob, Shared, coordination_error, policy, unavailable_error};

pub(crate) fn mark_updating(shared: &Arc<Shared>, job: &ProbeJob) -> bool {
    let Ok(_catalog_lifecycle) = shared.catalog_lifecycle.lock() else {
        return false;
    };
    let Ok(mut state) = shared.state.lock() else {
        return false;
    };
    let Some(entry) = state.accounts.get_mut(&job.capability) else {
        return false;
    };
    if entry.revoked
        || entry.catalog_revision != job.catalog_revision
        || entry.envelope.generation != job.generation
        || entry.envelope.phase != UsageRefreshPhase::Queued
    {
        return false;
    }
    entry.envelope.phase = UsageRefreshPhase::Updating;
    if shared
        .store
        .store(&entry.envelope, job.started_at_epoch)
        .is_err()
    {
        // Retain the catalog transaction while resolving the owned generation;
        // reacquiring its lifecycle mutex here would deadlock this worker.
        finish_failure_in_state(
            shared,
            &mut state,
            job,
            coordination_error(
                UsageCoordinationErrorKind::Unavailable,
                "usage state store is unavailable",
            ),
            None,
            job.started_at_epoch,
        );
        return false;
    }
    shared.changed.notify_all();
    true
}

pub(crate) fn finish_success(
    shared: &Arc<Shared>,
    job: &ProbeJob,
    view: FocusedUsageView,
    finished_at_epoch: i64,
) {
    let Ok(_catalog_lifecycle) = shared.catalog_lifecycle.lock() else {
        return;
    };
    let Ok(mut state) = shared.state.lock() else {
        return;
    };
    let Some(entry) = state.accounts.get_mut(&job.capability) else {
        return;
    };
    if entry.revoked
        || entry.catalog_revision != job.catalog_revision
        || entry.envelope.generation != job.generation
        || !entry.envelope.phase.is_active()
    {
        return;
    }
    let view = sanitize_usage_view(view);
    entry.envelope.phase = UsageRefreshPhase::Completed;
    entry.envelope.terminal_result = Some(view.clone());
    entry.envelope.last_good = Some(view);
    entry.envelope.terminal_error = None;
    entry.envelope.completed_at_epoch = Some(finished_at_epoch);
    entry.envelope.rate_limit_deadline_epoch = None;
    entry.envelope.retry_deadline_epoch = None;
    entry.envelope.success_deadline_epoch = Some(finished_at_epoch.saturating_add(
        i64::try_from(shared.config.success_cooldown.as_secs()).unwrap_or(i64::MAX),
    ));
    entry.envelope.consecutive_failures = 0;
    persist_terminal(shared, &mut state, &job.capability, finished_at_epoch);
}

pub(crate) fn finish_failure(
    shared: &Arc<Shared>,
    job: &ProbeJob,
    kind: UsageCoordinationErrorKind,
    message: &str,
    retry_at_epoch: Option<i64>,
    finished_at_epoch: i64,
) {
    let Ok(_catalog_lifecycle) = shared.catalog_lifecycle.lock() else {
        return;
    };
    let Ok(mut state) = shared.state.lock() else {
        return;
    };
    finish_failure_in_state(
        shared,
        &mut state,
        job,
        coordination_error(kind, message),
        retry_at_epoch,
        finished_at_epoch,
    );
}

/// Resolve a failure inside the caller's existing catalog/state transaction.
pub(crate) fn finish_failure_in_state(
    shared: &Arc<Shared>,
    state: &mut CoordinatorState,
    job: &ProbeJob,
    error: UsageCoordinationError,
    retry_at_epoch: Option<i64>,
    finished_at_epoch: i64,
) {
    let Some(entry) = state.accounts.get_mut(&job.capability) else {
        return;
    };
    if entry.revoked
        || entry.catalog_revision != job.catalog_revision
        || entry.envelope.generation != job.generation
        || !entry.envelope.phase.is_active()
    {
        return;
    }
    let kind = error.kind;
    entry.envelope.phase = UsageRefreshPhase::Failed;
    entry.envelope.terminal_result = None;
    entry.envelope.terminal_error = Some(error);
    entry.envelope.completed_at_epoch = Some(finished_at_epoch);
    let consecutive_failures = entry.envelope.consecutive_failures.saturating_add(1);
    let retry_at_epoch = if policy::is_retryable(kind) {
        policy::retry_deadline(
            shared.config.retry_policy,
            &job.capability,
            job.generation,
            consecutive_failures,
            retry_at_epoch,
            finished_at_epoch,
        )
    } else {
        retry_at_epoch
    };
    entry.envelope.retry_deadline_epoch = retry_at_epoch;
    if kind == UsageCoordinationErrorKind::RateLimited {
        entry.envelope.rate_limit_deadline_epoch = retry_at_epoch;
    } else {
        entry.envelope.rate_limit_deadline_epoch = None;
    }
    entry.envelope.success_deadline_epoch = None;
    entry.envelope.consecutive_failures = consecutive_failures;
    persist_terminal(shared, state, &job.capability, finished_at_epoch);
}

pub(crate) fn persist_terminal(
    shared: &Arc<Shared>,
    state: &mut CoordinatorState,
    capability: &UsageAccountCapability,
    now_epoch: i64,
) {
    let Some(entry) = state.accounts.get_mut(capability) else {
        return;
    };
    if shared.store.store(&entry.envelope, now_epoch).is_err() {
        state
            .blocked
            .insert(capability.clone(), unavailable_error());
    } else {
        entry.record_terminal();
    }
    shared.changed.notify_all();
}
