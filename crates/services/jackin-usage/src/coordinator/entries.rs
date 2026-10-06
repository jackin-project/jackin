// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Account entries and catalog transactions.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use std::sync::{Arc, Condvar, Mutex};

use super::policy::UsageActivity;
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationError, UsageGenerationView,
};

use super::{
    AccountStateEnvelope, AccountStateStore, TERMINAL_HISTORY_LIMIT, UsageCoordinatorConfig,
    UsageProviderExecutor, generation_view, reconcile_executor_catalog, restore_catalog_preimages,
    state_error, unavailable_error,
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
        catalog_revision: Option<String>,
    ) -> Self {
        let mut history = VecDeque::new();
        if envelope.phase.is_terminal() {
            history.push_back(generation_view(&envelope));
        }
        Self {
            envelope,
            history,
            recovery_pending,
            cadence: AccountCadence {
                activity: UsageActivity::Idle,
                low_power: false,
                next_due_epoch: now_epoch,
            },
            catalog_revision,
            revoked: false,
            fenced_generations: BTreeSet::new(),
        }
    }

    pub(crate) fn record_terminal(&mut self) {
        self.history.push_back(generation_view(&self.envelope));
        while self.history.len() > TERMINAL_HISTORY_LIMIT {
            drop(self.history.pop_front());
        }
    }
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
pub(crate) struct CatalogTransaction {
    pub(crate) shared: Arc<Shared>,
    pub(crate) previous_state: CoordinatorState,
    pub(crate) preimages: BTreeMap<UsageAccountCapability, CatalogAccountPreimage>,
    pub(crate) previous_entries: Vec<UsageCatalogEntry>,
    pub(crate) previous_catalog_revision: Option<String>,
}

impl CatalogTransaction {
    /// Restore coordinator memory, account state, and executor bindings.
    /// Corrupt preimages remain in quarantine by design; they are not safe to
    /// put back into the active account namespace.
    pub(crate) fn rollback(self, now_epoch: i64) -> Result<(), UsageCoordinationError> {
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
