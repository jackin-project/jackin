use std::sync::Arc;
use std::time::Duration;

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
    assert_eq!(
        as_dyn.now().duration_since(t0),
        Duration::from_millis(500)
    );
}
