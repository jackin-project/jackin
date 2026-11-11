// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{Clock, Timestamp};
use tokio::sync::Notify;

use crate::sync::locked;

const START: i64 = 1_767_225_600;
const WAKE_YIELDS: u32 = 64;

#[derive(Debug)]
pub struct FakeClock {
    state: Mutex<ClockState>,
    working: AtomicUsize,
    quiet: Notify,
}

#[derive(Debug)]
pub struct Working {
    clock: Arc<FakeClock>,
}

impl Drop for Working {
    fn drop(&mut self) {
        if self.clock.working.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.clock.quiet.notify_waiters();
        }
    }
}

#[derive(Debug, Default)]
struct ClockState {
    offset: i32,
    elapsed: Duration,
    slept: Vec<Duration>,
    waiting: BTreeMap<u64, Duration>,
    next: u64,
}

struct Waiter<'a> {
    state: &'a Mutex<ClockState>,
    id: u64,
}

impl Drop for Waiter<'_> {
    fn drop(&mut self) {
        locked(self.state).waiting.remove(&self.id);
    }
}

impl FakeClock {
    pub fn new() -> FakeClock {
        FakeClock {
            state: Mutex::new(ClockState::default()),
            working: AtomicUsize::new(0),
            quiet: Notify::new(),
        }
    }

    pub fn working(self: &Arc<FakeClock>) -> Working {
        self.working.fetch_add(1, Ordering::SeqCst);
        Working {
            clock: Arc::clone(self),
        }
    }

    async fn until_quiet(&self) {
        loop {
            let quiet = self.quiet.notified();
            let mut quiet = std::pin::pin!(quiet);
            quiet.as_mut().enable();
            if self.working.load(Ordering::SeqCst) == 0 {
                return;
            }
            quiet.await;
        }
    }

    pub fn advance(&self, by: Duration) {
        locked(&self.state).elapsed += by;
    }

    pub fn set_utc_offset(&self, seconds: i32) {
        locked(&self.state).offset = seconds;
    }

    pub fn slept(&self) -> Vec<Duration> {
        locked(&self.state).slept.clone()
    }

    pub fn sleepers(&self) -> usize {
        locked(&self.state).waiting.len()
    }

    fn progress(&self) -> (usize, Duration) {
        let state = locked(&self.state);
        (state.slept.len(), state.elapsed)
    }

    pub async fn until_stalled(&self, window: Duration) {
        loop {
            let seen = self.progress();
            tokio::time::sleep(window).await;
            if self.progress() == seen {
                return;
            }
        }
    }
}

impl Default for FakeClock {
    fn default() -> FakeClock {
        FakeClock::new()
    }
}

#[async_trait]
impl Clock for FakeClock {
    fn now(&self) -> Timestamp {
        let elapsed = locked(&self.state).elapsed.as_secs();
        Timestamp(START.saturating_add(i64::try_from(elapsed).unwrap_or(i64::MAX)))
    }

    fn utc_offset_secs(&self) -> i32 {
        locked(&self.state).offset
    }

    async fn sleep(&self, duration: Duration) {
        let (id, deadline) = {
            let mut state = locked(&self.state);
            let id = state.next;
            state.next += 1;
            let deadline = state.elapsed + duration;
            state.waiting.insert(id, deadline);
            (id, deadline)
        };
        let _waiter = Waiter {
            state: &self.state,
            id,
        };
        let mut yields = 0_u32;
        loop {
            self.until_quiet().await;
            tokio::task::yield_now().await;
            yields += 1;
            let mut state = locked(&self.state);
            let earliest = state.waiting.values().all(|other| *other >= deadline);
            let quiet = self.working.load(Ordering::SeqCst) == 0;
            if state.elapsed >= deadline || (quiet && yields >= WAKE_YIELDS && earliest) {
                state.elapsed = state.elapsed.max(deadline);
                state.slept.push(duration);
                return;
            }
            if !quiet {
                yields = 0;
            }
        }
    }
}
