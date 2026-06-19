mod quit_confirm {
    use super::super::tui::debug::console_location_debug;
    use super::super::tui::prompts::{
        AgentPickerChoices, OnPromptFailure, PromptOutcome, prompt_agent_for_launch,
        show_role_resolution_error,
    };
    use super::super::tui::{is_on_main_screen, letter_input_state};
    use super::super::{ConsoleStage, ConsoleState, tui};
    use crate::console::tui::state::{
        EditorState, FileBrowserTarget, ManagerStage, Modal, SecretsScopeTag, TextInputTarget,
    };
    use jackin_config::{AppConfig, LoadWorkspaceInput, ResolvedWorkspace};
    use jackin_console::tui::components::file_browser::FileBrowserState;
    use jackin_console::tui::run::consumes_letter_input;
    use jackin_core::{Agent, RoleSelector};
    use jackin_tui::ModalOutcome;
    use jackin_tui::components::{ConfirmState, TextInputState};

    fn fresh_state() -> ConsoleState {
        let cwd = std::env::temp_dir();
        let config = AppConfig::default();
        tui::new_console_state(&config, &cwd).unwrap()
    }

    fn key(code: crossterm::event::KeyCode) -> crossterm::event::KeyEvent {
        crossterm::event::KeyEvent {
            code,
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    #[test]
    fn main_screen_is_list_with_no_modal() {
        let state = fresh_state();
        assert!(is_on_main_screen(&state));
        assert!(!consumes_letter_input(letter_input_state(&state)));
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
            state: FileBrowserState::from_listing(
                jackin_console::services::file_browser::listing_from_home().unwrap(),
            ),
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
        assert!(consumes_letter_input(letter_input_state(&state)));
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
            jackin_console::tui::debug::key_debug_name_for_input(
                key(crossterm::event::KeyCode::Char('s')),
                consumes_letter_input(letter_input_state(&state)),
            ),
            "Char(<redacted>)"
        );
        assert_eq!(
            jackin_console::tui::debug::key_debug_name_for_input(
                key(crossterm::event::KeyCode::Enter),
                consumes_letter_input(letter_input_state(&state)),
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
        let mut state = ConfirmState::new("Exit jackin'?");
        assert!(matches!(
            state.handle_key(key(crossterm::event::KeyCode::Char('y'))),
            ModalOutcome::Commit(true)
        ));
    }

    #[test]
    fn quit_confirm_handle_key_n_returns_commit_false() {
        let mut state = ConfirmState::new("Exit jackin'?");
        assert!(matches!(
            state.handle_key(key(crossterm::event::KeyCode::Char('n'))),
            ModalOutcome::Commit(false)
        ));
    }

    #[test]
    fn quit_confirm_handle_key_esc_cancels() {
        let mut state = ConfirmState::new("Exit jackin'?");
        assert!(matches!(
            state.handle_key(key(crossterm::event::KeyCode::Esc)),
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

    fn unresolved_workspace() -> ResolvedWorkspace {
        ResolvedWorkspace {
            label: "scratch".to_owned(),
            workdir: "/workspace".to_owned(),
            mounts: Vec::new(),
            default_agent: None,
            keep_awake_enabled: false,
            git_pull_on_entry: false,
        }
    }

    fn run_prompt_for_unknown_role(on_failure: OnPromptFailure) -> (ConsoleState, PromptOutcome) {
        let cwd = std::env::temp_dir();
        let config = AppConfig::default();
        let mut state = tui::new_console_state(&config, &cwd).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let workspace = unresolved_workspace();
        let input = LoadWorkspaceInput::CurrentDir;
        let outcome = prompt_agent_for_launch(
            &mut state,
            &selector,
            &workspace,
            input,
            on_failure,
            AgentPickerChoices::Failed(anyhow::anyhow!("unknown role")),
        );
        (state, outcome)
    }

    #[test]
    fn prompt_agent_for_launch_skips_resolution_when_workspace_default_agent_set() {
        let cwd = std::env::temp_dir();
        let config = AppConfig::default();
        let mut state = tui::new_console_state(&config, &cwd).unwrap();
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
}

mod op_cache_invalidation {
    use crate::console::services::op_picker::invalidate_cache_for_ref;
    use jackin_core::OpRef;
    use jackin_env::OpCache;
    use jackin_env::{OpField, OpItem};
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn invalidate_op_cache_for_ref_drops_items_and_fields() {
        let cache = Rc::new(RefCell::new(OpCache::default()));
        let account = Some("ACCT");
        cache.borrow_mut().put_items(
            account,
            "v1",
            vec![OpItem {
                id: "i1".into(),
                name: "Claude".into(),
                subtitle: String::new(),
            }],
        );
        cache.borrow_mut().put_fields(
            account,
            "v1",
            "i1",
            vec![OpField {
                id: "f1".into(),
                label: "token".into(),
                field_type: "CONCEALED".into(),
                concealed: true,
                reference: String::new(),
            }],
        );

        invalidate_cache_for_ref(
            &cache,
            &OpRef {
                op: "op://v1/i1/f1".into(),
                path: "Work/Claude/token".into(),
                account: Some("ACCT".into()),
            },
        );

        assert!(cache.borrow().get_items(account, "v1").is_none());
        assert!(cache.borrow().get_fields(account, "v1", "i1").is_none());
    }

    #[test]
    fn invalidate_op_cache_for_ref_ignores_unparseable_ref() {
        let cache = Rc::new(RefCell::new(OpCache::default()));
        invalidate_cache_for_ref(
            &cache,
            &OpRef {
                op: "not-a-ref".into(),
                path: String::new(),
                account: None,
            },
        );
    }
}
