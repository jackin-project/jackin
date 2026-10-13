// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Account entries and catalog transactions.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Duration;

use std::sync::{Arc, Condvar, Mutex};

use super::policy::{CLAUDE_MIN_ATTEMPT_INTERVAL, UsageActivity};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationError, UsageGenerationView,
};

use super::{
    AccountStateEnvelope, AccountStateStore, ClockSample, TERMINAL_HISTORY_LIMIT,
    UsageCoordinatorConfig, UsageProviderExecutor, account_cooldown_deadline, generation_view,
    reconcile_executor_catalog, restore_catalog_preimages, state_error, unavailable_error,
};

/// In-memory periodic cadence for one account. Due times are scheduling
/// hints only; shared retry/rate-limit/success deadlines always win.
#[derive(Clone)]
pub(crate) struct AccountCadence {
    pub(crate) activity: UsageActivity,
    pub(crate) low_power: bool,
    pub(crate) next_due_epoch: i64,
}

#[derive(Clone)]
pub(crate) struct AccountEntry {
    pub(crate) envelope: AccountStateEnvelope,
    pub(crate) history: VecDeque<UsageGenerationView>,
    pub(crate) recovery_pending: bool,
    pub(crate) cadence: AccountCadence,
    pub(crate) catalog_revision: Option<String>,
    pub(crate) revoked: bool,
    /// In-process monotonic guard for Claude deadlines, plus the conservative
    /// attempt floor installed when loading a prior invocation after restart.
    pub(crate) cooldown_not_before_monotonic: Option<Duration>,
    /// A provider dispatch reserved by a generation that outlived its catalog
    /// revision. Re-added capabilities stay blocked until the work completes
    /// and its completion-anchored cooldown is persisted.
    pub(crate) pending_provider_generation: Option<u64>,
    /// Generations fenced by a catalog revision change. Keeping this separate
    /// from terminal history makes an in-flight join wake and fail
    /// immediately even when the capability id itself is unchanged.
    pub(crate) fenced_generations: BTreeSet<u64>,
}

impl AccountEntry {
    pub(crate) fn new(
        envelope: AccountStateEnvelope,
        recovery_pending: bool,
        now_epoch: i64,
        clock_sample: ClockSample,
        catalog_revision: Option<String>,
    ) -> Self {
        let mut history = VecDeque::new();
        if envelope.phase.is_terminal() {
            history.push_back(generation_view(&envelope));
        }
        let next_due_epoch = account_cooldown_deadline(&envelope)
            .filter(|deadline| *deadline > now_epoch)
            .unwrap_or(now_epoch);
        let cooldown_not_before_monotonic = [
            runtime_cooldown_deadline(&envelope, clock_sample),
            recovered_attempt_deadline(&envelope, clock_sample),
        ]
        .into_iter()
        .flatten()
        .max();
        Self {
            envelope,
            history,
            recovery_pending,
            cadence: AccountCadence {
                activity: UsageActivity::Idle,
                low_power: false,
                next_due_epoch,
            },
            catalog_revision,
            revoked: false,
            cooldown_not_before_monotonic,
            pending_provider_generation: None,
            fenced_generations: BTreeSet::new(),
        }
    }

    pub(crate) fn record_terminal(&mut self) {
        self.history.push_back(generation_view(&self.envelope));
        while self.history.len() > TERMINAL_HISTORY_LIMIT {
            drop(self.history.pop_front());
        }
    }

    pub(crate) fn refresh_runtime_cooldown(&mut self, clock_sample: ClockSample) {
        self.cooldown_not_before_monotonic = [
            self.cooldown_not_before_monotonic,
            runtime_cooldown_deadline(&self.envelope, clock_sample),
        ]
        .into_iter()
        .flatten()
        .max();
    }
}

fn recovered_attempt_deadline(
    envelope: &AccountStateEnvelope,
    clock_sample: ClockSample,
) -> Option<Duration> {
    // Monotonic origins do not survive process restarts. If a provider
    // invocation was durably reserved, conservatively treat it as recent for
    // one full interval after recovery, even when its epoch deadline appears
    // expired. Queued-only records have no invocation marker and are excluded.
    if envelope.capability.surface_id != "claude" || envelope.provider_invoked_at_epoch.is_none() {
        return None;
    }
    Some(
        clock_sample
            .monotonic
            .saturating_add(CLAUDE_MIN_ATTEMPT_INTERVAL),
    )
}

/// Wall-clock counterpart for a persisted invocation whose account entry has
/// not been loaded into this process's monotonic clock domain yet.
pub(crate) fn recovered_attempt_deadline_epoch(
    envelope: &AccountStateEnvelope,
    clock_sample: ClockSample,
) -> Option<i64> {
    if envelope.capability.surface_id != "claude" || envelope.provider_invoked_at_epoch.is_none() {
        return None;
    }
    Some(
        ClockSample {
            wall_epoch: clock_sample
                .wall_epoch
                .saturating_add(CLAUDE_MIN_ATTEMPT_INTERVAL),
            monotonic: clock_sample.monotonic,
        }
        .ceil_epoch(),
    )
}

fn runtime_cooldown_deadline(
    envelope: &AccountStateEnvelope,
    clock_sample: ClockSample,
) -> Option<Duration> {
    if envelope.capability.surface_id != "claude" {
        return None;
    }
    let deadline = account_cooldown_deadline(envelope)?;
    let deadline_wall = Duration::from_secs(u64::try_from(deadline.max(0)).unwrap_or(u64::MAX));
    let remaining = deadline_wall.saturating_sub(clock_sample.wall_epoch);
    if remaining.is_zero() {
        return None;
    }
    Some(clock_sample.monotonic.saturating_add(remaining))
}

/// Earliest scheduler wake that honors cadence, persisted cooldowns, and the
/// live monotonic cooldown mirror. The monotonic deadline is projected through
/// the paired clock sample so schedulers can wait for an in-memory recovery
/// floor without dispatching a request that admission must suppress.
pub(crate) fn effective_due_epoch(entry: &AccountEntry, clock_sample: ClockSample) -> i64 {
    [
        Some(entry.cadence.next_due_epoch),
        account_cooldown_deadline(&entry.envelope),
        monotonic_cooldown_deadline_epoch(entry, clock_sample),
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or(entry.cadence.next_due_epoch)
}

/// Project an active in-memory Claude cooldown into the paired wall-clock
/// domain, rounding up so persisting it cannot shorten the monotonic guard.
pub(crate) fn monotonic_cooldown_deadline_epoch(
    entry: &AccountEntry,
    clock_sample: ClockSample,
) -> Option<i64> {
    entry
        .cooldown_not_before_monotonic
        .filter(|deadline| *deadline > clock_sample.monotonic)
        .map(|deadline| {
            ClockSample {
                wall_epoch: clock_sample
                    .wall_epoch
                    .saturating_add(deadline.saturating_sub(clock_sample.monotonic)),
                monotonic: clock_sample.monotonic,
            }
            .ceil_epoch()
        })
}

#[derive(Clone, Default)]
pub(crate) struct CoordinatorState {
    pub(crate) accounts: BTreeMap<UsageAccountCapability, AccountEntry>,
    pub(crate) blocked: BTreeMap<UsageAccountCapability, UsageCoordinationError>,
    pub(crate) catalog: Option<BTreeMap<UsageAccountCapability, String>>,
    pub(crate) catalog_revision: Option<String>,
}

pub(crate) struct Shared {
    pub(crate) state: Mutex<CoordinatorState>,
    /// Catalog replacement is a transaction boundary. Executor bindings and
    /// in-memory revision fencing must never observe two rotations interleaved.
    pub(crate) catalog_lifecycle: Mutex<()>,
    pub(crate) changed: Condvar,
    pub(crate) executor: Arc<dyn UsageProviderExecutor>,
    pub(crate) store: Arc<dyn AccountStateStore>,
    pub(crate) config: UsageCoordinatorConfig,
    pub(crate) clock: Arc<dyn super::MonotonicClock>,
    #[cfg(test)]
    pub(crate) before_provider_call_hook: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

#[derive(Clone)]
pub(crate) enum CatalogAccountPreimage {
    Missing,
    Present(Box<AccountStateEnvelope>),
    /// The old bytes were unreadable. Rotation quarantines them instead of
    /// treating one revoked account as a catalog-wide failure.
    Corrupt,
}

/// Coordinator-side catalog commit whose durable projection can still reject
/// the catalog and restore the exact pre-rotation state.
pub struct CatalogTransaction {
    pub(crate) shared: Arc<Shared>,
    pub(crate) previous_state: CoordinatorState,
    pub(crate) preimages: BTreeMap<UsageAccountCapability, CatalogAccountPreimage>,
    pub(crate) previous_entries: Vec<UsageCatalogEntry>,
    pub(crate) previous_catalog_revision: Option<String>,
}

impl std::fmt::Debug for CatalogTransaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CatalogTransaction").finish_non_exhaustive()
    }
}

impl CatalogTransaction {
    /// Restore coordinator memory, account state, and executor bindings.
    /// Corrupt preimages remain in quarantine by design; they are not safe to
    /// put back into the active account namespace.
    pub fn rollback(self, now_epoch: i64) -> Result<(), UsageCoordinationError> {
        let _catalog_lifecycle = self
            .shared
            .catalog_lifecycle
            .lock()
            .map_err(|_| unavailable_error())?;
        let mut state = self.shared.state.lock().map_err(|_| unavailable_error())?;
        let durable = restore_catalog_preimages(
            &self.shared,
            &self.preimages,
            self.preimages.keys().cloned().collect(),
            now_epoch,
        )
        .map_err(state_error);
        let executor = reconcile_executor_catalog(
            &self.shared,
            self.previous_catalog_revision.as_deref(),
            &self.previous_entries,
        );
        *state = self.previous_state;
        self.shared.changed.notify_all();
        match (durable, executor) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), _) => Err(error),
            (_, Err(error)) => Err(error),
        }
    }
}
