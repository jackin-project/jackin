// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{Clock, Timestamp};

use crate::sync::locked;

const START: i64 = 1_767_225_600;
const WAKE_YIELDS: u32 = 64;

#[derive(Debug)]
pub struct FakeClock {
    state: Mutex<ClockState>,
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
            tokio::task::yield_now().await;
            yields += 1;
            let mut state = locked(&self.state);
            let earliest = state.waiting.values().all(|other| *other >= deadline);
            if state.elapsed >= deadline || (yields >= WAKE_YIELDS && earliest) {
                state.elapsed = state.elapsed.max(deadline);
                state.slept.push(duration);
                return;
            }
        }
    }
}
