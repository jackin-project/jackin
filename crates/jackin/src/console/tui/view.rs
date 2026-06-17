//! View functions for the workspace manager TUI.

pub mod editor;
mod frame;
#[cfg(test)]
mod list;
pub(crate) mod settings;
#[cfg(test)]
mod tests;

pub use frame::render;
