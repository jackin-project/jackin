//! Terminal-rendering code for the jackin-capsule multiplexer.
//!
//! Everything that directly renders, paints, or computes the in-container
//! terminal UI lives here, per the TUI source-location convention in
//! `tui-design-decisions.mdx`.

pub mod app;
pub mod components;
pub mod dialog;
pub mod message;
pub mod render;
pub mod selection;
pub mod socket_backend;
