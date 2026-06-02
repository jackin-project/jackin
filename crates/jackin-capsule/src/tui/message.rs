//! Daemon dispatch actions.
//!
//! Input parsing answers "what did the terminal send?" Actions answer "what
//! should the multiplexer do with it?" so dispatch can become testable without
//! a live PTY or attach socket.

use crate::tui::components::branch_context_bar::BranchContextBarHit;
use crate::tui::components::dialog::{
    ConfirmKind, DialogAction, PaletteCommand, PickerIntent, SplitDirection,
};
use crate::tui::input::{ArrowDir, InputEvent, PrefixCommand};

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
        Some(BranchContextBarHit::Container) => Some(Action::OpenContainerInfo),
        None => None,
    }
}

fn is_wheel_button(button: u8) -> bool {
    (64..96).contains(&button)
}

#[cfg(test)]
mod tests {
    use super::{
        Action, ConfirmedActionRoute, InputDispatchContext, branch_context_bar_click_action,
        confirmed_action_route, input_event_action, mouse_chrome_update_action,
        mouse_release_action, palette_command_route,
        pane_button_motion_action, status_bar_click_action, PaletteCommandRoute,
        StatusBarClickState,
    };
    use crate::tui::components::branch_context_bar::BranchContextBarHit;
    use crate::tui::components::dialog::{ConfirmKind, PaletteCommand};
    use crate::tui::input::InputEvent;
    use crate::tui::input::PrefixCommand;
    use crate::tui::components::dialog::{PickerIntent, SplitDirection};
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

    #[test]
    fn palette_command_route_keeps_dialog_drill_down_semantics() {
        assert_eq!(
            palette_command_route(PaletteCommand::Split, 2),
            PaletteCommandRoute::OpenSplitDirectionPicker
        );
        assert_eq!(
            palette_command_route(PaletteCommand::NewTab, 1),
            PaletteCommandRoute::OpenAgentPicker(PickerIntent::NewTab)
        );
        assert_eq!(
            palette_command_route(PaletteCommand::Close, 1),
            PaletteCommandRoute::ConfirmAction(ConfirmKind::CloseTab)
        );
        assert_eq!(
            palette_command_route(PaletteCommand::Close, 2),
            PaletteCommandRoute::OpenCloseTargetPicker
        );
        assert_eq!(
            palette_command_route(PaletteCommand::Exit, 2),
            PaletteCommandRoute::ConfirmAction(ConfirmKind::Exit)
        );
    }

    #[test]
    fn confirmed_action_route_maps_confirm_kind_to_terminal_action() {
        assert_eq!(
            confirmed_action_route(ConfirmKind::ClosePane),
            ConfirmedActionRoute::ClosePane
        );
        assert_eq!(
            confirmed_action_route(ConfirmKind::CloseTab),
            ConfirmedActionRoute::CloseTab
        );
        assert_eq!(
            confirmed_action_route(ConfirmKind::Exit),
            ConfirmedActionRoute::ExitAllSessions
        );
    }

    #[test]
    fn pane_motion_action_prefers_drag_then_selection_then_forward() {
        assert_eq!(
            pane_button_motion_action(true, true, 4, 8),
            Action::DragMotion { row: 4, col: 8 }
        );
        assert_eq!(
            pane_button_motion_action(false, true, 4, 8),
            Action::SelectionMotion { row: 4, col: 8 }
        );
        assert_eq!(
            pane_button_motion_action(false, false, 4, 8),
            Action::ForwardMouse {
                row: 4,
                col: 8,
                button: 32,
                press: true,
            }
        );
    }

    #[test]
    fn mouse_release_action_closes_tui_gestures_before_forwarding() {
        assert_eq!(
            mouse_release_action(true, true, 4, 8, 0),
            Action::EndDragResize
        );
        assert_eq!(
            mouse_release_action(false, true, 4, 8, 0),
            Action::FinalizeSelection
        );
        assert_eq!(
            mouse_release_action(false, true, 4, 8, 1),
            Action::ForwardMouse {
                row: 4,
                col: 8,
                button: 1,
                press: false,
            }
        );
    }

    #[test]
    fn status_bar_click_action_routes_tabs_before_menu() {
        assert_eq!(
            status_bar_click_action(StatusBarClickState {
                tab: Some(2),
                tab_count: 3,
                double_click: false,
                menu_hit: true,
            }),
            Some(Action::SwitchTab(2))
        );
        assert_eq!(
            status_bar_click_action(StatusBarClickState {
                tab: Some(2),
                tab_count: 3,
                double_click: true,
                menu_hit: false,
            }),
            Some(Action::OpenRenameTab(2))
        );
        assert_eq!(
            status_bar_click_action(StatusBarClickState {
                tab: Some(3),
                tab_count: 3,
                double_click: true,
                menu_hit: true,
            }),
            Some(Action::OpenPalette)
        );
        assert_eq!(
            status_bar_click_action(StatusBarClickState::default()),
            None
        );
    }

    #[test]
    fn branch_context_bar_click_action_routes_context_and_container() {
        assert_eq!(
            branch_context_bar_click_action(Some(BranchContextBarHit::Context)),
            Some(Action::OpenGithubContext)
        );
        assert_eq!(
            branch_context_bar_click_action(Some(BranchContextBarHit::Container)),
            Some(Action::OpenContainerInfo)
        );
        assert_eq!(branch_context_bar_click_action(None), None);
    }
}
