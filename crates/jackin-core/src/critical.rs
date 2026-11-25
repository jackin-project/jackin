// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

struct Writes {
    in_flight: Mutex<usize>,
    settled: Condvar,
}

static WRITES: Writes = Writes {
    in_flight: Mutex::new(0),
    settled: Condvar::new(),
};

fn count() -> MutexGuard<'static, usize> {
    WRITES
        .in_flight
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug)]
#[must_use = "the write is critical only while the guard is held"]
pub struct CriticalWrite(());

impl CriticalWrite {
    pub fn begin() -> Self {
        let mut in_flight = count();
        *in_flight = in_flight.saturating_add(1);
        Self(())
    }
}

impl Drop for CriticalWrite {
    fn drop(&mut self) {
        let mut in_flight = count();
        *in_flight = in_flight.saturating_sub(1);
        if *in_flight == 0 {
            WRITES.settled.notify_all();
        }
    }
}

pub fn settle_critical_writes(limit: Duration) -> bool {
    let (in_flight, _) = WRITES
        .settled
        .wait_timeout_while(count(), limit, |in_flight| *in_flight > 0)
        .unwrap_or_else(PoisonError::into_inner);
    *in_flight == 0
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Instant;

    use super::*;

    #[test]
    fn settling_waits_for_each_write_and_gives_up_at_the_limit() {
        assert!(settle_critical_writes(Duration::ZERO));
        let held = CriticalWrite::begin();
        let started = Instant::now();
        assert!(!settle_critical_writes(Duration::from_millis(100)));
        assert!(started.elapsed() >= Duration::from_millis(100));
        let (done, finished) = mpsc::channel();
        let releaser = std::thread::spawn(move || {
            let (_hold, wait) = mpsc::channel::<()>();
            let _timed_out = wait.recv_timeout(Duration::from_millis(150)).is_err();
            drop(held);
            done.send(()).is_ok()
        });
        assert!(settle_critical_writes(Duration::from_secs(5)));
        assert_eq!(finished.recv_timeout(Duration::from_secs(5)), Ok(()));
        assert_eq!(releaser.join().ok(), Some(true));
        assert!(settle_critical_writes(Duration::ZERO));
    }
}
