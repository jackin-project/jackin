//! Terminal-rendering code for the jackin-capsule multiplexer.
//!
//! Everything that directly renders, paints, or computes the in-container
//! terminal UI lives here, per the TUI source-location convention in
//! `tui-design-decisions.mdx`.

pub mod model;
pub mod components;
pub mod effect;
pub(crate) mod host_colors;
pub mod input;
pub(crate) mod keymap;
pub mod layout;
pub mod message;
pub mod render;
pub mod run;
pub mod selection;
pub mod socket_backend;
pub mod subscriptions;
pub mod terminal;
pub mod title;
pub mod update;
pub mod view;
