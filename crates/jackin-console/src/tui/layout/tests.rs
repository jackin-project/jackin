// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `layout`.
use super::{
    MIN_DRAGGABLE_WIDTH, MOUSE_HORIZONTAL_SCROLL_STEP, MOUSE_VERTICAL_SCROLL_STEP,
    SCREEN_HEADER_HEIGHT, SEAM_HIT_SLACK, ScrollbarAxis, TAB_STRIP_HEIGHT, apply_horizontal_scroll,
    apply_scrollbar_drag, apply_vertical_scroll, bordered_content_hit_at_position,
    horizontal_split_pane_dims, is_horizontally_scrollable, list_body_area,
    list_content_visual_index_at, near_seam, point_in_rect, scroll_selection_at_position,
    scroll_viewport_height, scroll_viewport_width, scrollbar_drag_offset, split_pct_from_drag,
    split_seam_column, tab_cell_at_position, tabbed_content_area,
};
use ratatui::layout::Rect;

#[test]
fn split_seam_column_uses_saturating_percent_math() {
    assert_eq!(split_seam_column(30, 100), 30);
    assert_eq!(split_seam_column(30, 0), 0);
}

#[test]
fn near_seam_uses_one_column_hit_slack() {
    assert_eq!(MIN_DRAGGABLE_WIDTH, 40);
    assert_eq!(SEAM_HIT_SLACK, 1);
    assert_eq!(MOUSE_HORIZONTAL_SCROLL_STEP, 1);
    assert_eq!(MOUSE_VERTICAL_SCROLL_STEP, 1);

    assert!(near_seam(29, 30));
    assert!(near_seam(30, 30));
    assert!(near_seam(31, 30));
    assert!(!near_seam(28, 30));
    assert!(!near_seam(32, 30));
}

#[test]
fn horizontal_split_pane_dims_match_ratatui_percentage_layout() {
    assert_eq!(horizontal_split_pane_dims(30, 100), (0, 30, 30, 70));
    assert_eq!(horizontal_split_pane_dims(33, 101), (0, 33, 33, 68));
}

#[test]
fn list_body_area_reserves_header_and_footer() {
    assert_eq!(
        list_body_area(Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 24,
        }),
        Rect {
            x: 0,
            y: 2,
            width: 100,
            height: 20,
        }
    );
}

#[test]
fn list_content_visual_index_excludes_borders() {
    let area = Rect {
        x: 0,
        y: 0,
        width: 100,
        height: 24,
    };
    assert_eq!(list_content_visual_index_at(1, 3, area, 30), Some(0));
    assert_eq!(list_content_visual_index_at(1, 4, area, 30), Some(1));
    assert_eq!(list_content_visual_index_at(0, 3, area, 30), None);
    assert_eq!(list_content_visual_index_at(30, 3, area, 30), None);
    assert_eq!(list_content_visual_index_at(1, 2, area, 30), None);
    assert_eq!(list_content_visual_index_at(1, 21, area, 30), None);
}

#[test]
fn split_pct_from_drag_handles_signed_delta_and_bounds() {
    assert_eq!(split_pct_from_drag(30, 30, 50, 100), 50);
    assert_eq!(split_pct_from_drag(30, 30, 0, 100), 0);
    assert_eq!(split_pct_from_drag(80, 80, 200, 100), 100);
}

#[test]
fn scrollbar_drag_offset_maps_pointer_to_scroll_offset() {
    let area = Rect {
        x: 0,
        y: 0,
        width: 20,
        height: 5,
    };
    assert_eq!(
        scrollbar_drag_offset(ScrollbarAxis::Horizontal, area, 100, 10, 4),
        Some(44)
    );
    assert_eq!(
        scrollbar_drag_offset(ScrollbarAxis::Vertical, area, 100, 19, 2),
        Some(48)
    );
    assert_eq!(
        scrollbar_drag_offset(ScrollbarAxis::Horizontal, area, 10, 10, 4),
        None
    );
}

#[test]
fn apply_scrollbar_drag_updates_offset_when_pointer_hits_track() {
    let area = Rect {
        x: 0,
        y: 0,
        width: 20,
        height: 5,
    };
    let mut value = 0;

    assert!(apply_scrollbar_drag(
        ScrollbarAxis::Horizontal,
        &mut value,
        area,
        100,
        10,
        4
    ));
    assert_eq!(value, 44);
    assert!(!apply_scrollbar_drag(
        ScrollbarAxis::Horizontal,
        &mut value,
        area,
        10,
        10,
        4
    ));
    assert_eq!(value, 44);
}

#[test]
fn scroll_selection_at_position_runs_only_inside_area() {
    let area = Rect {
        x: 2,
        y: 3,
        width: 5,
        height: 4,
    };
    let mut offset = 0;

    assert!(!scroll_selection_at_position(area, 1, 3, 2, |delta| {
        offset += delta;
        true
    }));
    assert_eq!(offset, 0);
    assert!(scroll_selection_at_position(area, 2, 3, 2, |delta| {
        offset += delta;
        true
    }));
    assert_eq!(offset, 2);
}

#[test]
fn point_in_rect_uses_half_open_edges() {
    let area = Rect {
        x: 2,
        y: 3,
        width: 5,
        height: 4,
    };

    assert!(point_in_rect(2, 3, area));
    assert!(point_in_rect(6, 6, area));
    assert!(!point_in_rect(7, 3, area));
    assert!(!point_in_rect(2, 7, area));
}

#[test]
fn bordered_content_hit_at_position_excludes_border_and_applies_scroll() {
    let area = Rect {
        x: 10,
        y: 5,
        width: 20,
        height: 6,
    };
    let hit = |row| (row != 3).then_some(row);

    assert_eq!(
        bordered_content_hit_at_position(area, 11, 6, 0, hit),
        Some(0)
    );
    assert_eq!(
        bordered_content_hit_at_position(area, 11, 7, 2, Some),
        Some(3)
    );
    assert_eq!(
        bordered_content_hit_at_position(area, 11, 6, 3, |row| (row != 3).then_some(row)),
        None
    );
    assert_eq!(bordered_content_hit_at_position(area, 10, 6, 0, Some), None);
    assert_eq!(bordered_content_hit_at_position(area, 29, 6, 0, Some), None);
    assert_eq!(bordered_content_hit_at_position(area, 11, 5, 0, Some), None);
    assert_eq!(
        bordered_content_hit_at_position(area, 11, 10, 0, Some),
        None
    );
}

#[test]
fn scroll_apply_helpers_use_scrollable_panel_viewports() {
    let area = Rect {
        x: 0,
        y: 0,
        width: 10,
        height: 6,
    };

    let mut horizontal = 0;
    apply_horizontal_scroll(&mut horizontal, 20, area, 40);
    assert_eq!(horizontal, 20);
    apply_horizontal_scroll(&mut horizontal, 20, area, 40);
    assert_eq!(horizontal, 32);

    let mut vertical = 0;
    apply_vertical_scroll(&mut vertical, 20, area, 40);
    assert_eq!(vertical, 20);
    apply_vertical_scroll(&mut vertical, 20, area, 40);
    assert_eq!(vertical, 36);

    assert_eq!(scroll_viewport_width(area), 8);
    assert_eq!(scroll_viewport_height(area), 4);
    assert!(is_horizontally_scrollable(area, 40));
    assert!(!is_horizontally_scrollable(area, 8));
}

#[test]
fn tabbed_content_area_excludes_header_tabs_and_footer() {
    let term = Rect {
        x: 0,
        y: 0,
        width: 120,
        height: 40,
    };
    assert_eq!(
        tabbed_content_area(term, 3),
        Rect {
            x: 0,
            y: SCREEN_HEADER_HEIGHT + TAB_STRIP_HEIGHT,
            width: 120,
            height: 32,
        }
    );
}

#[test]
fn tab_cell_at_position_uses_shared_tab_layout() {
    let labels = ["General", "Mounts", "Auth"];

    assert_eq!(
        tab_cell_at_position(SCREEN_HEADER_HEIGHT, 1, &labels),
        Some(0)
    );
    assert_eq!(
        tab_cell_at_position(SCREEN_HEADER_HEIGHT + 1, 11, &labels),
        Some(1)
    );
    assert_eq!(
        tab_cell_at_position(SCREEN_HEADER_HEIGHT - 1, 1, &labels),
        None
    );
    assert_eq!(
        tab_cell_at_position(SCREEN_HEADER_HEIGHT + TAB_STRIP_HEIGHT, 1, &labels),
        None
    );
}

#[test]
fn tab_hover_index_at_position_tracks_full_tab_cells() {
    let labels = ["General", "Mounts", "Auth"];

    assert_eq!(
        super::tab_hover_index_at_position(SCREEN_HEADER_HEIGHT, 1, &labels),
        Some(0)
    );
    assert_eq!(
        super::tab_hover_index_at_position(SCREEN_HEADER_HEIGHT + 1, 11, &labels),
        Some(1)
    );
    assert_eq!(
        super::tab_hover_index_at_position(SCREEN_HEADER_HEIGHT - 1, 1, &labels),
        None
    );
    assert_eq!(
        super::tab_hover_index_at_position(SCREEN_HEADER_HEIGHT + TAB_STRIP_HEIGHT, 1, &labels),
        None
    );
}
