// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn hover_tracker_returns_hovered_element() {
    let mut tracker: HoverTracker<&str> = HoverTracker::new();
    tracker.register(Rect::new(10, 5, 8, 1), "chip");
    tracker.register(Rect::new(0, 0, 5, 1), "tab");

    // Inside chip rect.
    assert_eq!(tracker.hovered(12, 5), Some(&"chip"));
    // Inside tab rect.
    assert_eq!(tracker.hovered(2, 0), Some(&"tab"));
    // Outside everything.
    assert_eq!(tracker.hovered(20, 20), None);
}

#[test]
fn hover_tracker_clear_removes_registrations() {
    let mut tracker: HoverTracker<u8> = HoverTracker::new();
    tracker.register(Rect::new(0, 0, 10, 1), 1);
    tracker.clear();
    assert_eq!(tracker.hovered(5, 0), None);
}

#[test]
fn any_hovered_drives_pointer_shape() {
    let mut tracker: HoverTracker<&str> = HoverTracker::new();
    tracker.register(Rect::new(5, 5, 4, 1), "btn");
    assert!(
        tracker.any_hovered(6, 5),
        "pointer should be hand over button"
    );
    assert!(
        !tracker.any_hovered(0, 0),
        "pointer should be default off button"
    );
}
