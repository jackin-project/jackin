//! View functions for the workspace manager TUI.

pub mod editor;
mod frame;
#[cfg(test)]
mod consistency_tests;
#[cfg(test)]
mod frame_tests;
pub(crate) mod list;
pub(crate) mod modal;
pub(crate) mod settings;
#[cfg(test)]
mod snapshot_tests;

pub use frame::render;

pub(crate) use jackin_tui::components::scrollable_panel::{
    viewport_width as scroll_viewport_width,
};
