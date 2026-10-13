// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Entry lifecycle and cadence.

use super::policy::UsageActivity;
use jackin_protocol::control::{FocusedUsageView, UsageSnapshotStatus};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageGenerationView, UsageRefreshPhase,
};

use super::{
    AccountEntry, AccountStateEnvelope, CoordinatorState, TERMINAL_HISTORY_LIMIT,
    catalog_revoked_error, policy, unavailable_error,
};

pub(crate) fn revoke_entry(entry: &mut AccountEntry, now_epoch: i64) {
    entry.fenced_generations.insert(entry.envelope.generation);
    entry.envelope.generation = entry.envelope.generation.saturating_add(1);
    entry.envelope.phase = UsageRefreshPhase::Failed;
    entry.envelope.terminal_result = None;
    entry.envelope.terminal_error = Some(catalog_revoked_error());
    entry.envelope.started_at_epoch = None;
    entry.envelope.completed_at_epoch = Some(now_epoch);
    entry.envelope.rate_limit_deadline_epoch = None;
    entry.envelope.retry_deadline_epoch = None;
    entry.envelope.success_deadline_epoch = None;
    entry.recovery_pending = false;
    entry.catalog_revision = None;
    entry.revoked = true;
    entry.record_terminal();
}

pub(crate) fn reset_entry(entry: &mut AccountEntry, now_epoch: i64, revision: String) {
    entry.fenced_generations.insert(entry.envelope.generation);
    entry.envelope.generation = entry.envelope.generation.saturating_add(1);
    entry.envelope.phase = UsageRefreshPhase::Idle;
    entry.envelope.terminal_result = None;
    entry.envelope.last_good = None;
    entry.envelope.terminal_error = None;
    entry.envelope.started_at_epoch = None;
    entry.envelope.completed_at_epoch = None;
    entry.envelope.rate_limit_deadline_epoch = None;
    entry.envelope.retry_deadline_epoch = None;
    entry.envelope.success_deadline_epoch = None;
    entry.envelope.consecutive_failures = 0;
    entry.history.clear();
    while entry.fenced_generations.len() > TERMINAL_HISTORY_LIMIT {
        let Some(oldest) = entry.fenced_generations.iter().next().copied() else {
            break;
        };
        entry.fenced_generations.remove(&oldest);
    }
    entry.recovery_pending = false;
    entry.cadence.next_due_epoch = now_epoch;
    entry.catalog_revision = Some(revision);
    entry.revoked = false;
}

pub(crate) fn data_bearing(view: &FocusedUsageView) -> bool {
    if view.status == UsageSnapshotStatus::Unsupported {
        return true;
    }
    !view.buckets.is_empty() && view.status == UsageSnapshotStatus::Fresh
}

/// Jittered periodic deadline: tier cadence plus a deterministic
/// `[0, cadence/4]` skew, so accounts spread out instead of polling in
/// lockstep. The capability and generation seed it, so joined callers never
/// derive different due times.
pub(crate) fn cadence_deadline(
    activity: UsageActivity,
    low_power: bool,
    capability: &UsageAccountCapability,
    generation: u64,
    from_epoch: i64,
) -> i64 {
    let base = policy::cadence(activity, low_power).as_secs();
    let span = base / 4 + 1;
    let jitter = cadence_jitter_seed(capability, generation) % span;
    from_epoch.saturating_add(i64::try_from(base.saturating_add(jitter)).unwrap_or(i64::MAX))
}

pub(crate) fn cadence_jitter_seed(capability: &UsageAccountCapability, generation: u64) -> u64 {
    let mut seed = 0xcbf2_9ce4_8422_2325u64;
    for byte in capability
        .account_id
        .as_bytes()
        .iter()
        .chain(capability.surface_id.as_bytes())
        .chain(generation.to_le_bytes().iter())
    {
        seed ^= u64::from(*byte);
        seed = seed.wrapping_mul(0x0100_0000_01b3);
    }
    seed
}

pub(crate) fn record_blocked_terminal(
    state: &mut CoordinatorState,
    capability: &UsageAccountCapability,
    envelope: &AccountStateEnvelope,
) -> Result<(), UsageCoordinationError> {
    if !envelope.phase.is_terminal() {
        return Ok(());
    }
    let Some(entry) = state.accounts.get_mut(capability) else {
        return Err(unavailable_error());
    };
    if entry
        .history
        .iter()
        .any(|view| view.generation == envelope.generation)
    {
        return Ok(());
    }
    entry.record_terminal();
    Ok(())
}

pub(crate) fn generation_view(envelope: &AccountStateEnvelope) -> UsageGenerationView {
    UsageGenerationView {
        capability: envelope.capability.clone(),
        generation: envelope.generation,
        phase: envelope.phase,
        snapshot: envelope
            .terminal_result
            .clone()
            .or_else(|| envelope.last_good.clone()),
        error: envelope.terminal_error.clone(),
        retry_at_epoch: [
            envelope.rate_limit_deadline_epoch,
            envelope.retry_deadline_epoch,
        ]
        .into_iter()
        .flatten()
        .max(),
    }
}
