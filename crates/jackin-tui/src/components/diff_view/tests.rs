use super::{DiffViewState, SinglePaneKind};

#[test]
fn side_by_side_empty_before() {
    let state = DiffViewState::side_by_side("", "hello\n", "before", "after");
    assert!(state.total_rows() >= 1);
}

#[test]
fn side_by_side_no_change() {
    let state = DiffViewState::side_by_side("same\n", "same\n", "before", "after");
    // Identical content: no diff groups, no rows.
    assert_eq!(state.total_rows(), 0);
}

#[test]
fn single_pane_added() {
    let state = DiffViewState::single_pane("line1\nline2\n", SinglePaneKind::Added, "new_file.rs");
    assert_eq!(state.total_rows(), 2);
}

#[test]
fn single_pane_deleted() {
    let state = DiffViewState::single_pane("gone\n", SinglePaneKind::Deleted, "old.rs");
    assert_eq!(state.total_rows(), 1);
}

#[test]
fn scroll_clamps() {
    let mut state = DiffViewState::single_pane("a\nb\nc\n", SinglePaneKind::Added, "f");
    state.scroll_down();
    state.scroll_down();
    state.scroll_down();
    state.scroll_down();
    assert_eq!(state.scroll_y, 2); // clamped to total_rows - 1
    state.scroll_up();
    assert_eq!(state.scroll_y, 1);
}
