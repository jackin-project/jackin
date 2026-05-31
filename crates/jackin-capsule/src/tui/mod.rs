//! Terminal-rendering code for the jackin-capsule multiplexer.
//!
//! Everything that directly renders, paints, or computes the in-container
//! terminal UI lives here, per the TUI source-location convention in
//! `tui-design-decisions.mdx`.

pub mod branch_context_bar;
pub mod chrome_widget;
pub mod dialog;
pub mod dialog_widgets;
pub mod pane_widget;
pub mod render;
pub mod socket_backend;
pub mod statusbar;
