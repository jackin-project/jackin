// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `layout`.

use super::tab_hover_index_at_position;
use super::{
    MIN_DRAGGABLE_WIDTH, MOUSE_HORIZONTAL_SCROLL_STEP, MOUSE_VERTICAL_SCROLL_STEP,
    SCREEN_HEADER_HEIGHT, SEAM_HIT_SLACK, ScrollbarAxis, TAB_STRIP_HEIGHT, apply_scrollbar_drag,
    bordered_content_hit_at_position, horizontal_split_pane_dims, is_horizontally_scrollable,
    list_body_area, list_content_visual_index_at, near_seam, point_in_rect,
    scroll_selection_at_position, scroll_viewport_height, scroll_viewport_width,
    scrollbar_drag_offset, split_pct_from_drag, split_seam_column, tab_cell_at_position,
    tabbed_content_area,
};

use ratatui::layout::Rect;

mod case_01;
