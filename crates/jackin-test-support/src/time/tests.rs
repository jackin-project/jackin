//! Self-tests for [`ManualClock`](super::ManualClock).

use super::ManualClock;
use std::time::{Duration, SystemTime};

#[test]
fn epoch_starts_at_unix_epoch() {
    assert_eq!(ManualClock::epoch().now(), SystemTime::UNIX_EPOCH);
}

#[test]
fn advance_moves_forward_and_reports_elapsed() {
    let clock = ManualClock::epoch();
    let start = clock.now();
    let tick = clock.advance(Duration::from_secs(30));
    assert_eq!(tick, start + Duration::from_secs(30));
    assert_eq!(clock.now(), tick);
    assert_eq!(clock.elapsed_since(start), Duration::from_secs(30));
}

#[test]
fn elapsed_saturates_for_future_instants() {
    let clock = ManualClock::epoch();
    let future = clock.now() + Duration::from_mins(1);
    assert_eq!(clock.elapsed_since(future), Duration::ZERO);
}

#[test]
fn set_allows_backwards_jumps() {
    let clock = ManualClock::epoch();
    clock.advance(Duration::from_mins(1));
    clock.set(SystemTime::UNIX_EPOCH);
    assert_eq!(clock.now(), SystemTime::UNIX_EPOCH);
}

#[test]
fn clones_share_one_time() {
    let clock = ManualClock::epoch();
    let twin = clock.clone();
    clock.advance(Duration::from_secs(5));
    assert_eq!(twin.now(), clock.now());
    twin.set(SystemTime::UNIX_EPOCH + Duration::from_secs(99));
    assert_eq!(clock.now(), twin.now());
}

#[test]
fn due_and_deadline_drive_cadence_checks() {
    let clock = ManualClock::epoch();
    let start = clock.now();
    let interval = Duration::from_secs(10);
    assert!(!clock.is_due(start, interval));
    assert_eq!(clock.deadline(start, interval), start + interval);
    clock.advance(interval);
    assert!(clock.is_due(start, interval));
}

#[test]
fn advances_from_threads_accumulate() {
    let clock = ManualClock::epoch();
    let worker = clock.clone();
    let handle = std::thread::spawn(move || {
        worker.advance(Duration::from_secs(1));
    });
    let joined_cleanly = handle.join().is_ok();
    assert!(joined_cleanly, "worker thread panicked");
    assert_eq!(clock.now(), SystemTime::UNIX_EPOCH + Duration::from_secs(1));
}
