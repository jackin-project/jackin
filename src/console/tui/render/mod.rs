//! Render functions for the workspace manager TUI.

pub mod editor;
pub(crate) mod editor_geometry;
mod frame;
#[cfg(test)]
mod consistency_tests;
#[cfg(test)]
mod frame_tests;
pub(crate) mod footer;
pub(crate) mod list;
pub(crate) mod list_geometry;
pub(crate) mod modal;
pub(crate) mod modal_layout;
pub(crate) mod mount_display;
mod pre_render;
pub(crate) mod save_preview;
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
    render_horizontal_scrollbar, render_line_with_fixed_prefix_scroll, render_scrollable_block_at,
    render_vertical_scrollbar,
};
