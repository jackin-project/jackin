//! Daemon dispatch actions.
//!
//! Input parsing answers "what did the terminal send?" Actions answer "what
//! should the multiplexer do with it?" so dispatch can become testable without
//! a live PTY or attach socket.

use crate::tui::components::branch_context_bar::BranchContextBarHit;
use crate::tui::components::dialog::{
    ConfirmKind, DialogAction, PaletteCommand, PickerIntent, SplitDirection,
};
use crate::tui::input::{ArrowDir, InputEvent, PrefixCommand, is_wheel_button};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    OpenPalette,
    /// Ctrl+Q: open the "Exit jackin'?" confirmation (data-loss variant).
    RequestExit,
    OpenContainerInfo,
    OpenGithubContext,
    OpenUsage,
    OpenRenameTab(usize),
    OpenAgentPicker(PickerIntent),
    SwitchTab(usize),
    NextTab,
    PreviousTab,
    JumpTab(usize),
    SplitFocused(SplitDirection),
    MoveFocus(ArrowDir),
    ToggleZoom,
    CloseFocusedPane,
    CloseFocusedTab,
    ClearFocusedPane,
    Detach,
    RefreshUsage,
    Palette(PaletteCommand),
    Prefix(PrefixCommand),
    ResizePane(ArrowDir),
    FocusReport(bool),
    MouseChromeUpdate {
        row: u16,
        col: u16,
        button: u8,
    },
    Wheel {
        row: u16,
        col: u16,
        button: u8,
    },
    FocusPaneAt {
        row: u16,
        col: u16,
    },
    PanePrimaryPress {
        row: u16,
        col: u16,
    },
    PaneButtonMotion {
        row: u16,
        col: u16,
    },
    StatusBarClick {
        col: u16,
    },
    BranchContextBarClick {
        row: u16,
        col: u16,
    },
    ForwardMouse {
        row: u16,
        col: u16,
        button: u8,
        press: bool,
    },
    MouseRelease {
        row: u16,
        col: u16,
        button: u8,
    },
    PaneData(Vec<u8>),
    StartDragResize {
        row: u16,
        col: u16,
    },
    DragMotion {
        row: u16,
        col: u16,
    },
    EndDragResize,
    StartSelection {
        row: u16,
        col: u16,
    },
    SelectionMotion {
        row: u16,
        col: u16,
    },
    FinalizeSelection,
    DialogClick {
        row: u16,
        col: u16,
    },
    Dialog(DialogAction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InputDispatchContext {
    pub dialog_captures_input: bool,
    pub branch_context_hit: bool,
}

pub fn mouse_chrome_update_action(event: &InputEvent) -> Option<Action> {
    match event {
        InputEvent::MousePress { col, row, button }
        | InputEvent::MouseRelease { col, row, button } => Some(Action::MouseChromeUpdate {
            row: *row,
            col: *col,
            button: *button,
        }),
        _ => None,
    }
}

pub fn input_event_action(event: &InputEvent, context: InputDispatchContext) -> Option<Action> {
    match event {
        InputEvent::Data(_) => None,
        InputEvent::OpenPalette => Some(Action::OpenPalette),
        InputEvent::RequestExit => Some(Action::RequestExit),
        InputEvent::PrefixCommand(cmd) => Some(Action::Prefix(*cmd)),
        InputEvent::ResizePane(dir) => Some(Action::ResizePane(*dir)),
        InputEvent::FocusIn | InputEvent::FocusOut => {
            Some(Action::FocusReport(matches!(event, InputEvent::FocusIn)))
        }
        InputEvent::MousePress { col, row, button }
            if context.dialog_captures_input && *button == 0 && !is_wheel_button(*button) =>
        {
            Some(Action::DialogClick {
                row: *row,
                col: *col,
            })
        }
        // A wheel event still reaches the dialog scroll handler while a dialog
        // captures input — it scrolls the dialog body (Action::Wheel dispatch),
        // not the pane behind it. Must precede the swallow arm below, which
        // otherwise eats every non-left-click press including the wheel.
        InputEvent::MousePress { col, row, button }
            if context.dialog_captures_input && is_wheel_button(*button) =>
        {
            Some(Action::Wheel {
                row: *row,
                col: *col,
                button: *button,
            })
        }
        InputEvent::MousePress { .. } if context.dialog_captures_input => None,
        InputEvent::MouseRelease { .. } if context.dialog_captures_input => None,
        InputEvent::MouseRelease { col, row, button } => Some(Action::MouseRelease {
            row: *row,
            col: *col,
            button: *button,
        }),
        InputEvent::MousePress { col, row, button } if is_wheel_button(*button) => {
            Some(Action::Wheel {
                row: *row,
                col: *col,
                button: *button,
            })
        }
        InputEvent::MousePress {
            row,
            col,
            button: 0,
        } if context.branch_context_hit => Some(Action::BranchContextBarClick {
            row: *row,
            col: *col,
        }),
        InputEvent::MousePress {
            row: 0,
            col,
            button: 0,
        } => Some(Action::StatusBarClick { col: *col }),
        InputEvent::MousePress { col, row, button } => {
            if *button == 32 {
                Some(Action::PaneButtonMotion {
                    row: *row,
                    col: *col,
                })
            } else if *button == 0 {
                Some(Action::PanePrimaryPress {
                    row: *row,
                    col: *col,
                })
            } else {
                Some(Action::ForwardMouse {
                    row: *row,
                    col: *col,
                    button: *button,
                    press: true,
                })
            }
        }
    }
}

pub fn prefix_command_action(cmd: &PrefixCommand) -> Option<Action> {
    match cmd {
        PrefixCommand::NewTab => Some(Action::OpenAgentPicker(PickerIntent::NewTab)),
        PrefixCommand::NextTab => Some(Action::NextTab),
        PrefixCommand::PrevTab => Some(Action::PreviousTab),
        PrefixCommand::JumpTab(i) => Some(Action::JumpTab(*i)),
        PrefixCommand::SplitTopBottom => Some(Action::SplitFocused(SplitDirection::Below)),
        PrefixCommand::SplitSideBySide => Some(Action::SplitFocused(SplitDirection::Right)),
        PrefixCommand::MoveFocus(dir) => Some(Action::MoveFocus(*dir)),
        PrefixCommand::ZoomToggle => Some(Action::ToggleZoom),
        PrefixCommand::KillPane => Some(Action::CloseFocusedPane),
        PrefixCommand::KillTab => Some(Action::CloseFocusedTab),
        PrefixCommand::ClearPane => Some(Action::ClearFocusedPane),
        PrefixCommand::Detach => Some(Action::Detach),
        PrefixCommand::Usage => Some(Action::OpenUsage),
        PrefixCommand::Palette => Some(Action::OpenPalette),
        PrefixCommand::Redraw => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaletteCommandRoute {
    OpenSplitDirectionPicker,
    OpenAgentPicker(PickerIntent),
    NextTab,
    PreviousTab,
    ConfirmAction(ConfirmKind),
    OpenCloseTargetPicker,
    ToggleZoom,
    ClearPane,
    OpenUsage,
}

pub(crate) fn palette_command_route(
    cmd: PaletteCommand,
    active_tab_pane_count: usize,
) -> PaletteCommandRoute {
    match cmd {
        PaletteCommand::Split => PaletteCommandRoute::OpenSplitDirectionPicker,
        PaletteCommand::NewTab => PaletteCommandRoute::OpenAgentPicker(PickerIntent::NewTab),
        PaletteCommand::NextTab => PaletteCommandRoute::NextTab,
        PaletteCommand::PrevTab => PaletteCommandRoute::PreviousTab,
        PaletteCommand::Close if active_tab_pane_count == 1 => {
            PaletteCommandRoute::ConfirmAction(ConfirmKind::CloseTab)
        }
        PaletteCommand::Close => PaletteCommandRoute::OpenCloseTargetPicker,
        PaletteCommand::ZoomPane => PaletteCommandRoute::ToggleZoom,
        PaletteCommand::ClearPane => PaletteCommandRoute::ClearPane,
        PaletteCommand::Usage => PaletteCommandRoute::OpenUsage,
        PaletteCommand::Exit => PaletteCommandRoute::ConfirmAction(ConfirmKind::Exit),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfirmedActionRoute {
    ClosePane,
    CloseTab,
    ExitAllSessions,
}

pub(crate) fn confirmed_action_route(kind: ConfirmKind) -> ConfirmedActionRoute {
    match kind {
        ConfirmKind::ClosePane => ConfirmedActionRoute::ClosePane,
        ConfirmKind::CloseTab => ConfirmedActionRoute::CloseTab,
        ConfirmKind::Exit => ConfirmedActionRoute::ExitAllSessions,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaletteToggleRoute {
    CloseDialog,
    OpenPalette,
}

pub(crate) fn palette_toggle_route(dialog_open: bool) -> PaletteToggleRoute {
    if dialog_open {
        PaletteToggleRoute::CloseDialog
    } else {
        PaletteToggleRoute::OpenPalette
    }
}

pub fn pane_button_motion_action(dragging: bool, selecting: bool, row: u16, col: u16) -> Action {
    if dragging {
        Action::DragMotion { row, col }
    } else if selecting {
        Action::SelectionMotion { row, col }
    } else {
        Action::ForwardMouse {
            row,
            col,
            button: 32,
            press: true,
        }
    }
}

pub fn mouse_release_action(
    dragging: bool,
    selecting: bool,
    row: u16,
    col: u16,
    button: u8,
) -> Action {
    if dragging && (button & 0b11) == 0 {
        Action::EndDragResize
    } else if selecting && (button & 0b11) == 0 {
        Action::FinalizeSelection
    } else {
        Action::ForwardMouse {
            row,
            col,
            button,
            press: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatusBarClickState {
    pub tab: Option<usize>,
    pub tab_count: usize,
    pub double_click: bool,
    pub menu_hit: bool,
}

pub fn status_bar_click_action(state: StatusBarClickState) -> Option<Action> {
    if let Some(idx) = state.tab
        && idx < state.tab_count
    {
        return Some(if state.double_click {
            Action::OpenRenameTab(idx)
        } else {
            Action::SwitchTab(idx)
        });
    }
    state.menu_hit.then_some(Action::OpenPalette)
}

pub(crate) fn branch_context_bar_click_action(hit: Option<BranchContextBarHit>) -> Option<Action> {
    match hit {
        Some(BranchContextBarHit::Context) => Some(Action::OpenGithubContext),
        Some(BranchContextBarHit::UsageStatus) => Some(Action::OpenUsage),
        Some(BranchContextBarHit::Container) => Some(Action::OpenContainerInfo),
        // Debug chip also opens the shared ContainerInfo dialog.
        Some(BranchContextBarHit::DebugChip) => Some(Action::OpenContainerInfo),
        None => None,
    }
}

#[cfg(test)]
mod tests;
