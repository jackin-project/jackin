//! Compatibility facade for the shared error dialog component.

pub use jackin_tui::components::error_dialog::{
    ErrorPopupState, estimated_message_rows, render_error_dialog as render, required_height,
};
