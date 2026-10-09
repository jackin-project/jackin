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
    USAGE_BROKER_MAX_FRAME_BYTES, UsageBrokerRequest, UsageBrokerResponse, UsageCoordinationError,
};

use jackin_usage_coordinator::UsageCoordinator;

use crate::{
    BROKER_CONNECTION_QUEUE, BROKER_CONNECTION_WORKERS, BrokerCatalogRefresh, BrokerStartupCleanup,
    MonitorStore, PUBLISH_TICK, ServePolicy, dispatch, protocol_error, publish, unavailable, waits,
};

const CLOCK_WAKE_DETECTION_THRESHOLD_SECS: u64 = 2;

pub(crate) struct ServeConfig {
    pub(crate) listener: UnixListener,
    pub(crate) coordinator: Arc<UsageCoordinator>,
    pub(crate) build_id: String,
    pub(crate) cleanup: BrokerStartupCleanup,
    pub(crate) policy: ServePolicy,
    pub(crate) publisher: publish::ProjectionPublisher,
    pub(crate) monitor_store: Arc<MonitorStore>,
    pub(crate) catalog_refresh: Option<Arc<BrokerCatalogRefresh>>,
}

struct ConnectionContext {
    coordinator: Arc<UsageCoordinator>,
    build_id: Arc<str>,
    publisher: publish::ProjectionPublisher,
    monitor_store: Arc<MonitorStore>,
    shutdown: Arc<AtomicBool>,
    catalog_refresh: Option<Arc<BrokerCatalogRefresh>>,
    wait_pool: Arc<waits::WaitPool>,
}

pub(crate) fn serve(config: ServeConfig) {
    let ServeConfig {
        listener,
        coordinator,
        build_id,
        mut cleanup,
        policy,
        publisher,
        monitor_store,
        catalog_refresh,
    } = config;
    let (connections, receiver) = mpsc::sync_channel(BROKER_CONNECTION_QUEUE);
    let receiver = Arc::new(Mutex::new(receiver));
    let build_id = Arc::<str>::from(build_id.as_str());
    let shutdown = Arc::new(AtomicBool::new(false));
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
        catalog_refresh,
        wait_pool,
    });
    let workers = spawn_connection_workers(receiver, context);
    if workers.is_empty() {
        drop(listener);
        drop(cleanup);
        return;
    }
    if listener.set_nonblocking(true).is_err() {
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
    let mut last_activity = started;
    let mut last_renewal = started;
    loop {
        if shutdown.load(Ordering::Acquire) {
            break;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                last_activity = Instant::now();
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
                let now = Instant::now();
                if now.duration_since(last_renewal) >= policy.lease_renewal {
                    if !cleanup.renew(policy.lease_duration) {
                        break;
                    }
                    last_renewal = now;
                }
                if now.duration_since(last_activity) >= policy.idle_exit
                    && coordinator.is_idle()
                    && !monitor_store.has_active()
                {
                    break;
                }
                std::thread::park_timeout(Duration::from_millis(50));
            }
            Err(_) => break,
        }
    }
    drop(connections);
    for worker in workers {
        drop(worker.join());
    }
    publisher_shutdown.store(true, Ordering::Relaxed);
    if let Some(ticker) = ticker {
        drop(ticker.join());
    }
    drop(listener);
    drop(cleanup);
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
    let response = match read_request(&mut stream) {
        Ok(request) if waits::is_wait(&request.operation) => {
            context.wait_pool.enqueue(stream, request);
            return;
        }
        Ok(request) => dispatch(
            &context.coordinator,
            request,
            &context.build_id,
            &context.publisher,
            &context.monitor_store,
            &context.shutdown,
            context.catalog_refresh.as_deref(),
        ),
        Err(error) => UsageBrokerResponse::Error { error },
    };
    write_response(&mut stream, response);
}

fn spawn_publisher_ticker(
    publisher: publish::ProjectionPublisher,
    coordinator: Arc<UsageCoordinator>,
    monitor_store: Arc<MonitorStore>,
    shutdown: Arc<AtomicBool>,
) -> Option<std::thread::JoinHandle<()>> {
    // Incremental publication merges completed accounts as they finish. A
    // stalled account never blocks healthy accounts; dispatch also publishes.
    let initial_projection_id = publisher
        .current_projection()
        .ok()
        .map(|projection| projection.projection_id);
    let mut last_observed_projection_id = initial_projection_id;
    jackin_telemetry::spawn::thread_joined_named("usage-broker-publisher".to_owned(), move || {
        let mut last_ticked_wake = None;
        let mut last_tick_epoch = chrono::Utc::now().timestamp();
        let mut last_tick_instant = Instant::now();
        while !shutdown.load(Ordering::Relaxed) {
            std::thread::park_timeout(PUBLISH_TICK);
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            let now_epoch = chrono::Utc::now().timestamp();
            if wall_clock_wake_detected(last_tick_epoch, last_tick_instant.elapsed(), now_epoch) {
                let _recalculated = coordinator.note_wake(now_epoch);
            }
            last_tick_epoch = now_epoch;
            last_tick_instant = Instant::now();
            if !coordinator.is_idle() {
                publisher.publish_due(now_epoch);
            }
            if let Ok(projection) = publisher.current_projection()
                && last_observed_projection_id.as_deref() != Some(projection.projection_id.as_str())
            {
                let _ignored = monitor_store.observe_projection(&projection, now_epoch);
                last_observed_projection_id = Some(projection.projection_id);
            }
            let next_wake = monitor_store.next_wake();
            if let Some(wake_epoch) = next_wake
                && wake_epoch <= now_epoch
                && last_ticked_wake != Some(wake_epoch)
            {
                let _ignored = monitor_store.tick(now_epoch);
                last_ticked_wake = Some(wake_epoch);
            } else if next_wake.is_none_or(|wake_epoch| wake_epoch > now_epoch) {
                last_ticked_wake = None;
            }
        }
    })
    .ok()
}

pub(crate) fn wall_clock_wake_detected(
    last_tick_epoch: i64,
    monotonic_elapsed: Duration,
    now_epoch: i64,
) -> bool {
    let wall_elapsed = now_epoch.saturating_sub(last_tick_epoch);
    let monotonic_elapsed = i64::try_from(monotonic_elapsed.as_secs()).unwrap_or(i64::MAX);
    wall_elapsed.abs_diff(monotonic_elapsed) > CLOCK_WAKE_DETECTION_THRESHOLD_SECS
}

pub(crate) fn write_response(stream: &mut UnixStream, response: UsageBrokerResponse) {
    if let Ok(mut bytes) = serde_json::to_vec(&response)
        && bytes.len() < USAGE_BROKER_MAX_FRAME_BYTES
    {
        bytes.push(b'\n');
        write_with_deadline(stream, &bytes, Duration::from_secs(1));
    }
}

pub(crate) fn write_with_deadline(stream: &mut UnixStream, mut bytes: &[u8], timeout: Duration) {
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

pub(crate) fn read_request(
    stream: &mut UnixStream,
) -> Result<UsageBrokerRequest, UsageCoordinationError> {
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
