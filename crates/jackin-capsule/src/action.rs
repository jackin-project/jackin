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
    OpenContainerInfo,
    OpenGithubContext,
    OpenRenameTab(usize),
    SwitchTab(usize),
    Prefix(PrefixCommand),
    ResizePane(ArrowDir),
    FocusReport(bool),
    Wheel { row: u16, col: u16, button: u8 },
    PaneData(Vec<u8>),
    StartDragResize { row: u16, col: u16 },
    DragMotion { row: u16, col: u16 },
    EndDragResize,
    StartSelection { row: u16, col: u16 },
    SelectionMotion { row: u16, col: u16 },
    FinalizeSelection,
    Dialog(DialogAction),
}
