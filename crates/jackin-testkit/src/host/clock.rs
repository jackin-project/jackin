// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{Clock, Timestamp};

use crate::sync::locked;

const START: i64 = 1_767_225_600;

#[derive(Debug)]
pub struct FakeClock {
    state: Mutex<ClockState>,
}

#[derive(Debug, Default)]
struct ClockState {
    elapsed: Duration,
    slept: Vec<Duration>,
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

    async fn sleep(&self, duration: Duration) {
        {
            let mut state = locked(&self.state);
            state.elapsed += duration;
            state.slept.push(duration);
        }
        tokio::task::yield_now().await;
    }
}
