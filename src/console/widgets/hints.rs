//! Compatibility facade for the shared hint-bar component.
//!
//! New TUI surfaces should import `jackin_tui::components::HintBar` or the
//! `render_hint_bar` helpers directly. Existing console call sites keep this
//! module path while the larger architecture migration proceeds.

pub use jackin_tui::components::{
    hint_line as line, render_hint_bar as render, render_wrapped_hint_bar as render_wrapped,
    wrapped_height,
};
