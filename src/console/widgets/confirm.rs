//! Compatibility facade for the shared confirm dialog component.

pub use jackin_tui::components::confirm_dialog::{
    ConfirmFocus, ConfirmKind, ConfirmState, render_confirm_dialog as render, required_height,
    width_pct,
};
