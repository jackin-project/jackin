// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker socket serve loop and framing.

use std::io::{Read, Write};

use std::os::unix::net::{UnixListener, UnixStream};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jackin_protocol::usage_broker::{
    USAGE_BROKER_MAX_FRAME_BYTES, UsageAccountCapability, UsageBrokerRequest, UsageBrokerResponse,
    UsageCoordinationError,
};

use super::catalog::BrokerCatalogRefresh;
use super::dispatch_ops::{DispatchControls, dispatch_with_liveness};
use super::monitor::MonitorStore;
use super::waits;
use super::{
    BROKER_CONNECTION_QUEUE, BROKER_CONNECTION_WORKERS, BrokerStartupCleanup, PUBLISH_TICK,
    ServePolicy, protocol_error, publish, unavailable,
};
use crate::coordinator::UsageCoordinator;
use crate::coordinator::policy::UsageActivity;

const CLOCK_WAKE_DETECTION_THRESHOLD_SECS: u64 = 2;
const MONITOR_TICK_RETRY_DELAY: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ClockSample {
    wall_epoch: i64,
    /// Monotonic elapsed time since the owning clock started.
    monotonic_elapsed: Duration,
}

trait TickerClock: Send {
    fn initial_sample(&self) -> ClockSample;
    fn next_sample(&mut self) -> Option<ClockSample>;
}

struct SystemTickerClock {
    started: Instant,
}

impl SystemTickerClock {
    fn new() -> Self {
        Self {
            started: Instant::now(),
        }
    }

    fn sample(&self) -> ClockSample {
        ClockSample {
            wall_epoch: chrono::Utc::now().timestamp(),
            monotonic_elapsed: self.started.elapsed(),
        }
    }
}

impl TickerClock for SystemTickerClock {
    fn initial_sample(&self) -> ClockSample {
        self.sample()
    }

    fn next_sample(&mut self) -> Option<ClockSample> {
        std::thread::park_timeout(PUBLISH_TICK);
        Some(self.sample())
    }
}

#[derive(Debug, Clone, Copy)]
struct TickRetry {
    wake_epoch: i64,
    after_monotonic: Duration,
}

struct TickerState {
    last_sample: ClockSample,
    last_observed_projection_id: Option<String>,
    last_ticked_wake: Option<i64>,
    retry: Option<TickRetry>,
    last_collection_check: Option<Duration>,
    last_publish_attempt: Option<Duration>,
    observation_retry_after: Option<Duration>,
}

pub(super) struct ServeConfig {
    pub(super) listener: UnixListener,
    pub(super) coordinator: Arc<UsageCoordinator>,
    pub(super) build_id: String,
    pub(super) cleanup: BrokerStartupCleanup,
    pub(super) policy: ServePolicy,
    pub(super) publisher: publish::ProjectionPublisher,
    pub(super) monitor_store: Arc<MonitorStore>,
    pub(super) catalog_refresh: Option<Arc<BrokerCatalogRefresh>>,
    pub(super) collector_liveness: Option<Arc<crate::usage::ClaudeCollectorLiveness>>,
}

struct ConnectionContext {
    coordinator: Arc<UsageCoordinator>,
    build_id: Arc<str>,
    publisher: publish::ProjectionPublisher,
    monitor_store: Arc<MonitorStore>,
    shutdown: Arc<AtomicBool>,
    fenced: Arc<AtomicBool>,
    catalog_refresh: Option<Arc<BrokerCatalogRefresh>>,
    wait_pool: Arc<waits::WaitPool>,
    collector_liveness: Option<Arc<crate::usage::ClaudeCollectorLiveness>>,
}

impl ConnectionContext {
    fn is_fenced(&self) -> bool {
        self.fenced.load(Ordering::Acquire) || self.shutdown.load(Ordering::Acquire)
    }
}

pub(super) fn serve(config: ServeConfig) {
    let ServeConfig {
        listener,
        coordinator,
        build_id,
        mut cleanup,
        policy,
        publisher,
        monitor_store,
        catalog_refresh,
        collector_liveness,
    } = config;
    let (connections, receiver) = mpsc::sync_channel(BROKER_CONNECTION_QUEUE);
    let receiver = Arc::new(Mutex::new(receiver));
    let build_id = Arc::<str>::from(build_id.as_str());
    let shutdown = collector_liveness.as_ref().map_or_else(
        || Arc::new(AtomicBool::new(false)),
        |live| live.shutdown_flag(),
    );
    let fenced = Arc::new(AtomicBool::new(false));
    let wait_pool = Arc::new(waits::WaitPool::new(
        Arc::clone(&coordinator),
        Arc::clone(&build_id),
        publisher.clone(),
        Arc::clone(&monitor_store),
        Arc::clone(&shutdown),
        catalog_refresh.clone(),
    ));
    let context = Arc::new(ConnectionContext {
        coordinator: Arc::clone(&coordinator),
        build_id,
        publisher: publisher.clone(),
        monitor_store: Arc::clone(&monitor_store),
        shutdown: Arc::clone(&shutdown),
        fenced: Arc::clone(&fenced),
        catalog_refresh,
        wait_pool,
        collector_liveness: collector_liveness.as_ref().map(Arc::clone),
    });
    let workers = spawn_connection_workers(receiver, Arc::clone(&context));
    if workers.is_empty() {
        invalidate_foreground_collector(&monitor_store, collector_liveness.as_ref());
        drop(listener);
        drop(cleanup);
        return;
    }
    if listener.set_nonblocking(true).is_err() {
        invalidate_foreground_collector(&monitor_store, collector_liveness.as_ref());
        drop(connections);
        for worker in workers {
            drop(worker.join());
        }
        drop(listener);
        drop(cleanup);
        return;
    }
    let publisher_shutdown = Arc::new(AtomicBool::new(false));
    let ticker = spawn_publisher_ticker(
        publisher.clone(),
        Arc::clone(&coordinator),
        Arc::clone(&monitor_store),
        Arc::clone(&publisher_shutdown),
    );
    let started = Instant::now();
    let initial_sample = system_clock_sample(started);
    let mut last_activity = initial_sample.monotonic_elapsed;
    let mut last_renewal = initial_sample;
    loop {
        if shutdown.load(Ordering::Acquire) {
            break;
        }
        // Lease and idle maintenance precede accept so a queued client cannot
        // postpone wake recovery or allow an expired owner to serve requests.
        let now = system_clock_sample(started);
        if interval_elapsed(now, last_renewal, policy.lease_renewal) {
            if !cleanup.renew(policy.lease_duration) {
                fenced.store(true, Ordering::Release);
                break;
            }
            last_renewal = now;
        }
        if should_exit_idle(
            now.monotonic_elapsed,
            last_activity,
            policy.idle_exit,
            coordinator.is_idle(),
            monitor_store.has_active(),
            collector_liveness.is_some(),
        ) {
            break;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                last_activity = now.monotonic_elapsed;
                match connections.try_send(stream) {
                    Ok(()) => {}
                    Err(
                        TrySendError::Full(mut stream) | TrySendError::Disconnected(mut stream),
                    ) => {
                        write_response(
                            &mut stream,
                            UsageBrokerResponse::Error {
                                error: unavailable(),
                            },
                        );
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::park_timeout(Duration::from_millis(50));
            }
            Err(_) => break,
        }
    }
    invalidate_foreground_collector(&monitor_store, collector_liveness.as_ref());
    // Fence all remaining connections before disconnecting the queue. Workers
    // check this gate both before handling queued streams and before dispatch.
    fenced.store(true, Ordering::Release);
    drop(connections);
    for worker in workers {
        drop(worker.join());
    }
    publisher_shutdown.store(true, Ordering::Relaxed);
    if let Some(ticker) = ticker {
        drop(ticker.join());
    }
    drop(listener);
    // Dropping the context joins admitted long polls; keep the lease locked
    // until those workers and all synchronous connection workers are gone.
    drop(context);
    drop(cleanup);
}

fn invalidate_foreground_collector(
    monitor_store: &MonitorStore,
    liveness: Option<&Arc<crate::usage::ClaudeCollectorLiveness>>,
) {
    if let Some(liveness) = liveness {
        liveness.deactivate();
        monitor_store.set_experimental_collector_source(None);
    }
}

fn spawn_connection_workers(
    receiver: Arc<Mutex<mpsc::Receiver<UnixStream>>>,
    context: Arc<ConnectionContext>,
) -> Vec<std::thread::JoinHandle<()>> {
    let mut workers = Vec::with_capacity(BROKER_CONNECTION_WORKERS);
    for index in 0..BROKER_CONNECTION_WORKERS {
        let receiver = Arc::clone(&receiver);
        let context = Arc::clone(&context);
        let worker = jackin_telemetry::spawn::thread_joined_named(
            format!("usage-broker-connection-{index}"),
            move || loop {
                let stream = {
                    let Ok(receiver) = receiver.lock() else {
                        return;
                    };
                    receiver.recv()
                };
                let Ok(stream) = stream else {
                    return;
                };
                if context.is_fenced() {
                    drop(stream);
                    continue;
                }
                handle_stream(stream, &context);
            },
        );
        match worker {
            Ok(worker) => workers.push(worker),
            Err(_) => break,
        }
    }
    workers
}

fn handle_stream(mut stream: UnixStream, context: &ConnectionContext) {
    if context.is_fenced() {
        return;
    }
    let request = match read_request(&mut stream) {
        Ok(request) => request,
        Err(error) => {
            if !context.is_fenced() {
                write_response(&mut stream, UsageBrokerResponse::Error { error });
            }
            return;
        }
    };
    if context.is_fenced() {
        return;
    }
    if waits::is_wait(&request.operation) {
        if context.is_fenced() {
            return;
        }
        context.wait_pool.enqueue(stream, request);
        return;
    }
    if context.is_fenced() {
        return;
    }
    let response = dispatch_with_liveness(
        &context.coordinator,
        request,
        &context.build_id,
        &context.publisher,
        &context.monitor_store,
        DispatchControls {
            shutdown: &context.shutdown,
            catalog_refresh: context.catalog_refresh.as_deref(),
            collector_liveness: context.collector_liveness.as_deref(),
        },
    );
    write_response(&mut stream, response);
}

fn spawn_publisher_ticker(
    publisher: publish::ProjectionPublisher,
    coordinator: Arc<UsageCoordinator>,
    monitor_store: Arc<MonitorStore>,
    shutdown: Arc<AtomicBool>,
) -> Option<std::thread::JoinHandle<()>> {
    spawn_publisher_ticker_with_clock(
        publisher,
        coordinator,
        monitor_store,
        shutdown,
        Box::new(SystemTickerClock::new()),
    )
}

fn spawn_publisher_ticker_with_clock(
    publisher: publish::ProjectionPublisher,
    coordinator: Arc<UsageCoordinator>,
    monitor_store: Arc<MonitorStore>,
    shutdown: Arc<AtomicBool>,
    mut clock: Box<dyn TickerClock>,
) -> Option<std::thread::JoinHandle<()>> {
    // Incremental publication merges completed accounts as they finish. A
    // stalled account never blocks healthy accounts; dispatch also publishes.
    let initial_projection_id = publisher
        .current_projection()
        .ok()
        .map(|projection| projection.projection_id);
    let last_sample = clock.initial_sample();
    jackin_telemetry::spawn::thread_joined_named("usage-broker-publisher".to_owned(), move || {
        let mut state = TickerState {
            last_sample,
            last_observed_projection_id: initial_projection_id,
            last_ticked_wake: None,
            retry: None,
            last_collection_check: None,
            last_publish_attempt: None,
            observation_retry_after: None,
        };
        while !shutdown.load(Ordering::Relaxed) {
            let Some(sample) = clock.next_sample() else {
                break;
            };
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            publisher_tick_step(&publisher, &coordinator, &monitor_store, sample, &mut state);
        }
    })
    .ok()
}

fn publisher_tick_step(
    publisher: &publish::ProjectionPublisher,
    coordinator: &UsageCoordinator,
    monitor_store: &MonitorStore,
    sample: ClockSample,
    state: &mut TickerState,
) {
    let monotonic_elapsed = sample
        .monotonic_elapsed
        .saturating_sub(state.last_sample.monotonic_elapsed);
    if wall_clock_wake_detected(
        state.last_sample.wall_epoch,
        monotonic_elapsed,
        sample.wall_epoch,
    ) {
        let _recalculated = coordinator.note_wake(sample.wall_epoch);
    }
    state.last_sample = sample;

    if state.last_collection_check.is_none_or(|last_check| {
        sample.monotonic_elapsed.saturating_sub(last_check) >= MONITOR_TICK_RETRY_DELAY
    }) {
        collect_due_for_active_monitors(publisher, coordinator, monitor_store, sample.wall_epoch);
        state.last_collection_check = Some(sample.monotonic_elapsed);
    }

    // Provider completion makes the coordinator idle before this ticker runs
    // again. Publish terminal state even in that case so the last projection
    // cannot remain queued/updating until a client happens to read or refresh.
    // A one-second monotonic poll catches asynchronous completion while
    // keeping unchanged or persistence-blocked accounts off the 200ms loop.
    if state.last_publish_attempt.is_none_or(|last_attempt| {
        sample.monotonic_elapsed.saturating_sub(last_attempt) >= MONITOR_TICK_RETRY_DELAY
    }) {
        let _published = publisher.publish_due(sample.wall_epoch);
        state.last_publish_attempt = Some(sample.monotonic_elapsed);
    }
    if let Ok(projection) = publisher.current_projection()
        && state.last_observed_projection_id.as_deref() != Some(projection.projection_id.as_str())
        && state
            .observation_retry_after
            .is_none_or(|retry_after| sample.monotonic_elapsed >= retry_after)
    {
        if monitor_store
            .observe_projection(&projection, sample.wall_epoch)
            .is_ok()
        {
            state.last_observed_projection_id = Some(projection.projection_id);
            state.observation_retry_after = None;
        } else {
            state.observation_retry_after = Some(
                sample
                    .monotonic_elapsed
                    .saturating_add(MONITOR_TICK_RETRY_DELAY),
            );
        }
    }

    let next_wake = monitor_store.next_wake();
    if let Some(wake_epoch) = next_wake.filter(|wake_epoch| *wake_epoch <= sample.wall_epoch) {
        if state
            .retry
            .is_some_and(|retry| retry.wake_epoch != wake_epoch)
        {
            state.retry = None;
        }
        let retry_waiting = state.retry.is_some_and(|retry| {
            retry.wake_epoch == wake_epoch && sample.monotonic_elapsed < retry.after_monotonic
        });
        if state.last_ticked_wake != Some(wake_epoch) && !retry_waiting {
            match monitor_store.tick(sample.wall_epoch) {
                Ok(()) => {
                    state.last_ticked_wake = Some(wake_epoch);
                    state.retry = None;
                }
                Err(_) => {
                    state.retry = Some(TickRetry {
                        wake_epoch,
                        after_monotonic: sample
                            .monotonic_elapsed
                            .saturating_add(MONITOR_TICK_RETRY_DELAY),
                    });
                }
            }
        }
    } else {
        state.last_ticked_wake = None;
        state.retry = None;
    }
}

pub(super) fn collect_due_for_active_monitors(
    publisher: &publish::ProjectionPublisher,
    coordinator: &UsageCoordinator,
    monitor_store: &MonitorStore,
    now_epoch: i64,
) {
    let catalog_capabilities = publisher.catalog_capabilities();
    let mut capabilities = monitor_store
        .collection_accounts()
        .into_iter()
        .map(|account_id| UsageAccountCapability {
            surface_id: "claude".to_owned(),
            account_id,
        })
        .collect::<Vec<_>>();
    capabilities.retain(|capability| catalog_capabilities.contains(capability));
    if capabilities.is_empty() {
        return;
    }
    for capability in &capabilities {
        publisher.observe(capability);
        let _activity = coordinator.set_activity(
            capability,
            UsageActivity::DirectInteraction,
            false,
            now_epoch,
        );
    }
    if coordinator
        .next_due_epoch_for_capabilities(capabilities.clone(), now_epoch)
        .is_some_and(|due| due <= now_epoch)
    {
        let _started_or_joined = coordinator.poll_due_for_capabilities(capabilities, now_epoch);
    }
}

fn system_clock_sample(started: Instant) -> ClockSample {
    ClockSample {
        wall_epoch: chrono::Utc::now().timestamp(),
        monotonic_elapsed: started.elapsed(),
    }
}

fn interval_elapsed(now: ClockSample, last: ClockSample, interval: Duration) -> bool {
    let interval_seconds = interval
        .as_secs()
        .saturating_add(u64::from(interval.subsec_nanos() != 0));
    let wall_interval = i64::try_from(interval_seconds).unwrap_or(i64::MAX);
    now.wall_epoch.saturating_sub(last.wall_epoch) >= wall_interval
        || now.monotonic_elapsed.saturating_sub(last.monotonic_elapsed) >= interval
}

fn should_exit_idle(
    now_monotonic: Duration,
    last_activity_monotonic: Duration,
    idle_exit: Duration,
    coordinator_idle: bool,
    has_active_monitor: bool,
    has_foreground_liveness: bool,
) -> bool {
    now_monotonic.saturating_sub(last_activity_monotonic) >= idle_exit
        && coordinator_idle
        && !has_active_monitor
        && !has_foreground_liveness
}

#[cfg(test)]
mod tests;

pub(super) fn wall_clock_wake_detected(
    last_tick_epoch: i64,
    monotonic_elapsed: Duration,
    now_epoch: i64,
) -> bool {
    let wall_elapsed = now_epoch.saturating_sub(last_tick_epoch);
    let monotonic_elapsed = i64::try_from(monotonic_elapsed.as_secs()).unwrap_or(i64::MAX);
    wall_elapsed.abs_diff(monotonic_elapsed) > CLOCK_WAKE_DETECTION_THRESHOLD_SECS
}

pub(super) fn write_response(stream: &mut UnixStream, response: UsageBrokerResponse) {
    if let Ok(mut bytes) = serde_json::to_vec(&response)
        && bytes.len() < USAGE_BROKER_MAX_FRAME_BYTES
    {
        bytes.push(b'\n');
        write_with_deadline(stream, &bytes, Duration::from_secs(1));
    }
}

pub(super) fn write_with_deadline(stream: &mut UnixStream, mut bytes: &[u8], timeout: Duration) {
    if stream.set_nonblocking(true).is_err() {
        return;
    }
    let deadline = Instant::now() + timeout;
    while !bytes.is_empty() && Instant::now() < deadline {
        match stream.write(bytes) {
            Ok(0) => return,
            Ok(written) => bytes = &bytes[written..],
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::park_timeout(Duration::from_millis(1));
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

fn read_request(stream: &mut UnixStream) -> Result<UsageBrokerRequest, UsageCoordinationError> {
    stream.set_nonblocking(true).map_err(|_| unavailable())?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 1024];
    while Instant::now() < deadline {
        match stream.read(&mut chunk) {
            Ok(0) => return Err(protocol_error()),
            Ok(read) => {
                bytes.extend_from_slice(&chunk[..read]);
                if bytes.len() > USAGE_BROKER_MAX_FRAME_BYTES {
                    return Err(protocol_error());
                }
                if bytes.last() == Some(&b'\n') {
                    return serde_json::from_slice(&bytes).map_err(|_| protocol_error());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::park_timeout(Duration::from_millis(1));
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return Err(unavailable()),
        }
    }
    Err(unavailable())
}
