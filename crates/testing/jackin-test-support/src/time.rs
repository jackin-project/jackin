use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualClockError {
    Overflow,
}

impl std::fmt::Display for ManualClockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overflow => f.write_str("manual clock time exceeds representable range"),
        }
    }
}

impl std::error::Error for ManualClockError {}

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
    #[must_use]
    pub fn new(start: SystemTime) -> Self {
        Self {
            now: Arc::new(Mutex::new(start)),
        }
    }

    #[must_use]
    pub fn epoch() -> Self {
        Self::new(SystemTime::UNIX_EPOCH)
    }

    fn lock_now(&self) -> MutexGuard<'_, SystemTime> {
        self.now.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[must_use]
    pub fn now(&self) -> SystemTime {
        *self.lock_now()
    }

    pub fn set(&self, time: SystemTime) {
        *self.lock_now() = time;
    }

    pub fn advance(&self, delta: Duration) -> Result<SystemTime, ManualClockError> {
        let mut now = self.lock_now();
        let next = now.checked_add(delta).ok_or(ManualClockError::Overflow)?;
        *now = next;
        Ok(next)
    }

    #[must_use]
    pub fn elapsed_since(&self, earlier: SystemTime) -> Duration {
        self.now().duration_since(earlier).unwrap_or_default()
    }

    pub fn deadline(
        &self,
        from: SystemTime,
        interval: Duration,
    ) -> Result<SystemTime, ManualClockError> {
        from.checked_add(interval).ok_or(ManualClockError::Overflow)
    }

    #[must_use]
    pub fn is_due(&self, from: SystemTime, interval: Duration) -> bool {
        self.elapsed_since(from) >= interval
    }
}
