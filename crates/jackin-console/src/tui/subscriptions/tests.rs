// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `subscriptions`.
use super::*;

#[test]
fn worker_disconnect_messages_are_subscription_owned() {
    assert_eq!(
        drift_check_worker_disconnected_message(),
        "drift check worker disconnected"
    );
    assert_eq!(
        isolation_cleanup_worker_disconnected_message(),
        "isolation cleanup worker disconnected"
    );
    assert_eq!(
        role_loader_worker_disconnected_message(),
        "role loader worker disconnected"
    );
    assert_eq!(
        op_read_worker_disconnected_message(),
        "op read worker disconnected"
    );
    assert_eq!(
        instance_refresh_worker_disconnected_message(),
        "instance refresh worker disconnected"
    );
}

#[test]
fn instance_refresh_throttle_plan_starts_when_due() {
    let now = std::time::Instant::now();
    let plan = instance_refresh_throttle_plan(
        InstanceRefreshThrottleState {
            in_flight: false,
            last_refresh: None,
            generation: 41,
        },
        now,
    );

    assert_eq!(plan.last_refresh, Some(now));
    assert_eq!(plan.generation, 42);
    assert_eq!(plan.start_generation, Some(42));
}

#[test]
fn instance_refresh_throttle_plan_waits_while_in_flight_or_recent() {
    let now = std::time::Instant::now();
    let recent = now.checked_sub(INSTANCE_REFRESH_INTERVAL / 2).unwrap();
    let in_flight = instance_refresh_throttle_plan(
        InstanceRefreshThrottleState {
            in_flight: true,
            last_refresh: Some(recent),
            generation: 7,
        },
        now,
    );
    let throttled = instance_refresh_throttle_plan(
        InstanceRefreshThrottleState {
            in_flight: false,
            last_refresh: Some(recent),
            generation: 7,
        },
        now,
    );

    assert_eq!(in_flight.start_generation, None);
    assert_eq!(in_flight.generation, 7);
    assert_eq!(in_flight.last_refresh, Some(recent));
    assert_eq!(throttled.start_generation, None);
    assert_eq!(throttled.generation, 7);
    assert_eq!(throttled.last_refresh, Some(recent));
}

#[test]
fn instance_refresh_throttle_plan_wraps_generation() {
    let now = std::time::Instant::now();
    let plan = instance_refresh_throttle_plan(
        InstanceRefreshThrottleState {
            in_flight: false,
            last_refresh: Some(now.checked_sub(INSTANCE_REFRESH_INTERVAL).unwrap()),
            generation: u64::MAX,
        },
        now,
    );

    assert_eq!(plan.generation, 0);
    assert_eq!(plan.start_generation, Some(0));
}

#[test]
fn forced_instance_refresh_generation_wraps() {
    assert_eq!(forced_instance_refresh_generation(4), 5);
    assert_eq!(forced_instance_refresh_generation(u64::MAX), 0);
}
