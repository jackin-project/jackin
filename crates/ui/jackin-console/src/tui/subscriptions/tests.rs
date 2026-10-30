// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::time::Duration;

#[test]
fn instance_refresh_throttle_plan_starts_when_due() {
    let interval = Duration::from_millis(500);
    let now = std::time::Instant::now();
    let plan = instance_refresh_throttle_plan(
        InstanceRefreshThrottleState {
            in_flight: false,
            last_refresh: None,
            interval,
            generation: 7,
        },
        now,
    );
    assert_eq!(
        plan,
        InstanceRefreshThrottlePlan {
            last_refresh: Some(now),
            generation: 8,
            start_generation: Some(8),
        }
    );
    let stale = now
        .checked_sub(Duration::from_secs(10))
        .expect("stale instant");
    let plan = instance_refresh_throttle_plan(
        InstanceRefreshThrottleState {
            in_flight: false,
            last_refresh: Some(stale),
            interval,
            generation: 8,
        },
        now,
    );
    assert_eq!(
        plan,
        InstanceRefreshThrottlePlan {
            last_refresh: Some(now),
            generation: 9,
            start_generation: Some(9),
        }
    );
}

#[test]
fn instance_refresh_throttle_plan_waits_while_in_flight_or_recent() {
    let interval = Duration::from_millis(500);
    let now = std::time::Instant::now();
    let stale = now
        .checked_sub(Duration::from_secs(10))
        .expect("stale instant");
    let plan = instance_refresh_throttle_plan(
        InstanceRefreshThrottleState {
            in_flight: true,
            last_refresh: Some(stale),
            interval,
            generation: 7,
        },
        now,
    );
    assert_eq!(
        plan,
        InstanceRefreshThrottlePlan {
            last_refresh: Some(stale),
            generation: 7,
            start_generation: None,
        }
    );
    let plan = instance_refresh_throttle_plan(
        InstanceRefreshThrottleState {
            in_flight: false,
            last_refresh: Some(now),
            interval,
            generation: 7,
        },
        now,
    );
    assert_eq!(
        plan,
        InstanceRefreshThrottlePlan {
            last_refresh: Some(now),
            generation: 7,
            start_generation: None,
        }
    );
}
