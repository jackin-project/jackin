// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Entry lifecycle and cadence.

use super::policy::UsageActivity;
use jackin_protocol::control::{FocusedUsageView, UsageSnapshotStatus};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageGenerationView, UsageRefreshPhase,
};

use super::{
    AccountEntry, AccountStateEnvelope, ClockSample, CoordinatorState, TERMINAL_HISTORY_LIMIT,
    catalog_revoked_error, policy, unavailable_error,
};

pub(crate) fn revoke_entry(entry: &mut AccountEntry, now_epoch: i64, clock_sample: ClockSample) {
    entry.fenced_generations.insert(entry.envelope.generation);
    revoke_envelope(&mut entry.envelope, now_epoch, clock_sample);
    entry.refresh_runtime_cooldown(clock_sample);
    entry.history.clear();
    entry.recovery_pending = false;
    entry.catalog_revision = None;
    entry.revoked = true;
    entry.record_terminal();
}

/// Fence a stored generation while retaining account-level cooldown inputs.
/// Materialized provider results are cleared because a removed capability may
/// later be re-added after its credentials or authorization changed.
pub(crate) fn revoke_envelope(
    envelope: &mut AccountStateEnvelope,
    now_epoch: i64,
    clock_sample: ClockSample,
) {
    if envelope.phase == UsageRefreshPhase::Updating
        && let Some(floor) =
            policy::minimum_attempt_deadline(&envelope.capability, Some(clock_sample.ceil_epoch()))
    {
        envelope.retry_deadline_epoch = Some(
            envelope
                .retry_deadline_epoch
                .map_or(floor, |deadline| deadline.max(floor)),
        );
    }
    envelope.generation = envelope.generation.saturating_add(1);
    envelope.phase = UsageRefreshPhase::Failed;
    envelope.terminal_result = None;
    envelope.last_good = None;
    envelope.terminal_error = Some(catalog_revoked_error());
    envelope.started_at_epoch = None;
    envelope.completed_at_epoch = Some(now_epoch);
}

/// Minimal durable projection for a removed account. A pending dispatch stays
/// in the existing `Updating` phase until terminal completion, even after its
/// current cooldown expires, so repeated catalog rotations cannot erase the
/// restart recovery marker. Provider results and errors are always removed.
pub(crate) fn cooldown_tombstone(
    envelope: &AccountStateEnvelope,
    now_epoch: i64,
    pending_attempt: bool,
) -> Option<AccountStateEnvelope> {
    if !pending_attempt {
        account_cooldown_deadline(envelope).filter(|deadline| *deadline > now_epoch)?;
    }
    let mut tombstone = if pending_attempt {
        pending_attempt_envelope(envelope, now_epoch)
    } else {
        envelope.clone()
    };
    if !pending_attempt {
        tombstone.phase = UsageRefreshPhase::Idle;
        tombstone.started_at_epoch = None;
    }
    tombstone.terminal_result = None;
    tombstone.last_good = None;
    tombstone.terminal_error = None;
    tombstone.completed_at_epoch = None;
    Some(tombstone)
}

/// Persist a result-free active marker until a dispatch fenced from the
/// current catalog reaches its terminal boundary.
pub(crate) fn pending_attempt_envelope(
    envelope: &AccountStateEnvelope,
    now_epoch: i64,
) -> AccountStateEnvelope {
    let mut pending = envelope.clone();
    pending.phase = UsageRefreshPhase::Updating;
    pending.terminal_result = None;
    pending.last_good = None;
    pending.terminal_error = None;
    pending.started_at_epoch = pending.provider_invoked_at_epoch.or(Some(now_epoch));
    pending.completed_at_epoch = None;
    pending
}

pub(crate) fn reset_entry(entry: &mut AccountEntry, now_epoch: i64, revision: String) {
    entry.fenced_generations.insert(entry.envelope.generation);
    reset_envelope(&mut entry.envelope);
    entry.history.clear();
    while entry.fenced_generations.len() > TERMINAL_HISTORY_LIMIT {
        let Some(oldest) = entry.fenced_generations.iter().next().copied() else {
            break;
        };
        entry.fenced_generations.remove(&oldest);
    }
    entry.recovery_pending = false;
    entry.cadence.next_due_epoch = account_cooldown_deadline(&entry.envelope)
        .filter(|deadline| *deadline > now_epoch)
        .unwrap_or(now_epoch);
    entry.catalog_revision = Some(revision);
    entry.revoked = false;
}

/// Reset materialized results while retaining provider cooldowns and the
/// invocation timestamp that still governs the same canonical account.
pub(crate) fn reset_envelope(envelope: &mut AccountStateEnvelope) {
    envelope.generation = envelope.generation.saturating_add(1);
    envelope.phase = UsageRefreshPhase::Idle;
    envelope.terminal_result = None;
    envelope.last_good = None;
    envelope.terminal_error = None;
    envelope.completed_at_epoch = None;
}

/// Latest durable refresh deadline for one account, including Claude's hard
/// attempt floor. Periodic cadence remains an in-memory scheduling hint.
pub(crate) fn account_cooldown_deadline(envelope: &AccountStateEnvelope) -> Option<i64> {
    [
        envelope.rate_limit_deadline_epoch,
        envelope.retry_deadline_epoch,
        envelope.success_deadline_epoch,
        policy::minimum_attempt_deadline(&envelope.capability, envelope.provider_invoked_at_epoch),
    ]
    .into_iter()
    .flatten()
    .max()
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
