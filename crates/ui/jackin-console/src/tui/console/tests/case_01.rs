// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn startup_error_opens_list_error_dialog() {
    let config = AppConfig::default();
    let cwd = std::path::Path::new("/");
    let state = new_console_state_with_startup_error(
        &config,
        cwd,
        false,
        Some((
            "Docker daemon not reachable".into(),
            "failed to connect to Docker daemon".into(),
        )),
    )
    .expect("console state");

    let ConsoleStage::Manager(manager) = state.stage;
    let Some(Modal::ErrorPopup { state: popup }) = manager.list_modal else {
        panic!("startup Docker failure should open ErrorDialog");
    };
    assert_eq!(popup.title, "Docker daemon not reachable");
    assert_eq!(popup.message, "failed to connect to Docker daemon");
}

#[test]
fn main_screen_is_list_with_no_modal() {
    let state = fresh_state();
    assert!(is_on_main_screen(&state));
    assert!(!consumes_letter_input(letter_input_state_for_console(
        &state
    )));
}

#[test]
fn editor_stage_is_not_main_screen() {
    let mut state = fresh_state();
    let ConsoleStage::Manager(ms) = &mut state.stage;
    ms.stage = ManagerStage::Editor(EditorState::new_create());
    assert!(!is_on_main_screen(&state));
}

#[test]
fn list_modal_is_not_main_screen() {
    let mut state = fresh_state();
    let ConsoleStage::Manager(ms) = &mut state.stage;
    ms.list_modal = Some(Modal::FileBrowser {
        target: FileBrowserTarget::CreateFirstMountSrc,
        state: FileBrowserState::from_listing(listing_from_home().unwrap()),
    });
    assert!(!is_on_main_screen(&state));
}

#[test]
fn text_input_modal_consumes_letter_input() {
    let mut state = fresh_state();
    let ConsoleStage::Manager(ms) = &mut state.stage;
    let mut editor = EditorState::new_create();
    editor.modal = Some(Modal::TextInput {
        target: TextInputTarget::EnvKey {
            scope: SecretsScopeTag::Workspace,
        },
        state: TextInputState::new("Key", ""),
    });
    ms.stage = ManagerStage::Editor(editor);
    assert!(consumes_letter_input(letter_input_state_for_console(
        &state
    )));
    assert!(!is_on_main_screen(&state));
}

#[test]
fn debug_key_redacts_text_input_characters() {
    let mut state = fresh_state();
    let ConsoleStage::Manager(ms) = &mut state.stage;
    let mut editor = EditorState::new_create();
    editor.modal = Some(Modal::TextInput {
        target: TextInputTarget::EnvValue {
            scope: SecretsScopeTag::Workspace,
            key: "TOKEN".into(),
        },
        state: TextInputState::new("Value", ""),
    });
    ms.stage = ManagerStage::Editor(editor);

    assert_eq!(
        key_debug_name_for_input(
            key(crossterm::event::KeyCode::Char('s')),
            consumes_letter_input(letter_input_state_for_console(&state)),
        ),
        "Char(<redacted>)"
    );
    assert_eq!(
        key_debug_name_for_input(
            key(crossterm::event::KeyCode::Enter),
            consumes_letter_input(letter_input_state_for_console(&state)),
        ),
        "Enter"
    );
}

#[test]
fn debug_location_includes_stage_and_modal_without_values() {
    let mut state = fresh_state();
    let ConsoleStage::Manager(ms) = &mut state.stage;
    let mut editor = EditorState::new_create();
    editor.modal = Some(Modal::TextInput {
        target: TextInputTarget::EnvValue {
            scope: SecretsScopeTag::Workspace,
            key: "TOKEN".into(),
        },
        state: TextInputState::new("Value", ""),
    });
    ms.stage = ManagerStage::Editor(editor);

    let location = console_location_debug(&state);
    assert!(location.contains("editor"), "{location}");
    assert!(location.contains("modal=TextInput"), "{location}");
    assert!(!location.contains("TOKEN"), "{location}");
}

#[test]
fn quit_confirm_handle_key_y_commits_exit() {
    let mut state = ConfirmState::new("Exit jackin❯?");
    assert!(matches!(
        state.handle_key(key(crossterm::event::KeyCode::Char('y')).into()),
        ModalOutcome::Commit(true)
    ));
}

#[test]
fn quit_confirm_handle_key_n_returns_commit_false() {
    let mut state = ConfirmState::new("Exit jackin❯?");
    assert!(matches!(
        state.handle_key(key(crossterm::event::KeyCode::Char('n')).into()),
        ModalOutcome::Commit(false)
    ));
}

#[test]
fn quit_confirm_handle_key_esc_cancels() {
    let mut state = ConfirmState::new("Exit jackin❯?");
    assert!(matches!(
        state.handle_key(key(crossterm::event::KeyCode::Esc).into()),
        ModalOutcome::Cancel
    ));
}

#[test]
fn show_role_resolution_error_opens_error_popup_with_role_and_error() {
    let mut state = fresh_state();
    let selector = RoleSelector::new(Some("acme"), "agent-smith");
    let error = anyhow::anyhow!("network is unreachable");

    show_role_resolution_error(&mut state, &selector, &error);

    let ConsoleStage::Manager(ms) = &mut state.stage;
    let Some(Modal::ErrorPopup { state: popup }) = ms.list_modal.as_ref() else {
        panic!("expected ErrorPopup, got {:?}", ms.list_modal);
    };
    let body = format!("{popup:?}");
    assert!(
        body.contains("acme/agent-smith"),
        "popup must reference the failing role selector: {body}"
    );
    assert!(
        body.contains("network is unreachable"),
        "popup must surface the underlying error: {body}"
    );
}

#[test]
fn prompt_agent_for_launch_skips_resolution_when_workspace_default_agent_set() {
    let cwd = std::env::temp_dir();
    let config = AppConfig::default();
    let mut state = new_console_state(&config, &cwd).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut workspace = unresolved_workspace();
    workspace.default_agent = Some(Agent::Codex);

    let outcome = prompt_agent_for_launch(
        &mut state,
        &selector,
        &workspace,
        LoadWorkspaceInput::CurrentDir,
        OnPromptFailure::ClearPending,
        AgentPickerChoices::Failed(anyhow::anyhow!("must not be observed")),
    );

    assert!(matches!(outcome, PromptOutcome::Launch));
    let ConsoleStage::Manager(ms) = &state.stage;
    assert!(
        ms.list_modal.is_none(),
        "no modal must be opened on the default-agent short-circuit"
    );
    assert!(
        ms.status_overlay.is_none(),
        "no status overlay must be left behind on the default-agent short-circuit"
    );
}

#[test]
fn prompt_agent_for_launch_restore_pending_keeps_input_for_retry() {
    let (state, outcome) = run_prompt_for_unknown_role(OnPromptFailure::RestorePending);
    assert!(matches!(outcome, PromptOutcome::Defer));
    assert!(
        state.pending_launch.is_some(),
        "RestorePending must hold the input so the operator can retry after dismissing the error"
    );
    let ConsoleStage::Manager(ms) = &state.stage;
    assert!(
        matches!(ms.list_modal, Some(Modal::ErrorPopup { .. })),
        "Failed outcome must surface the error popup regardless of restore policy"
    );
}

#[test]
fn prompt_agent_for_launch_clear_pending_drops_input() {
    let (state, outcome) = run_prompt_for_unknown_role(OnPromptFailure::ClearPending);
    assert!(matches!(outcome, PromptOutcome::Defer));
    assert!(
        state.pending_launch.is_none(),
        "ClearPending must drop the input so a fresh workspace pick re-resolves cleanly"
    );
    let ConsoleStage::Manager(ms) = &state.stage;
    assert!(
        matches!(ms.list_modal, Some(Modal::ErrorPopup { .. })),
        "Failed outcome must surface the error popup regardless of restore policy"
    );
}

#[test]
fn committed_role_prompt_restores_pending_plan_on_resolve_error() {
    // Resolution failures surface as an in-TUI popup with the console
    // staying alive, so the taken plan must be restored — otherwise the
    // still-visible role picker silently stops working.
    let (temp, config) = unresolvable_mount_config();
    let mut state = new_console_state(&AppConfig::default(), temp.path()).unwrap();
    store_pending_launch_plan(&mut state, LoadWorkspaceInput::Saved("ws".into()));
    let role = RoleSelector::new(None, "agent-smith");

    let error = committed_role_prompt(&mut state, &config, temp.path(), role).unwrap_err();

    assert!(error.to_string().contains("mount source does not exist"));
    assert!(
        take_pending_launch_plan(&mut state).is_some(),
        "pending launch plan must be restored after a resolve error"
    );
}

#[test]
fn launch_with_committed_agent_restores_pending_plans_on_resolve_error() {
    let (temp, config) = unresolvable_mount_config();
    let mut state = new_console_state(&AppConfig::default(), temp.path()).unwrap();
    store_pending_launch_plan(&mut state, LoadWorkspaceInput::Saved("ws".into()));
    let role = RoleSelector::new(None, "agent-smith");
    state.pending_launch_role = Some(role);

    let error =
        launch_with_committed_agent(&mut state, &config, temp.path(), Agent::Claude).unwrap_err();

    assert!(error.to_string().contains("mount source does not exist"));
    assert!(
        crate::tui::model::take_pending_launch_and_role_plan(&mut state).is_some(),
        "pending launch and role plans must be restored after a resolve error"
    );
}
