// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Per-account single-flight refresh generations owned by the host broker.

pub mod policy;
mod state;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use jackin_protocol::control::{FocusedUsageView, UsageSnapshotStatus};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageGenerationView, UsageRefreshPhase,
};

pub use state::{
    AccountStateEnvelope, AccountStateStore, FileAccountStateStore, FileProjectionStateStore,
    ProjectionAlias, ProjectionStateEnvelope, StateStoreError,
};

use self::policy::UsageActivity;
use self::state::sanitize_usage_view;

const TERMINAL_HISTORY_LIMIT: usize = 8;

/// Result returned by a host-owned provider adapter.
#[derive(Debug, Clone)]
pub enum ProviderProbeOutcome {
    /// Data-bearing provider result.
    Success(Box<FocusedUsageView>),
    /// Typed failure that preserves last-good quota.
    Failure {
        /// Stable failure kind.
        kind: UsageCoordinationErrorKind,
        /// Sanitized operator-facing message.
        message: String,
        /// Provider-supplied retry deadline, when present.
        retry_at_epoch: Option<i64>,
    },
}

impl ProviderProbeOutcome {
    /// Wrap one data-bearing provider result without exposing wire-size details.
    #[must_use]
    pub fn success(view: FocusedUsageView) -> Self {
        Self::Success(Box::new(view))
    }
}

/// Configurable provider execution port.
pub trait UsageProviderExecutor: Send + Sync {
    /// Execute one canonical account probe. Implementations own bounded network
    /// timeouts; the coordinator retains generation ownership until this call
    /// actually returns.
    fn probe(&self, capability: &UsageAccountCapability, generation: u64) -> ProviderProbeOutcome;

    /// Reconcile provider bindings before a new catalog revision can start
    /// work. A failed reconciliation does not admit the new catalog.
    fn reconcile_catalog(
        &self,
        _entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        Ok(())
    }

    /// Reconcile provider bindings against a complete caller catalog. The
    /// default keeps older in-process executors source-compatible; broker
    /// executors that can rediscover credentials should compare both values.
    fn reconcile_catalog_revision(
        &self,
        _catalog_revision: &str,
        entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        self.reconcile_catalog(entries)
    }

    /// Validate a catalog without changing provider bindings.
    ///
    /// The publisher calls this before touching coordinator durable state.
    /// Implementations that need external discovery or binding allocation can
    /// reject here and retain the previous catalog unchanged.
    fn validate_catalog(
        &self,
        _entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        Ok(())
    }

    /// Validate a complete caller catalog before durable or in-memory
    /// mutation. The default delegates to the legacy entry-only hook.
    fn validate_catalog_revision(
        &self,
        _catalog_revision: &str,
        entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        self.validate_catalog(entries)
    }
}

/// Coordinator scheduling policy.
#[derive(Debug, Clone, Copy)]
pub struct UsageCoordinatorConfig {
    /// Maximum number of distinct account probes that may run concurrently.
    pub max_concurrency: usize,
    /// Ambient success cooldown.
    pub success_cooldown: Duration,
    /// Probe deadline used for terminal classification after the worker returns.
    pub provider_timeout: Duration,
    /// Maximum queued account generations.
    pub queue_capacity: usize,
    /// Broker-owned adaptive retry/jitter policy.
    pub retry_policy: policy::UsagePolicy,
}

impl Default for UsageCoordinatorConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 4,
            success_cooldown: Duration::from_mins(5),
            provider_timeout: Duration::from_secs(30),
            queue_capacity: 256,
            retry_policy: policy::UsagePolicy::default(),
        }
    }
}

/// Exact capability allowlist used by a per-container relay.
#[derive(Debug, Clone, Default)]
pub struct UsageCapabilitySet {
    allowed: BTreeSet<UsageAccountCapability>,
}

impl UsageCapabilitySet {
    /// Build an immutable exact-account allowlist.
    #[must_use]
    pub fn new(capabilities: impl IntoIterator<Item = UsageAccountCapability>) -> Self {
        Self {
            allowed: capabilities.into_iter().collect(),
        }
    }

    /// Reject an account absent from the launch-derived capability set.
    pub fn authorize(
        &self,
        capability: &UsageAccountCapability,
    ) -> Result<(), UsageCoordinationError> {
        if self.allowed.contains(capability) {
            Ok(())
        } else {
            Err(coordination_error(
                UsageCoordinationErrorKind::Unauthorized,
                "usage account capability is not authorized",
            ))
        }
    }

    /// Resolve a provider surface only when this scope authorizes exactly one
    /// canonical account. Missing and ambiguous mappings are equally denied.
    pub fn resolve_surface(
        &self,
        surface_id: &str,
    ) -> Result<UsageAccountCapability, UsageCoordinationError> {
        let mut matches = self
            .allowed
            .iter()
            .filter(|capability| capability.surface_id == surface_id);
        let Some(capability) = matches.next() else {
            return Err(coordination_error(
                UsageCoordinationErrorKind::Unauthorized,
                "usage provider surface is not authorized",
            ));
        };
        if matches.next().is_some() {
            return Err(coordination_error(
                UsageCoordinationErrorKind::Unauthorized,
                "usage provider surface is not uniquely authorized",
            ));
        }
        Ok(capability.clone())
    }

    /// Number of authorized canonical accounts.
    #[must_use]
    pub fn len(&self) -> usize {
        self.allowed.len()
    }

    /// Whether no account capability is authorized.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.allowed.is_empty()
    }
}

/// In-memory periodic cadence for one account. Due times are scheduling
/// hints only; shared retry/rate-limit/success deadlines always win.
#[derive(Clone)]
struct AccountCadence {
    activity: UsageActivity,
    low_power: bool,
    next_due_epoch: i64,
}

#[derive(Clone)]
struct AccountEntry {
    envelope: AccountStateEnvelope,
    history: VecDeque<UsageGenerationView>,
    recovery_pending: bool,
    cadence: AccountCadence,
    catalog_revision: Option<String>,
    revoked: bool,
    /// Generations fenced by a catalog revision change. Keeping this separate
    /// from terminal history makes an in-flight join wake and fail
    /// immediately even when the capability id itself is unchanged.
    fenced_generations: BTreeSet<u64>,
}

impl AccountEntry {
    fn new(
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

    fn record_terminal(&mut self) {
        self.history.push_back(generation_view(&self.envelope));
        while self.history.len() > TERMINAL_HISTORY_LIMIT {
            drop(self.history.pop_front());
        }
    }
}

#[derive(Clone, Default)]
struct CoordinatorState {
    accounts: BTreeMap<UsageAccountCapability, AccountEntry>,
    blocked: BTreeMap<UsageAccountCapability, UsageCoordinationError>,
    catalog: Option<BTreeMap<UsageAccountCapability, String>>,
    catalog_revision: Option<String>,
}

struct Shared {
    state: Mutex<CoordinatorState>,
    /// Catalog replacement is a transaction boundary. Executor bindings and
    /// in-memory revision fencing must never observe two rotations interleaved.
    catalog_lifecycle: Mutex<()>,
    changed: Condvar,
    executor: Arc<dyn UsageProviderExecutor>,
    store: Arc<dyn AccountStateStore>,
    config: UsageCoordinatorConfig,
}

#[derive(Clone)]
enum CatalogAccountPreimage {
    Missing,
    Present(Box<AccountStateEnvelope>),
    /// The old bytes were unreadable. Rotation quarantines them instead of
    /// treating one revoked account as a catalog-wide failure.
    Corrupt,
}

/// Coordinator-side catalog commit whose durable projection can still reject
/// the catalog and restore the exact pre-rotation state.
pub(crate) struct CatalogTransaction {
    shared: Arc<Shared>,
    previous_state: CoordinatorState,
    preimages: BTreeMap<UsageAccountCapability, CatalogAccountPreimage>,
    previous_entries: Vec<UsageCatalogEntry>,
    previous_catalog_revision: Option<String>,
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

#[derive(Debug)]
struct ProbeJob {
    capability: UsageAccountCapability,
    generation: u64,
    started_at_epoch: i64,
    catalog_revision: Option<String>,
}

enum WorkerMessage {
    Probe(ProbeJob),
    Shutdown,
}

/// Host-authoritative refresh coordinator.
pub struct UsageCoordinator {
    shared: Arc<Shared>,
    jobs: SyncSender<WorkerMessage>,
    workers: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for UsageCoordinator {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UsageCoordinator")
            .field("max_concurrency", &self.shared.config.max_concurrency)
            .field("queue_capacity", &self.shared.config.queue_capacity)
            .finish_non_exhaustive()
    }
}

impl UsageCoordinator {
    /// Start a bounded coordinator worker pool.
    #[must_use]
    pub fn new(
        executor: Arc<dyn UsageProviderExecutor>,
        store: Arc<dyn AccountStateStore>,
        config: UsageCoordinatorConfig,
    ) -> Self {
        Self::start(executor, store, config, None, None)
    }

    /// Start a coordinator with the catalog from the last durable broker
    /// publication. The broker reconciles it with current discovery before
    /// admitting new work.
    #[must_use]
    pub fn with_catalog(
        executor: Arc<dyn UsageProviderExecutor>,
        store: Arc<dyn AccountStateStore>,
        config: UsageCoordinatorConfig,
        catalog: impl IntoIterator<Item = UsageCatalogEntry>,
    ) -> Self {
        let catalog = catalog
            .into_iter()
            .map(|entry| (entry.capability, entry.revision))
            .collect();
        Self::start(executor, store, config, Some(catalog), None)
    }

    /// Start a coordinator with the exact revision persisted beside the
    /// catalog. Broker recovery uses this so executor rollback is fenced by
    /// the same caller/service revision as the forward rotation.
    #[must_use]
    pub(crate) fn with_catalog_revision(
        executor: Arc<dyn UsageProviderExecutor>,
        store: Arc<dyn AccountStateStore>,
        config: UsageCoordinatorConfig,
        catalog: impl IntoIterator<Item = UsageCatalogEntry>,
        catalog_revision: String,
    ) -> Self {
        let catalog = catalog
            .into_iter()
            .map(|entry| (entry.capability, entry.revision))
            .collect();
        Self::start(
            executor,
            store,
            config,
            Some(catalog),
            Some(catalog_revision),
        )
    }

    fn start(
        executor: Arc<dyn UsageProviderExecutor>,
        store: Arc<dyn AccountStateStore>,
        config: UsageCoordinatorConfig,
        catalog: Option<BTreeMap<UsageAccountCapability, String>>,
        catalog_revision: Option<String>,
    ) -> Self {
        let config = UsageCoordinatorConfig {
            max_concurrency: config.max_concurrency.max(1),
            queue_capacity: config.queue_capacity.max(1),
            ..config
        };
        let shared = Arc::new(Shared {
            state: Mutex::new(CoordinatorState {
                catalog,
                catalog_revision,
                ..CoordinatorState::default()
            }),
            catalog_lifecycle: Mutex::new(()),
            changed: Condvar::new(),
            executor,
            store,
            config,
        });
        let (jobs, receiver) = mpsc::sync_channel(config.queue_capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(config.max_concurrency);
        for index in 0..config.max_concurrency {
            let worker_shared = Arc::clone(&shared);
            let worker_receiver = Arc::clone(&receiver);
            let name = format!("usage-coordinator-{index}");
            match jackin_telemetry::spawn::thread_joined_named(name, move || {
                coordinator_worker(&worker_shared, &worker_receiver);
            }) {
                Ok(worker) => workers.push(worker),
                Err(_) => break,
            }
        }
        Self {
            shared,
            jobs,
            workers,
        }
    }

    /// Reconcile the live broker catalog with the last durable catalog.
    /// Removed capabilities are fenced in memory and purged from durable
    /// account state. Revision changes fence old work and discard its
    /// materialized result before the capability can refresh again.
    pub fn reconcile_catalog(
        &self,
        entries: impl IntoIterator<Item = UsageCatalogEntry>,
        now_epoch: i64,
    ) -> Result<(), UsageCoordinationError> {
        self.reconcile_catalog_transaction_with_revision(None, entries, now_epoch)
            .map(|_| ())
    }

    /// Apply one catalog rotation with the caller's content-derived catalog
    /// revision. Broker processes use this stronger boundary so executor
    /// discovery can reject a caller/service catalog mismatch before state is
    /// changed.
    pub(crate) fn reconcile_catalog_transaction_with_revision(
        &self,
        catalog_revision: Option<&str>,
        entries: impl IntoIterator<Item = UsageCatalogEntry>,
        now_epoch: i64,
    ) -> Result<CatalogTransaction, UsageCoordinationError> {
        let entries = entries.into_iter().collect::<Vec<_>>();
        let _catalog_lifecycle = self
            .shared
            .catalog_lifecycle
            .lock()
            .map_err(|_| unavailable_error())?;
        let mut state = self.shared.state.lock().map_err(|_| unavailable_error())?;
        validate_catalog_entries(&entries)?;
        if let Some(catalog_revision) = catalog_revision {
            self.shared
                .executor
                .validate_catalog_revision(catalog_revision, &entries)?;
        } else {
            self.shared.executor.validate_catalog(&entries)?;
        }
        let previous_state = state.clone();
        let previous = state.catalog.clone().unwrap_or_else(|| {
            state
                .accounts
                .iter()
                .filter_map(|(capability, entry)| {
                    entry
                        .catalog_revision
                        .clone()
                        .map(|revision| (capability.clone(), revision))
                })
                .collect()
        });
        let next = entries
            .iter()
            .cloned()
            .map(|entry| (entry.capability, entry.revision))
            .collect::<BTreeMap<_, _>>();
        let purge = catalog_purge_set(&state, &previous, &next);
        let preimages = purge
            .iter()
            .map(|capability| {
                self.shared
                    .store
                    .load(capability, now_epoch)
                    .map(|envelope| {
                        (
                            capability.clone(),
                            envelope.map_or(CatalogAccountPreimage::Missing, |envelope| {
                                CatalogAccountPreimage::Present(Box::new(envelope))
                            }),
                        )
                    })
                    .or_else(|error| match error {
                        StateStoreError::Corrupt => {
                            Ok((capability.clone(), CatalogAccountPreimage::Corrupt))
                        }
                        StateStoreError::Unavailable => Err(state_error(error)),
                    })
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        if let Err(error) = reconcile_executor_catalog(&self.shared, catalog_revision, &entries) {
            let rollback = reconcile_executor_catalog(
                &self.shared,
                previous_state.catalog_revision.as_deref(),
                &catalog_entries_from_map(&previous),
            );
            return Err(preserve_catalog_error(error, rollback));
        }

        let mut completed_purges = BTreeSet::new();
        for capability in &purge {
            let result: Result<(), StateStoreError> = match preimages.get(capability) {
                Some(CatalogAccountPreimage::Corrupt) => self.shared.store.quarantine(capability),
                Some(CatalogAccountPreimage::Missing | CatalogAccountPreimage::Present(_)) => {
                    self.shared.store.purge(capability)
                }
                None => Err(StateStoreError::Unavailable),
            };
            if let Err(error) = result.map_err(state_error) {
                let durable = restore_catalog_preimages(
                    &self.shared,
                    &preimages,
                    completed_purges,
                    now_epoch,
                )
                .map_err(state_error);
                let executor = reconcile_executor_catalog(
                    &self.shared,
                    previous_state.catalog_revision.as_deref(),
                    &catalog_entries_from_map(&previous),
                );
                return Err(preserve_catalog_error(
                    error,
                    first_catalog_rollback_error(durable, executor),
                ));
            }
            completed_purges.insert(capability.clone());
        }

        let account_capabilities = state.accounts.keys().cloned().collect::<Vec<_>>();
        for capability in account_capabilities {
            let Some(entry) = state.accounts.get_mut(&capability) else {
                continue;
            };
            match next.get(&capability) {
                None => {
                    revoke_entry(entry, now_epoch);
                    state.blocked.remove(&capability);
                }
                Some(revision)
                    if entry.revoked || entry.catalog_revision.as_ref() != Some(revision) =>
                {
                    reset_entry(entry, now_epoch, revision.clone());
                    state.blocked.remove(&capability);
                }
                Some(_) => {}
            }
        }
        state.catalog = Some(next);
        state.catalog_revision = catalog_revision.map(str::to_owned);
        let previous_catalog_revision = previous_state.catalog_revision.clone();
        self.shared.changed.notify_all();
        Ok(CatalogTransaction {
            shared: Arc::clone(&self.shared),
            previous_state,
            preimages,
            previous_entries: catalog_entries_from_map(&previous),
            previous_catalog_revision,
        })
    }

    /// Read current state without dispatching provider work.
    pub fn current(
        &self,
        capability: &UsageAccountCapability,
        now_epoch: i64,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        let _catalog_lifecycle = self
            .shared
            .catalog_lifecycle
            .lock()
            .map_err(|_| unavailable_error())?;
        self.ensure_loaded(capability, now_epoch)?;
        let state = self.shared.state.lock().map_err(|_| unavailable_error())?;
        if let Some(error) = state.blocked.get(capability) {
            return Err(error.clone());
        }
        let entry = state
            .accounts
            .get(capability)
            .ok_or_else(unavailable_error)?;
        // Read-only access retains the materialized last-good view after
        // revocation. New work and new loads are fenced below; an existing
        // session may keep displaying its immutable materialized result until
        // it explicitly stops or recreates.
        Ok(generation_view(&entry.envelope))
    }

    /// Start or join one generation. A stale observed generation always adopts
    /// the winner and cannot queue a second force refresh.
    pub fn request_refresh(
        &self,
        capability: &UsageAccountCapability,
        observed_generation: u64,
        force: bool,
        now_epoch: i64,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        let catalog_lifecycle = self
            .shared
            .catalog_lifecycle
            .lock()
            .map_err(|_| unavailable_error())?;
        self.ensure_loaded(capability, now_epoch)?;
        let mut state = self.shared.state.lock().map_err(|_| unavailable_error())?;
        if let Some(error) = state.blocked.get(capability) {
            return Err(error.clone());
        }
        let entry = state
            .accounts
            .get_mut(capability)
            .ok_or_else(unavailable_error)?;
        if entry.revoked {
            return Err(catalog_revoked_error());
        }
        if entry.envelope.phase.is_active()
            || (!entry.recovery_pending && observed_generation < entry.envelope.generation)
        {
            return Ok(generation_view(&entry.envelope));
        }
        if entry
            .envelope
            .rate_limit_deadline_epoch
            .is_some_and(|deadline| deadline > now_epoch)
            || entry
                .envelope
                .retry_deadline_epoch
                .is_some_and(|deadline| deadline > now_epoch)
            || (!force
                && entry
                    .envelope
                    .success_deadline_epoch
                    .is_some_and(|deadline| deadline > now_epoch))
        {
            return Ok(generation_view(&entry.envelope));
        }

        let previous = entry.envelope.clone();
        let recovery_pending = entry.recovery_pending;
        entry.recovery_pending = false;
        entry.envelope.generation = entry.envelope.generation.saturating_add(1);
        entry.envelope.phase = UsageRefreshPhase::Queued;
        entry.envelope.started_at_epoch = Some(now_epoch);
        entry.envelope.completed_at_epoch = None;
        entry.envelope.terminal_result = None;
        entry.envelope.terminal_error = None;
        entry.envelope.retry_deadline_epoch = None;
        let generation = entry.envelope.generation;
        if self.shared.store.store(&entry.envelope, now_epoch).is_err() {
            entry.envelope = previous;
            entry.recovery_pending = recovery_pending;
            let error = unavailable_error();
            state.blocked.insert(capability.clone(), error.clone());
            return Err(error);
        }
        let queued = generation_view(&entry.envelope);
        let catalog_revision = entry.catalog_revision.clone();
        drop(state);

        let job = ProbeJob {
            capability: capability.clone(),
            generation,
            started_at_epoch: now_epoch,
            catalog_revision,
        };
        drop(catalog_lifecycle);
        match self.jobs.try_send(WorkerMessage::Probe(job)) {
            Ok(()) => Ok(queued),
            Err(TrySendError::Full(message) | TrySendError::Disconnected(message)) => match message
            {
                WorkerMessage::Probe(job) => self.fail_without_probe(
                    &job,
                    UsageCoordinationErrorKind::Unavailable,
                    "usage coordinator queue is unavailable",
                    now_epoch,
                ),
                WorkerMessage::Shutdown => Err(unavailable_error()),
            },
        }
    }

    /// Issue one request per unique canonical account.
    pub fn request_refresh_all(
        &self,
        requests: impl IntoIterator<Item = (UsageAccountCapability, u64)>,
        force: bool,
        now_epoch: i64,
    ) -> Vec<Result<UsageGenerationView, UsageCoordinationError>> {
        let unique = requests.into_iter().collect::<BTreeMap<_, _>>();
        unique
            .into_iter()
            .map(|(capability, observed)| {
                self.request_refresh(&capability, observed, force, now_epoch)
            })
            .collect()
    }

    /// Select the periodic cadence tier for one account. A due account stays
    /// due; otherwise the next poll moves earlier when the new cadence is
    /// shorter. Never dispatches provider work.
    pub fn set_activity(
        &self,
        capability: &UsageAccountCapability,
        activity: UsageActivity,
        low_power: bool,
        now_epoch: i64,
    ) -> Result<(), UsageCoordinationError> {
        let _catalog_lifecycle = self
            .shared
            .catalog_lifecycle
            .lock()
            .map_err(|_| unavailable_error())?;
        self.ensure_loaded(capability, now_epoch)?;
        let mut state = self.shared.state.lock().map_err(|_| unavailable_error())?;
        if let Some(error) = state.blocked.get(capability) {
            return Err(error.clone());
        }
        let entry = state
            .accounts
            .get_mut(capability)
            .ok_or_else(unavailable_error)?;
        if entry.revoked {
            return Err(catalog_revoked_error());
        }
        entry.cadence.activity = activity;
        entry.cadence.low_power = low_power;
        let deadline = cadence_deadline(
            activity,
            low_power,
            capability,
            entry.envelope.generation,
            now_epoch,
        );
        entry.cadence.next_due_epoch = entry.cadence.next_due_epoch.min(deadline);
        Ok(())
    }

    /// Earliest periodic due time across known accounts, for scheduler sleep.
    /// `None` when no account is tracked yet.
    #[must_use]
    pub fn next_due_epoch(&self) -> Option<i64> {
        let _catalog_lifecycle = self.shared.catalog_lifecycle.lock().ok()?;
        self.shared.state.lock().ok().and_then(|state| {
            state
                .accounts
                .values()
                .map(|entry| entry.cadence.next_due_epoch)
                .min()
        })
    }

    /// Poll every account whose periodic cadence is due. Each due account
    /// issues at most one ambient (non-force) refresh, which joins in-flight
    /// work and honors shared Retry-After/cooldown deadlines, so one call can
    /// never produce a burst of missed polls. Returns the started or joined
    /// views; blocked accounts are skipped.
    pub fn poll_due(&self, now_epoch: i64) -> Vec<UsageGenerationView> {
        let due: Vec<(UsageAccountCapability, u64)> = {
            let Ok(_catalog_lifecycle) = self.shared.catalog_lifecycle.lock() else {
                return Vec::new();
            };
            self.shared
                .state
                .lock()
                .map(|state| {
                    state
                        .accounts
                        .iter()
                        .filter(|(capability, entry)| {
                            !entry.revoked
                                && !state.blocked.contains_key(*capability)
                                && now_epoch >= entry.cadence.next_due_epoch
                        })
                        .map(|(capability, entry)| (capability.clone(), entry.envelope.generation))
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut views = Vec::with_capacity(due.len());
        for (capability, observed) in due {
            let Ok(view) = self.request_refresh(&capability, observed, false, now_epoch) else {
                continue;
            };
            self.advance_cadence(&capability, observed, now_epoch);
            views.push(view);
        }
        views
    }

    /// Recalculate due times after sleep/wake or network reconnection. Every
    /// missed due time becomes one jittered cadence deadline from now, so the
    /// next [`UsageCoordinator::poll_due`] issues at most one poll per
    /// account. Future due times are untouched. Returns the number of
    /// recalculated accounts. Never dispatches provider work.
    pub fn note_wake(&self, now_epoch: i64) -> usize {
        let Ok(_catalog_lifecycle) = self.shared.catalog_lifecycle.lock() else {
            return 0;
        };
        let Ok(mut state) = self.shared.state.lock() else {
            return 0;
        };
        let mut recalculated = 0;
        for (capability, entry) in &mut state.accounts {
            if entry.cadence.next_due_epoch < now_epoch {
                entry.cadence.next_due_epoch = cadence_deadline(
                    entry.cadence.activity,
                    entry.cadence.low_power,
                    capability,
                    entry.envelope.generation,
                    now_epoch,
                );
                recalculated += 1;
            }
        }
        recalculated
    }

    fn advance_cadence(
        &self,
        capability: &UsageAccountCapability,
        observed_generation: u64,
        now_epoch: i64,
    ) {
        let Ok(_catalog_lifecycle) = self.shared.catalog_lifecycle.lock() else {
            return;
        };
        let Ok(mut state) = self.shared.state.lock() else {
            return;
        };
        let Some(entry) = state.accounts.get_mut(capability) else {
            return;
        };
        if entry.envelope.generation > observed_generation {
            entry.cadence.next_due_epoch = cadence_deadline(
                entry.cadence.activity,
                entry.cadence.low_power,
                capability,
                entry.envelope.generation,
                now_epoch,
            );
            return;
        }
        let shared_deadline = [
            entry.envelope.rate_limit_deadline_epoch,
            entry.envelope.retry_deadline_epoch,
            entry.envelope.success_deadline_epoch,
        ]
        .into_iter()
        .flatten()
        .filter(|deadline| *deadline > now_epoch)
        .max();
        entry.cadence.next_due_epoch = shared_deadline.unwrap_or_else(|| {
            cadence_deadline(
                entry.cadence.activity,
                entry.cadence.low_power,
                capability,
                entry.envelope.generation,
                now_epoch,
            )
        });
    }

    /// Whether no queued or active generation is retained by this authority.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        let Ok(_catalog_lifecycle) = self.shared.catalog_lifecycle.lock() else {
            return false;
        };
        self.shared.state.lock().is_ok_and(|state| {
            state
                .accounts
                .values()
                .all(|entry| !entry.envelope.phase.is_active())
        })
    }

    /// Wait for one named generation. A wait timeout never changes ownership.
    pub fn join_generation(
        &self,
        capability: &UsageAccountCapability,
        generation: u64,
        timeout: Duration,
        now_epoch: i64,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        {
            let _catalog_lifecycle = self
                .shared
                .catalog_lifecycle
                .lock()
                .map_err(|_| unavailable_error())?;
            self.ensure_loaded(capability, now_epoch)?;
        }
        let deadline = Instant::now() + timeout;
        loop {
            let catalog_lifecycle = self
                .shared
                .catalog_lifecycle
                .lock()
                .map_err(|_| unavailable_error())?;
            let state = self.shared.state.lock().map_err(|_| unavailable_error())?;
            if let Some(error) = state.blocked.get(capability) {
                return Err(error.clone());
            }
            let entry = state
                .accounts
                .get(capability)
                .ok_or_else(unavailable_error)?;
            if entry.revoked {
                return Err(catalog_revoked_error());
            }
            if entry.fenced_generations.contains(&generation) {
                return Err(catalog_revoked_error());
            }
            if let Some(terminal) = entry
                .history
                .iter()
                .find(|view| view.generation == generation)
            {
                return Ok(terminal.clone());
            }
            if entry.envelope.generation == generation && entry.envelope.phase.is_terminal() {
                return Ok(generation_view(&entry.envelope));
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(coordination_error(
                    UsageCoordinationErrorKind::WaitTimeout,
                    "usage refresh is still updating",
                ));
            };
            drop(catalog_lifecycle);
            let (next_state, wait) = self
                .shared
                .changed
                .wait_timeout(state, remaining)
                .map_err(|_| unavailable_error())?;
            drop(next_state);
            if wait.timed_out() {
                return Err(coordination_error(
                    UsageCoordinationErrorKind::WaitTimeout,
                    "usage refresh is still updating",
                ));
            }
        }
    }

    fn ensure_loaded(
        &self,
        capability: &UsageAccountCapability,
        now_epoch: i64,
    ) -> Result<(), UsageCoordinationError> {
        {
            let mut state = self.shared.state.lock().map_err(|_| unavailable_error())?;
            if let Some(error) = state.blocked.get(capability).cloned() {
                let Some(envelope) = state
                    .accounts
                    .get(capability)
                    .map(|entry| entry.envelope.clone())
                else {
                    return Err(error);
                };
                if self.shared.store.store(&envelope, now_epoch).is_err() {
                    return Err(error);
                }
                state.blocked.remove(capability);
                record_blocked_terminal(&mut state, capability, &envelope)?;
                self.shared.changed.notify_all();
            }
            if state.accounts.contains_key(capability) {
                return Ok(());
            }
            if state
                .catalog
                .as_ref()
                .is_some_and(|catalog| !catalog.contains_key(capability))
            {
                return Err(catalog_revoked_error());
            }
        }
        let loaded = self.shared.store.load(capability, now_epoch);
        let mut state = self.shared.state.lock().map_err(|_| unavailable_error())?;
        if state.accounts.contains_key(capability) {
            return Ok(());
        }
        if state
            .catalog
            .as_ref()
            .is_some_and(|catalog| !catalog.contains_key(capability))
        {
            return Err(catalog_revoked_error());
        }
        let catalog_revision = state
            .catalog
            .as_ref()
            .and_then(|catalog| catalog.get(capability).cloned());
        match loaded {
            Ok(envelope) => {
                let mut envelope =
                    envelope.unwrap_or_else(|| AccountStateEnvelope::idle(capability.clone()));
                let recovery_pending = envelope.phase.is_active();
                if recovery_pending {
                    envelope.phase = UsageRefreshPhase::Failed;
                    envelope.terminal_result = None;
                    envelope.terminal_error = Some(coordination_error(
                        UsageCoordinationErrorKind::OwnerLost,
                        "usage refresh owner exited before completion",
                    ));
                    envelope.completed_at_epoch = Some(now_epoch);
                    envelope.retry_deadline_epoch = None;
                    envelope.success_deadline_epoch = None;
                    envelope.consecutive_failures = envelope.consecutive_failures.saturating_add(1);
                    if self.shared.store.store(&envelope, now_epoch).is_err() {
                        let error = unavailable_error();
                        state.blocked.insert(capability.clone(), error.clone());
                        return Err(error);
                    }
                }
                state.accounts.insert(
                    capability.clone(),
                    AccountEntry::new(envelope, recovery_pending, now_epoch, catalog_revision),
                );
                Ok(())
            }
            Err(error) => {
                let error = state_error(error);
                state.blocked.insert(capability.clone(), error.clone());
                Err(error)
            }
        }
    }

    fn fail_without_probe(
        &self,
        job: &ProbeJob,
        kind: UsageCoordinationErrorKind,
        message: &str,
        now_epoch: i64,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        finish_failure(&self.shared, job, kind, message, None, now_epoch);
        self.current(&job.capability, now_epoch)
    }
}

impl Drop for UsageCoordinator {
    fn drop(&mut self) {
        for _ in &self.workers {
            drop(self.jobs.send(WorkerMessage::Shutdown));
        }
        for worker in self.workers.drain(..) {
            drop(worker.join());
        }
    }
}

fn coordinator_worker(shared: &Arc<Shared>, receiver: &Arc<Mutex<Receiver<WorkerMessage>>>) {
    loop {
        let message = {
            let Ok(receiver) = receiver.lock() else {
                return;
            };
            receiver.recv()
        };
        match message {
            Ok(WorkerMessage::Probe(job)) => execute_probe(shared, job),
            Ok(WorkerMessage::Shutdown) | Err(_) => return,
        }
    }
}

fn execute_probe(shared: &Arc<Shared>, job: ProbeJob) {
    if !mark_updating(shared, &job) {
        return;
    }
    let started = Instant::now();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        shared.executor.probe(&job.capability, job.generation)
    }));
    let finished_at_epoch = job
        .started_at_epoch
        .saturating_add(i64::try_from(started.elapsed().as_secs()).unwrap_or(i64::MAX));
    if started.elapsed() > shared.config.provider_timeout {
        finish_failure(
            shared,
            &job,
            UsageCoordinationErrorKind::ProviderTimeout,
            "usage provider probe timed out",
            None,
            finished_at_epoch,
        );
        return;
    }
    match outcome {
        Ok(ProviderProbeOutcome::Success(view)) if data_bearing(&view) => {
            finish_success(shared, &job, *view, finished_at_epoch);
        }
        Ok(ProviderProbeOutcome::Success(_)) => finish_failure(
            shared,
            &job,
            UsageCoordinationErrorKind::ProviderUnavailable,
            "usage provider returned no quota data",
            None,
            finished_at_epoch,
        ),
        Ok(ProviderProbeOutcome::Failure {
            kind,
            message,
            retry_at_epoch,
        }) => finish_failure(
            shared,
            &job,
            kind,
            &message,
            retry_at_epoch,
            finished_at_epoch,
        ),
        Err(_) => finish_failure(
            shared,
            &job,
            UsageCoordinationErrorKind::OwnerLost,
            "usage provider worker failed",
            None,
            finished_at_epoch,
        ),
    }
}

fn mark_updating(shared: &Arc<Shared>, job: &ProbeJob) -> bool {
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
        // This worker already owns the queued generation. Resolve it through
        // the terminal path so its owner cannot remain active after the job
        // has been discarded.
        drop(state);
        finish_failure(
            shared,
            job,
            UsageCoordinationErrorKind::Unavailable,
            "usage state store is unavailable",
            None,
            job.started_at_epoch,
        );
        return false;
    }
    shared.changed.notify_all();
    true
}

fn finish_success(
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

fn finish_failure(
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
    entry.envelope.phase = UsageRefreshPhase::Failed;
    entry.envelope.terminal_result = None;
    entry.envelope.terminal_error = Some(coordination_error(kind, message));
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
    persist_terminal(shared, &mut state, &job.capability, finished_at_epoch);
}

fn persist_terminal(
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

fn validate_catalog_entries(entries: &[UsageCatalogEntry]) -> Result<(), UsageCoordinationError> {
    let mut capabilities = BTreeSet::new();
    for entry in entries {
        if entry.revision.is_empty() || !capabilities.insert(entry.capability.clone()) {
            return Err(coordination_error(
                UsageCoordinationErrorKind::CorruptState,
                "usage broker catalog contains an invalid or duplicate entry",
            ));
        }
    }
    Ok(())
}

fn catalog_purge_set(
    state: &CoordinatorState,
    previous: &BTreeMap<UsageAccountCapability, String>,
    next: &BTreeMap<UsageAccountCapability, String>,
) -> BTreeSet<UsageAccountCapability> {
    let mut purge = BTreeSet::new();
    for (capability, revision) in previous {
        if next
            .get(capability)
            .is_none_or(|current| current != revision)
        {
            purge.insert(capability.clone());
        }
    }
    for (capability, entry) in &state.accounts {
        if next
            .get(capability)
            .is_none_or(|revision| entry.catalog_revision.as_ref() != Some(revision))
        {
            purge.insert(capability.clone());
        }
    }
    for capability in state.blocked.keys() {
        if !next.contains_key(capability) {
            purge.insert(capability.clone());
        }
    }
    purge
}

fn catalog_entries_from_map(
    catalog: &BTreeMap<UsageAccountCapability, String>,
) -> Vec<UsageCatalogEntry> {
    catalog
        .iter()
        .map(|(capability, revision)| UsageCatalogEntry {
            capability: capability.clone(),
            revision: revision.clone(),
        })
        .collect()
}

fn reconcile_executor_catalog(
    shared: &Arc<Shared>,
    catalog_revision: Option<&str>,
    entries: &[UsageCatalogEntry],
) -> Result<(), UsageCoordinationError> {
    match catalog_revision {
        Some(catalog_revision) => shared
            .executor
            .reconcile_catalog_revision(catalog_revision, entries),
        None => shared.executor.reconcile_catalog(entries),
    }
}

fn restore_catalog_preimages(
    shared: &Arc<Shared>,
    preimages: &BTreeMap<UsageAccountCapability, CatalogAccountPreimage>,
    capabilities: BTreeSet<UsageAccountCapability>,
    now_epoch: i64,
) -> Result<(), StateStoreError> {
    for capability in capabilities {
        let Some(preimage) = preimages.get(&capability) else {
            return Err(StateStoreError::Unavailable);
        };
        match preimage {
            CatalogAccountPreimage::Present(envelope) => {
                shared.store.store(envelope, now_epoch)?;
            }
            CatalogAccountPreimage::Missing => {
                shared.store.purge(&capability)?;
            }
            // The invalid bytes were deliberately quarantined during the
            // failed rotation. Reintroducing them would re-poison recovery.
            CatalogAccountPreimage::Corrupt => {}
        }
    }
    Ok(())
}

fn first_catalog_rollback_error(
    durable: Result<(), UsageCoordinationError>,
    executor: Result<(), UsageCoordinationError>,
) -> Result<(), UsageCoordinationError> {
    match (durable, executor) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), _) | (Ok(()), Err(error)) => Err(error),
    }
}

fn preserve_catalog_error(
    primary: UsageCoordinationError,
    rollback: Result<(), UsageCoordinationError>,
) -> UsageCoordinationError {
    match rollback {
        Ok(()) => primary,
        Err(rollback) => coordination_error(
            primary.kind,
            format!(
                "{}; catalog rollback failed: {}",
                primary.message, rollback.message
            ),
        ),
    }
}

fn revoke_entry(entry: &mut AccountEntry, now_epoch: i64) {
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

fn reset_entry(entry: &mut AccountEntry, now_epoch: i64, revision: String) {
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

fn data_bearing(view: &FocusedUsageView) -> bool {
    if view.status == UsageSnapshotStatus::Unsupported {
        return true;
    }
    !view.buckets.is_empty() && view.status == UsageSnapshotStatus::Fresh
}

/// Jittered periodic deadline: tier cadence plus a deterministic
/// `[0, cadence/4]` skew, so accounts spread out instead of polling in
/// lockstep. The capability and generation seed it, so joined callers never
/// derive different due times.
fn cadence_deadline(
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

fn cadence_jitter_seed(capability: &UsageAccountCapability, generation: u64) -> u64 {
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

fn record_blocked_terminal(
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

fn generation_view(envelope: &AccountStateEnvelope) -> UsageGenerationView {
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

fn state_error(error: StateStoreError) -> UsageCoordinationError {
    match error {
        StateStoreError::Unavailable => unavailable_error(),
        StateStoreError::Corrupt => coordination_error(
            UsageCoordinationErrorKind::CorruptState,
            "usage coordinator state is corrupt",
        ),
    }
}

fn unavailable_error() -> UsageCoordinationError {
    coordination_error(
        UsageCoordinationErrorKind::Unavailable,
        "usage coordinator is unavailable",
    )
}

fn catalog_revoked_error() -> UsageCoordinationError {
    coordination_error(
        UsageCoordinationErrorKind::CatalogRevoked,
        "usage account capability was removed from the current broker catalog",
    )
}

fn coordination_error(
    kind: UsageCoordinationErrorKind,
    message: impl AsRef<str>,
) -> UsageCoordinationError {
    UsageCoordinationError {
        kind,
        message: message
            .as_ref()
            .chars()
            .filter(|character| !character.is_control())
            .take(256)
            .collect(),
    }
}

#[cfg(test)]
mod tests;
