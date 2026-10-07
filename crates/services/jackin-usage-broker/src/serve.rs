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
    BROKER_CONNECTION_QUEUE, BROKER_CONNECTION_WORKERS, BrokerStartupCleanup, PUBLISH_TICK,
    ServePolicy, dispatch, protocol_error, publish, unavailable, waits,
};

pub(crate) struct ServeConfig {
    pub(crate) listener: UnixListener,
    pub(crate) coordinator: Arc<UsageCoordinator>,
    pub(crate) build_id: String,
    pub(crate) cleanup: BrokerStartupCleanup,
    pub(crate) policy: ServePolicy,
    pub(crate) publisher: publish::ProjectionPublisher,
}

pub(crate) fn serve(config: ServeConfig) {
    let ServeConfig {
        listener,
        coordinator,
        build_id,
        mut cleanup,
        policy,
        publisher,
    } = config;
    let (connections, receiver) = mpsc::sync_channel(BROKER_CONNECTION_QUEUE);
    let receiver = Arc::new(Mutex::new(receiver));
    let build_id = Arc::<str>::from(build_id.as_str());
    let wait_pool = waits::WaitPool::new(
        Arc::clone(&coordinator),
        Arc::clone(&build_id),
        publisher.clone(),
    );
    let wait_pool = Arc::new(wait_pool);
    let mut workers = Vec::with_capacity(BROKER_CONNECTION_WORKERS);
    for index in 0..BROKER_CONNECTION_WORKERS {
        let receiver = Arc::clone(&receiver);
        let coordinator = Arc::clone(&coordinator);
        let build_id = Arc::clone(&build_id);
        let publisher = publisher.clone();
        let wait_pool = Arc::clone(&wait_pool);
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
                handle_stream(stream, &coordinator, &build_id, &publisher, &wait_pool);
            },
        );
        match worker {
            Ok(worker) => workers.push(worker),
            Err(_) => break,
        }
    }
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
    // Incremental publisher: while any generation is active, merge completed
    // accounts into the canonical projection as they finish. One stalled
    // account never blocks healthy accounts; dispatch-path publishing covers
    // promptness when this ticker cannot spawn.
    let publisher_shutdown = Arc::new(AtomicBool::new(false));
    let ticker = {
        let publisher = publisher.clone();
        let ticker_coordinator = Arc::clone(&coordinator);
        let shutdown = Arc::clone(&publisher_shutdown);
        jackin_telemetry::spawn::thread_joined_named(
            "usage-broker-publisher".to_owned(),
            move || {
                while !shutdown.load(Ordering::Relaxed) {
                    std::thread::park_timeout(PUBLISH_TICK);
                    if shutdown.load(Ordering::Relaxed) {
                        break;
                    }
                    if !ticker_coordinator.is_idle() {
                        publisher.publish_due(chrono::Utc::now().timestamp());
                    }
                }
            },
        )
        .ok()
    };
    let started = Instant::now();
    let mut last_activity = started;
    let mut last_renewal = started;
    loop {
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
                if now.duration_since(last_activity) >= policy.idle_exit && coordinator.is_idle() {
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

pub(crate) fn handle_stream(
    mut stream: UnixStream,
    coordinator: &UsageCoordinator,
    build_id: &str,
    publisher: &publish::ProjectionPublisher,
    waits: &waits::WaitPool,
) {
    let response = match read_request(&mut stream) {
        Ok(request) if waits::is_wait(&request.operation) => {
            waits.enqueue(stream, request);
            return;
        }
        Ok(request) => dispatch(coordinator, request, build_id, publisher),
        Err(error) => UsageBrokerResponse::Error { error },
    };
    write_response(&mut stream, response);
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
