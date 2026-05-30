//! Compatibility facade for the shared error dialog component.

pub use jackin_tui::components::error_dialog::{
    ErrorPopupState, estimated_message_rows, required_height,
    render_error_dialog as render,
};
