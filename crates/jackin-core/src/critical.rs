// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Condvar, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

#[derive(Debug, Default)]
struct Writes {
    in_flight: Mutex<usize>,
    settled: Condvar,
}

impl Writes {
    fn count(&self) -> MutexGuard<'_, usize> {
        self.in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

static PROCESS: LazyLock<WriteTracker> = LazyLock::new(WriteTracker::new);

#[derive(Clone, Debug, Default)]
pub struct WriteTracker(Arc<Writes>);

impl WriteTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn process() -> Self {
        PROCESS.clone()
    }

    pub fn begin(&self) -> CriticalWrite {
        let mut in_flight = self.0.count();
        *in_flight = in_flight.saturating_add(1);
        CriticalWrite(self.clone())
    }

    pub fn settle(&self, limit: Duration) -> bool {
        let (in_flight, _) = self
            .0
            .settled
            .wait_timeout_while(self.0.count(), limit, |in_flight| *in_flight > 0)
            .unwrap_or_else(PoisonError::into_inner);
        *in_flight == 0
    }
}

#[derive(Debug)]
#[must_use = "the write is critical only while the guard is held"]
pub struct CriticalWrite(WriteTracker);

impl CriticalWrite {
    pub fn begin() -> Self {
        WriteTracker::process().begin()
    }
}

impl Drop for CriticalWrite {
    fn drop(&mut self) {
        let mut in_flight = self.0.0.count();
        *in_flight = in_flight.saturating_sub(1);
        if *in_flight == 0 {
            self.0.0.settled.notify_all();
        }
    }
}

pub fn settle_critical_writes(limit: Duration) -> bool {
    WriteTracker::process().settle(limit)
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Instant;

    use super::*;

    #[test]
    fn settling_waits_for_each_write_and_gives_up_at_the_limit() {
        let writes = WriteTracker::new();
        assert!(writes.settle(Duration::ZERO));
        let held = writes.begin();
        let started = Instant::now();
        assert!(!writes.settle(Duration::from_millis(100)));
        assert!(started.elapsed() >= Duration::from_millis(100));
        let (done, finished) = mpsc::channel();
        let releaser = std::thread::spawn(move || {
            let (_hold, wait) = mpsc::channel::<()>();
            let _timed_out = wait.recv_timeout(Duration::from_millis(150)).is_err();
            drop(held);
            done.send(()).is_ok()
        });
        assert!(writes.settle(Duration::from_secs(5)));
        assert_eq!(finished.recv_timeout(Duration::from_secs(5)), Ok(()));
        assert_eq!(releaser.join().ok(), Some(true));
        assert!(writes.settle(Duration::ZERO));
    }

    #[test]
    fn trackers_do_not_see_each_others_writes() {
        let (first, second) = (WriteTracker::new(), WriteTracker::new());
        let held = first.begin();
        assert!(!first.settle(Duration::ZERO));
        assert!(second.settle(Duration::ZERO));
        drop(held);
        assert!(first.settle(Duration::ZERO));
    }

    #[test]
    fn the_process_tracker_is_shared_by_every_handle() {
        let held = CriticalWrite::begin();
        assert!(!WriteTracker::process().settle(Duration::ZERO));
        drop(held);
        assert!(WriteTracker::process().settle(Duration::ZERO));
    }
}
