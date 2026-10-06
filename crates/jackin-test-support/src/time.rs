//! Controllable clock helpers for broker cadence tests.
//!
//! [`ManualClock`] is a shared, thread-safe fake clock: tests set or advance
//! it explicitly instead of sleeping, so cadence/interval logic runs
//! deterministically. Standard library only.
//!
//! # Example
//!
//! ```
//! use jackin_test_support::time::ManualClock;
//! use std::time::Duration;
//!
//! let clock = ManualClock::epoch();
//! let start = clock.now();
//! clock.advance(Duration::from_secs(30));
//! assert_eq!(clock.elapsed_since(start), Duration::from_secs(30));
//! assert!(clock.is_due(start, Duration::from_secs(30)));
//! ```

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// A manually advanced clock shared by clone (`Arc` + `Mutex`).
///
/// Clones observe the same time, so a broker under test on one thread and
/// the test driver on another stay in sync without sleeps.
#[derive(Debug, Clone)]
pub struct ManualClock {
    now: Arc<Mutex<SystemTime>>,
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::epoch()
    }
}

impl ManualClock {
    /// Clock starting at `start`.
    #[must_use]
    pub fn new(start: SystemTime) -> Self {
        Self {
            now: Arc::new(Mutex::new(start)),
        }
    }

    /// Clock starting at the Unix epoch (stable snapshot-friendly origin).
    #[must_use]
    pub fn epoch() -> Self {
        Self::new(SystemTime::UNIX_EPOCH)
    }

    /// Current fake time.
    ///
    /// A poisoned mutex yields the Unix epoch rather than panicking: test
    /// drivers must stay usable after a worker thread failure.
    #[must_use]
    pub fn now(&self) -> SystemTime {
        self.now
            .lock()
            .map_or(SystemTime::UNIX_EPOCH, |guard| *guard)
    }

    /// Move the clock to `time` (backwards jumps allowed).
    pub fn set(&self, time: SystemTime) {
        if let Ok(mut guard) = self.now.lock() {
            *guard = time;
        }
    }

    /// Move the clock forward by `delta`, returning the new time.
    pub fn advance(&self, delta: Duration) -> SystemTime {
        let mut new = self.now();
        if let Ok(mut guard) = self.now.lock() {
            *guard += delta;
            new = *guard;
        }
        new
    }

    /// `now - earlier`, saturating at zero when `earlier` is in the future.
    #[must_use]
    pub fn elapsed_since(&self, earlier: SystemTime) -> Duration {
        self.now().duration_since(earlier).unwrap_or_default()
    }

    /// Deadline `interval` after `from`, for cadence scheduling.
    #[must_use]
    pub fn deadline(&self, from: SystemTime, interval: Duration) -> SystemTime {
        from + interval
    }

    /// True when at least `interval` has passed since `from`.
    #[must_use]
    pub fn is_due(&self, from: SystemTime, interval: Duration) -> bool {
        self.elapsed_since(from) >= interval
    }
}

#[cfg(test)]
mod tests;
