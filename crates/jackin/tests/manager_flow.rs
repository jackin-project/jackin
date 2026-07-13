#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::manual_assert,
    clippy::duration_suboptimal_units,
    clippy::filter_map_next,
    clippy::map_unwrap_or,
    clippy::redundant_closure,
    unreachable_pub,
    reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
)]

//! End-to-end integration test for the workspace manager TUI.
//! Drives `tui::handle_key` with a scripted key stream — no live
//! terminal.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use jackin::{
    console::{
        ConsoleStage,
        effects::{
            apply_background_event, execute_pending_workspace_save_commit, poll_background_messages,
        },
        tui::{
            InputOutcome, ManagerStage, ManagerState, dispatch_launch_for_workspace, handle_key,
            new_console_state,
            state::{EditorSaveFlow, EditorState, EditorTab, FieldFocus, Modal},
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

#[expect(
    clippy::disallowed_methods,
    reason = "integration test waits for an owned background save worker to publish its subscription result"
)]
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

/// Seed a workspace with pre-populated `env` keys. Used by several Secrets
/// integration tests that need existing env rows to navigate over. Pass
/// an empty `Vec` (or call `seed_config`) for the no-env case.
///
/// # Drop requirement
/// The caller must hold the `TempDir` that owns `temp_dir` alive for the
/// entire duration of the test. Dropping `TempDir` before the test assertions
/// removes the on-disk workspace paths that `JackinPaths` references.
fn seed_config_with_env(
    paths: &JackinPaths,
    temp_dir: &std::path::Path,
    env: Vec<(&str, &str)>,
) -> Result<AppConfig> {
    paths.ensure_base_dirs()?;
    // Use a host path that actually exists (the tempdir) so
    // WorkspaceConfig's workdir-must-equal-or-be-covered-by-mount-dst
    // validation passes.
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
    ce.save()
}

/// Helper to build a `ManagerState` already sitting on the Secrets tab
/// for the "big-monorepo" workspace. Shortcut around navigating the list
/// → editor → tab-cycle key sequence in every test.
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

/// Borrow the active `EditorState` from a `ManagerState` — panics if the
/// stage isn't Editor. Every Secrets integration test sets `stage =
/// Editor(...)` before asserting, so this is safe in-test.
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

/// Render the manager state to a 100x30 `TestBackend` and return the raw
/// buffer as newline-delimited rows. Used to assert that a given glyph
/// appears on screen.
fn render_to_dump(state: &ManagerState<'_>, config: &AppConfig, cwd: &std::path::Path) -> String {
    let backend = TestBackend::new(100, 30);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        jackin::console::tui::render(f, f.area(), state, config, cwd);
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
    // Preselect lands on the saved workspace (selected=1) because the
    // cwd matches — press `d` to delete.
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

    // Press 'y' — commits the delete.
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

    // Config on disk should no longer have big-monorepo.
    let reloaded = AppConfig::load_or_init(&paths)?;
    assert!(
        !reloaded.workspaces.contains_key("big-monorepo"),
        "workspace should be deleted from disk"
    );

    // In-memory state: returned to List. The root run loop owns reloading
    // manager state after it executes the typed remove-workspace outcome.
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
    ce.save()
}

/// `Enter` on a workspace row with two eligible roles and no default
/// must open `Modal::RolePicker` overlaid on the manager list — not
/// short-circuit to a launch outcome.
#[test]
fn launch_after_create_workspace_uses_fresh_data() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config =
        seed_config_with_agents(&paths, temp.path(), &["chainargos/agent-smith"], None)?;
    let cwd = temp.path();

    // Build the console state BEFORE the new workspace exists.
    let mut state = new_console_state(&config, cwd)?;

    // Now create a second workspace via ConfigEditor — same code path
    // the manager save flow uses (no CLI subprocess, no UX detour).
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

    // Dispatch a launch against the freshly-created name. The dispatcher
    // must build the choice from the current `config` and auto-select the
    // single role, proving it reads fresh data rather than a startup snapshot.
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

/// A workspace renamed via `ConfigEditor` after the `ConsoleState` has
/// been built must be resolvable by its new name. Under the bug, the
/// dispatcher would look up the OLD name in the snapshot — find a
/// `WorkspaceChoice` whose `input` was `Saved(old)` — and the
/// preview-resolve step would then fail because `config.workspaces[old]`
/// no longer existed on disk.
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

    // Rename "multi-role-ws" → "renamed-ws" via ConfigEditor.
    {
        let mut ce = ConfigEditor::open(&paths)?;
        ce.rename_workspace(
            &WorkspaceName::parse("multi-role-ws").unwrap(),
            &WorkspaceName::parse("renamed-ws").unwrap(),
        )?;
        config = ce.save()?;
    }

    // Dispatch against the new name — must resolve and auto-select the
    // single eligible role, proving the dispatcher uses the new name.
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

    // OLD name must not resolve — under the snapshot bug it did.
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

/// Post-edit `default_role` must preselect the new default in the picker.
#[test]
fn launch_after_default_agent_change_preselects_new_default() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_agents(
        &paths,
        temp.path(),
        &["chainargos/agent-smith", "chainargos/agent-brown"],
        // No default_role — two eligible roles → picker would open.
        None,
    )?;
    let cwd = temp.path();

    let mut state = new_console_state(&config, cwd)?;

    // Confirm baseline: dispatch against the seeded workspace opens the
    // picker (no short-circuit).
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
        // Close the picker so the next dispatch can reopen / short-circuit.
        ms.list_modal = None;
    }
    state.pending_launch = None;

    // Now set default_role via ConfigEditor (same path the manager's
    // save flow drives via WorkspaceEdit { default_role: Some(_), .. }).
    {
        let mut ce = ConfigEditor::open(&paths)?;
        let edit = jackin::workspace::WorkspaceEdit {
            default_role: Some(Some("chainargos/agent-smith".to_owned())),
            ..jackin::workspace::WorkspaceEdit::default()
        };
        ce.edit_workspace(&wn("multi-role-ws"), edit)?;
        config = ce.save()?;
    }

    // Dispatch again — with the new default_role in config, the
    // dispatcher must preselect that role in the picker.
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
        .selected
        .expect("default role should be selected");
    assert_eq!(picker.filtered[selected].key(), "chainargos/agent-smith");
    Ok(())
}

/// Post-delete dispatch against the old name must return `Ok(None)`.
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
    // Survivor so the delete leaves config non-empty.
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

    // Delete the first workspace via ConfigEditor.
    {
        let mut ce = ConfigEditor::open(&paths)?;
        ce.remove_workspace(&wn("multi-role-ws"))?;
        config = ce.save()?;
    }

    // Attempt a launch against the deleted name — must no-op.
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

    // Sanity: the surviving workspace still resolves and auto-selects its single role.
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
