//! Tests for `message`.
use super::prefix_command_action;
use super::{
    Action, ConfirmedActionRoute, InputDispatchContext, PaletteCommandRoute, PaletteToggleRoute,
    StatusBarClickState, branch_context_bar_click_action, confirmed_action_route,
    input_event_action, mouse_chrome_update_action, mouse_release_action, palette_command_route,
    palette_toggle_route, pane_button_motion_action, status_bar_click_action,
};
use crate::tui::components::branch_context_bar::BranchContextBarHit;
use crate::tui::components::dialog::{ConfirmKind, PaletteCommand};
use crate::tui::components::dialog::{PickerIntent, SplitDirection};
use crate::tui::input::InputEvent;
use crate::tui::input::PrefixCommand;

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
fn modified_primary_press_maps_to_visible_url_open_intent() {
    for button in [8u8, 16, 24] {
        assert_eq!(
            input_event_action(
                &InputEvent::MousePress {
                    row: 2,
                    col: 3,
                    button,
                },
                InputDispatchContext::default(),
            ),
            Some(Action::OpenVisibleUrlAt {
                row: 2,
                col: 3,
                button,
            }),
            "button {button} should be host-open-url intent",
        );
    }
    assert_eq!(
        input_event_action(
            &InputEvent::MousePress {
                row: 2,
                col: 3,
                button: 4,
            },
            InputDispatchContext::default(),
        ),
        Some(Action::ForwardMouse {
            row: 2,
            col: 3,
            button: 4,
            press: true,
        }),
        "shift-only primary press should keep the existing mouse fallback",
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
fn dialog_lets_wheel_events_through_for_body_scroll() {
    // Regression: a dialog must NOT swallow wheel events — they scroll the
    // dialog body. Vertical (64/65) and horizontal (66/67) wheel buttons both
    // become Action::Wheel even while a dialog captures input.
    let context = InputDispatchContext {
        dialog_captures_input: true,
        branch_context_hit: false,
    };
    for button in [64u8, 65, 66, 67] {
        assert_eq!(
            input_event_action(
                &InputEvent::MousePress {
                    row: 5,
                    col: 9,
                    button
                },
                context
            ),
            Some(Action::Wheel {
                row: 5,
                col: 9,
                button,
            }),
            "wheel button {button} must reach the dialog scroll handler",
        );
    }
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
        palette_command_route(PaletteCommand::ExportFile, 2),
        PaletteCommandRoute::OpenExportFileDialog {
            reveal_after_export: false,
            open_after_export: false
        }
    );
    assert_eq!(
        palette_command_route(PaletteCommand::ExportFileAndReveal, 2),
        PaletteCommandRoute::OpenExportFileDialog {
            reveal_after_export: true,
            open_after_export: false
        }
    );
    assert_eq!(
        palette_command_route(PaletteCommand::ExportFileAndOpen, 2),
        PaletteCommandRoute::OpenExportFileDialog {
            reveal_after_export: false,
            open_after_export: true
        }
    );
    assert_eq!(
        palette_command_route(PaletteCommand::ExportFileUnderCursor, 2),
        PaletteCommandRoute::ExportFileUnderCursor {
            reveal_after_export: false,
            open_after_export: false
        }
    );
    assert_eq!(
        palette_command_route(PaletteCommand::ExportFileUnderCursorAndReveal, 2),
        PaletteCommandRoute::ExportFileUnderCursor {
            reveal_after_export: true,
            open_after_export: false
        }
    );
    assert_eq!(
        palette_command_route(PaletteCommand::ExportFileUnderCursorAndOpen, 2),
        PaletteCommandRoute::ExportFileUnderCursor {
            reveal_after_export: false,
            open_after_export: true
        }
    );
    assert_eq!(
        palette_command_route(PaletteCommand::ExportSelectedFile, 2),
        PaletteCommandRoute::ExportSelectedFile {
            reveal_after_export: false,
            open_after_export: false
        }
    );
    assert_eq!(
        palette_command_route(PaletteCommand::ExportSelectedFileAndReveal, 2),
        PaletteCommandRoute::ExportSelectedFile {
            reveal_after_export: true,
            open_after_export: false
        }
    );
    assert_eq!(
        palette_command_route(PaletteCommand::ExportSelectedFileAndOpen, 2),
        PaletteCommandRoute::ExportSelectedFile {
            reveal_after_export: false,
            open_after_export: true
        }
    );
    assert_eq!(
        palette_command_route(PaletteCommand::StageImageFromClipboardPath, 2),
        PaletteCommandRoute::StageImageFromClipboardPath
    );
    assert_eq!(
        palette_command_route(PaletteCommand::PasteImageFromClipboard, 2),
        PaletteCommandRoute::PasteImageFromClipboard
    );
    assert_eq!(
        palette_command_route(PaletteCommand::StageImageFromClipboard, 2),
        PaletteCommandRoute::StageImageFromClipboard
    );
    assert_eq!(
        palette_command_route(PaletteCommand::OpenLinkUnderCursor, 2),
        PaletteCommandRoute::OpenLinkUnderCursor
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
fn palette_toggle_route_closes_existing_dialog_before_opening_palette() {
    assert_eq!(palette_toggle_route(true), PaletteToggleRoute::CloseDialog);
    assert_eq!(palette_toggle_route(false), PaletteToggleRoute::OpenPalette);
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
    assert_eq!(
        branch_context_bar_click_action(Some(BranchContextBarHit::UsageStatus)),
        Some(Action::OpenUsage)
    );
    assert_eq!(branch_context_bar_click_action(None), None);
}
