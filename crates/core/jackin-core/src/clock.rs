use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

pub trait Clock: Send + Sync + std::fmt::Debug {
    fn now(&self) -> Instant;

    fn now_system(&self) -> SystemTime;
}

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

#[derive(Debug)]
pub struct ManualClock {
    base: Instant,
    system_base: SystemTime,
    offset_ns: AtomicU64,
}

impl ManualClock {
    pub fn new() -> Self {
        Self {
            base: Instant::now(),
            system_base: SystemTime::now(),
            offset_ns: AtomicU64::new(0),
        }
    }

    pub fn with_system_base(system_base: SystemTime) -> Self {
        Self {
            base: Instant::now(),
            system_base,
            offset_ns: AtomicU64::new(0),
        }
    }

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
