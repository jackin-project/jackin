//! Render functions for the workspace manager TUI.

pub mod editor;
pub(crate) mod editor_geometry;
mod frame;
#[cfg(test)]
mod frame_tests;
pub(crate) mod list;
pub(crate) mod list_geometry;
pub(crate) mod modal;
pub(crate) mod modal_layout;
pub(crate) mod mount_display;
mod pre_render;
pub(crate) mod settings;
pub(crate) mod settings_geometry;
#[cfg(test)]
mod snapshot_tests;

pub use frame::render;
pub use pre_render::prepare_for_render;

pub(super) use jackin_console::tui::layout::centered_rect_fixed;

pub(crate) use jackin_tui::components::scrollable_panel::{
    is_scrollable, viewport_height as scroll_viewport_height,
    viewport_width as scroll_viewport_width,
};
pub(super) use jackin_tui::components::scrollable_panel::{
    line_width, max_line_width, render_horizontal_scrollbar, render_line_with_fixed_prefix_scroll,
    render_scrollable_block_at, render_vertical_scrollbar,
};
pub(super) use jackin_tui::theme::{CYAN, CYAN_DIM};
pub(super) use jackin_tui::theme::{
    PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, TAB_BG_INACTIVE_HOVER, WHITE,
};
