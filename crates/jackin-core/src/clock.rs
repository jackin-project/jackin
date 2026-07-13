//! Injected time source. Production code takes a `Clock` (or a `&dyn Clock`)
//! wherever behavior depends on elapsed time, so tests advance time
//! deterministically instead of sleeping. First consumer: capsule clipboard
//! transfer expiry. Do not use for content addressing / file naming — only
//! for behavior that varies with time.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Injected time source for elapsed-time behavior.
///
/// Production code holds a `Clock` (often `Arc<dyn Clock>`) so tests can
/// substitute [`ManualClock`] and advance time without sleeping.
pub trait Clock: Send + Sync + std::fmt::Debug {
    /// Current instant according to this clock.
    fn now(&self) -> Instant;
}

/// The real wall clock. The default everywhere outside tests.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Deterministic test clock: starts at an arbitrary epoch, advances only via
/// [`ManualClock::advance`]. Lives in production code (not `cfg(test)`) so
/// downstream crates' tests can use it without a test-support feature dance;
/// it is inert unless constructed.
#[derive(Debug)]
pub struct ManualClock {
    base: Instant,
    offset_ns: AtomicU64,
}

impl ManualClock {
    /// Create a manual clock starting at an arbitrary epoch (offset zero).
    pub fn new() -> Self {
        Self {
            base: Instant::now(),
            offset_ns: AtomicU64::new(0),
        }
    }

    /// Advance the clock by `by`. Concurrent advances accumulate.
    pub fn advance(&self, by: Duration) {
        let add = u64::try_from(by.as_nanos()).unwrap_or(u64::MAX);
        self.offset_ns.fetch_add(add, Ordering::Relaxed);
    }
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        let offset = self.offset_ns.load(Ordering::Relaxed);
        self.base + Duration::from_nanos(offset)
    }
}

#[cfg(test)]
mod tests;
