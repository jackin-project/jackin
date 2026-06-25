use super::*;

#[test]
fn sequence_tracker_accepts_first_report_from_new_source() {
    let mut t = SequenceTracker::new();
    assert!(t.accept("hook-1", 42));
}

#[test]
fn sequence_tracker_accepts_increasing_sequence() {
    let mut t = SequenceTracker::new();
    t.accept("hook-1", 100);
    assert!(t.accept("hook-1", 101));
}

#[test]
fn sequence_tracker_rejects_equal_sequence() {
    let mut t = SequenceTracker::new();
    t.accept("hook-1", 100);
    assert!(!t.accept("hook-1", 100));
}

#[test]
fn sequence_tracker_rejects_decreasing_sequence() {
    let mut t = SequenceTracker::new();
    t.accept("hook-1", 100);
    assert!(!t.accept("hook-1", 99));
}

#[test]
fn sequence_tracker_independent_sources_dont_interfere() {
    let mut t = SequenceTracker::new();
    t.accept("hook-a", 100);
    t.accept("hook-b", 50);
    assert!(!t.accept("hook-a", 99)); // stale for hook-a
    assert!(t.accept("hook-b", 51)); // fine for hook-b
}

#[test]
fn clear_source_allows_reregistration() {
    let mut t = SequenceTracker::new();
    t.accept("hook-1", 100);
    t.clear_source("hook-1");
    // After clear, even seq=1 is accepted
    assert!(t.accept("hook-1", 1));
}

#[test]
fn clear_all_allows_every_source_to_reregister() {
    let mut t = SequenceTracker::new();
    t.accept("hook-1", 100);
    t.accept("hook-2", 200);
    t.clear_all();

    assert!(t.accept("hook-1", 1));
    assert!(t.accept("hook-2", 1));
}

#[test]
fn reporter_accept_valid_sequence() {
    let mut t = SequenceTracker::new();
    assert!(t.accept("reporter-1", 1000));
}

#[test]
fn reporter_reject_stale_sequence() {
    let mut t = SequenceTracker::new();
    t.accept("reporter-1", 1000);
    assert!(!t.accept("reporter-1", 999));
}

#[test]
fn reporter_reject_wrong_source_after_clear() {
    let mut t = SequenceTracker::new();
    t.accept("source-a", 100);
    t.clear_source("source-a");
    // After clear, source-a can re-register from any seq
    assert!(t.accept("source-a", 1));
}
