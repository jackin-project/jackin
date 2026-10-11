// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Probe completion.

use std::sync::Arc;

use jackin_protocol::control::FocusedUsageView;
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageCoordinationErrorKind, UsageRefreshPhase,
};

use super::state::sanitize_usage_view;
use super::{
    ClockSample, CoordinatorState, ProbeJob, Shared, cooldown_tombstone, coordination_error,
    policy, unavailable_error,
};

pub(crate) fn mark_updating(shared: &Arc<Shared>, job: &ProbeJob) -> Option<i64> {
    let Ok(_catalog_lifecycle) = shared.catalog_lifecycle.lock() else {
        return None;
    };
    let Ok(mut state) = shared.state.lock() else {
        return None;
    };
    let entry = state.accounts.get_mut(&job.capability)?;
    if entry.revoked
        || entry.catalog_revision != job.catalog_revision
        || entry.envelope.generation != job.generation
        || entry.envelope.phase != UsageRefreshPhase::Queued
    {
        return None;
    }
    let previous_invocation = entry.envelope.provider_invoked_at_epoch;
    let previous_reload_fence = entry.envelope.reload_fence_required;
    let queue_wait = shared.clock.now().saturating_sub(job.admitted_at_monotonic);
    let provider_invoked_at_epoch = job
        .admitted_at_epoch
        .saturating_add(i64::try_from(queue_wait.as_secs()).unwrap_or(i64::MAX));
    entry.envelope.phase = UsageRefreshPhase::Updating;
    entry.envelope.provider_invoked_at_epoch = Some(provider_invoked_at_epoch);
    entry.envelope.reload_fence_required = false;
    if shared
        .store
        .store(&entry.envelope, provider_invoked_at_epoch)
        .is_err()
    {
        entry.envelope.provider_invoked_at_epoch = previous_invocation;
        entry.envelope.reload_fence_required = previous_reload_fence;
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
            shared.clock.sample(provider_invoked_at_epoch),
            false,
        );
        return None;
    }
    entry.pending_provider_generation = Some(job.generation);
    shared.changed.notify_all();
    Some(provider_invoked_at_epoch)
}

pub(crate) fn finish_success(
    shared: &Arc<Shared>,
    job: &ProbeJob,
    view: FocusedUsageView,
    finished_at: ClockSample,
) {
    let Ok(_catalog_lifecycle) = shared.catalog_lifecycle.lock() else {
        return;
    };
    let Ok(mut state) = shared.state.lock() else {
        return;
    };
    let Some(entry) = state.accounts.get(&job.capability) else {
        return;
    };
    if entry.revoked
        || entry.catalog_revision != job.catalog_revision
        || entry.envelope.generation != job.generation
        || !entry.envelope.phase.is_active()
    {
        finish_fenced_provider_attempt(shared, &mut state, job, finished_at, None);
        return;
    }
    let Some(entry) = state.accounts.get_mut(&job.capability) else {
        return;
    };
    let finished_at_epoch = finished_at.ceil_epoch();
    let view = sanitize_usage_view(view);
    entry.envelope.phase = UsageRefreshPhase::Completed;
    entry.envelope.terminal_result = Some(view.clone());
    entry.envelope.last_good = Some(view);
    entry.envelope.terminal_error = None;
    entry.envelope.completed_at_epoch = Some(finished_at_epoch);
    entry.envelope.rate_limit_deadline_epoch = None;
    entry.envelope.retry_deadline_epoch = None;
    let success_deadline = finished_at_epoch.saturating_add(
        i64::try_from(shared.config.success_cooldown.as_secs()).unwrap_or(i64::MAX),
    );
    let completion_floor =
        policy::minimum_attempt_deadline(&job.capability, Some(finished_at_epoch));
    entry.envelope.success_deadline_epoch =
        Some(completion_floor.map_or(success_deadline, |deadline| deadline.max(success_deadline)));
    entry.envelope.consecutive_failures = 0;
    entry.pending_provider_generation = None;
    entry.refresh_runtime_cooldown(finished_at);
    persist_terminal(shared, &mut state, &job.capability, finished_at_epoch);
}

pub(crate) fn finish_failure(
    shared: &Arc<Shared>,
    job: &ProbeJob,
    kind: UsageCoordinationErrorKind,
    message: &str,
    retry_at_epoch: Option<i64>,
    finished_at: ClockSample,
    attempted_provider: bool,
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
        finished_at,
        attempted_provider,
    );
}

/// Resolve a failure inside the caller's existing catalog/state transaction.
pub(crate) fn finish_failure_in_state(
    shared: &Arc<Shared>,
    state: &mut CoordinatorState,
    job: &ProbeJob,
    error: UsageCoordinationError,
    retry_at_epoch: Option<i64>,
    finished_at: ClockSample,
    attempted_provider: bool,
) {
    let Some(entry) = state.accounts.get(&job.capability) else {
        return;
    };
    if entry.revoked
        || entry.catalog_revision != job.catalog_revision
        || entry.envelope.generation != job.generation
        || !entry.envelope.phase.is_active()
    {
        if attempted_provider {
            finish_fenced_provider_attempt(
                shared,
                state,
                job,
                finished_at,
                Some((error.kind, retry_at_epoch)),
            );
        }
        return;
    }
    let Some(entry) = state.accounts.get_mut(&job.capability) else {
        return;
    };
    let finished_at_epoch = finished_at.ceil_epoch();
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
    let retry_at_epoch = match policy::minimum_attempt_deadline(
        &job.capability,
        entry.envelope.provider_invoked_at_epoch,
    ) {
        Some(floor) => Some(retry_at_epoch.map_or(floor, |deadline| deadline.max(floor))),
        None => retry_at_epoch,
    };
    let retry_at_epoch = if attempted_provider {
        match policy::minimum_attempt_deadline(&job.capability, Some(finished_at_epoch)) {
            Some(floor) => Some(retry_at_epoch.map_or(floor, |deadline| deadline.max(floor))),
            None => retry_at_epoch,
        }
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
    entry.pending_provider_generation = None;
    entry.refresh_runtime_cooldown(finished_at);
    persist_terminal(shared, state, &job.capability, finished_at_epoch);
}

/// Finish a provider call fenced by catalog removal or revision change. The
/// data and error remain discarded, but its cooldown starts at completion.
fn finish_fenced_provider_attempt(
    shared: &Arc<Shared>,
    state: &mut CoordinatorState,
    job: &ProbeJob,
    finished_at: ClockSample,
    provider_failure: Option<(UsageCoordinationErrorKind, Option<i64>)>,
) {
    let Some(entry) = state.accounts.get_mut(&job.capability) else {
        return;
    };
    if entry.pending_provider_generation != Some(job.generation) {
        return;
    }
    entry.pending_provider_generation = None;
    let finished_at_epoch = finished_at.ceil_epoch();
    if let Some(floor) = policy::minimum_attempt_deadline(&job.capability, Some(finished_at_epoch))
    {
        entry.envelope.retry_deadline_epoch = Some(
            entry
                .envelope
                .retry_deadline_epoch
                .map_or(floor, |deadline| deadline.max(floor)),
        );
    }
    if let Some((kind, provider_deadline)) = provider_failure {
        let failures = entry.envelope.consecutive_failures.saturating_add(1);
        let retry = if policy::is_retryable(kind) {
            policy::retry_deadline(
                shared.config.retry_policy,
                &job.capability,
                job.generation,
                failures,
                provider_deadline,
                finished_at_epoch,
            )
        } else {
            provider_deadline
        };
        if let Some(retry) = retry {
            entry.envelope.retry_deadline_epoch = Some(
                entry
                    .envelope
                    .retry_deadline_epoch
                    .map_or(retry, |deadline| deadline.max(retry)),
            );
            if kind == UsageCoordinationErrorKind::RateLimited {
                entry.envelope.rate_limit_deadline_epoch = Some(
                    entry
                        .envelope
                        .rate_limit_deadline_epoch
                        .map_or(retry, |deadline| deadline.max(retry)),
                );
            }
        }
        if policy::is_retryable(kind) {
            entry.envelope.consecutive_failures = failures;
        }
    } else {
        let success_deadline = finished_at_epoch.saturating_add(
            i64::try_from(shared.config.success_cooldown.as_secs()).unwrap_or(i64::MAX),
        );
        let success_deadline =
            policy::minimum_attempt_deadline(&job.capability, Some(finished_at_epoch))
                .map_or(success_deadline, |floor| floor.max(success_deadline));
        entry.envelope.success_deadline_epoch = Some(
            entry
                .envelope
                .success_deadline_epoch
                .map_or(success_deadline, |deadline| deadline.max(success_deadline)),
        );
    }
    entry.refresh_runtime_cooldown(finished_at);

    let persist_epoch =
        finished_at_epoch.max(entry.envelope.provider_invoked_at_epoch.unwrap_or(0));
    let envelope = if entry.revoked {
        cooldown_tombstone(&entry.envelope, persist_epoch, false)
    } else {
        Some(entry.envelope.clone())
    };
    let persisted = envelope.as_ref().map_or(Ok(()), |envelope| {
        shared.store.store(envelope, persist_epoch)
    });
    if persisted.is_err() {
        state
            .blocked
            .insert(job.capability.clone(), unavailable_error());
    }
    shared.changed.notify_all();
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
