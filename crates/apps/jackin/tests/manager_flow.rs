#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
)]
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use jackin::{
    console::{
        ConsoleStage,
        adapter::{
            InputOutcome, ManagerStage, ManagerState, dispatch_launch_for_workspace, handle_key,
            new_console_state,
            state::{EditorSaveFlow, EditorState, EditorTab, FieldFocus, Modal},
        },
        effects::{
            apply_background_event, execute_pending_workspace_save_commit, poll_background_messages,
        },
    },
    workspace::{MountConfig, WorkspaceConfig, WorkspaceRoleOverride},
};
use jackin_config::{AppConfig, ConfigEditor};
use jackin_core::JackinPaths;
use jackin_core::WorkspaceName;
fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
}
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tempfile::tempdir;

const fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn wait_for_config_save(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
) -> Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while std::time::Instant::now() < deadline {
        let events = poll_background_messages(state, config, paths);
        for event in events {
            let config_save_finished = matches!(
                event,
                jackin_console::tui::state::update::ManagerBackgroundEvent::ConfigSaveFinished(_)
            );
            apply_background_event(state, config, paths, cwd, event);
            if config_save_finished {
                return Ok(());
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    anyhow::bail!("timed out waiting for config save worker")
}

fn mark_pending_save_drift_checked_for_test(state: &mut ManagerState<'_>) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let EditorSaveFlow::PendingCommit { plan, .. } = &mut editor.save_flow else {
        return;
    };
    plan.isolated_cleanup_complete = true;
}

fn seed_config(paths: &JackinPaths, temp_dir: &std::path::Path) -> Result<AppConfig> {
    seed_config_with_env(paths, temp_dir, vec![])
}

fn seed_config_with_env(
    paths: &JackinPaths,
    temp_dir: &std::path::Path,
    env: Vec<(&str, &str)>,
) -> Result<AppConfig> {
    paths.ensure_base_dirs()?;
    let host_path = temp_dir.display().to_string();
    let env_map: std::collections::BTreeMap<String, jackin_core::EnvValue> = env
        .into_iter()
        .map(|(k, v)| (k.to_owned(), jackin_core::EnvValue::Plain(v.to_owned())))
        .collect();
    let ws = WorkspaceConfig {
        workdir: host_path.clone(),
        mounts: vec![MountConfig {
            src: host_path.clone(),
            dst: host_path,
            readonly: false,
            isolation: jackin_core::MountIsolation::Shared,
        }],
        env: env_map,
        ..Default::default()
    };
    let mut ce = ConfigEditor::open(paths)?;
    ce.create_workspace(&WorkspaceName::parse("big-monorepo").unwrap(), ws)?;
    Ok(ce.save()?)
}

fn manager_on_secrets_tab<'a>(config: &AppConfig, cwd: &std::path::Path) -> ManagerState<'a> {
    let mut state = ManagerState::from_config(config, cwd);
    let ws = config
        .workspaces
        .get("big-monorepo")
        .expect("seed must create big-monorepo")
        .clone();
    let mut editor = EditorState::new_edit("big-monorepo".into(), ws);
    editor.active_tab = EditorTab::Secrets;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0);
    state.stage = ManagerStage::Editor(editor);
    state
}

fn editor_mut<'s, 'a>(state: &'s mut ManagerState<'a>) -> &'s mut EditorState<'a> {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!("expected Editor stage");
    };
    editor
}

fn editor<'s, 'a>(state: &'s ManagerState<'a>) -> &'s EditorState<'a> {
    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("expected Editor stage");
    };
    editor
}

fn render_to_dump(state: &ManagerState<'_>, config: &AppConfig, cwd: &std::path::Path) -> String {
    let backend = TestBackend::new(100, 30);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        jackin::console::adapter::render(f, f.area(), state, config, cwd);
    })
    .unwrap();
    let buf = term.backend().buffer();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn delete_workspace_via_manager() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;

    assert!(
        config.workspaces.contains_key("big-monorepo"),
        "seed failed"
    );

    let cwd = temp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    assert_eq!(state.workspaces.len(), 1);
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('d')),
    )?;
    assert!(
        matches!(state.stage, ManagerStage::ConfirmDelete { .. }),
        "expected ConfirmDelete stage after 'd', got {:?}",
        state.stage
    );

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('y')),
    )?;
    assert!(
        matches!(outcome, InputOutcome::Continue),
        "expected Continue outcome after confirm delete, got {outcome:?}"
    );

    config.workspaces.remove("big-monorepo");
    let mut editor = ConfigEditor::open(&paths)?;
    editor.remove_workspace(&wn("big-monorepo"))?;
    editor.save()?;

    let reloaded = AppConfig::load_or_init(&paths)?;
    assert!(
        !reloaded.workspaces.contains_key("big-monorepo"),
        "workspace should be deleted from disk"
    );

    assert!(matches!(state.stage, ManagerStage::List));

    Ok(())
}

#[path = "manager_flow/auth_claude.rs"]
mod auth_claude;
#[path = "manager_flow/auth_common.rs"]
mod auth_common;
#[path = "manager_flow/auth_github.rs"]
mod auth_github;
#[path = "manager_flow/secrets.rs"]
mod secrets;

fn seed_config_with_agents(
    paths: &JackinPaths,
    temp_dir: &std::path::Path,
    agent_keys: &[&str],
    default_role: Option<&str>,
) -> Result<AppConfig> {
    paths.ensure_base_dirs()?;
    let host_path = temp_dir.display().to_string();
    let mut config = AppConfig::default();
    for key in agent_keys {
        config.roles.insert(
            (*key).to_owned(),
            jackin_config::RoleSource {
                git: format!("https://example.invalid/jackin-{key}.git"),
                trusted: true,
                env: std::collections::BTreeMap::new(),
            },
        );
    }
    let toml = toml::to_string(&config)?;
    std::fs::write(&paths.config_file, toml)?;

    let ws = WorkspaceConfig {
        workdir: host_path.clone(),
        mounts: vec![MountConfig {
            src: host_path.clone(),
            dst: host_path,
            readonly: false,
            isolation: jackin_core::MountIsolation::Shared,
        }],
        allowed_roles: agent_keys.iter().map(|s| (*s).to_owned()).collect(),
        default_role: default_role.map(String::from),
        ..Default::default()
    };
    let mut ce = ConfigEditor::open(paths)?;
    ce.create_workspace(&WorkspaceName::parse("multi-role-ws").unwrap(), ws)?;
    Ok(ce.save()?)
}

#[test]
fn launch_after_create_workspace_uses_fresh_data() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config =
        seed_config_with_agents(&paths, temp.path(), &["chainargos/agent-smith"], None)?;
    let cwd = temp.path();

    let mut state = new_console_state(&config, cwd)?;

    let host_path = cwd.display().to_string();
    let new_ws = WorkspaceConfig {
        workdir: host_path.clone(),
        mounts: vec![MountConfig {
            src: host_path.clone(),
            dst: host_path,
            readonly: false,
            isolation: jackin_core::MountIsolation::Shared,
        }],
        allowed_roles: vec!["chainargos/agent-smith".to_owned()],
        default_role: Some("chainargos/agent-smith".to_owned()),
        ..Default::default()
    };
    {
        let mut ce = ConfigEditor::open(&paths)?;
        ce.create_workspace(&WorkspaceName::parse("freshly-created").unwrap(), new_ws)?;
        config = ce.save()?;
    }

    let outcome = dispatch_launch_for_workspace(
        &mut state,
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("freshly-created".into()),
    )?;
    let (role, _workspace, agent) = outcome.expect(
        "freshly-created workspace must auto-select and return directly; under the bug, \
         ConsoleState.workspaces was a startup snapshot and didn't include the new name",
    );
    assert_eq!(role.key(), "chainargos/agent-smith");
    assert!(agent.is_none());
    Ok(())
}

#[test]
fn launch_after_rename_uses_new_name() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_agents(
        &paths,
        temp.path(),
        &["chainargos/agent-smith"],
        Some("chainargos/agent-smith"),
    )?;
    let cwd = temp.path();

    let mut state = new_console_state(&config, cwd)?;

    {
        let mut ce = ConfigEditor::open(&paths)?;
        ce.rename_workspace(
            &WorkspaceName::parse("multi-role-ws").unwrap(),
            &WorkspaceName::parse("renamed-ws").unwrap(),
        )?;
        config = ce.save()?;
    }

    let outcome = dispatch_launch_for_workspace(
        &mut state,
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("renamed-ws".into()),
    )?;
    let (role, _workspace, agent) = outcome
        .expect("renamed workspace must resolve under the new name and auto-select the role");
    assert_eq!(role.key(), "chainargos/agent-smith");
    assert!(agent.is_none());

    let stale_outcome = dispatch_launch_for_workspace(
        &mut state,
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("multi-role-ws".into()),
    )?;
    assert!(
        stale_outcome.is_none(),
        "the old (renamed-away) name must not resolve to a launch outcome; got {stale_outcome:?}"
    );
    Ok(())
}

#[test]
fn launch_after_default_agent_change_preselects_new_default() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_agents(
        &paths,
        temp.path(),
        &["chainargos/agent-smith", "chainargos/agent-brown"],
        None,
    )?;
    let cwd = temp.path();

    let mut state = new_console_state(&config, cwd)?;

    let baseline = dispatch_launch_for_workspace(
        &mut state,
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("multi-role-ws".into()),
    )?;
    assert!(
        baseline.is_none(),
        "baseline (no default_role) must open the picker, not direct-launch"
    );
    {
        let ConsoleStage::Manager(ms) = &mut state.stage;
        ms.list_modal = None;
    }
    state.pending_launch = None;

    {
        let mut ce = ConfigEditor::open(&paths)?;
        let edit = jackin::workspace::WorkspaceEdit {
            default_role: Some(Some("chainargos/agent-smith".to_owned())),
            ..jackin::workspace::WorkspaceEdit::default()
        };
        ce.edit_workspace(&wn("multi-role-ws"), edit)?;
        config = ce.save()?;
    }

    let after = dispatch_launch_for_workspace(
        &mut state,
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("multi-role-ws".into()),
    )?;
    assert!(
        after.is_none(),
        "after default_role is set, dispatch must keep the operator in the picker flow"
    );
    let ConsoleStage::Manager(ms) = &state.stage;
    let picker = ms
        .inline_role_picker
        .as_ref()
        .expect("post-default dispatch must open the inline picker");
    let selected = picker
        .list_state
        .selected()
        .copied()
        .expect("default role should be selected");
    assert_eq!(picker.filtered[selected].key(), "chainargos/agent-smith");
    Ok(())
}

#[test]
fn launch_after_delete_workspace_does_not_resolve_old_choice() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    seed_config_with_agents(
        &paths,
        temp.path(),
        &["chainargos/agent-smith"],
        Some("chainargos/agent-smith"),
    )?;
    let host_path = temp.path().display().to_string();
    let second = WorkspaceConfig {
        workdir: host_path.clone(),
        mounts: vec![MountConfig {
            src: host_path.clone(),
            dst: host_path,
            readonly: false,
            isolation: jackin_core::MountIsolation::Shared,
        }],
        allowed_roles: vec!["chainargos/agent-smith".to_owned()],
        default_role: Some("chainargos/agent-smith".to_owned()),
        ..Default::default()
    };
    let mut config = {
        let mut ce = ConfigEditor::open(&paths)?;
        ce.create_workspace(&WorkspaceName::parse("survivor-ws").unwrap(), second)?;
        ce.save()?
    };
    let cwd = temp.path();

    let mut state = new_console_state(&config, cwd)?;

    {
        let mut ce = ConfigEditor::open(&paths)?;
        ce.remove_workspace(&wn("multi-role-ws"))?;
        config = ce.save()?;
    }

    let outcome = dispatch_launch_for_workspace(
        &mut state,
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("multi-role-ws".into()),
    )?;
    assert!(
        outcome.is_none(),
        "deleted workspace must not resolve to a launch outcome; under the bug, \
         the snapshot retained it and the dispatcher would have launched a ghost; \
         got {outcome:?}"
    );

    let alive = dispatch_launch_for_workspace(
        &mut state,
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("survivor-ws".into()),
    )?;
    let (role, _workspace, agent) =
        alive.expect("survivor-ws must still resolve and auto-select its role");
    assert_eq!(role.key(), "chainargos/agent-smith");
    assert!(agent.is_none());
    Ok(())
}
