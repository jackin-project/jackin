// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Immediately admitted, bounded long polls never occupy control workers.

use std::os::unix::net::UnixStream;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::{
    BrokerCatalogRefresh, MonitorStore, dispatch, monitor::MONITOR_WATCH_TIMEOUT_CAP_MS,
    unavailable, write_response,
};
use jackin_protocol::usage_broker::{
    UsageBrokerOperation, UsageBrokerRequest, UsageBrokerResponse,
};
use jackin_protocol::usage_monitor::MonitorOperation;
use jackin_usage_coordinator::UsageCoordinator;

// The broker contract includes twenty simultaneous Capsule clients plus Desktop.
// Excess admission fails immediately; no short wait can queue behind a long one.
const MAX_WAIT_TASKS: usize = 32;

pub(super) struct WaitPool {
    coordinator: Arc<UsageCoordinator>,
    build_id: Arc<str>,
    publisher: crate::publish::ProjectionPublisher,
    monitor_store: Arc<MonitorStore>,
    shutdown: Arc<AtomicBool>,
    catalog_refresh: Option<Arc<BrokerCatalogRefresh>>,
    workers: Mutex<Vec<std::thread::JoinHandle<()>>>,
}

impl WaitPool {
    pub(super) fn new(
        coordinator: Arc<UsageCoordinator>,
        build_id: Arc<str>,
        publisher: crate::publish::ProjectionPublisher,
        monitor_store: Arc<MonitorStore>,
        shutdown: Arc<AtomicBool>,
        catalog_refresh: Option<Arc<BrokerCatalogRefresh>>,
    ) -> Self {
        Self {
            coordinator,
            build_id,
            publisher,
            monitor_store,
            shutdown,
            catalog_refresh,
            workers: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn enqueue(&self, mut stream: UnixStream, mut request: UsageBrokerRequest) {
        let accepted = Instant::now();
        let Ok(mut workers) = self.workers.lock() else {
            reject(&mut stream);
            return;
        };
        let mut index = 0;
        while index < workers.len() {
            if workers[index].is_finished() {
                drop(workers.swap_remove(index).join());
            } else {
                index += 1;
            }
        }
        if workers.len() >= MAX_WAIT_TASKS {
            drop(workers);
            reject(&mut stream);
            return;
        }
        let Ok(mut worker_stream) = stream.try_clone() else {
            drop(workers);
            reject(&mut stream);
            return;
        };
        let coordinator = Arc::clone(&self.coordinator);
        let build_id = Arc::clone(&self.build_id);
        let publisher = self.publisher.clone();
        let monitor_store = Arc::clone(&self.monitor_store);
        let shutdown = Arc::clone(&self.shutdown);
        let catalog_refresh = self.catalog_refresh.clone();
        if let Ok(worker) = jackin_telemetry::spawn::thread_joined_named(
            "usage-broker-wait".to_owned(),
            move || {
                account_for_dispatch_time(&mut request.operation, accepted.elapsed());
                let response = dispatch(
                    &coordinator,
                    request,
                    &build_id,
                    &publisher,
                    &monitor_store,
                    &shutdown,
                    catalog_refresh.as_deref(),
                );
                write_response(&mut worker_stream, response);
            },
        ) {
            workers.push(worker);
        } else {
            drop(workers);
            reject(&mut stream);
        }
    }
}

impl Drop for WaitPool {
    fn drop(&mut self) {
        let workers = self
            .workers
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // No queued waits exist. Each accepted join has at most 30 seconds;
        // response writing has its own absolute deadline.
        for worker in workers.drain(..) {
            drop(worker.join());
        }
    }
}

fn reject(stream: &mut UnixStream) {
    write_response(
        stream,
        UsageBrokerResponse::Error {
            error: unavailable(),
        },
    );
}

pub(super) const fn is_wait(operation: &UsageBrokerOperation) -> bool {
    matches!(
        operation,
        UsageBrokerOperation::Join { .. }
            | UsageBrokerOperation::JoinForCapability { .. }
            | UsageBrokerOperation::JoinPublication { .. }
            | UsageBrokerOperation::JoinPublicationForSurface { .. }
            | UsageBrokerOperation::Monitor {
                request: MonitorOperation::Watch { .. }
            }
    )
}

fn account_for_dispatch_time(operation: &mut UsageBrokerOperation, elapsed: Duration) {
    let (UsageBrokerOperation::Join { timeout_ms, .. }
    | UsageBrokerOperation::JoinForCapability { timeout_ms, .. }
    | UsageBrokerOperation::JoinPublication { timeout_ms, .. }
    | UsageBrokerOperation::JoinPublicationForSurface { timeout_ms, .. }
    | UsageBrokerOperation::Monitor {
        request: MonitorOperation::Watch { timeout_ms, .. },
    }) = operation
    else {
        return;
    };
    *timeout_ms = (*timeout_ms)
        .min(MONITOR_WATCH_TIMEOUT_CAP_MS)
        .saturating_sub(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX));
}
