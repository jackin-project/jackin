use std::sync::{Mutex, MutexGuard, PoisonError};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualClockAdvanceError {
    Overflow,
}

impl std::fmt::Display for ManualClockAdvanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overflow => f.write_str("manual clock advance exceeds representable time"),
        }
    }
}

impl std::error::Error for ManualClockAdvanceError {}

#[derive(Debug, Clone, Copy)]
struct ClockState {
    monotonic: Instant,
    system: SystemTime,
}

#[derive(Debug)]
pub struct ManualClock {
    state: Mutex<ClockState>,
}

impl ManualClock {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(ClockState {
                monotonic: Instant::now(),
                system: SystemTime::now(),
            }),
        }
    }

    pub fn with_system_base(system_base: SystemTime) -> Self {
        Self {
            state: Mutex::new(ClockState {
                monotonic: Instant::now(),
                system: system_base,
            }),
        }
    }

    fn lock_state(&self) -> MutexGuard<'_, ClockState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn advance(&self, by: Duration) -> Result<(), ManualClockAdvanceError> {
        let mut state = self.lock_state();
        let monotonic = state
            .monotonic
            .checked_add(by)
            .ok_or(ManualClockAdvanceError::Overflow)?;
        let system = state
            .system
            .checked_add(by)
            .ok_or(ManualClockAdvanceError::Overflow)?;

        *state = ClockState { monotonic, system };
        Ok(())
    }
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        self.lock_state().monotonic
    }

    fn now_system(&self) -> SystemTime {
        self.lock_state().system
    }
}

#[cfg(test)]
mod tests;
