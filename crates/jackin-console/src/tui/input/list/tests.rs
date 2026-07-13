// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `list`.
//! List-stage tests: row-0 (current dir) gating, Enter routing,
//! `o`-key resolver to GitHub URLs, and the `GithubPicker` modal.
use super::super::InputOutcome;
use super::*;
use crate::tui::input::test_support::{key, mount};
use crate::tui::message::ConsoleInstanceAction;
use crate::tui::state::AgentChoiceState;
use crate::tui::state::{ManagerStage, ManagerState, Modal, MountScrollFocus};
use crossterm::event::{KeyCode, KeyEvent};
use jackin_config::AppConfig;
use jackin_config::WorkspaceConfig;
use jackin_core::JackinPaths;
use jackin_core::instance::{InstanceIndexEntry, InstanceStatus};
use ratatui::layout::Rect;
use tempfile::TempDir;

type ManagerEffect = crate::tui::effect::ConsoleManagerEffect<
    jackin_core::RoleSelector,
    jackin_config::RoleSource,
    jackin_core::OpRef,
>;

fn handle_key(
    state: &mut ManagerState<'_>,
    config: &mut jackin_config::AppConfig,
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
        list_modal_open: state.list_modal.is_some(),
        inline_new_session_picker_open: state.inline_new_session_picker.is_some(),
        inline_provider_picker_open: state.inline_provider_picker.is_some(),
        launch_provider_picker_open: state.launch_provider_picker.is_some(),
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
        ConsoleInputDispatchPlan::InlineProviderPicker => {
            return Ok(handle_inline_provider_picker(state, key));
        }
        ConsoleInputDispatchPlan::LaunchProviderPicker => {
            return Ok(handle_launch_provider_picker(state, key));
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
            let plan = instance_purge_key_plan(confirm_state.handle_key(key), container.clone());
            match plan {
                InstancePurgeKeyPlan::Purge { container } => {
                    drop(update_manager(state, ManagerMessage::ReturnToList));
                    return Ok(InputOutcome::InstanceAction {
                        container,
                        action: ConsoleInstanceAction::Purge,
                    });
                }
                InstancePurgeKeyPlan::ReturnToList => {
                    drop(update_manager(state, ManagerMessage::ReturnToList));
                    return Ok(InputOutcome::Continue);
                }
                InstancePurgeKeyPlan::Continue => return Ok(InputOutcome::Continue),
            }
        }
        _ => {}
    }
    Ok(InputOutcome::Continue)
}

/// Build a git repo under `root` with a `github.com` origin remote on
/// `branch`. Returns the path so callers can use it as a mount src.
fn make_github_repo(root: &std::path::Path, name: &str, branch: &str) -> std::path::PathBuf {
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

/// Helper: seed an `AppConfig` + `ManagerState` with `ws` as a saved workspace,
/// cwd far away so selection lands on row 1 (the saved workspace).
fn list_state_selecting_ws(
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

fn instance_entry(container: &str, status: InstanceStatus, workdir: &str) -> InstanceIndexEntry {
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

fn current_dir_instance_entry(
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

fn provider_choices() -> Vec<jackin_protocol::Provider> {
    vec![
        jackin_protocol::Provider::Anthropic,
        jackin_protocol::Provider::Zai,
    ]
}

fn codex_provider_choices() -> Vec<jackin_protocol::Provider> {
    vec![
        jackin_protocol::Provider::Openai,
        jackin_protocol::Provider::Minimax,
    ]
}

/// Open the new-session agent picker for `agent` with `providers` available,
/// then commit it with Enter and return the resulting outcome.
fn commit_new_session_picker(
    state: &mut ManagerState<'_>,
    agent: jackin_core::Agent,
    providers: Vec<jackin_protocol::Provider>,
) -> InputOutcome {
    let mut picker = AgentChoiceState::with_choices(vec![agent]);
    picker.focused = agent;
    state.inline_new_session_picker = Some(("jackin-demo-architect".into(), picker, providers));
    handle_new_session_picker(state, key(KeyCode::Enter))
}

#[test]
fn new_session_provider_picker_skips_when_no_choice() {
    // Single-provider Codex must dispatch directly, mirroring Claude.
    let config = AppConfig::default();
    let tmp = tempfile::tempdir().unwrap();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let outcome = commit_new_session_picker(
        &mut state,
        jackin_core::Agent::Codex,
        vec![jackin_protocol::Provider::Openai],
    );

    match outcome {
        InputOutcome::InstanceAction { container, action } => {
            assert_eq!(container, "jackin-demo-architect");
            assert_eq!(
                action,
                ConsoleInstanceAction::NewSessionWithAgent(jackin_core::Agent::Codex,)
            );
        }
        other => panic!("expected direct new-session dispatch; got {other:?}"),
    }
    assert!(
        state.inline_provider_picker.is_none(),
        "single-provider Codex must not open the provider picker"
    );
}

#[test]
fn new_session_provider_picker_opens_for_claude() {
    let config = AppConfig::default();
    let tmp = tempfile::tempdir().unwrap();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let outcome =
        commit_new_session_picker(&mut state, jackin_core::Agent::Claude, provider_choices());

    assert!(matches!(outcome, InputOutcome::Continue));
    let Some(picker) = state.inline_provider_picker else {
        panic!("Claude with providers must open provider picker");
    };
    assert_eq!(picker.context, "jackin-demo-architect");
    assert_eq!(picker.agent, jackin_core::Agent::Claude);
    assert_eq!(picker.providers().len(), 2);
    assert_eq!(picker.selected(), 0);
}

#[test]
fn new_session_provider_picker_opens_for_codex_with_multiple_providers() {
    // Codex with two providers configured opens the picker.
    let config = AppConfig::default();
    let tmp = tempfile::tempdir().unwrap();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let outcome = commit_new_session_picker(
        &mut state,
        jackin_core::Agent::Codex,
        codex_provider_choices(),
    );

    assert!(matches!(outcome, InputOutcome::Continue));
    let Some(picker) = state.inline_provider_picker else {
        panic!("Codex with multiple providers must open the provider picker");
    };
    assert_eq!(picker.context, "jackin-demo-architect");
    assert_eq!(picker.agent, jackin_core::Agent::Codex);
    assert_eq!(picker.providers().len(), 2);
    assert_eq!(picker.providers()[0], jackin_protocol::Provider::Openai);
    assert_eq!(picker.providers()[1], jackin_protocol::Provider::Minimax);
}

#[test]
fn new_session_picker_does_not_offer_host_config_providers_for_running_container() {
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    config.env.insert(
        "ZAI_API_KEY".into(),
        jackin_core::EnvValue::Plain("host-key-added-after-launch".into()),
    );
    state.instances = vec![instance_entry(
        "jackin-demo-architect-running",
        InstanceStatus::Running,
        workdir,
    )];
    state.expand_workspace(0);
    state.selected = state
        .index_of_row(crate::tui::state::ManagerListRow::WorkspaceInstance(0, 0))
        .expect("expanded workspace instance row exists");

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('n')),
    )
    .unwrap();

    assert!(matches!(outcome, InputOutcome::Continue));
    let Some((_container, _picker, providers)) = state.inline_new_session_picker.as_ref() else {
        panic!("N on a running instance must open the agent picker");
    };
    assert!(
        providers.is_empty(),
        "host config must not offer providers for an already-running container"
    );
}

fn live_snapshot() -> jackin_protocol::InstanceSnapshot {
    jackin_protocol::InstanceSnapshot {
        tabs: vec![jackin_protocol::control::TabSnapshot {
            label: "Codex".into(),
            focused_pane: 1,
            panes: vec![jackin_protocol::control::PaneSnapshot {
                session_id: 1,
                label: "Codex".into(),
                agent: Some("codex".into()),
                state: jackin_protocol::control::AgentState::Idle,
                agent_status_report: None,
            }],
        }],
        active_tab: 0,
    }
}

#[test]
fn right_on_current_directory_parent_expands_even_with_live_snapshot() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let cwd = tmp.path();
    let workdir = cwd.display().to_string();
    let container = "jackin-current-dir-the-architect-live";

    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);
    state.instances = vec![current_dir_instance_entry(
        container,
        InstanceStatus::Running,
        &workdir,
    )];
    state
        .instance_snapshots
        .insert(container.into(), live_snapshot());

    let outcome = handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right)).unwrap();

    assert!(matches!(outcome, InputOutcome::Continue));
    assert!(
        state.current_dir_expanded,
        "→ on the Current directory parent must expand the tree"
    );
    assert!(
        !state.preview_focused,
        "preview focus is only reachable from instance child rows"
    );
    assert!(matches!(
        state.row_at(1),
        Some(crate::tui::state::ManagerListRow::CurrentDirectoryInstance(
            0
        ))
    ));
}

#[test]
fn right_on_non_expandable_overflowing_sidebar_scrolls_horizontally() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let cwd = tmp.path();

    let mut config = AppConfig::default();
    config.workspaces.insert(
        "chainargos-blockchain-nodes-with-a-very-long-name".into(),
        WorkspaceConfig::default(),
    );
    let mut state = ManagerState::from_config(&config, cwd);
    state.selected = 1;
    state.cached_term_size = Rect::new(0, 0, 70, 24);
    state.set_list_names_focused(true);

    let outcome = handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right)).unwrap();

    assert!(matches!(outcome, InputOutcome::Continue));
    assert!(
        state.list_names_scroll_x > 0,
        "→ should scroll the focused overflowing sidebar when the row has no expand action"
    );
}

#[test]
fn left_on_non_expandable_overflowing_sidebar_scrolls_horizontally() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let cwd = tmp.path();

    let mut config = AppConfig::default();
    config.workspaces.insert(
        "chainargos-blockchain-nodes-with-a-very-long-name".into(),
        WorkspaceConfig::default(),
    );
    let mut state = ManagerState::from_config(&config, cwd);
    state.selected = 1;
    state.cached_term_size = Rect::new(0, 0, 70, 24);
    state.set_list_names_focused(true);
    state.list_names_scroll_x = 8;

    let outcome = handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Left)).unwrap();

    assert!(matches!(outcome, InputOutcome::Continue));
    assert!(
        state.list_names_scroll_x < 8,
        "← should scroll the focused overflowing sidebar when the row has no collapse action"
    );
}

/// `e` and `d` on the current-directory row must be silent no-ops —
/// no modal, no stage transition.
#[test]
fn current_directory_row_silently_ignores_edit_and_delete() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let cwd = tmp.path();

    let mut config = AppConfig::default();
    config.workspaces.insert(
        "some-ws".into(),
        WorkspaceConfig {
            workdir: "/unrelated".into(),
            mounts: vec![],
            ..Default::default()
        },
    );
    let mut state = ManagerState::from_config(&config, cwd);
    assert_eq!(state.selected, 0);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('e')),
    )
    .unwrap();
    assert!(
        matches!(&state.stage, ManagerStage::List),
        "e on row 0 must not open the Editor; got {:?}",
        state.stage
    );

    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('d')),
    )
    .unwrap();
    assert!(
        matches!(&state.stage, ManagerStage::List),
        "d on row 0 must not open ConfirmDelete; got {:?}",
        state.stage
    );
}

/// Enter on row 0 returns `LaunchCurrentDir`; Enter on row 1 returns
/// `LaunchNamed(<name>)`. Pins the index arithmetic that maps list-row
/// indices to launch targets.
#[test]
fn enter_on_current_directory_returns_launch_current_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let cwd = tmp.path();

    let mut config = AppConfig::default();
    config.workspaces.insert(
        "alpha".into(),
        WorkspaceConfig {
            workdir: "/alpha".into(),
            mounts: vec![],
            ..Default::default()
        },
    );
    let mut state = ManagerState::from_config(&config, cwd);
    state.selected = 0;
    let outcome = handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
    assert!(
        matches!(outcome, InputOutcome::LaunchCurrentDir),
        "row 0 Enter must produce LaunchCurrentDir"
    );

    state.selected = 1;
    let outcome = handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
    match outcome {
        InputOutcome::LaunchNamed(name) => assert_eq!(name, "alpha"),
        other => panic!("row 1 Enter must produce LaunchNamed(\"alpha\"); got {other:?}"),
    }
}

#[test]
fn w_on_saved_workspace_returns_prewarm_named() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let cwd = tmp.path();

    let mut config = AppConfig::default();
    config.workspaces.insert(
        "alpha".into(),
        WorkspaceConfig {
            workdir: "/alpha".into(),
            mounts: vec![],
            ..Default::default()
        },
    );
    let mut state = ManagerState::from_config(&config, cwd);

    state.selected = 0;
    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('w')),
    )
    .unwrap();
    assert!(matches!(outcome, InputOutcome::Continue));

    state.selected = 1;
    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('w')),
    )
    .unwrap();
    match outcome {
        InputOutcome::PrewarmNamed(name) => assert_eq!(name, "alpha"),
        other => panic!("row 1 W must produce PrewarmNamed(\"alpha\"); got {other:?}"),
    }
}

#[test]
fn s_opens_settings_stage() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let cwd = tmp.path();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, cwd);

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('s')),
    )
    .unwrap();

    assert!(matches!(outcome, InputOutcome::Continue));
    assert!(
        matches!(&state.stage, ManagerStage::Settings(settings) if settings.mounts.pending.is_empty())
    );
}

#[test]
fn instance_shortcuts_return_selected_workspace_actions() {
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    state.instances = vec![instance_entry(
        "jackin-demo-architect-123456",
        InstanceStatus::RestoreAvailable,
        workdir,
    )];

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('r')),
    )
    .unwrap();
    match outcome {
        InputOutcome::InstanceAction { container, action } => {
            assert_eq!(container, "jackin-demo-architect-123456");
            assert_eq!(action, ConsoleInstanceAction::Reconnect);
        }
        other => panic!("expected reconnect instance action; got {other:?}"),
    }

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('i')),
    )
    .unwrap();
    match outcome {
        InputOutcome::InstanceAction { container, action } => {
            assert_eq!(container, "jackin-demo-architect-123456");
            assert_eq!(action, ConsoleInstanceAction::Inspect);
        }
        other => panic!("expected inspect instance action; got {other:?}"),
    }

    // P now stages a confirm modal instead of dispatching Purge
    // directly — the action destroys role + DinD + volume + network
    // + local state in one stroke, so an unconditional confirmation
    // step keeps mis-keyed `P` from destroying running work.
    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('p')),
    )
    .unwrap();
    assert!(
        matches!(outcome, InputOutcome::Continue),
        "P should stage the confirm modal and return Continue; got {outcome:?}"
    );
    assert!(
        matches!(state.stage, ManagerStage::ConfirmInstancePurge { .. }),
        "P should have set ConfirmInstancePurge stage"
    );

    // Confirm via Y → the staged action fires.
    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('y')),
    )
    .unwrap();
    match outcome {
        InputOutcome::InstanceAction { container, action } => {
            assert_eq!(container, "jackin-demo-architect-123456");
            assert_eq!(action, ConsoleInstanceAction::Purge);
        }
        other => panic!("expected purge instance action after Y; got {other:?}"),
    }
}

#[test]
fn crashed_instance_is_visible_in_tree_and_enter_restarts_via_ladder() {
    // D15: a failed/stopped instance appears in the console tree, the
    // workspace expands to show it, and selecting + Enter routes it into the
    // restore ladder (the Reconnect action).
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    state.instances = vec![instance_entry(
        "jackin-demo-architect-crashed",
        InstanceStatus::Crashed,
        workdir,
    )];

    assert!(
        state.has_visible_instances(0),
        "a crashed instance must make the workspace expandable"
    );
    state.expand_workspace(0);
    state.selected = state
        .index_of_row(crate::tui::state::ManagerListRow::WorkspaceInstance(0, 0))
        .expect("crashed instance row must be selectable in the tree");

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Enter),
    )
    .unwrap();
    match outcome {
        InputOutcome::InstanceAction { container, action } => {
            assert_eq!(container, "jackin-demo-architect-crashed");
            assert_eq!(action, ConsoleInstanceAction::Reconnect);
        }
        other => panic!("expected reconnect (restart) instance action; got {other:?}"),
    }
}

#[test]
fn confirm_instance_purge_n_dismisses_without_dispatch() {
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    state.instances = vec![instance_entry(
        "jackin-demo-architect-cancel",
        InstanceStatus::Running,
        workdir,
    )];
    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('p')),
    )
    .unwrap();
    assert!(matches!(
        state.stage,
        ManagerStage::ConfirmInstancePurge { .. }
    ));
    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('n')),
    )
    .unwrap();
    assert!(
        matches!(outcome, InputOutcome::Continue),
        "N must return Continue (no dispatch); got {outcome:?}"
    );
    assert!(
        matches!(state.stage, ManagerStage::List),
        "N must reset stage to List"
    );
}

#[test]
fn confirm_instance_purge_esc_dismisses_without_dispatch() {
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    state.instances = vec![instance_entry(
        "jackin-demo-architect-esc",
        InstanceStatus::Running,
        workdir,
    )];
    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('p')),
    )
    .unwrap();
    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Esc),
    )
    .unwrap();
    assert!(matches!(outcome, InputOutcome::Continue));
    assert!(matches!(state.stage, ManagerStage::List));
}

#[test]
fn t_key_dispatches_stop_for_running_instance() {
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    state.instances = vec![instance_entry(
        "jackin-demo-architect-stop",
        InstanceStatus::Running,
        workdir,
    )];
    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('t')),
    )
    .unwrap();
    match outcome {
        InputOutcome::InstanceAction { container, action } => {
            assert_eq!(container, "jackin-demo-architect-stop");
            assert_eq!(action, ConsoleInstanceAction::Stop);
        }
        other => panic!("expected stop instance action; got {other:?}"),
    }
}

#[test]
fn t_key_shows_no_instance_popup_when_no_running_instance() {
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    // Only a CleanExited entry — Stop must not accept it.
    state.instances = vec![instance_entry(
        "jackin-demo-architect-stale",
        InstanceStatus::CleanExited,
        workdir,
    )];
    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('t')),
    )
    .unwrap();
    assert!(
        matches!(outcome, InputOutcome::Continue),
        "T on non-Running must yield Continue (with the no-instance modal); got {outcome:?}"
    );
    assert!(
        matches!(state.list_modal, Some(Modal::ErrorPopup { .. })),
        "expected ErrorPopup modal explaining no running instance"
    );
}

#[test]
fn a_key_starts_new_session_in_running_instance() {
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    state.instances = vec![instance_entry(
        "jackin-demo-architect-123456",
        InstanceStatus::Active,
        workdir,
    )];

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('a')),
    )
    .unwrap();
    match outcome {
        InputOutcome::InstanceAction { container, action } => {
            assert_eq!(container, "jackin-demo-architect-123456");
            assert_eq!(action, ConsoleInstanceAction::NewSession);
        }
        other => panic!("expected NewSession action; got {other:?}"),
    }
}

#[test]
fn x_key_opens_shell_in_running_instance() {
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    state.instances = vec![instance_entry(
        "jackin-demo-architect-123456",
        InstanceStatus::Active,
        workdir,
    )];

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('x')),
    )
    .unwrap();
    match outcome {
        InputOutcome::InstanceAction { container, action } => {
            assert_eq!(container, "jackin-demo-architect-123456");
            assert_eq!(action, ConsoleInstanceAction::Shell);
        }
        other => panic!("expected Shell action; got {other:?}"),
    }
}

#[test]
fn a_and_x_return_continue_for_non_running_instance() {
    let workdir = "/workspace/demo";
    let ws = WorkspaceConfig {
        workdir: workdir.into(),
        mounts: vec![],
        ..Default::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);
    // RestoreAvailable instance — not active/running, so a/x must return Continue.
    state.instances = vec![instance_entry(
        "jackin-demo-architect-123456",
        InstanceStatus::RestoreAvailable,
        workdir,
    )];

    for key_char in ['a', 'x'] {
        state.list_modal = None;
        let outcome = handle_key(
            &mut state,
            &mut config,
            &paths,
            tmp.path(),
            key(KeyCode::Char(key_char)),
        )
        .unwrap();
        assert!(
            matches!(outcome, InputOutcome::Continue),
            "'{key_char}' on non-running instance must return Continue; got {outcome:?}",
        );
        assert!(
            matches!(state.list_modal, Some(Modal::ErrorPopup { .. })),
            "'{key_char}' on non-running instance must open an ErrorPopup; got {:?}",
            state.list_modal,
        );
    }
}

#[test]
fn moving_selection_resets_mount_scroll_state() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let cwd = tmp.path();

    let mut config = AppConfig::default();
    config.workspaces.insert(
        "alpha".into(),
        WorkspaceConfig {
            workdir: "/alpha".into(),
            mounts: vec![],
            ..Default::default()
        },
    );
    // When no block is focused, Down navigates the workspace list and resets scroll.
    let mut state = ManagerState::from_config(&config, cwd);
    state.selected = 0;
    state.list_mounts_scroll_x = 24;
    state.list_global_mounts_scroll_x = 16;
    state.list_role_global_mounts_scroll_x = 8;
    state.set_list_scroll_focus(None);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down)).unwrap();

    assert_eq!(state.selected, 1);
    assert_eq!(state.list_mounts_scroll_x, 0);
    assert_eq!(state.list_global_mounts_scroll_x, 0);
    assert_eq!(state.list_role_global_mounts_scroll_x, 0);
    assert_eq!(state.list_scroll_focus(), None);
}

#[test]
fn down_key_with_focused_block_clamps_vertical_scroll_without_selection_move() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let cwd = tmp.path();

    let mut config = AppConfig::default();
    config.workspaces.insert(
        "alpha".into(),
        WorkspaceConfig {
            workdir: "/alpha".into(),
            mounts: vec![],
            ..Default::default()
        },
    );
    // When a block is focused, Down scrolls that block vertically, not the list.
    let mut state = ManagerState::from_config(&config, cwd);
    state.selected = 0;
    state.set_list_scroll_focus(Some(MountScrollFocus::Workspace));

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down)).unwrap();

    assert_eq!(
        state.selected, 0,
        "selection must not change while block focused"
    );
    assert_eq!(
        state.list_mounts_scroll_y, 0,
        "non-overflowing block stays clamped"
    );
}

// ── List-view `o` key → GitHub resolver + picker ──────────────────

#[test]
fn resolve_github_mounts_returns_one_per_github_repo() {
    // A workspace with two github mounts + one folder + one gitlab repo
    // should yield exactly two picker choices.
    let tmp = tempfile::tempdir().unwrap();
    let repo_a = make_github_repo(tmp.path(), "repo-a", "main");
    let repo_b = make_github_repo(tmp.path(), "repo-b", "dev");
    let plain = tmp.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    // Gitlab repo should be skipped.
    let gitlab = tmp.path().join("gl");
    let gl_git = gitlab.join(".git");
    std::fs::create_dir_all(&gl_git).unwrap();
    std::fs::write(gl_git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::write(
        gl_git.join("config"),
        "[remote \"origin\"]\n    url = git@gitlab.com:owner/repo.git\n",
    )
    .unwrap();

    let ws = WorkspaceConfig {
        mounts: vec![
            mount(repo_a.to_str().unwrap(), "/a"),
            mount(plain.to_str().unwrap(), "/p"),
            mount(repo_b.to_str().unwrap(), "/b"),
            mount(gitlab.to_str().unwrap(), "/g"),
        ],
        ..WorkspaceConfig::default()
    };

    let choices = crate::github_mounts::resolve_for_workspace(&ws);
    assert_eq!(choices.len(), 2);
    // URLs track the HEAD ref per-repo.
    let urls: Vec<&str> = choices.iter().map(|c| c.url.as_str()).collect();
    assert!(urls.contains(&"https://github.com/owner/repo-a/tree/main"));
    assert!(urls.contains(&"https://github.com/owner/repo-b/tree/dev"));
    // Branch label matches Named variant.
    let branches: Vec<&str> = choices.iter().map(|c| c.branch.as_str()).collect();
    assert!(branches.contains(&"main"));
    assert!(branches.contains(&"dev"));
}

#[test]
fn list_o_with_single_github_mount_has_one_resolved_url() {
    // Input queues typed URL-open effect; browser side effects stay in
    // the effect executor.
    let tmp = tempfile::tempdir().unwrap();
    let repo = make_github_repo(tmp.path(), "solo", "trunk");
    let ws = WorkspaceConfig {
        mounts: vec![mount(repo.to_str().unwrap(), "/solo")],
        ..WorkspaceConfig::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('o')),
    )
    .unwrap();

    assert!(matches!(outcome, InputOutcome::Continue));
    let effects = state.drain_effects();
    match effects.first() {
        Some(ManagerEffect::OpenUrl(url)) => {
            assert_eq!(url, "https://github.com/owner/solo/tree/trunk");
        }
        other => panic!("expected OpenUrl effect, got {other:?}"),
    }
}

#[test]
fn list_o_with_multiple_github_mounts_opens_picker() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_a = make_github_repo(tmp.path(), "repo-a", "main");
    let repo_b = make_github_repo(tmp.path(), "repo-b", "main");
    let ws = WorkspaceConfig {
        mounts: vec![
            mount(repo_a.to_str().unwrap(), "/a"),
            mount(repo_b.to_str().unwrap(), "/b"),
        ],
        ..WorkspaceConfig::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('o')),
    )
    .unwrap();

    match &state.list_modal {
        Some(Modal::GithubPicker { state: picker }) => {
            assert_eq!(picker.choices.len(), 2);
        }
        other => panic!("expected GithubPicker modal; got {other:?}"),
    }
}

#[test]
fn list_o_with_zero_github_mounts_is_silent_noop() {
    let tmp_src = tempfile::tempdir().unwrap();
    let plain = tmp_src.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    let ws = WorkspaceConfig {
        mounts: vec![mount(plain.to_str().unwrap(), "/p")],
        ..WorkspaceConfig::default()
    };
    let (mut state, mut config, paths, tmp) = list_state_selecting_ws(ws);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('o')),
    )
    .unwrap();

    assert!(state.list_modal.is_none(), "no modal when no GitHub URLs");
}

#[test]
fn list_o_on_row_zero_is_silent_noop() {
    // Row 0 is "Current directory" — O must be a silent no-op.
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    config
        .workspaces
        .insert("demo".into(), WorkspaceConfig::default());
    let mut state = ManagerState::from_config(&config, tmp.path());
    state.selected = 0;

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('o')),
    )
    .unwrap();

    assert!(
        state.list_modal.is_none(),
        "O on row 0 must not open a modal"
    );
}

#[test]
fn picker_commit_closes_list_modal_and_clears_state() {
    // Seed the state directly with an open GithubPicker, then commit.
    // The input layer must not open the browser. It closes the modal and
    // returns a typed URL-open outcome for the run loop.
    use crate::{github_mounts::GithubChoice, tui::components::github_picker::GithubPickerState};
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    state.list_modal = Some(Modal::GithubPicker {
        state: GithubPickerState::new(vec![GithubChoice {
            src: "/tmp/a".into(),
            branch: "main".into(),
            url: "file:///dev/null".into(),
        }]),
    });

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Enter),
    )
    .unwrap();

    // Browser side effects are not executed in the input handler. The
    // modal closes and queues a URL-open effect for the run loop.
    assert!(
        !matches!(state.list_modal, Some(Modal::GithubPicker { .. })),
        "GithubPicker must be gone after Enter"
    );
    assert!(matches!(outcome, InputOutcome::Continue));
    let effects = state.drain_effects();
    match effects.as_slice() {
        [ManagerEffect::OpenUrl(url)] => assert_eq!(url, "file:///dev/null"),
        other => panic!("expected OpenUrl effect, got {other:?}"),
    }
}

#[test]
fn container_info_enter_copies_default_value_without_dismissing() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    state.list_modal = Some(Modal::ContainerInfo {
        state: jackin_tui::components::ContainerInfoState::new(
            "Debug info",
            vec![
                jackin_tui::components::ContainerInfoRow::new("jackin version", "0.6.0-dev"),
                jackin_tui::components::ContainerInfoRow::new("Run ID", "jk-run-123")
                    .copyable()
                    .emphasised(),
            ],
        ),
    });

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Enter),
    )
    .unwrap();

    assert!(matches!(outcome, InputOutcome::Continue));
    assert!(
        matches!(state.list_modal, Some(Modal::ContainerInfo { .. })),
        "Enter copies but keeps Debug info open for copied feedback"
    );
    match state.drain_effects().as_slice() {
        [ManagerEffect::CopyContainerInfoValue { row, payload }] => {
            assert_eq!(*row, 1);
            assert_eq!(payload, "jk-run-123");
        }
        other => panic!("expected CopyContainerInfoValue effect, got {other:?}"),
    }
}

#[test]
fn picker_esc_closes_without_opening_url() {
    use crate::{github_mounts::GithubChoice, tui::components::github_picker::GithubPickerState};
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    state.list_modal = Some(Modal::GithubPicker {
        state: GithubPickerState::new(vec![GithubChoice {
            src: "/tmp/a".into(),
            branch: "main".into(),
            url: "https://github.com/owner/repo/tree/main".into(),
        }]),
    });

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Esc),
    )
    .unwrap();

    assert!(state.list_modal.is_none());
}
