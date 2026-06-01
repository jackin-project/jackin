//! Daemon dispatch actions.
//!
//! Input parsing answers "what did the terminal send?" Actions answer "what
//! should the multiplexer do with it?" so dispatch can become testable without
//! a live PTY or attach socket.

use crate::tui::{
    dialog::{DialogAction, PaletteCommand, PickerIntent, SplitDirection},
    input::{ArrowDir, InputEvent, PrefixCommand},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    OpenPalette,
    OpenContainerInfo,
    OpenGithubContext,
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
        InputEvent::PrefixCommand(cmd) => Some(Action::Prefix(cmd.clone())),
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
        InputEvent::MousePress { row, col, button: 0 } if context.branch_context_hit => {
            Some(Action::BranchContextBarClick {
                row: *row,
                col: *col,
            })
        }
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
        PrefixCommand::Palette => Some(Action::OpenPalette),
        PrefixCommand::Redraw => None,
    }
}

fn is_wheel_button(button: u8) -> bool {
    (64..96).contains(&button)
}

#[cfg(test)]
mod tests {
    use super::{Action, InputDispatchContext, input_event_action, mouse_chrome_update_action};
    use crate::tui::input::InputEvent;
    use crate::tui::input::PrefixCommand;
    use crate::tui::dialog::{PickerIntent, SplitDirection};
    use super::prefix_command_action;

    #[test]
    fn mouse_press_updates_chrome_before_main_action() {
        let event = InputEvent::MousePress {
            row: 2,
            col: 3,
            button: 0,
        };
        assert_eq!(
            mouse_chrome_update_action(&event),
            Some(Action::MouseChromeUpdate {
                row: 2,
                col: 3,
                button: 0,
            })
        );
        assert_eq!(
            input_event_action(&event, InputDispatchContext::default()),
            Some(Action::PanePrimaryPress { row: 2, col: 3 })
        );
    }

    #[test]
    fn dialog_captures_mouse_press_and_release() {
        let context = InputDispatchContext {
            dialog_captures_input: true,
            branch_context_hit: false,
        };
        assert_eq!(
            input_event_action(
                &InputEvent::MousePress {
                    row: 2,
                    col: 3,
                    button: 0,
                },
                context
            ),
            Some(Action::DialogClick { row: 2, col: 3 })
        );
        assert_eq!(
            input_event_action(
                &InputEvent::MouseRelease {
                    row: 2,
                    col: 3,
                    button: 0,
                },
                context
            ),
            None
        );
    }

    #[test]
    fn branch_context_click_wins_before_status_body_dispatch() {
        let event = InputEvent::MousePress {
            row: 4,
            col: 7,
            button: 0,
        };
        assert_eq!(
            input_event_action(
                &event,
                InputDispatchContext {
                    dialog_captures_input: false,
                    branch_context_hit: true,
                },
            ),
            Some(Action::BranchContextBarClick { row: 4, col: 7 })
        );
    }

    #[test]
    fn prefix_commands_map_to_semantic_actions() {
        assert_eq!(
            prefix_command_action(&PrefixCommand::NewTab),
            Some(Action::OpenAgentPicker(PickerIntent::NewTab))
        );
        assert_eq!(
            prefix_command_action(&PrefixCommand::SplitTopBottom),
            Some(Action::SplitFocused(SplitDirection::Below))
        );
        assert_eq!(prefix_command_action(&PrefixCommand::Redraw), None);
    }
}
