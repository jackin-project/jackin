// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) type ManagerEffect = crate::tui::effect::ConsoleManagerEffect<
    jackin_core::RoleSelector,
    jackin_config::RoleSource,
    jackin_core::OpRef,
>;

pub(super) fn handle_key(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    use crate::tui::effect::ConsoleEffect;
    use crate::tui::model::{
        ConsoleInputDispatchFacts, ConsoleInputDispatchPlan, ConsoleManagerStageRoute,
        console_input_dispatch_plan,
    };
    use crate::tui::screens::workspaces::update::{InstancePurgeKeyPlan, instance_purge_key_plan};
    use crate::tui::state::update::{ManagerMessage, update_manager};

    let stage_modal_facts = state.stage.modal_facts();
    let dispatch_plan = console_input_dispatch_plan(ConsoleInputDispatchFacts {
        keyboard_help_open: state.keyboard_help.is_some(),
        list_modal_open: state.list_modal.is_some(),
        inline_new_session_picker_open: state.inline_new_session_picker.is_some(),
        inline_account_picker_open: state.inline_account_picker.is_some(),
        launch_account_picker_open: state.launch_account_picker.is_some(),
        inline_agent_picker_open: state.inline_agent_picker.is_some(),
        inline_role_picker_open: state.inline_role_picker.is_some(),
        editor_modal_open: stage_modal_facts.editor_modal_open,
        settings_error_popup_open: stage_modal_facts.settings_error_popup_open,
        settings_mounts_modal_open: stage_modal_facts.settings_mounts_modal_open,
        settings_env_modal_open: stage_modal_facts.settings_env_modal_open,
        settings_auth_modal_open: stage_modal_facts.settings_auth_modal_open,
        create_prelude_modal_open: stage_modal_facts.create_prelude_modal_open,
        stage_route: state.stage.route(),
    });
    match dispatch_plan {
        ConsoleInputDispatchPlan::ListModal => return Ok(handle_list_modal(state, key)),
        ConsoleInputDispatchPlan::InlineNewSessionPicker => {
            return Ok(handle_new_session_picker(state, key));
        }
        ConsoleInputDispatchPlan::InlineAccountPicker => {
            return Ok(handle_inline_account_picker(state, key));
        }
        ConsoleInputDispatchPlan::LaunchAccountPicker => {
            return Ok(handle_launch_account_picker(state, key));
        }
        ConsoleInputDispatchPlan::InlineAgentPicker => {
            return Ok(handle_inline_agent_picker(state, key));
        }
        ConsoleInputDispatchPlan::InlineRolePicker => {
            return Ok(handle_inline_role_picker(state, key));
        }
        ConsoleInputDispatchPlan::Stage(ConsoleManagerStageRoute::List) => {
            let outcome = handle_list_key(state, config, paths, cwd, key)?;
            state.request_effect(ConsoleEffect::RequestActiveMountInfoRefresh.into());
            return Ok(outcome);
        }
        ConsoleInputDispatchPlan::Stage(ConsoleManagerStageRoute::ConfirmInstancePurge) => {
            let ManagerStage::ConfirmInstancePurge {
                container,
                state: confirm_state,
                ..
            } = &mut state.stage
            else {
                return Ok(InputOutcome::Continue);
            };
            let plan =
                instance_purge_key_plan(confirm_state.handle_key(key.into()), container.clone());
            match plan {
                InstancePurgeKeyPlan::Purge { container } => {
                    update_manager(state, ManagerMessage::ReturnToList);
                    return Ok(InputOutcome::InstanceAction {
                        container,
                        action: ConsoleInstanceAction::Purge,
                    });
                }
                InstancePurgeKeyPlan::ReturnToList => {
                    update_manager(state, ManagerMessage::ReturnToList);
                    return Ok(InputOutcome::Continue);
                }
                InstancePurgeKeyPlan::Continue => return Ok(InputOutcome::Continue),
            }
        }
        _ => {}
    }
    Ok(InputOutcome::Continue)
}

pub(super) fn make_github_repo(
    root: &std::path::Path,
    name: &str,
    branch: &str,
) -> std::path::PathBuf {
    let path = root.join(name);
    let git_dir = path.join(".git");
    std::fs::create_dir_all(&git_dir).unwrap();
    std::fs::write(git_dir.join("HEAD"), format!("ref: refs/heads/{branch}\n")).unwrap();
    std::fs::write(
        git_dir.join("config"),
        format!("[remote \"origin\"]\n    url = git@github.com:owner/{name}.git\n"),
    )
    .unwrap();
    path
}

pub(super) fn list_state_selecting_ws(
    ws: WorkspaceConfig,
) -> (ManagerState<'static>, AppConfig, JackinPaths, TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    config.workspaces.insert("demo".into(), ws);
    let mut state = ManagerState::from_config(&config, tmp.path());
    state.selected = 1; // force selection onto the saved workspace row
    state
        .mount_info_cache
        .refresh_mounts(&config.workspaces["demo"].mounts);
    (state, config, paths, tmp)
}

pub(super) fn instance_entry(
    container: &str,
    status: InstanceStatus,
    workdir: &str,
) -> InstanceIndexEntry {
    InstanceIndexEntry {
        instance_id: format!("{container}-id"),
        container_base: container.into(),
        workspace_name: Some("demo".into()),
        workspace_label: "demo".into(),
        workdir: workdir.into(),
        role_key: "the-architect".into(),
        agent_runtime: "codex".into(),
        status,
        updated_at: "2026-05-11T00:00:00Z".into(),
    }
}

pub(super) fn current_dir_instance_entry(
    container: &str,
    status: InstanceStatus,
    workdir: &str,
) -> InstanceIndexEntry {
    InstanceIndexEntry {
        instance_id: format!("{container}-id"),
        container_base: container.into(),
        workspace_name: None,
        workspace_label: workdir.into(),
        workdir: workdir.into(),
        role_key: "the-architect".into(),
        agent_runtime: "codex".into(),
        status,
        updated_at: "2026-05-11T00:00:00Z".into(),
    }
}

pub(super) fn provider_choices() -> Vec<jackin_protocol::Provider> {
    vec![
        jackin_protocol::Provider::Anthropic,
        jackin_protocol::Provider::Zai,
    ]
}

pub(super) fn codex_provider_choices() -> Vec<jackin_protocol::Provider> {
    vec![
        jackin_protocol::Provider::Openai,
        jackin_protocol::Provider::Minimax,
    ]
}

pub(super) fn commit_new_session_picker(
    state: &mut ManagerState<'_>,
    agent: jackin_core::Agent,
    providers: Vec<jackin_protocol::Provider>,
) -> InputOutcome {
    let mut picker = AgentChoiceState::with_choices(vec![agent]);
    picker.focused = agent;
    let accounts = providers
        .into_iter()
        .map(|provider| {
            let provider = match provider {
                jackin_protocol::Provider::Anthropic => jackin_config::AiProvider::Anthropic,
                jackin_protocol::Provider::Openai => jackin_config::AiProvider::OpenAi,
                jackin_protocol::Provider::Zai => jackin_config::AiProvider::Zai,
                jackin_protocol::Provider::Minimax => jackin_config::AiProvider::Minimax,
                jackin_protocol::Provider::Kimi => panic!("unexpected test provider"),
            };
            crate::services::launch::AccountChoice {
                id: provider.slug().to_owned(),
                name: provider.slug().to_owned(),
                provider,
                agents: vec![agent],
                configuration_id: None,
                instance_id: Some(provider.slug().to_owned()),
            }
        })
        .collect();
    state.inline_new_session_picker = Some(("jackin-demo-architect".into(), picker, accounts));
    handle_new_session_picker(state, key(KeyCode::Enter))
}

pub(super) fn api_key_account(
    name: &str,
    provider: jackin_config::AiProvider,
) -> jackin_config::AccountConfig {
    jackin_config::AccountConfig {
        enabled: true,
        name: name.into(),
        provider,
        credential: jackin_config::AccountCredential::ApiKey {
            value: jackin_core::EnvValue::Plain("test-key".into()),
            base_url: None,
            model: None,
        },
    }
}

pub(super) fn running_session_state(
    configure: impl FnOnce(&mut AppConfig),
) -> (ManagerState<'static>, AppConfig, JackinPaths, TempDir) {
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        accounts: vec!["a-claude".into(), "z-claude".into(), "o-codex".into()],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    config.accounts.insert(
        "a-claude".into(),
        api_key_account("A", jackin_config::AiProvider::Anthropic),
    );
    config.accounts.insert(
        "z-claude".into(),
        api_key_account("Z", jackin_config::AiProvider::Anthropic),
    );
    config.accounts.insert(
        "o-codex".into(),
        api_key_account("O", jackin_config::AiProvider::OpenAi),
    );
    config.accounts.insert(
        "outside".into(),
        api_key_account("Outside", jackin_config::AiProvider::Anthropic),
    );
    configure(&mut config);
    state.instances = vec![instance_entry(
        "jackin-demo-architect-running",
        InstanceStatus::Running,
        workdir,
    )];
    state.live_instance_admissions.insert(
        "jackin-demo-architect-running".into(),
        vec![
            crate::services::launch::LiveInstanceAdmission {
                instance_id: "claude-a".into(),
                agent: jackin_core::Agent::Claude,
                account_id: "a-claude".into(),
            },
            crate::services::launch::LiveInstanceAdmission {
                instance_id: "claude-z".into(),
                agent: jackin_core::Agent::Claude,
                account_id: "z-claude".into(),
            },
            crate::services::launch::LiveInstanceAdmission {
                instance_id: "codex-o".into(),
                agent: jackin_core::Agent::Codex,
                account_id: "o-codex".into(),
            },
        ],
    );
    state.expand_workspace(0);
    state.selected = state
        .index_of_row(crate::tui::state::ManagerListRow::WorkspaceInstance(0, 0))
        .expect("expanded workspace instance row exists");
    (state, config, paths, tmp)
}

pub(super) fn open_and_commit_new_session(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    agent: jackin_core::Agent,
) -> InputOutcome {
    let outcome = handle_key(state, config, paths, cwd, key(KeyCode::Char('n'))).unwrap();
    assert!(
        matches!(outcome, InputOutcome::Continue),
        "n must open the agent picker; got {outcome:?}"
    );
    let Some((_, picker, _)) = state.inline_new_session_picker.as_mut() else {
        panic!("n on a running instance must open the agent picker");
    };
    picker.focused = agent;
    handle_new_session_picker(state, key(KeyCode::Enter))
}

pub(super) fn assert_no_eligible_account_popup(state: &ManagerState<'_>, scope_needle: &str) {
    let Some(Modal::ErrorPopup { state: popup }) = &state.list_modal else {
        panic!(
            "expected no-eligible-account ErrorPopup; got {:?}",
            state.list_modal
        );
    };
    assert_eq!(popup.title, "No eligible account");
    assert!(
        popup.message.contains("claude"),
        "popup must name the agent; got {:?}",
        popup.message
    );
    assert!(
        popup.message.contains(scope_needle),
        "popup must name the scope; got {:?}",
        popup.message
    );
    assert!(
        popup.message.contains("binding"),
        "popup must point at the binding remedy; got {:?}",
        popup.message
    );
}

pub(super) fn live_snapshot() -> jackin_protocol::InstanceSnapshot {
    jackin_protocol::InstanceSnapshot {
        tabs: vec![jackin_protocol::control::TabSnapshot {
            label: "Codex".into(),
            instance: Some("codex-main".into()),
            account_id: Some("acc-1".into()),
            focused_pane: 1,
            panes: vec![jackin_protocol::control::PaneSnapshot {
                session_id: 1,
                label: "Codex".into(),
                agent: Some("codex".into()),
                account_id: Some("acc-1".into()),
                state: jackin_protocol::control::AgentState::Idle,
                agent_status_report: None,
            }],
        }],
        active_tab: 0,
    }
}
