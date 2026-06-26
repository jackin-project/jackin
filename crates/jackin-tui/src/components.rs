//! Reusable Ratatui components shared by jackin❯ terminal surfaces.

pub mod bottom_chrome;
pub mod brand_header;
pub mod button_strip;
pub mod confirm_dialog;
pub mod container_info;
pub mod dialog_layout;
pub mod diff_view;
pub mod error_dialog;
pub mod filter_input;
pub mod focus_owner;
pub mod hint_bar;
pub mod hover_tracker;
pub mod modal_backdrop;
pub mod modal_lifecycle;
pub mod panel;
pub mod save_discard_dialog;
pub mod scrollable_panel;
pub mod select_list;
pub mod status_footer;
pub mod status_popup;
pub mod tab_strip;
pub mod text_input;
pub mod toast;

pub use super::keymap::{
    KeyBinding, KeyChord, Keymap, LogicalKey, Mods, SCROLL_HINT_KEYMAP, ScrollHintAxis, Visibility,
    chord_glyph, raw_bytes_to_chord,
};
pub use bottom_chrome::{BOTTOM_CHROME_ROWS, BottomChromeAreas, bottom_chrome_areas};
pub use brand_header::{BrandHeader, brand_header_line, render_brand_header};
pub use button_strip::{ButtonStrip, ButtonStripItem, button_strip_line, button_style};
pub use confirm_dialog::{
    CONFIRM_KEYMAP, ConfirmAction, ConfirmFocus, ConfirmKind, ConfirmState, confirm_hint_spans,
    exit_confirm_state, exit_confirm_state_with_data_loss, render_confirm_dialog,
    required_height as confirm_required_height, width_pct as confirm_width_pct,
};
pub use container_info::{
    ContainerInfoRow, ContainerInfoState, DebugInfo,
    clamp_dialog_scroll as clamp_container_info_scroll,
    copy_payload_at as container_info_copy_payload_at, debug_info_hint_spans,
    hyperlink_overlay as container_info_hyperlink_overlay,
    hyperlink_payload_at as container_info_hyperlink_payload_at,
    hyperlink_regions as container_info_hyperlink_regions, render_container_info,
    required_height as container_info_required_height,
};
pub use dialog_layout::{
    DIALOG_HORIZONTAL_SCROLL_STEP, DialogBodyScroll, ScrollAxes, ScrollAxis, dialog_inner_chunks,
    dialog_inner_height, dialog_scroll_axes, mouse_scroll_delta, render_dialog_shell,
    render_scrollable_dialog_body, scroll_hint_spans,
};
pub use diff_view::{DiffViewState, SinglePaneKind, diff_view_hint_spans, render_diff_view};
pub use error_dialog::{
    ERROR_POPUP_KEYMAP, ErrorPopupAction, ErrorPopupState, error_popup_hint_spans,
    estimated_message_rows, render_error_dialog, render_error_dialog_in, required_height,
};
pub use filter_input::{FilterInput, filter_input_line, render_filter_input};
pub use focus_owner::FocusOwner;
pub use hint_bar::{
    HintBar, line as hint_line, render_hint_bar, render_wrapped_hint_bar, wrapped_height,
};
pub use hover_tracker::HoverTracker;
pub use modal_backdrop::ModalBackdrop;
pub use modal_lifecycle::{ModalClickResult, classify_click, render_backdrop};
pub use panel::{
    Panel, PanelFocus, modal_block, modal_block_inactive, panel_body_area, unfocused_block,
};
pub use save_discard_dialog::{
    SAVE_DISCARD_KEYMAP, SaveDiscardAction, SaveDiscardChoice, SaveDiscardFocus, SaveDiscardState,
    render_save_discard_dialog, save_discard_hint_spans,
};
pub use scrollable_panel::{
    SCROLLBAR_HORIZONTAL_THUMB, SCROLLBAR_TRACK, ScrollableList, ScrollbarStyle,
    apply_scroll_delta, apply_scroll_delta_unclamped, apply_term_width_scroll_delta,
    clamp_scroll_offset, cursor_follow_offset, effective_offset, horizontal_scrollbar_area,
    is_scrollable, line_width, max_line_width, max_offset, render_horizontal_scrollbar,
    render_line_with_fixed_prefix_scroll, render_lines_with_offset_in_area,
    render_scrollable_block, render_selected_lines_in_area, render_vertical_scrollbar,
    render_vertical_scrollbar_in_area, render_vertical_scrollbar_in_area_with_style,
    render_vertical_scrollbar_with_style, scrollbar_offset_for_track_position,
    vertical_scrollbar_area, viewport_height, viewport_width,
};
pub use select_list::{
    PickerRow, SELECT_LIST_KEYMAP, SelectList, SelectListAction, SelectListState,
    render_picker_lines, render_picker_list, render_select_list, select_list_hint_spans,
};
pub use status_footer::{
    StatusFooter, StatusFooterHover, StatusRightGroup, compact_usage_status_label,
    render_status_footer, render_status_footer_right_group, status_footer_debug_chip_rect,
    status_footer_right_chip_rect, status_right_group_layout,
};
pub use status_popup::{StatusPopupState, render_status_popup};
pub use tab_strip::{TabStrip, tab_cell_style, tab_label_line, tab_underline_line};
pub use text_input::{
    BorderStyle, TEXT_INPUT_KEYMAP, TextField, TextInput, TextInputAction, TextInputState,
    render_labeled_text_input_dialog, render_text_input, text_input_hint_spans,
    text_input_prompt_rect,
};
pub use toast::{Toast, render_toast, toast_rect};
