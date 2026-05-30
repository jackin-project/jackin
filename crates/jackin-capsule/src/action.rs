//! Daemon dispatch actions.
//!
//! Input parsing answers "what did the terminal send?" Actions answer "what
//! should the multiplexer do with it?" so dispatch can become testable without
//! a live PTY or attach socket.

use crate::{
    dialog::DialogAction,
    input::{ArrowDir, PrefixCommand},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    OpenPalette,
    Prefix(PrefixCommand),
    ResizePane(ArrowDir),
    Dialog(DialogAction),
}
