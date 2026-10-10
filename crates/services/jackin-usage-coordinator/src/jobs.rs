// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Worker messages and coordinator handle.

use std::sync::Arc;
use std::sync::mpsc::SyncSender;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use jackin_protocol::usage_broker::{UsageAccountCapability, UsageCredentialScope};

use super::Shared;

#[derive(Debug)]
pub(crate) struct ProbeJob {
    pub(crate) capability: UsageAccountCapability,
    pub(crate) generation: u64,
    /// Admission time for this generation. It remains distinct from provider
    /// invocation time so queue wait never consumes the Claude attempt floor.
    pub(crate) admitted_at_epoch: i64,
    pub(crate) admitted_at_monotonic: Duration,
    pub(crate) catalog_revision: Option<String>,
    pub(crate) credential_scope: Option<UsageCredentialScope>,
}

pub(crate) trait MonotonicClock: Send + Sync {
    fn now(&self) -> Duration;
}

pub(crate) struct SystemMonotonicClock {
    origin: Instant,
}

impl Default for SystemMonotonicClock {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl MonotonicClock for SystemMonotonicClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }
}

pub(crate) enum WorkerMessage {
    Probe(ProbeJob),
    Shutdown,
}

/// Host-authoritative refresh coordinator.
pub struct UsageCoordinator {
    pub(crate) shared: Arc<Shared>,
    pub(crate) jobs: SyncSender<WorkerMessage>,
    pub(crate) workers: Vec<JoinHandle<()>>,
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
