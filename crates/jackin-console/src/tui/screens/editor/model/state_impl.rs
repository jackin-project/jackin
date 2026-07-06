//! `EditorState` impl blocks grouped by editor responsibility.
//!
//! The child modules are semantic seams: pending/console trait adapters,
//! navigation and modal lifecycle helpers, and workspace mutation/change
//! accounting. Keep future editor impls near the behavior they own.

mod navigation;
mod pending;
mod workspace;
