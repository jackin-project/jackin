use super::*;

#[test]
fn screen_sequence_is_monotonic_and_visits_end() {
    let mut tracker = ScreenVisitTracker::new();
    tracker
        .enter(schema::enums::ScreenId::WorkspaceList)
        .unwrap();
    assert_eq!(tracker.sequence(), 1);
    tracker
        .enter(schema::enums::ScreenId::WorkspaceEditor)
        .unwrap();
    assert_eq!(tracker.sequence(), 2);
    tracker.exit(schema::enums::TransitionReason::Back).unwrap();
    assert_eq!(tracker.current_screen(), None);
}

#[test]
fn widget_focus_replaces_prior_focus() {
    let mut tracker = WidgetFocusTracker::default();
    tracker.focus("general").unwrap();
    tracker.focus("mounts").unwrap();
    tracker.unfocus().unwrap();
    assert!(tracker.current.is_none());
}
