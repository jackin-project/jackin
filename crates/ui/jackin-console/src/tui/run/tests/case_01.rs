// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
        quit_confirm_plan(jackin_oppicker::ModalOutcome::Commit(true)),
        QuitConfirmPlan::Exit
    );
    assert_eq!(
        quit_confirm_plan(jackin_oppicker::ModalOutcome::Commit(false)),
        QuitConfirmPlan::Dismiss
    );
    assert_eq!(
        quit_confirm_plan(jackin_oppicker::ModalOutcome::Cancel),
        QuitConfirmPlan::Dismiss
    );
    assert_eq!(
        quit_confirm_plan(jackin_oppicker::ModalOutcome::Continue),
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
fn debug_invocation_id_label_uses_only_the_active_invocation() {
    assert_eq!(
        debug_invocation_id_label(Some("invocation-active")),
        "invocation-active"
    );
    assert_eq!(debug_invocation_id_label(Some("")), "");
    assert_eq!(debug_invocation_id_label(None), "");
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
