use std::sync::{Arc, Barrier};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{Clock, ManualClock, ManualClockAdvanceError, SystemClock};

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
fn system_clock_now_is_monotonic_non_decreasing() {
    let clock = SystemClock;
    let a = clock.now();
    let b = clock.now();
    assert!(b >= a);
}

#[test]
fn manual_clock_advance_moves_now_by_exactly_the_delta() {
    let clock = ManualClock::new();
    let before = clock.now();
    let delta = Duration::from_secs(42);
    clock
        .advance(delta)
        .expect("representable monotonic advance");
    let after = clock.now();
    assert_eq!(after.duration_since(before), delta);
}

#[test]
fn manual_clock_shared_refs_observe_the_same_advance() {
    let clock = Arc::new(ManualClock::new());
    let a = Arc::clone(&clock);
    let b = Arc::clone(&clock);
    let t0 = a.now();
    clock
        .advance(Duration::from_millis(500))
        .expect("representable shared advance");
    assert_eq!(a.now().duration_since(t0), Duration::from_millis(500));
    assert_eq!(b.now().duration_since(t0), Duration::from_millis(500));
    let as_dyn: Arc<dyn Clock> = clock;
    assert_eq!(as_dyn.now().duration_since(t0), Duration::from_millis(500));
}

#[test]
fn manual_clock_advance_moves_wall_clock_by_exactly_the_delta() {
    let base = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let clock = ManualClock::with_system_base(base);
    assert_eq!(clock.now_system(), base);
    clock
        .advance(Duration::from_hours(1))
        .expect("representable wall-clock advance");
    assert_eq!(
        clock.now_system().duration_since(base).unwrap(),
        Duration::from_hours(1)
    );
}

#[test]
fn manual_clock_both_faces_share_offset() {
    let base = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
    let clock = ManualClock::with_system_base(base);
    let mono0 = clock.now();
    let wall0 = clock.now_system();
    clock
        .advance(Duration::from_secs(7))
        .expect("representable paired advance");
    assert_eq!(clock.now().duration_since(mono0), Duration::from_secs(7));
    assert_eq!(
        clock.now_system().duration_since(wall0).unwrap(),
        Duration::from_secs(7)
    );
}

#[test]
fn manual_clock_zero_duration_succeeds_without_changing_either_face() {
    let clock = ManualClock::new();
    let monotonic = clock.now();
    let system = clock.now_system();

    clock
        .advance(Duration::ZERO)
        .expect("zero-duration advance is representable");

    assert_eq!(clock.now(), monotonic);
    assert_eq!(clock.now_system(), system);
}

#[test]
fn manual_clock_rejects_duration_max_without_changing_either_face() {
    let clock = ManualClock::new();
    let monotonic = clock.now();
    let system = clock.now_system();

    let result = clock.advance(Duration::MAX);

    assert_eq!(result, Err(ManualClockAdvanceError::Overflow));
    assert_eq!(clock.now(), monotonic);
    assert_eq!(clock.now_system(), system);

    clock
        .advance(Duration::from_secs(1))
        .expect("valid advance after rejected overflow");
    assert_eq!(
        clock.now().duration_since(monotonic),
        Duration::from_secs(1)
    );
    assert_eq!(
        clock.now_system().duration_since(system).unwrap(),
        Duration::from_secs(1)
    );
}

#[test]
fn manual_clock_wall_overflow_is_rejected_without_panicking_or_advancing_monotonic_time() {
    let base = SystemTime::UNIX_EPOCH;
    let boundary = base
        .checked_add(last_representable_duration_from(base))
        .expect("discovered wall-clock boundary is representable");
    let clock = ManualClock::with_system_base(boundary);
    let monotonic = clock.now();
    let advance = Duration::from_nanos(1);
    assert!(monotonic.checked_add(advance).is_some());

    let result = clock.advance(advance);

    assert_eq!(result, Err(ManualClockAdvanceError::Overflow));
    assert_eq!(clock.now(), monotonic);
    assert_eq!(clock.now_system(), boundary);
}

#[test]
fn manual_clock_advances_accumulate_past_old_nanosecond_counter_boundary() {
    let clock = ManualClock::new();
    let initial_monotonic = clock.now();
    let initial_system = clock.now_system();
    let first = Duration::from_nanos(u64::MAX);
    let second = Duration::from_nanos(1);
    let first_monotonic = initial_monotonic.checked_add(first);
    let first_system = initial_system.checked_add(first);

    if first_monotonic.is_some() && first_system.is_some() {
        clock
            .advance(first)
            .expect("both clock faces can represent the first large increment");
        let committed_monotonic = clock.now();
        let committed_system = clock.now_system();
        let second_monotonic = committed_monotonic.checked_add(second);
        let second_system = committed_system.checked_add(second);

        if second_monotonic.is_some() && second_system.is_some() {
            clock
                .advance(second)
                .expect("both faces can cross the old u64 nanosecond boundary");
            assert_eq!(
                clock.now().duration_since(initial_monotonic),
                first + second
            );
            assert_eq!(
                clock.now_system().duration_since(initial_system).unwrap(),
                first + second
            );
        } else {
            assert_eq!(
                clock.advance(second),
                Err(ManualClockAdvanceError::Overflow)
            );
            assert_eq!(clock.now(), committed_monotonic);
            assert_eq!(clock.now_system(), committed_system);
        }
    } else {
        assert_eq!(clock.advance(first), Err(ManualClockAdvanceError::Overflow));
        assert_eq!(clock.now(), initial_monotonic);
        assert_eq!(clock.now_system(), initial_system);
    }
}

#[test]
fn concurrent_manual_clock_advances_preserve_both_faces() {
    let clock = Arc::new(ManualClock::new());
    let initial_monotonic = clock.now();
    let initial_system = clock.now_system();
    const WORKERS: usize = 8;
    const ADVANCES_PER_WORKER: usize = 128;
    let barrier = Arc::new(Barrier::new(WORKERS));
    let mut workers = Vec::with_capacity(WORKERS);

    for _ in 0..WORKERS {
        let clock = Arc::clone(&clock);
        let barrier = Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            for _ in 0..ADVANCES_PER_WORKER {
                clock
                    .advance(Duration::from_millis(1))
                    .expect("small concurrent clock advance is representable");
            }
        }));
    }

    for worker in workers {
        worker.join().expect("clock worker completed");
    }

    let expected_millis = u64::try_from(WORKERS * ADVANCES_PER_WORKER)
        .expect("bounded worker advance count fits in u64");
    let expected = Duration::from_millis(expected_millis);
    assert_eq!(clock.now().duration_since(initial_monotonic), expected);
    assert_eq!(
        clock.now_system().duration_since(initial_system).unwrap(),
        expected
    );
}

#[test]
fn concurrent_boundary_advances_commit_exactly_one_complete_time_pair() {
    let epoch = SystemTime::UNIX_EPOCH;
    let boundary_offset = last_representable_duration_from(epoch);
    let boundary_nanos = boundary_offset.as_nanos();
    let delta = Duration::from_secs(1);
    let delta_nanos = delta.as_nanos();
    let initial_offset = duration_from_nanos(
        boundary_nanos
            .checked_sub(delta_nanos)
            .expect("SystemTime boundary exceeds one second"),
    );
    let initial_system = epoch
        .checked_add(initial_offset)
        .expect("wall-clock race start is representable");
    let boundary = epoch
        .checked_add(boundary_offset)
        .expect("wall-clock boundary is representable");
    let clock = Arc::new(ManualClock::with_system_base(initial_system));
    let initial_monotonic = clock.now();
    assert!(
        initial_monotonic.checked_add(delta + delta).is_some(),
        "monotonic face must accept at least two race increments"
    );

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
        .filter(|outcome| **outcome == Err(ManualClockAdvanceError::Overflow))
        .count();

    assert_eq!(successes, 1, "one paired boundary increment must commit");
    assert_eq!(overflows, WORKERS - 1, "all remaining increments must fail");
    assert_eq!(clock.now_system(), boundary);
    assert_eq!(clock.now().duration_since(initial_monotonic), delta);
}

#[test]
fn manual_clock_poison_recovers_the_last_committed_pair() {
    let clock = Arc::new(ManualClock::with_system_base(
        SystemTime::UNIX_EPOCH + Duration::from_secs(10),
    ));
    clock
        .advance(Duration::from_secs(7))
        .expect("initial committed advance");
    let committed_monotonic = clock.now();
    let committed_system = clock.now_system();
    let poisoning_clock = Arc::clone(&clock);

    let worker = std::thread::spawn(move || {
        let _state = poisoning_clock
            .state
            .lock()
            .expect("clock lock is unpoisoned before injected panic");
        std::panic::resume_unwind(Box::new("poison the lock before changing committed state"));
    });
    assert!(worker.join().is_err(), "poisoning worker must panic");

    assert_eq!(clock.now(), committed_monotonic);
    assert_eq!(clock.now_system(), committed_system);
    clock
        .advance(Duration::from_secs(1))
        .expect("valid advance must recover the poisoned lock");
    assert_eq!(
        clock.now().duration_since(committed_monotonic),
        Duration::from_secs(1)
    );
    assert_eq!(
        clock.now_system().duration_since(committed_system).unwrap(),
        Duration::from_secs(1)
    );
}
