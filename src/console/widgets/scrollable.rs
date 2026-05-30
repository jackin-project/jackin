//! Compatibility facade for shared scrollable panel components.

pub(crate) use jackin_tui::components::scrollable_panel::{
    apply_scroll_delta, clamp_scroll_offset, cursor_follow_offset, horizontal_scrollbar_area,
    is_scrollable, line_width, max_line_width, max_offset, render_horizontal_scrollbar,
    render_line_with_fixed_prefix_scroll, render_lines_with_offset_in_area,
    render_scrollable_block, render_selected_lines_in_area, render_vertical_scrollbar,
    scrollbar_offset_for_track_position, vertical_scrollbar_area, viewport_height, viewport_width,
};
