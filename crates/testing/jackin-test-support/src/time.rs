use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

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

    #[must_use]
    pub fn now(&self) -> SystemTime {
        self.now
            .lock()
            .map_or(SystemTime::UNIX_EPOCH, |guard| *guard)
    }

    pub fn set(&self, time: SystemTime) {
        if let Ok(mut guard) = self.now.lock() {
            *guard = time;
        }
    }

    pub fn advance(&self, delta: Duration) -> SystemTime {
        let mut new = self.now();
        if let Ok(mut guard) = self.now.lock() {
            *guard += delta;
            new = *guard;
        }
        new
    }

    #[must_use]
    pub fn elapsed_since(&self, earlier: SystemTime) -> Duration {
        self.now().duration_since(earlier).unwrap_or_default()
    }

    #[must_use]
    pub fn deadline(&self, from: SystemTime, interval: Duration) -> SystemTime {
        from + interval
    }

    #[must_use]
    pub fn is_due(&self, from: SystemTime, interval: Duration) -> bool {
        self.elapsed_since(from) >= interval
    }
}

#[cfg(test)]
mod tests;
