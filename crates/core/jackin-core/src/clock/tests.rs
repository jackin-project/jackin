// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::{Duration, SystemTime};

use super::{Clock, ManualClock, ManualClockAdvanceError};

#[test]
fn advance_moves_both_hands_together() {
    let clock = ManualClock::with_system_base(SystemTime::UNIX_EPOCH);
    let monotonic = clock.now();
    clock.advance(Duration::from_secs(5)).unwrap();
    assert_eq!(
        clock.now_system(),
        SystemTime::UNIX_EPOCH + Duration::from_secs(5)
    );
    assert_eq!(clock.now(), monotonic + Duration::from_secs(5));
}

#[test]
fn advance_overflow_rejects_without_moving_hands() {
    let clock = ManualClock::with_system_base(SystemTime::UNIX_EPOCH);
    let monotonic = clock.now();
    assert_eq!(
        clock.advance(Duration::MAX),
        Err(ManualClockAdvanceError::Overflow)
    );
    assert_eq!(clock.now_system(), SystemTime::UNIX_EPOCH);
    assert_eq!(clock.now(), monotonic);
}
