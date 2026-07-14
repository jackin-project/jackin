use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{Clock, ManualClock, SystemClock};

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
    clock.advance(delta);
    let after = clock.now();
    assert_eq!(after.duration_since(before), delta);
}

#[test]
fn manual_clock_shared_refs_observe_the_same_advance() {
    let clock = Arc::new(ManualClock::new());
    let a = Arc::clone(&clock);
    let b = Arc::clone(&clock);
    let t0 = a.now();
    clock.advance(Duration::from_millis(500));
    assert_eq!(a.now().duration_since(t0), Duration::from_millis(500));
    assert_eq!(b.now().duration_since(t0), Duration::from_millis(500));
    // Trait-object view still observes the same advance.
    let as_dyn: Arc<dyn Clock> = clock;
    assert_eq!(as_dyn.now().duration_since(t0), Duration::from_millis(500));
}

#[test]
fn manual_clock_advance_moves_wall_clock_by_exactly_the_delta() {
    let base = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let clock = ManualClock::with_system_base(base);
    assert_eq!(clock.now_system(), base);
    clock.advance(Duration::from_secs(3600));
    assert_eq!(
        clock.now_system().duration_since(base).unwrap(),
        Duration::from_secs(3600)
    );
}

#[test]
fn manual_clock_both_faces_share_offset() {
    let base = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
    let clock = ManualClock::with_system_base(base);
    let mono0 = clock.now();
    let wall0 = clock.now_system();
    clock.advance(Duration::from_secs(7));
    assert_eq!(clock.now().duration_since(mono0), Duration::from_secs(7));
    assert_eq!(
        clock.now_system().duration_since(wall0).unwrap(),
        Duration::from_secs(7)
    );
}
