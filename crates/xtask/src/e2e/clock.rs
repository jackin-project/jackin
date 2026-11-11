// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;
use std::time::{Duration, Instant};

use super::verdict::StepId;

pub trait Clock: Send + Sync + Debug {
    fn now(&self) -> Duration;
}

#[derive(Clone, Copy, Debug)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    #[must_use]
    pub fn new() -> SystemClock {
        SystemClock {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> SystemClock {
        SystemClock::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }
}

#[derive(Clone, Debug)]
pub struct Budgets {
    pub step_caps: [Duration; 18],
    pub command: Duration,
    pub exec: Duration,
    pub expect: Duration,
    pub cleanup_action: Duration,
    pub cleanup_settle: Duration,
    pub cleanup_total: Duration,
    pub global: Duration,
    pub usage_backoff: Duration,
    pub poll: Duration,
    pub key_gap: Duration,
    pub codex_age: Duration,
}

const fn secs(count: u64) -> Duration {
    Duration::from_secs(count)
}

impl Budgets {
    #[must_use]
    pub fn production() -> Budgets {
        Budgets {
            step_caps: [
                secs(1200),
                secs(30),
                secs(60),
                secs(600),
                secs(420),
                secs(90),
                secs(120),
                secs(420),
                secs(180),
                secs(300),
                secs(60),
                secs(180),
                secs(180),
                secs(180),
                secs(600),
                secs(1200),
                secs(90),
                secs(1200),
            ],
            command: secs(600),
            exec: secs(300),
            expect: secs(90),
            cleanup_action: secs(60),
            cleanup_settle: secs(60),
            cleanup_total: secs(300),
            global: secs(9000),
            usage_backoff: secs(20),
            poll: secs(2),
            key_gap: Duration::from_millis(500),
            codex_age: secs(15),
        }
    }

    #[must_use]
    pub fn uniform(cap: Duration) -> Budgets {
        Budgets {
            step_caps: [cap; 18],
            command: cap,
            exec: cap,
            expect: cap,
            cleanup_action: cap,
            cleanup_settle: cap,
            cleanup_total: cap * 4,
            global: cap * 40,
            usage_backoff: Duration::from_millis(5),
            poll: Duration::from_millis(5),
            key_gap: Duration::from_millis(5),
            codex_age: Duration::from_secs(2),
        }
    }

    #[must_use]
    pub fn step(&self, id: StepId) -> Duration {
        self.step_caps[id as usize]
    }
}
