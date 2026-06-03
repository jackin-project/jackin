//! Tests for `selection`.
use super::{
    SelectionState, move_selection_end, selection_start_for_inner_rect, selection_was_dragged,
};
use crate::tui::layout::Rect;

#[test]
fn selection_start_requires_inner_rect_hit() {
    let inner = Rect::new(10, 20, 5, 8);

    let sel = selection_start_for_inner_rect(7, inner, 12, 24).unwrap();
    assert_eq!(sel.session_id, 7);
    assert_eq!(sel.inner, inner);
    assert_eq!((sel.anchor_row, sel.anchor_col), (2, 4));
    assert_eq!((sel.end_row, sel.end_col), (2, 4));

    assert!(selection_start_for_inner_rect(7, inner, 9, 24).is_none());
    assert!(selection_start_for_inner_rect(7, inner, 12, 28).is_none());
}

#[test]
fn selection_motion_clamps_to_inner_rect() {
    let mut sel = SelectionState {
        session_id: 7,
        inner: Rect::new(10, 20, 5, 8),
        anchor_row: 1,
        anchor_col: 2,
        end_row: 1,
        end_col: 2,
    };

    move_selection_end(&mut sel, 99, 99);
    assert_eq!((sel.end_row, sel.end_col), (4, 7));

    move_selection_end(&mut sel, 0, 0);
    assert_eq!((sel.end_row, sel.end_col), (0, 0));
}

#[test]
fn same_cell_selection_is_not_a_drag() {
    let mut sel = SelectionState {
        session_id: 7,
        inner: Rect::new(10, 20, 5, 8),
        anchor_row: 1,
        anchor_col: 2,
        end_row: 1,
        end_col: 2,
    };
    assert!(!selection_was_dragged(&sel));

    move_selection_end(&mut sel, 12, 24);
    assert!(selection_was_dragged(&sel));
}
