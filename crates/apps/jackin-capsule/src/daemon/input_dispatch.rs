//! TUI input dispatch methods for the daemon-owned `Multiplexer`.

mod actions;
mod dialog_actions;
mod input_events;
mod pane_input;

#[cfg(test)]
pub(crate) use input_events::{TabBarFocusKey, tab_bar_focus_key};
