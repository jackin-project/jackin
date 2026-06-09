//! View functions for the workspace manager TUI.

#[cfg(test)]
mod consistency_tests;
pub mod editor;
mod frame;
#[cfg(test)]
mod frame_tests;
#[cfg(test)]
mod list;
pub(crate) mod settings;
#[cfg(test)]
mod snapshot_tests;

pub use frame::render;
