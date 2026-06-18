//! Tests for `run`.
use super::*;
use crate::tui::app::ConsoleManagerStageRoute;
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
        jackin_diagnostics::Screen::List
    );
    assert_eq!(
        diagnostics_screen_for_stage(ConsoleScreenStage::ConfirmDelete),
        jackin_diagnostics::Screen::List
    );
    assert_eq!(
        diagnostics_screen_for_stage(ConsoleScreenStage::ConfirmInstancePurge),
        jackin_diagnostics::Screen::List
    );
    assert_eq!(
        diagnostics_screen_for_stage(ConsoleScreenStage::Editor),
        jackin_diagnostics::Screen::Editor
    );
    assert_eq!(
        diagnostics_screen_for_stage(ConsoleScreenStage::Settings),
        jackin_diagnostics::Screen::Settings
    );
    assert_eq!(
        diagnostics_screen_for_stage(ConsoleScreenStage::CreatePrelude),
        jackin_diagnostics::Screen::Create
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
fn quit_intercept_ignores_main_text_input_and_modified_keys() {
    assert!(!should_open_quit_confirm(
        key(KeyCode::Char('q'), KeyModifiers::NONE),
        QuitInterceptState {
            on_main_screen: true,
            consumes_letter_input: false,
        },
    ));
    assert!(!should_open_quit_confirm(
        key(KeyCode::Char('q'), KeyModifiers::NONE),
        QuitInterceptState {
            on_main_screen: false,
            consumes_letter_input: true,
        },
    ));
    assert!(!should_open_quit_confirm(
        key(KeyCode::Char('q'), KeyModifiers::CONTROL),
        QuitInterceptState {
            on_main_screen: false,
            consumes_letter_input: false,
        },
    ));
}

#[test]
fn quit_confirm_plan_routes_confirm_outcomes() {
    assert_eq!(
        quit_confirm_plan(jackin_tui::ModalOutcome::Commit(true)),
        QuitConfirmPlan::Exit
    );
    assert_eq!(
        quit_confirm_plan(jackin_tui::ModalOutcome::Commit(false)),
        QuitConfirmPlan::Dismiss
    );
    assert_eq!(
        quit_confirm_plan(jackin_tui::ModalOutcome::Cancel),
        QuitConfirmPlan::Dismiss
    );
    assert_eq!(
        quit_confirm_plan(jackin_tui::ModalOutcome::Continue),
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
fn debug_run_id_label_uses_empty_fallback() {
    assert_eq!(debug_run_id_label(Some("run-1")), "run-1");
    assert_eq!(debug_run_id_label(None), "");
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
fn console_pointer_hand_uses_chrome_or_base_clickability() {
    assert!(!console_pointer_hand(false, false));
    assert!(console_pointer_hand(true, false));
    assert!(console_pointer_hand(false, true));
    assert!(console_pointer_hand(true, true));
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
