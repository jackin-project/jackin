//! Injected time source. Production code takes a `Clock` (or a `&dyn Clock`)
//! wherever behavior depends on elapsed or wall time, so tests advance time
//! deterministically instead of sleeping. First consumers: capsule clipboard
//! transfer expiry and observable expiry/retry/lifecycle boundaries. Do not
//! use for content addressing / file naming — only for behavior that varies
//! with time.
//!
//! Wall-clock face returns [`SystemTime`]; call sites that need
//! `chrono::DateTime<Utc>` convert at the boundary
//! (`DateTime::<Utc>::from(system_time)`) so this crate stays chrono-free.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

/// Injected time source for elapsed-time and wall-clock behavior.
///
/// Production code holds a `Clock` (often `Arc<dyn Clock>`) so tests can
/// substitute [`ManualClock`] and advance time without sleeping.
pub trait Clock: Send + Sync + std::fmt::Debug {
    /// Current monotonic instant according to this clock.
    fn now(&self) -> Instant;

    /// Current wall-clock time according to this clock.
    ///
    /// Used for TTL/expiry/lifecycle timestamps. Convert to
    /// `chrono::DateTime<Utc>` only at the call site that needs it.
    fn now_system(&self) -> SystemTime;
}

/// The real system clocks. The default everywhere outside tests.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn now_system(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// Deterministic test clock: starts at an arbitrary epoch, advances only via
/// [`ManualClock::advance`]. Lives in production code (not `cfg(test)`) so
/// downstream crates' tests can use it without a test-support feature dance;
/// it is inert unless constructed.
///
/// Both faces ([`Clock::now`] and [`Clock::now_system`]) share one offset so
/// a single `advance` moves monotonic and wall time together.
#[derive(Debug)]
pub struct ManualClock {
    base: Instant,
    system_base: SystemTime,
    offset_ns: AtomicU64,
}

impl ManualClock {
    /// Create a manual clock starting at an arbitrary epoch (offset zero).
    ///
    /// The wall-clock base is the real `SystemTime::now()` at construction so
    /// absolute dates remain plausible; tests that care about absolute wall
    /// time should use [`ManualClock::with_system_base`].
    pub fn new() -> Self {
        Self {
            base: Instant::now(),
            system_base: SystemTime::now(),
            offset_ns: AtomicU64::new(0),
        }
    }

    /// Create a manual clock with an explicit wall-clock base (tests that
    /// assert absolute expiry/lifecycle stamps).
    pub fn with_system_base(system_base: SystemTime) -> Self {
        Self {
            base: Instant::now(),
            system_base,
            offset_ns: AtomicU64::new(0),
        }
    }

    /// Advance both faces by `by`. Concurrent advances accumulate.
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

    fn now_system(&self) -> SystemTime {
        let offset = self.offset_ns.load(Ordering::Relaxed);
        self.system_base + Duration::from_nanos(offset)
    }
}

#[cfg(test)]
mod tests;
