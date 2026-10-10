// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Worker messages and coordinator handle.

use std::sync::Arc;
use std::sync::mpsc::SyncSender;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

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

/// A paired wall-clock and monotonic-clock observation.
///
/// The coordinator uses wall time for persisted deadlines and monotonic time
/// for in-process minimum-attempt floors. Implementations must keep
/// `monotonic` nondecreasing and ensure the two values describe the same
/// instant.
#[derive(Debug, Clone, Copy)]
pub struct ClockSample {
    /// Wall time since the Unix epoch, including its fractional second.
    pub wall_epoch: Duration,
    /// Monotonic time from the coordinator clock origin.
    pub monotonic: Duration,
}

impl ClockSample {
    /// Create a sample from an epoch timestamp and its paired monotonic value.
    #[must_use]
    pub fn anchored(epoch_seconds: i64, monotonic: Duration) -> Self {
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

/// Supplies paired time samples to a usage coordinator.
///
/// Each coordinator owns a fresh clock origin. A clock passed to
/// [`UsageCoordinator::new_with_clock`] should therefore start its monotonic
/// counter at zero or another stable origin and advance it monotonically for
/// the lifetime of that coordinator.
pub trait MonotonicClock: Send + Sync {
    /// Return monotonic elapsed time from this clock instance's origin.
    fn now(&self) -> Duration;

    /// Pair the caller's deterministic epoch with this monotonic sample.
    /// Production clocks override this with current system wall time.
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
        let _ = fallback_epoch;
        ClockSample {
            wall_epoch: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default(),
            monotonic: self.now(),
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
