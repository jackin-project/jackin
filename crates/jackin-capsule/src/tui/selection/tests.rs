//! Tests for `selection`.
use super::{
    SelectionState, move_selection_end, selection_start_for_inner_rect, selection_was_dragged,
    visible_selection,
};
use crate::tui::layout::Rect;

#[test]
fn selection_start_requires_inner_rect_hit() {
    let inner = Rect::new(10, 20, 5, 8);

    let sel = selection_start_for_inner_rect(7, inner, 12, 24, 0, 0).unwrap();
    assert_eq!(sel.session_id, 7);
    assert_eq!(sel.inner, inner);
    assert_eq!((sel.anchor_row, sel.anchor_col), (2, 4));
    assert_eq!((sel.end_row, sel.end_col), (2, 4));

    assert!(selection_start_for_inner_rect(7, inner, 9, 24, 0, 0).is_none());
    assert!(selection_start_for_inner_rect(7, inner, 12, 28, 0, 0).is_none());
}

#[test]
fn selection_start_records_content_row_when_scrolled() {
    let inner = Rect::new(10, 20, 5, 8);

    let sel = selection_start_for_inner_rect(7, inner, 12, 24, 12, 4).unwrap();

    assert_eq!(
        sel.anchor_row, 10,
        "content row = filled - offset + visible row"
    );
    assert_eq!((sel.anchor_col, sel.end_row, sel.end_col), (4, 10, 4));
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

    move_selection_end(&mut sel, 99, 99, 0, 0);
    assert_eq!((sel.end_row, sel.end_col), (4, 7));

    move_selection_end(&mut sel, 0, 0, 0, 0);
    assert_eq!((sel.end_row, sel.end_col), (0, 0));
}

#[test]
fn visible_selection_projects_content_rows_into_viewport() {
    let sel = SelectionState {
        session_id: 7,
        inner: Rect::new(10, 20, 5, 8),
        anchor_row: 9,
        anchor_col: 1,
        end_row: 12,
        end_col: 3,
    };

    let visible = visible_selection(&sel, 12, 4).expect("selection intersects viewport");

    assert_eq!((visible.start_row, visible.start_col), (1, 1));
    assert_eq!((visible.end_row, visible.end_col), (4, 3));
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

    move_selection_end(&mut sel, 12, 24, 0, 0);
    assert!(selection_was_dragged(&sel));
}
