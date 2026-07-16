// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `run`.
use super::*;
use crate::tui::model::ConsoleManagerStageRoute;
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

const fn mouse(kind: MouseEventKind) -> MouseEvent {
    MouseEvent {
        kind,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    }
}

const fn mouse_at(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn diagnostics_screen_maps_confirm_overlays_to_list() {
    assert_eq!(
        diagnostics_screen_for_stage(ConsoleScreenStage::List),
        jackin_telemetry::schema::enums::ScreenId::WorkspaceList
    );
    assert_eq!(
        diagnostics_screen_for_stage(ConsoleScreenStage::ConfirmDelete),
        jackin_telemetry::schema::enums::ScreenId::WorkspaceList
    );
    assert_eq!(
        diagnostics_screen_for_stage(ConsoleScreenStage::ConfirmInstancePurge),
        jackin_telemetry::schema::enums::ScreenId::WorkspaceList
    );
    assert_eq!(
        diagnostics_screen_for_stage(ConsoleScreenStage::Editor),
        jackin_telemetry::schema::enums::ScreenId::WorkspaceEditor
    );
    assert_eq!(
        diagnostics_screen_for_stage(ConsoleScreenStage::Settings),
        jackin_telemetry::schema::enums::ScreenId::Settings
    );
    assert_eq!(
        diagnostics_screen_for_stage(ConsoleScreenStage::CreatePrelude),
        jackin_telemetry::schema::enums::ScreenId::WorkspaceCreate
    );
}

#[test]
fn console_screen_stage_routes_manager_routes() {
    assert_eq!(
        console_screen_stage_for_route(ConsoleManagerStageRoute::List),
        ConsoleScreenStage::List
    );
    assert_eq!(
        console_screen_stage_for_route(ConsoleManagerStageRoute::Editor),
        ConsoleScreenStage::Editor
    );
    assert_eq!(
        console_screen_stage_for_route(ConsoleManagerStageRoute::Settings),
        ConsoleScreenStage::Settings
    );
    assert_eq!(
        console_screen_stage_for_route(ConsoleManagerStageRoute::CreatePrelude),
        ConsoleScreenStage::CreatePrelude
    );
    assert_eq!(
        console_screen_stage_for_route(ConsoleManagerStageRoute::ConfirmDelete),
        ConsoleScreenStage::ConfirmDelete
    );
    assert_eq!(
        console_screen_stage_for_route(ConsoleManagerStageRoute::ConfirmInstancePurge),
        ConsoleScreenStage::ConfirmInstancePurge
    );
}

#[test]
fn main_screen_requires_plain_workspace_list() {
    assert!(is_main_screen(MainScreenState {
        workspace_list: true,
        list_modal_open: false,
    }));
    assert!(!is_main_screen(MainScreenState {
        workspace_list: true,
        list_modal_open: true,
    }));
    assert!(!is_main_screen(MainScreenState {
        workspace_list: false,
        list_modal_open: false,
    }));
}

#[test]
fn main_screen_for_route_requires_plain_list_route() {
    assert!(is_main_screen_for_route(
        ConsoleManagerStageRoute::List,
        false
    ));
    assert!(!is_main_screen_for_route(
        ConsoleManagerStageRoute::List,
        true
    ));
    assert!(!is_main_screen_for_route(
        ConsoleManagerStageRoute::Editor,
        false
    ));
}

#[test]
fn quit_intercept_opens_off_main_for_bare_q() {
    let state = QuitInterceptState {
        on_main_screen: false,
        consumes_letter_input: false,
    };

    assert!(should_open_quit_confirm(
        key(KeyCode::Char('q'), KeyModifiers::NONE),
        state,
    ));
    assert!(should_open_quit_confirm(
        key(KeyCode::Char('Q'), KeyModifiers::SHIFT),
        state,
    ));
}

#[test]
fn quit_intercept_ignores_letter_input_and_allows_ctrl_q_everywhere() {
    // Bare q is blocked when a field consumes letter input (e.g. text filter).
    assert!(!should_open_quit_confirm(
        key(KeyCode::Char('q'), KeyModifiers::NONE),
        QuitInterceptState {
            on_main_screen: false,
            consumes_letter_input: true,
        },
    ));
    // Ctrl+Q is the explicit quit chord: it opens the confirm everywhere,
    // even while a field is consuming letter input.
    assert!(should_open_quit_confirm(
        key(KeyCode::Char('q'), KeyModifiers::CONTROL),
        QuitInterceptState {
            on_main_screen: false,
            consumes_letter_input: true,
        },
    ));
    // on_main_screen=true is preserved in the struct for API compatibility but
    // the host console no longer passes true — bare q opens the confirm on
    // every screen now that the workspace list is not exempt.
}

#[test]
fn quit_confirm_plan_routes_confirm_outcomes() {
    assert_eq!(
        quit_confirm_plan(termrock::ModalOutcome::Commit(true)),
        QuitConfirmPlan::Exit
    );
    assert_eq!(
        quit_confirm_plan(termrock::ModalOutcome::Commit(false)),
        QuitConfirmPlan::Dismiss
    );
    assert_eq!(
        quit_confirm_plan(termrock::ModalOutcome::Cancel),
        QuitConfirmPlan::Dismiss
    );
    assert_eq!(
        quit_confirm_plan(termrock::ModalOutcome::Continue),
        QuitConfirmPlan::Continue
    );
}

#[test]
fn letter_input_state_detects_text_and_filter_modals() {
    assert_eq!(
        letter_input_modal_kind(true, true, true),
        Some(LetterInputModalKind::TextInput)
    );
    assert_eq!(
        letter_input_modal_kind(false, true, true),
        Some(LetterInputModalKind::FilterPicker)
    );
    assert_eq!(
        letter_input_modal_kind(false, false, true),
        Some(LetterInputModalKind::Other)
    );
    assert_eq!(letter_input_modal_kind(false, false, false), None);

    assert!(consumes_letter_input(LetterInputState {
        editor_modal: Some(LetterInputModalKind::TextInput),
        ..LetterInputState::default()
    }));
    assert!(consumes_letter_input(LetterInputState {
        list_modal: Some(LetterInputModalKind::FilterPicker),
        ..LetterInputState::default()
    }));
    assert!(!consumes_letter_input(LetterInputState {
        settings_mount_modal: Some(LetterInputModalKind::Other),
        ..LetterInputState::default()
    }));
    assert!(!consumes_letter_input(LetterInputState::default()));
}

#[test]
fn letter_input_state_for_route_assigns_stage_modal_slot() {
    let list_kind = Some(LetterInputModalKind::Other);
    let stage_kind = Some(LetterInputModalKind::TextInput);

    assert_eq!(
        letter_input_state_for_route(ConsoleManagerStageRoute::Editor, list_kind, stage_kind),
        LetterInputState {
            list_modal: list_kind,
            editor_modal: stage_kind,
            ..LetterInputState::default()
        }
    );
    assert_eq!(
        letter_input_state_for_route(
            ConsoleManagerStageRoute::CreatePrelude,
            list_kind,
            stage_kind
        ),
        LetterInputState {
            list_modal: list_kind,
            create_prelude_modal: stage_kind,
            ..LetterInputState::default()
        }
    );
    assert_eq!(
        letter_input_state_for_route(ConsoleManagerStageRoute::Settings, list_kind, stage_kind),
        LetterInputState {
            list_modal: list_kind,
            settings_mount_modal: stage_kind,
            ..LetterInputState::default()
        }
    );
    assert_eq!(
        letter_input_state_for_route(ConsoleManagerStageRoute::List, list_kind, stage_kind),
        LetterInputState {
            list_modal: list_kind,
            ..LetterInputState::default()
        }
    );
}

#[test]
fn token_generate_status_message_names_target_scope() {
    assert_eq!(
        token_generate_scope_label(TokenGenerateScopeLabel::Workspace("proj")),
        "workspace \"proj\""
    );
    assert_eq!(
        token_generate_scope_label(TokenGenerateScopeLabel::WorkspaceRole {
            workspace: "proj",
            role: "ops",
        }),
        "workspace \"proj\" role \"ops\""
    );
    assert_eq!(
        token_generate_status_message(TokenGenerateScopeLabel::Global),
        "\nGenerating Claude OAuth token for global config -- complete the browser sign-in, then paste the code below.\n"
    );
}

#[test]
fn debug_run_id_label_prefers_active_run_then_env() {
    assert_eq!(
        debug_run_id_label(Some("run-active"), Some("run-env")),
        "run-active"
    );
    assert_eq!(debug_run_id_label(None, Some("run-env")), "run-env");
    assert_eq!(debug_run_id_label(Some(""), Some("run-env")), "run-env");
    assert_eq!(debug_run_id_label(None, None), "");
}

#[test]
fn modal_block_state_controls_base_surface_input() {
    assert!(no_modal_blocks_base_surface(ModalBlockState::default()));
    assert!(!no_modal_blocks_base_surface(ModalBlockState {
        quit_confirm: true,
        ..ModalBlockState::default()
    }));
    assert!(!no_modal_blocks_base_surface(ModalBlockState {
        list_modal: true,
        ..ModalBlockState::default()
    }));
    assert!(!no_modal_blocks_base_surface(ModalBlockState {
        editor_modal: true,
        ..ModalBlockState::default()
    }));
}

#[test]
fn startup_error_policy_uses_pending_and_list_modal_facts() {
    assert!(!startup_error_was_dismissed(true, true));
    assert!(startup_error_was_dismissed(true, false));
    assert!(!startup_error_was_dismissed(false, false));

    assert!(startup_error_modal_active(true, true));
    assert!(!startup_error_modal_active(true, false));
    assert!(!startup_error_modal_active(false, true));
}

#[test]
fn console_clickability_policy_routes_modal_and_stage_targets() {
    assert!(!console_clickable_at(ConsoleClickabilityFacts {
        pointer_supported: false,
        file_browser_url_target: true,
        container_info_copy_target: false,
        stage: ConsoleClickStageFacts::Other,
    }));

    assert!(console_clickable_at(ConsoleClickabilityFacts {
        pointer_supported: true,
        file_browser_url_target: true,
        container_info_copy_target: false,
        stage: ConsoleClickStageFacts::Other,
    }));
    assert!(console_clickable_at(ConsoleClickabilityFacts {
        pointer_supported: true,
        file_browser_url_target: false,
        container_info_copy_target: true,
        stage: ConsoleClickStageFacts::Other,
    }));

    assert!(console_clickable_at(ConsoleClickabilityFacts {
        pointer_supported: true,
        file_browser_url_target: false,
        container_info_copy_target: false,
        stage: ConsoleClickStageFacts::List {
            list_modal_open: false,
            workspace_list_target: true,
        },
    }));
    assert!(!console_clickable_at(ConsoleClickabilityFacts {
        pointer_supported: true,
        file_browser_url_target: false,
        container_info_copy_target: false,
        stage: ConsoleClickStageFacts::List {
            list_modal_open: true,
            workspace_list_target: true,
        },
    }));

    assert!(console_clickable_at(ConsoleClickabilityFacts {
        pointer_supported: true,
        file_browser_url_target: false,
        container_info_copy_target: false,
        stage: ConsoleClickStageFacts::Editor {
            modal_open: false,
            tab_target: false,
            mount_row_target: true,
            auth_row_target: false,
        },
    }));
    assert!(!console_clickable_at(ConsoleClickabilityFacts {
        pointer_supported: true,
        file_browser_url_target: false,
        container_info_copy_target: false,
        stage: ConsoleClickStageFacts::Editor {
            modal_open: true,
            tab_target: true,
            mount_row_target: true,
            auth_row_target: true,
        },
    }));

    assert!(console_clickable_at(ConsoleClickabilityFacts {
        pointer_supported: true,
        file_browser_url_target: false,
        container_info_copy_target: false,
        stage: ConsoleClickStageFacts::Settings {
            mounts_modal_open: false,
            env_modal_open: false,
            tab_target: false,
            trust_target: true,
        },
    }));
    assert!(!console_clickable_at(ConsoleClickabilityFacts {
        pointer_supported: true,
        file_browser_url_target: false,
        container_info_copy_target: false,
        stage: ConsoleClickStageFacts::Settings {
            mounts_modal_open: false,
            env_modal_open: true,
            tab_target: true,
            trust_target: true,
        },
    }));
}

#[test]
fn modal_mouse_layer_policy_routes_container_info_wheel_to_base() {
    assert!(modal_mouse_layer_consumes(
        mouse(MouseEventKind::ScrollDown),
        ConsoleModalMouseFacts {
            quit_confirm_open: true,
            list_modal_open: true,
            list_modal_container_info: true,
        },
    ));

    assert!(modal_mouse_layer_consumes(
        mouse(MouseEventKind::Down(crossterm::event::MouseButton::Left)),
        ConsoleModalMouseFacts {
            list_modal_open: true,
            list_modal_container_info: true,
            ..ConsoleModalMouseFacts::default()
        },
    ));

    assert!(modal_mouse_layer_consumes(
        mouse(MouseEventKind::ScrollDown),
        ConsoleModalMouseFacts {
            list_modal_open: true,
            list_modal_container_info: false,
            ..ConsoleModalMouseFacts::default()
        },
    ));

    assert!(!modal_mouse_layer_consumes(
        mouse(MouseEventKind::ScrollDown),
        ConsoleModalMouseFacts {
            list_modal_open: true,
            list_modal_container_info: true,
            ..ConsoleModalMouseFacts::default()
        },
    ));

    assert!(!modal_mouse_layer_consumes(
        mouse(MouseEventKind::Moved),
        ConsoleModalMouseFacts::default(),
    ));
}

#[test]
fn modal_mouse_layer_plan_gives_quit_confirm_precedence() {
    let quit_rect = Rect::new(10, 5, 20, 8);
    let list_rect = Rect::new(30, 5, 20, 8);

    let plan = modal_mouse_layer_plan(
        mouse_at(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            0,
            0,
        ),
        ConsoleModalMouseLayerFacts {
            quit_confirm_rect: Some(quit_rect),
            list_modal_rect: Some(list_rect),
            ..ConsoleModalMouseLayerFacts::default()
        },
    );

    assert_eq!(
        plan,
        ConsoleModalMouseLayerPlan {
            consumed: true,
            dismiss_quit_confirm: true,
            dismiss_list_modal: false,
        }
    );
}

#[test]
fn modal_mouse_layer_plan_dismisses_list_modal_only_when_allowed() {
    let list_rect = Rect::new(10, 5, 20, 8);

    let dismiss = modal_mouse_layer_plan(
        mouse_at(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            0,
            0,
        ),
        ConsoleModalMouseLayerFacts {
            list_modal_rect: Some(list_rect),
            ..ConsoleModalMouseLayerFacts::default()
        },
    );
    assert_eq!(
        dismiss,
        ConsoleModalMouseLayerPlan {
            consumed: true,
            dismiss_quit_confirm: false,
            dismiss_list_modal: true,
        }
    );

    let startup_error = modal_mouse_layer_plan(
        mouse_at(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            0,
            0,
        ),
        ConsoleModalMouseLayerFacts {
            list_modal_rect: Some(list_rect),
            startup_error_modal_active: true,
            ..ConsoleModalMouseLayerFacts::default()
        },
    );
    assert!(startup_error.consumed);
    assert!(!startup_error.dismiss_list_modal);

    let inside = modal_mouse_layer_plan(
        mouse_at(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            12,
            7,
        ),
        ConsoleModalMouseLayerFacts {
            list_modal_rect: Some(list_rect),
            ..ConsoleModalMouseLayerFacts::default()
        },
    );
    assert!(inside.consumed);
    assert!(!inside.dismiss_list_modal);
}

#[test]
fn modal_mouse_layer_plan_allows_container_info_wheel_fallthrough() {
    let plan = modal_mouse_layer_plan(
        mouse(MouseEventKind::ScrollDown),
        ConsoleModalMouseLayerFacts {
            list_modal_rect: Some(Rect::new(10, 5, 20, 8)),
            list_modal_container_info: true,
            ..ConsoleModalMouseLayerFacts::default()
        },
    );

    assert_eq!(
        plan,
        ConsoleModalMouseLayerPlan {
            consumed: false,
            dismiss_quit_confirm: false,
            dismiss_list_modal: false,
        }
    );
}

#[test]
fn debug_chip_activation_requires_click_hover_and_run() {
    assert!(debug_chip_activation_allowed(
        mouse(MouseEventKind::Down(crossterm::event::MouseButton::Left)),
        true,
        true,
        true,
    ));
    assert!(!debug_chip_activation_allowed(
        mouse(MouseEventKind::Moved),
        true,
        true,
        true,
    ));
    assert!(!debug_chip_activation_allowed(
        mouse(MouseEventKind::Down(crossterm::event::MouseButton::Left)),
        false,
        true,
        true,
    ));
    assert!(!debug_chip_activation_allowed(
        mouse(MouseEventKind::Down(crossterm::event::MouseButton::Left)),
        true,
        false,
        true,
    ));
    assert!(!debug_chip_activation_allowed(
        mouse(MouseEventKind::Down(crossterm::event::MouseButton::Left)),
        true,
        true,
        false,
    ));
}

#[test]
fn console_pointer_shape_uses_chrome_or_base_clickability() {
    assert_eq!(
        console_pointer_shape(false, false),
        termrock::PointerShape::Default
    );
    assert_eq!(
        console_pointer_shape(true, false),
        termrock::PointerShape::Pointer
    );
    assert_eq!(
        console_pointer_shape(false, true),
        termrock::PointerShape::Pointer
    );
    assert_eq!(
        console_pointer_shape(true, true),
        termrock::PointerShape::Pointer
    );
}

#[test]
fn startup_error_modal_blocks_outside_click_dismissal() {
    let modal_rect = Rect::new(10, 5, 30, 10);

    assert!(!should_dismiss_list_modal_for_outside_click(
        true, modal_rect, 0, 0
    ));
    assert!(!should_dismiss_list_modal_for_outside_click(
        true, modal_rect, 12, 8
    ));
    assert!(should_dismiss_list_modal_for_outside_click(
        false, modal_rect, 0, 0
    ));
    assert!(!should_dismiss_list_modal_for_outside_click(
        false, modal_rect, 12, 8
    ));
}

// ── ConsoleState accessor tests ───────────────────────────────────────────────

#[test]
fn no_modal_open_returns_false_while_list_modal_open() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::tui::console::{ConsoleStage, ConsoleState};
    use crate::tui::state::{ManagerState, update::ManagerMessage, update::update_manager};

    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();

    let op_cache = Rc::new(RefCell::new(jackin_env::OpCache::default()));
    let clean_manager = ManagerState::from_config(&config, cwd);
    let clean_state = ConsoleState::new(ConsoleStage::Manager(clean_manager), op_cache, false);
    assert!(
        no_modal_open(&clean_state),
        "no modal by default — chip is active"
    );

    let mut manager_with_modal = ManagerState::from_config(&config, cwd);
    let _unused = update_manager(
        &mut manager_with_modal,
        ManagerMessage::OpenListErrorPopup {
            title: "Error".into(),
            message: "something failed".into(),
        },
    );
    let op_cache2 = Rc::new(RefCell::new(jackin_env::OpCache::default()));
    let state_with_modal =
        ConsoleState::new(ConsoleStage::Manager(manager_with_modal), op_cache2, false);
    assert!(
        !no_modal_open(&state_with_modal),
        "list_modal open → chip and base surface must not fire"
    );
}

#[test]
fn no_modal_open_returns_false_while_quit_confirm_open() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::tui::console::{ConsoleStage, ConsoleState};
    use crate::tui::state::ManagerState;

    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();
    let manager = ManagerState::from_config(&config, cwd);
    let op_cache = Rc::new(RefCell::new(jackin_env::OpCache::default()));
    let mut state = ConsoleState::new(ConsoleStage::Manager(manager), op_cache, false);

    assert!(no_modal_open(&state), "no modal by default");
    state.open_quit_confirm();
    assert!(!no_modal_open(&state), "quit_confirm → chip must not fire");
}

#[test]
fn startup_error_exit_gate_fires_after_dialog_dismissal() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::tui::console::{ConsoleStage, ConsoleState};
    use crate::tui::state::ManagerState;

    let cwd = std::path::Path::new("/");
    let config = jackin_config::AppConfig::default();
    let mut manager = ManagerState::from_config(&config, cwd);
    manager.open_list_error_popup("Docker daemon not reachable", "docker socket missing");
    let op_cache = Rc::new(RefCell::new(jackin_env::OpCache::default()));
    let mut state = ConsoleState::new(ConsoleStage::Manager(manager), op_cache, false);

    assert!(!startup_error_dismissed(&state, true));

    let ConsoleStage::Manager(manager) = &mut state.stage;
    manager.list_modal = None;

    assert!(startup_error_dismissed(&state, true));
    assert!(!startup_error_dismissed(&state, false));
}
