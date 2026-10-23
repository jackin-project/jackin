use std::sync::{Arc, Barrier};
use std::time::{Duration, SystemTime};

use super::{ManualClock, ManualClockError};

const NANOS_PER_SECOND: u128 = 1_000_000_000;

fn duration_from_nanos(nanos: u128) -> Duration {
    let seconds = u64::try_from(nanos / NANOS_PER_SECOND)
        .expect("nanosecond search is bounded by Duration::MAX seconds");
    let subsecond_nanos = u32::try_from(nanos % NANOS_PER_SECOND)
        .expect("subsecond nanoseconds are below one second");
    Duration::new(seconds, subsecond_nanos)
}

fn last_representable_duration_from(base: SystemTime) -> Duration {
    let mut low = 0_u128;
    let mut high = Duration::MAX.as_nanos();
    assert!(base.checked_add(Duration::ZERO).is_some());
    assert!(
        base.checked_add(duration_from_nanos(high)).is_none(),
        "SystemTime must reject Duration::MAX for this finite boundary search"
    );

    while low < high {
        let distance = high - low;
        let middle = low + distance / 2 + distance % 2;
        if base.checked_add(duration_from_nanos(middle)).is_some() {
            low = middle;
        } else {
            high = middle - 1;
        }
    }

    let boundary = duration_from_nanos(low);
    assert!(base.checked_add(boundary).is_some());
    assert!(
        base.checked_add(duration_from_nanos(low + 1)).is_none(),
        "the discovered SystemTime boundary must be the last representable nanosecond"
    );
    boundary
}

#[test]
fn epoch_starts_at_unix_epoch() {
    assert_eq!(ManualClock::epoch().now(), SystemTime::UNIX_EPOCH);
}

#[test]
fn advance_moves_forward_and_reports_elapsed() {
    let clock = ManualClock::epoch();
    let start = clock.now();
    let tick = clock
        .advance(Duration::from_secs(30))
        .expect("representable clock advance");
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
    clock
        .advance(Duration::from_mins(1))
        .expect("representable clock advance");
    clock.set(SystemTime::UNIX_EPOCH);
    assert_eq!(clock.now(), SystemTime::UNIX_EPOCH);
}

#[test]
fn clones_share_one_time() {
    let clock = ManualClock::epoch();
    let twin = clock.clone();
    clock
        .advance(Duration::from_secs(5))
        .expect("representable shared advance");
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
    assert_eq!(clock.deadline(start, interval), Ok(start + interval));
    clock
        .advance(interval)
        .expect("representable deadline advance");
    assert!(clock.is_due(start, interval));
}

#[test]
fn advance_zero_duration_succeeds_without_changing_time() {
    let clock = ManualClock::epoch();
    let before = clock.now();

    assert_eq!(clock.advance(Duration::ZERO), Ok(before));
    assert_eq!(clock.now(), before);
}

#[test]
fn advance_rejects_overflow_without_panicking_or_mutating_time() {
    let epoch = SystemTime::UNIX_EPOCH;
    let boundary = epoch
        .checked_add(last_representable_duration_from(epoch))
        .expect("discovered wall-clock boundary is representable");
    let clock = ManualClock::new(boundary);
    let advance = Duration::from_nanos(1);

    assert_eq!(clock.advance(advance), Err(ManualClockError::Overflow));
    assert_eq!(clock.now(), boundary);
}

#[test]
fn duration_max_advance_is_rejected_without_poisoning_or_mutation() {
    let clock = ManualClock::epoch();
    let before = clock.now();
    assert!(
        before.checked_add(Duration::MAX).is_none(),
        "the test requires a finite SystemTime boundary below Duration::MAX"
    );

    assert_eq!(
        clock.advance(Duration::MAX),
        Err(ManualClockError::Overflow)
    );
    assert_eq!(clock.now(), before);
    assert_eq!(
        clock.advance(Duration::from_secs(1)),
        Ok(before + Duration::from_secs(1)),
        "rejected overflow must leave the lock usable"
    );
}

#[test]
fn deadline_reports_unrepresentable_time_instead_of_panicking() {
    let epoch = SystemTime::UNIX_EPOCH;
    let boundary = epoch
        .checked_add(last_representable_duration_from(epoch))
        .expect("discovered wall-clock boundary is representable");
    let clock = ManualClock::new(boundary);

    assert_eq!(
        clock.deadline(boundary, Duration::from_nanos(1)),
        Err(ManualClockError::Overflow)
    );
    assert_eq!(clock.now(), boundary, "deadline calculation must be pure");
}

#[test]
fn concurrent_boundary_advances_commit_exactly_one_time() {
    let epoch = SystemTime::UNIX_EPOCH;
    let boundary_offset = last_representable_duration_from(epoch);
    let boundary_nanos = boundary_offset.as_nanos();
    let delta = Duration::from_secs(1);
    let initial_offset = duration_from_nanos(
        boundary_nanos
            .checked_sub(delta.as_nanos())
            .expect("SystemTime boundary exceeds one second"),
    );
    let initial = epoch
        .checked_add(initial_offset)
        .expect("wall-clock race start is representable");
    let boundary = epoch
        .checked_add(boundary_offset)
        .expect("wall-clock boundary is representable");
    let clock = Arc::new(ManualClock::new(initial));

    const WORKERS: usize = 8;
    let barrier = Arc::new(Barrier::new(WORKERS));
    let mut workers = Vec::with_capacity(WORKERS);
    for _ in 0..WORKERS {
        let clock = Arc::clone(&clock);
        let barrier = Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            clock.advance(delta)
        }));
    }

    let outcomes: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().expect("boundary worker completed"))
        .collect();
    let successes = outcomes.iter().filter(|outcome| outcome.is_ok()).count();
    let overflows = outcomes
        .iter()
        .filter(|outcome| **outcome == Err(ManualClockError::Overflow))
        .count();

    assert_eq!(successes, 1, "one boundary increment must commit");
    assert_eq!(overflows, WORKERS - 1, "all remaining increments must fail");
    assert_eq!(clock.now(), boundary);
}

#[test]
fn advances_from_threads_accumulate() {
    let clock = ManualClock::epoch();
    let worker = clock.clone();
    let handle = std::thread::spawn(move || {
        worker
            .advance(Duration::from_secs(1))
            .expect("worker clock advance is representable");
    });
    let joined_cleanly = handle.join().is_ok();
    assert!(joined_cleanly, "worker thread panicked");
    assert_eq!(clock.now(), SystemTime::UNIX_EPOCH + Duration::from_secs(1));
}

#[test]
fn poisoned_lock_recovers_last_committed_time_for_reads_and_writes() {
    let clock = ManualClock::epoch();
    clock
        .advance(Duration::from_secs(45))
        .expect("initial committed advance");
    let committed = clock.now();
    let poisoning_clock = clock.clone();

    let worker = std::thread::spawn(move || {
        let _now = poisoning_clock
            .now
            .lock()
            .expect("clock lock is unpoisoned before injected panic");
        std::panic::resume_unwind(Box::new("poison the lock before changing committed time"));
    });
    assert!(worker.join().is_err(), "poisoning worker must panic");

    assert_eq!(clock.now(), committed);
    assert_eq!(
        clock.advance(Duration::from_secs(1)),
        Ok(committed + Duration::from_secs(1))
    );
    let backwards = SystemTime::UNIX_EPOCH + Duration::from_secs(5);
    clock.set(backwards);
    assert_eq!(clock.now(), backwards);
}
