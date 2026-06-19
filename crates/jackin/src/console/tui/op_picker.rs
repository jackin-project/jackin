//! 1Password vault/item/field picker modal — thin root facade.
//!
//! All state, input handlers, load-execution helpers, and tests now live in
//! `jackin-console`. This module re-exports the types the rest of the binary
//! crate needs.

pub(crate) use jackin_console::tui::op_picker::OpPickerState;
