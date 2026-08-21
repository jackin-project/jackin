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
    UsageAccountCapability, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageGenerationView, UsageRefreshPhase,
};

pub use state::{
    AccountStateEnvelope, AccountStateStore, FileAccountStateStore, FileProjectionStateStore,
    ProjectionAlias, ProjectionStateEnvelope, StateStoreError,
};

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

struct AccountEntry {
    envelope: AccountStateEnvelope,
    history: VecDeque<UsageGenerationView>,
    recovery_pending: bool,
}

impl AccountEntry {
    fn new(envelope: AccountStateEnvelope, recovery_pending: bool) -> Self {
        let mut history = VecDeque::new();
        if envelope.phase.is_terminal() {
            history.push_back(generation_view(&envelope));
        }
        Self {
            envelope,
            history,
            recovery_pending,
        }
    }

    fn record_terminal(&mut self) {
        self.history.push_back(generation_view(&self.envelope));
        while self.history.len() > TERMINAL_HISTORY_LIMIT {
            drop(self.history.pop_front());
        }
    }
}

#[derive(Default)]
struct CoordinatorState {
    accounts: BTreeMap<UsageAccountCapability, AccountEntry>,
    blocked: BTreeMap<UsageAccountCapability, UsageCoordinationError>,
}

struct Shared {
    state: Mutex<CoordinatorState>,
    changed: Condvar,
    executor: Arc<dyn UsageProviderExecutor>,
    store: Arc<dyn AccountStateStore>,
    config: UsageCoordinatorConfig,
}

#[derive(Debug)]
struct ProbeJob {
    capability: UsageAccountCapability,
    generation: u64,
    started_at_epoch: i64,
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
        let config = UsageCoordinatorConfig {
            max_concurrency: config.max_concurrency.max(1),
            queue_capacity: config.queue_capacity.max(1),
            ..config
        };
        let shared = Arc::new(Shared {
            state: Mutex::new(CoordinatorState::default()),
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

    /// Read current state without dispatching provider work.
    pub fn current(
        &self,
        capability: &UsageAccountCapability,
        now_epoch: i64,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        self.ensure_loaded(capability, now_epoch)?;
        let state = self.shared.state.lock().map_err(|_| unavailable_error())?;
        if let Some(error) = state.blocked.get(capability) {
            return Err(error.clone());
        }
        state
            .accounts
            .get(capability)
            .map(|entry| generation_view(&entry.envelope))
            .ok_or_else(unavailable_error)
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
        self.ensure_loaded(capability, now_epoch)?;
        let mut state = self.shared.state.lock().map_err(|_| unavailable_error())?;
        if let Some(error) = state.blocked.get(capability) {
            return Err(error.clone());
        }
        let entry = state
            .accounts
            .get_mut(capability)
            .ok_or_else(unavailable_error)?;
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
        drop(state);

        let job = ProbeJob {
            capability: capability.clone(),
            generation,
            started_at_epoch: now_epoch,
        };
        match self.jobs.try_send(WorkerMessage::Probe(job)) {
            Ok(()) => Ok(queued),
            Err(
                TrySendError::Full(WorkerMessage::Probe(job))
                | TrySendError::Disconnected(WorkerMessage::Probe(job)),
            ) => self.fail_without_probe(
                &job.capability,
                job.generation,
                UsageCoordinationErrorKind::Unavailable,
                "usage coordinator queue is unavailable",
                now_epoch,
            ),
            Err(
                TrySendError::Full(WorkerMessage::Shutdown)
                | TrySendError::Disconnected(WorkerMessage::Shutdown),
            ) => Err(unavailable_error()),
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

    /// Whether no queued or active generation is retained by this authority.
    #[must_use]
    pub fn is_idle(&self) -> bool {
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
        self.ensure_loaded(capability, now_epoch)?;
        let deadline = Instant::now() + timeout;
        let mut state = self.shared.state.lock().map_err(|_| unavailable_error())?;
        loop {
            if let Some(error) = state.blocked.get(capability) {
                return Err(error.clone());
            }
            let entry = state
                .accounts
                .get(capability)
                .ok_or_else(unavailable_error)?;
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
            let (next_state, wait) = self
                .shared
                .changed
                .wait_timeout(state, remaining)
                .map_err(|_| unavailable_error())?;
            state = next_state;
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
            let state = self.shared.state.lock().map_err(|_| unavailable_error())?;
            if state.accounts.contains_key(capability) {
                return Ok(());
            }
            if let Some(error) = state.blocked.get(capability) {
                return Err(error.clone());
            }
        }
        let loaded = self.shared.store.load(capability, now_epoch);
        let mut state = self.shared.state.lock().map_err(|_| unavailable_error())?;
        if state.accounts.contains_key(capability) {
            return Ok(());
        }
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
                    AccountEntry::new(envelope, recovery_pending),
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
        capability: &UsageAccountCapability,
        generation: u64,
        kind: UsageCoordinationErrorKind,
        message: &str,
        now_epoch: i64,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        finish_failure(
            &self.shared,
            capability,
            generation,
            kind,
            message,
            None,
            now_epoch,
        );
        self.current(capability, now_epoch)
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
            &job.capability,
            job.generation,
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
            &job.capability,
            job.generation,
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
            &job.capability,
            job.generation,
            kind,
            &message,
            retry_at_epoch,
            finished_at_epoch,
        ),
        Err(_) => finish_failure(
            shared,
            &job.capability,
            job.generation,
            UsageCoordinationErrorKind::OwnerLost,
            "usage provider worker failed",
            None,
            finished_at_epoch,
        ),
    }
}

fn mark_updating(shared: &Arc<Shared>, job: &ProbeJob) -> bool {
    let Ok(mut state) = shared.state.lock() else {
        return false;
    };
    let Some(entry) = state.accounts.get_mut(&job.capability) else {
        return false;
    };
    if entry.envelope.generation != job.generation
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
        state
            .blocked
            .insert(job.capability.clone(), unavailable_error());
        shared.changed.notify_all();
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
    let Ok(mut state) = shared.state.lock() else {
        return;
    };
    let Some(entry) = state.accounts.get_mut(&job.capability) else {
        return;
    };
    if entry.envelope.generation != job.generation || !entry.envelope.phase.is_active() {
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
    capability: &UsageAccountCapability,
    generation: u64,
    kind: UsageCoordinationErrorKind,
    message: &str,
    retry_at_epoch: Option<i64>,
    finished_at_epoch: i64,
) {
    let Ok(mut state) = shared.state.lock() else {
        return;
    };
    let Some(entry) = state.accounts.get_mut(capability) else {
        return;
    };
    if entry.envelope.generation != generation || !entry.envelope.phase.is_active() {
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
            capability,
            generation,
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
    persist_terminal(shared, &mut state, capability, finished_at_epoch);
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

fn data_bearing(view: &FocusedUsageView) -> bool {
    !view.buckets.is_empty()
        && !matches!(
            view.status,
            UsageSnapshotStatus::Unavailable
                | UsageSnapshotStatus::Unsupported
                | UsageSnapshotStatus::NeedsSecret
        )
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
        retry_at_epoch: envelope
            .rate_limit_deadline_epoch
            .or(envelope.retry_deadline_epoch),
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
