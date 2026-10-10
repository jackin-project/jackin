// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Worker messages and coordinator handle.

use std::sync::Arc;
use std::sync::mpsc::SyncSender;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
#[cfg(not(test))]
use std::time::{SystemTime, UNIX_EPOCH};

use jackin_protocol::usage_broker::{UsageAccountCapability, UsageCredentialScope};

use super::Shared;

#[derive(Debug)]
pub(crate) struct ProbeJob {
    pub(crate) capability: UsageAccountCapability,
    pub(crate) generation: u64,
    /// Paired wall and monotonic sample taken while admitting this generation.
    /// It remains distinct from provider invocation time so queue wait never
    /// consumes the Claude attempt floor.
    pub(crate) admitted_at_epoch: i64,
    pub(crate) catalog_revision: Option<String>,
    pub(crate) credential_scope: Option<UsageCredentialScope>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ClockSample {
    /// Wall time since the Unix epoch, including its fractional second.
    pub(crate) wall_epoch: Duration,
    /// Monotonic time from the coordinator clock origin.
    pub(crate) monotonic: Duration,
}

impl ClockSample {
    pub(crate) fn anchored(epoch_seconds: i64, monotonic: Duration) -> Self {
        let wall_epoch =
            Duration::from_secs(u64::try_from(epoch_seconds.max(0)).unwrap_or(u64::MAX));
        Self {
            wall_epoch,
            monotonic,
        }
    }

    pub(crate) fn floor_epoch(self) -> i64 {
        i64::try_from(self.wall_epoch.as_secs()).unwrap_or(i64::MAX)
    }

    /// Round upward so a persisted deadline never starts before this sample.
    pub(crate) fn ceil_epoch(self) -> i64 {
        self.floor_epoch()
            .saturating_add(if self.wall_epoch.subsec_nanos() != 0 {
                1
            } else {
                0
            })
    }
}

pub(crate) trait MonotonicClock: Send + Sync {
    fn now(&self) -> Duration;

    /// Return paired wall and monotonic values. The fallback anchors unit-test
    /// timelines whose callers supply fixture epochs instead of host time.
    fn sample(&self, fallback_epoch: i64) -> ClockSample {
        ClockSample::anchored(fallback_epoch, self.now())
    }
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

    fn sample(&self, fallback_epoch: i64) -> ClockSample {
        #[cfg(test)]
        {
            let monotonic =
                Duration::from_secs(u64::try_from(fallback_epoch.max(0)).unwrap_or(u64::MAX));
            ClockSample::anchored(fallback_epoch, monotonic)
        }
        #[cfg(not(test))]
        {
            let _ = fallback_epoch;
            ClockSample {
                wall_epoch: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default(),
                monotonic: self.now(),
            }
        }
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
