// `ConsoleStage` collapsed to a single variant in PR #171's Modal::RolePicker
// cleanup. The module is kept as-is (with `if let ConsoleStage::Manager(_)`
// patterns) so a future stage can be added without rewriting every match
// site. The irrefutable-pattern lint is allowed at the module level rather
// than peppering individual sites.
#![allow(irrefutable_let_patterns)]

pub mod manager;
pub mod op_cache;
mod preview;
mod prompts;
pub mod run;
pub mod state;
pub mod terminal;
pub mod widgets;

pub use op_cache::OpCache;
#[cfg(test)]
use prompts::{prompt_agent_for_launch, providers_for_launch};
pub(super) use run::consumes_letter_input;
#[cfg(test)]
use run::is_on_main_screen;
pub use run::run_console;
pub use state::ConsoleStage;
pub use state::ConsoleState;
pub use state::WorkspaceChoice;
pub use state::build_workspace_choice;
pub use terminal::TerminalSession;

use crate::app::context::preferred_agent_index;
use crate::config::AppConfig;
use crate::selector::RoleSelector;
use crate::workspace::{LoadWorkspaceInput, ResolvedWorkspace};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsoleOutcome {
    Launch(RoleSelector, ResolvedWorkspace, Option<crate::agent::Agent>),
    InstanceAction {
        container: String,
        action: ConsoleInstanceAction,
    },
    /// Operator selected an agent AND a provider in the console picker.
    /// The chosen `Provider` drives the env redirection (e.g. Z.AI's
    /// Anthropic-compatible endpoint) and the tab-name suffix.
    NewSessionWithProvider {
        container: String,
        agent: crate::agent::Agent,
        provider: jackin_protocol::Provider,
    },
    /// Initial launch with a provider selected in the console before the
    /// container is created. The provider flows into the capsule's initial
    /// attach so the first session uses the chosen provider.
    LaunchWithProvider {
        selector: RoleSelector,
        workspace: ResolvedWorkspace,
        agent: crate::agent::Agent,
        provider: jackin_protocol::Provider,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleInstanceAction {
    Reconnect,
    /// Reconnect and ask the in-container daemon to focus this
    /// pane (`session_id`) before forwarding output. Carries through
    /// to `attach::reconnect_or_create_session_with_focus` which
    /// appends the `--focus <id>` flag on the `docker exec`.
    ReconnectFocus(u64),
    NewSession,
    NewSessionWithAgent(crate::agent::Agent),
    Shell,
    Inspect,
    Stop,
    Purge,
}

impl ConsoleInstanceAction {
    /// Actions that don't replace the TUI with another foreground process
    /// (Stop/Purge) run inside the console event loop via
    /// `InstanceActionHandler`. The rest tear down the TUI so the launched
    /// container/agent can own the terminal.
    pub const fn runs_in_place(self) -> bool {
        matches!(self, Self::Stop | Self::Purge)
    }
}

/// Callback invoked for `runs_in_place` actions.
///
/// The handler performs the docker work (eject, purge). Making it async lets
/// the caller `.await` the work on the existing runtime without building a
/// separate runtime, so the reactor can service other tasks between awaits
/// while Docker/git calls are in flight.
pub trait InstanceActionHandler {
    async fn run_in_place(
        &mut self,
        container: &str,
        action: ConsoleInstanceAction,
    ) -> anyhow::Result<()>;
}

impl ConsoleState {
    /// Open the inline role picker for every eligible role count except zero.
    /// `WorkspaceChoice` is built fresh each call so manager edits take effect
    /// immediately.
    pub fn dispatch_launch_for_workspace(
        &mut self,
        config: &AppConfig,
        cwd: &std::path::Path,
        input: LoadWorkspaceInput,
    ) -> anyhow::Result<Option<(RoleSelector, ResolvedWorkspace, Option<crate::agent::Agent>)>>
    {
        let Some(choice) = build_workspace_choice(config, cwd, &input)? else {
            // Workspace was deleted between keypress and dispatch.
            return Ok(None);
        };
        let roles = choice.allowed_roles.clone();

        if roles.is_empty() {
            // Stay so the operator can fix `allowed_roles`
            // — a single Enter shouldn't terminate the TUI.
            let name = choice.name;
            if let ConsoleStage::Manager(ms) = &mut self.stage {
                let _ = manager::update_manager(
                    ms,
                    manager::ManagerMessage::OpenListErrorPopup {
                        title: "No eligible roles".into(),
                        message: format!(
                            "Workspace \"{name}\" has no allowed roles configured.\n\nAdd at least one role to `allowed_roles` in the workspace settings."
                        ),
                    },
                );
            }
            self.pending_launch = None;
            self.pending_launch_role = None;
        } else if roles.len() == 1 {
            // Single role — skip picker and proceed directly to agent selection.
            let role = roles.into_iter().next().unwrap();
            return preview::resolve_selected_workspace(config, cwd, &choice, &role)
                .map(|workspace| Some((role, workspace, None)));
        } else {
            let selected = preferred_agent_index(
                &roles,
                choice.last_role.as_deref(),
                choice.default_role.as_deref(),
            );
            self.pending_launch = Some(input);
            self.pending_launch_role = None;
            if let ConsoleStage::Manager(ms) = &mut self.stage {
                let mut picker =
                    crate::console::widgets::role_picker::RolePickerState::with_confirm_label(
                        roles, "launch",
                    );
                if let Some(selected) = selected {
                    picker.list_state.select(Some(selected));
                }
                ms.inline_role_picker = Some(picker);
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod quit_confirm_tests {
    //! Pin the gates for the Q-intercept and the
    //! `ConfirmState::handle_key` outcomes the run-loop dispatches.
    use super::prompts::{
        OnPromptFailure, PromptOutcome, console_location_debug, key_debug_name,
        show_role_resolution_error,
    };
    use super::*;
    use crate::console::manager::state::{
        EditorState, FileBrowserTarget, ManagerStage, Modal, SecretsScopeTag, TextInputTarget,
    };
    use crate::console::widgets::{ModalOutcome, file_browser::FileBrowserState};
    use crate::paths::JackinPaths;
    use jackin_tui::components::{ConfirmState, TextInputState};

    fn fresh_state() -> ConsoleState {
        let cwd = std::env::temp_dir();
        let config = AppConfig::default();
        ConsoleState::new(&config, &cwd).unwrap()
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
        assert!(!consumes_letter_input(&state));
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
        // FileBrowser stands in for any list-anchored modal — predicate
        // only checks `is_some`.
        ms.list_modal = Some(Modal::FileBrowser {
            target: FileBrowserTarget::CreateFirstMountSrc,
            state: FileBrowserState::new_from_home().unwrap(),
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
        assert!(consumes_letter_input(&state));
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
            key_debug_name(&state, key(crossterm::event::KeyCode::Char('s'))),
            "Char(<redacted>)"
        );
        assert_eq!(
            key_debug_name(&state, key(crossterm::event::KeyCode::Enter)),
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
        let mut s = ConfirmState::new("Exit jackin'?");
        assert!(matches!(
            s.handle_key(key(crossterm::event::KeyCode::Char('y'))),
            ModalOutcome::Commit(true)
        ));
    }

    #[test]
    fn quit_confirm_handle_key_n_returns_commit_false() {
        let mut s = ConfirmState::new("Exit jackin'?");
        assert!(matches!(
            s.handle_key(key(crossterm::event::KeyCode::Char('n'))),
            ModalOutcome::Commit(false)
        ));
    }

    #[test]
    fn quit_confirm_handle_key_esc_cancels() {
        let mut s = ConfirmState::new("Exit jackin'?");
        assert!(matches!(
            s.handle_key(key(crossterm::event::KeyCode::Esc)),
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
    fn providers_for_launch_include_all_zai_env_layers() {
        let mut config = AppConfig::default();
        config.env.insert(
            "ZAI_API_KEY".into(),
            crate::operator_env::EnvValue::Plain("global-key".into()),
        );
        config.workspaces.insert(
            "global-demo".into(),
            crate::workspace::WorkspaceConfig::default(),
        );
        assert_eq!(
            super::providers_for_launch(
                &config,
                "global-demo",
                "the-architect",
                crate::agent::Agent::Claude,
            )
            .len(),
            2
        );
        config.env.clear();

        let mut workspace = crate::workspace::WorkspaceConfig::default();
        workspace.env.insert(
            "ZAI_API_KEY".into(),
            crate::operator_env::EnvValue::Plain("workspace-key".into()),
        );
        config.workspaces.insert("workspace-demo".into(), workspace);
        assert_eq!(
            super::providers_for_launch(
                &config,
                "workspace-demo",
                "the-architect",
                crate::agent::Agent::Claude,
            )
            .len(),
            2
        );

        config.workspaces.remove("workspace-demo");
        let mut role = crate::config::RoleSource::default();
        role.env.insert(
            "ZAI_API_KEY".into(),
            crate::operator_env::EnvValue::Plain("role-key".into()),
        );
        config.roles.insert("the-architect".into(), role);
        config.workspaces.insert(
            "role-demo".into(),
            crate::workspace::WorkspaceConfig::default(),
        );
        assert_eq!(
            super::providers_for_launch(
                &config,
                "role-demo",
                "the-architect",
                crate::agent::Agent::Claude,
            )
            .len(),
            2
        );

        config.roles.clear();
        let mut workspace_role = crate::workspace::WorkspaceConfig::default();
        let mut role_override = crate::workspace::WorkspaceRoleOverride::default();
        role_override.env.insert(
            "ZAI_API_KEY".into(),
            crate::operator_env::EnvValue::Plain("workspace-role-key".into()),
        );
        workspace_role
            .roles
            .insert("the-architect".into(), role_override);
        config
            .workspaces
            .insert("workspace-role-demo".into(), workspace_role);
        let providers = super::providers_for_launch(
            &config,
            "workspace-role-demo",
            "the-architect",
            crate::agent::Agent::Claude,
        );
        assert_eq!(providers.len(), 2);
        assert_eq!(providers[1], jackin_protocol::Provider::Zai);
    }

    #[test]
    fn providers_for_launch_rejects_non_claude_agents() {
        let mut config = AppConfig::default();
        config.env.insert(
            "ZAI_API_KEY".into(),
            crate::operator_env::EnvValue::Plain("global-key".into()),
        );
        config
            .workspaces
            .insert("demo".into(), crate::workspace::WorkspaceConfig::default());

        let providers = super::providers_for_launch(
            &config,
            "demo",
            "the-architect",
            crate::agent::Agent::Codex,
        );

        assert!(providers.is_empty());
    }

    fn unresolved_workspace() -> ResolvedWorkspace {
        ResolvedWorkspace {
            label: "scratch".to_string(),
            workdir: "/workspace".to_string(),
            mounts: Vec::new(),
            default_agent: None,
            keep_awake_enabled: false,
            git_pull_on_entry: false,
        }
    }

    async fn run_prompt_for_unknown_role(
        on_failure: OnPromptFailure,
    ) -> (ConsoleState, PromptOutcome) {
        use ratatui::backend::TestBackend;
        let cwd = std::env::temp_dir();
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        // Empty config → resolve_supported_agents_for_console errors on
        // the unregistered selector; helper routes that into Failed.
        let config = AppConfig::default();
        let mut state = ConsoleState::new(&config, &cwd).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let workspace = unresolved_workspace();
        let mut runner = crate::runtime::FakeRunner::default();
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let input = LoadWorkspaceInput::CurrentDir;
        let outcome = super::prompt_agent_for_launch(
            &mut terminal,
            &mut state,
            &paths,
            &config,
            &cwd,
            &mut runner,
            &selector,
            &workspace,
            input,
            on_failure,
        )
        .await
        .unwrap();
        (state, outcome)
    }

    #[tokio::test]
    async fn prompt_agent_for_launch_skips_resolution_when_workspace_default_agent_set() {
        // workspace.default_agent.is_some() must short-circuit before
        // any git work — operators with a configured default never
        // wait on a network round trip just to confirm a launch.
        use ratatui::backend::TestBackend;
        let cwd = std::env::temp_dir();
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        let config = AppConfig::default();
        let mut state = ConsoleState::new(&config, &cwd).unwrap();
        let selector = RoleSelector::new(None, "agent-smith");
        let mut workspace = unresolved_workspace();
        workspace.default_agent = Some(crate::agent::Agent::Codex);
        let mut runner = crate::runtime::FakeRunner::default();
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        let outcome = super::prompt_agent_for_launch(
            &mut terminal,
            &mut state,
            &paths,
            &config,
            &cwd,
            &mut runner,
            &selector,
            &workspace,
            LoadWorkspaceInput::CurrentDir,
            OnPromptFailure::ClearPending,
        )
        .await
        .unwrap();

        assert!(matches!(outcome, PromptOutcome::Launch));
        assert!(
            runner.recorded.is_empty(),
            "workspace default_agent must short-circuit before any git work: {:?}",
            runner.recorded
        );
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

    #[tokio::test]
    async fn prompt_agent_for_launch_restore_pending_keeps_input_for_retry() {
        let (state, outcome) = run_prompt_for_unknown_role(OnPromptFailure::RestorePending).await;
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

    #[tokio::test]
    async fn prompt_agent_for_launch_clear_pending_drops_input() {
        let (state, outcome) = run_prompt_for_unknown_role(OnPromptFailure::ClearPending).await;
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

#[cfg(test)]
mod op_cache_invalidation_tests {
    use crate::console::op_cache::OpCache;
    use crate::console::prompts::invalidate_op_cache_for_ref;
    use crate::operator_env::{OpField, OpItem, OpRef};
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

        invalidate_op_cache_for_ref(
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
        // A non-op:// literal must be a no-op, not a panic.
        invalidate_op_cache_for_ref(
            &cache,
            &OpRef {
                op: "not-a-ref".into(),
                path: String::new(),
                account: None,
            },
        );
    }
}
