//! Reusable Ratatui components shared by jackin' terminal surfaces.

pub mod brand_header;
pub mod confirm_dialog;
pub mod error_dialog;
pub mod filter_input;
pub mod hint_bar;
pub mod select_list;
pub mod scrollable_panel;
pub mod status_footer;
pub mod text_input;

pub use brand_header::{BrandHeader, brand_header_line, render_brand_header};
pub use confirm_dialog::{
    ConfirmFocus, ConfirmKind, ConfirmState, render_confirm_dialog, required_height as confirm_required_height,
    width_pct as confirm_width_pct,
};
pub use error_dialog::{ErrorPopupState, estimated_message_rows, render_error_dialog, required_height};
pub use filter_input::{FilterInput, filter_input_line, render_filter_input};
pub use hint_bar::{
    HintBar, line as hint_line, render_hint_bar, render_wrapped_hint_bar, wrapped_height,
};
pub use select_list::{SelectList, SelectListState, render_select_list};
pub use scrollable_panel::{
    apply_scroll_delta, apply_scroll_delta_unclamped, apply_term_width_scroll_delta,
    clamp_scroll_offset, cursor_follow_offset, effective_offset, horizontal_scrollbar_area,
    is_scrollable, line_width, max_line_width, max_offset, render_horizontal_scrollbar,
    render_line_with_fixed_prefix_scroll, render_lines_with_offset_in_area,
    render_scrollable_block, render_selected_lines_in_area, render_vertical_scrollbar,
    render_vertical_scrollbar_in_area, scrollbar_offset_for_track_position, vertical_scrollbar_area,
    viewport_height, viewport_width,
};
pub use status_footer::{StatusFooter, render_status_footer};
pub use text_input::{BorderStyle, TextInput, TextInputState, render_text_input};
