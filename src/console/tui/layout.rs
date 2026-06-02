//! Root-console layout and geometry adapters.

pub(crate) mod editor;
pub(crate) mod list;
mod prepare;
pub(crate) mod settings;

pub use prepare::prepare_for_render;
