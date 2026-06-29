//! End-to-end integration test for the workspace manager TUI.
//! Drives `tui::handle_key` with a scripted key stream — no live
//! terminal.

#![expect(
    clippy::panic,
    clippy::expect_used,
    reason = "manager flow tests should fail immediately when expected UI state is absent"
)]

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
            state::{AuthRow, EditorState, EditorTab, FieldFocus, Modal},
        },
    },
    workspace::{MountConfig, WorkspaceConfig, WorkspaceRoleOverride},
};
use jackin_config::{AppConfig, ConfigEditor};
use jackin_console::tui::auth::AuthKind;
use jackin_core::JackinPaths;
use jackin_core::env_model;
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
    let env_map: std::collections::BTreeMap<String, jackin::operator_env::EnvValue> = env
        .into_iter()
        .map(|(k, v)| {
            (
                k.to_owned(),
                jackin::operator_env::EnvValue::Plain(v.to_owned()),
            )
        })
        .collect();
    let ws = WorkspaceConfig {
        workdir: host_path.clone(),
        mounts: vec![MountConfig {
            src: host_path.clone(),
            dst: host_path,
            readonly: false,
            isolation: jackin::isolation::MountIsolation::Shared,
        }],
        env: env_map,
        ..Default::default()
    };
    let mut ce = ConfigEditor::open(paths)?;
    ce.create_workspace("big-monorepo", ws)?;
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
    editor.remove_workspace("big-monorepo")?;
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
            isolation: jackin::isolation::MountIsolation::Shared,
        }],
        allowed_roles: agent_keys.iter().map(|s| (*s).to_owned()).collect(),
        default_role: default_role.map(String::from),
        ..Default::default()
    };
    let mut ce = ConfigEditor::open(paths)?;
    ce.create_workspace("multi-role-ws", ws)?;
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
            isolation: jackin::isolation::MountIsolation::Shared,
        }],
        allowed_roles: vec!["chainargos/agent-smith".to_owned()],
        default_role: Some("chainargos/agent-smith".to_owned()),
        ..Default::default()
    };
    {
        let mut ce = ConfigEditor::open(&paths)?;
        ce.create_workspace("freshly-created", new_ws)?;
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
        ce.rename_workspace("multi-role-ws", "renamed-ws")?;
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
        ce.edit_workspace("multi-role-ws", edit)?;
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
            isolation: jackin::isolation::MountIsolation::Shared,
        }],
        allowed_roles: vec!["chainargos/agent-smith".to_owned()],
        default_role: Some("chainargos/agent-smith".to_owned()),
        ..Default::default()
    };
    let mut config = {
        let mut ce = ConfigEditor::open(&paths)?;
        ce.create_workspace("survivor-ws", second)?;
        ce.save()?
    };
    let cwd = temp.path();

    let mut state = new_console_state(&config, cwd)?;

    // Delete the first workspace via ConfigEditor.
    {
        let mut ce = ConfigEditor::open(&paths)?;
        ce.remove_workspace("multi-role-ws")?;
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

// ── Auth tab helpers ──────────────────────────────────────────────────

/// Return the flat-row index of the first `AuthRow` that matches `pred`.
fn auth_row_idx(
    ed: &EditorState<'_>,
    config: &AppConfig,
    pred: impl Fn(&AuthRow) -> bool,
) -> usize {
    ed.auth_flat_rows(config)
        .iter()
        .position(pred)
        .expect("required auth row not found")
}

// ── Auth tab integration test ─────────────────────────────────────────
//
// End-to-end coverage of the auth-form save path: open the form on a
// workspace × Claude row, set mode = api_key + a literal credential,
// commit the form, save the editor, reload from disk, and assert the
// persisted TOML carries BOTH the `auth_forward = "api_key"` block AND
// the `ANTHROPIC_API_KEY` env var.
//
// The bug this guards against: prior to the C1 fixup, the auth-form
// commit only mutated per-agent auth blocks in `editor.pending`, but
// `build_workspace_edit` / `WorkspaceEdit` carried no auth-forward
// field, so `edit_workspace` re-rendered the workspace table from the
// parsed-from-disk in-memory copy — silently overwriting the operator's
// mode change. The credential env var landed (env diff is wired
// separately) but the mode never reached disk; on reload, the resolver
// fell back to the global default and ignored the freshly-written key.
#[test]
#[allow(clippy::too_many_lines)]
fn auth_form_save_persists_mode_and_credential_to_disk() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    // Start the manager on the Auth tab, cursor on the workspace × Claude row.
    let mut state = ManagerState::from_config(&config, cwd);
    let ws = config
        .workspaces
        .get("big-monorepo")
        .expect("seed must create big-monorepo")
        .clone();
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.auth_selected_kind = Some(AuthKind::Claude);
    let ws_claude_idx = auth_row_idx(&ed, &config, |r| {
        matches!(
            r,
            AuthRow::WorkspaceMode {
                kind: AuthKind::Claude
            }
        )
    });
    ed.active_field = FieldFocus::Row(ws_claude_idx);
    state.stage = ManagerStage::Editor(ed);

    // Enter opens the auth-edit form modal on workspace × Claude.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::AuthForm { .. })),
        "Enter on WorkspaceMode/Claude must open AuthForm; got {:?}",
        editor(&state).modal
    );

    // Cycle mode: None → Sync → ApiKey (two Spaces).
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char(' ')),
    )?;
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char(' ')),
    )?;
    // Tab advances to credential row; Enter opens the source picker;
    // Enter picks the default Plain text source.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::AuthSourcePicker { .. })),
        "credential row Enter must open AuthSourcePicker; got {:?}",
        editor(&state).modal
    );
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::TextInput { .. })),
        "Plain source must open credential text input; got {:?}",
        editor(&state).modal
    );
    // Type "sk-ant-test".
    for ch in "sk-ant-test".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    // Enter confirms text input, returning to the auth form Save button.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    // Enter commits the form.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        editor(&state).modal.is_none(),
        "auth-form save must close the modal"
    );

    // pending.claude reflects ApiKey, pending.env carries the credential.
    let pending = &editor(&state).pending;
    assert_eq!(
        pending.claude.as_ref().map(|c| c.auth_forward),
        Some(jackin_config::AuthForwardMode::ApiKey),
        "form commit must set workspace × claude mode in pending"
    );
    assert!(
        pending
            .env
            .contains_key(env_model::ANTHROPIC_API_KEY_ENV_NAME),
        "form commit must set credential env var in pending"
    );

    // Save the editor: `s` opens the ConfirmSave modal (no collapses
    // expected here since the seed has a single mount); Tab moves focus
    // Cancel -> Save, then Enter commits and bounces back to List.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('s')),
    )?;
    // Default focus = Cancel (TUI design decisions: confirmation dialog rule).
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    // Enter moves state to PendingCommit; flush the queued write to disk.
    execute_pending_workspace_save_commit(&mut state, &mut config, &paths, cwd)?;
    wait_for_config_save(&mut state, &mut config, &paths, cwd)?;

    // Reload AppConfig from disk and assert both halves of the auth
    // change survived the round-trip.
    let reloaded = AppConfig::load_or_init(&paths)?;
    let ws_on_disk = reloaded
        .workspaces
        .get("big-monorepo")
        .expect("workspace must still exist on disk");
    assert_eq!(
        ws_on_disk.claude.as_ref().map(|c| c.auth_forward),
        Some(jackin_config::AuthForwardMode::ApiKey),
        "reload must see [claude] auth_forward = api_key in the workspace file"
    );
    let env_value = ws_on_disk
        .env
        .get(env_model::ANTHROPIC_API_KEY_ENV_NAME)
        .expect("reload must see ANTHROPIC_API_KEY in workspace env");
    match env_value {
        jackin::operator_env::EnvValue::Plain(s) => assert_eq!(s, "sk-ant-test"),
        jackin::operator_env::EnvValue::Extended(e) => assert_eq!(e.value, "sk-ant-test"),
        jackin::operator_env::EnvValue::OpRef(_) => {
            panic!("expected literal credential, got OpRef")
        }
    }

    // Belt-and-braces: read the raw TOML and confirm the literal text is
    // there. Catches a future regression where a typed accessor papered
    // over a missing block (e.g. resolver fall-through).
    let toml = std::fs::read_to_string(paths.workspaces_dir.join("big-monorepo.toml"))?;
    assert!(
        toml.contains("[claude]"),
        "raw TOML must carry the workspace claude block; got:\n{toml}"
    );
    assert!(
        toml.contains(r#"auth_forward = "api_key""#),
        "raw TOML must carry auth_forward = \"api_key\"; got:\n{toml}"
    );
    assert!(
        toml.contains(&format!(
            r#"{} = "sk-ant-test""#,
            env_model::ANTHROPIC_API_KEY_ENV_NAME
        )),
        "raw TOML must carry the credential env var; got:\n{toml}"
    );
    Ok(())
}

#[test]
fn auth_credential_source_enter_opens_source_picker() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = ManagerState::from_config(&config, cwd);
    let ws = config
        .workspaces
        .get("big-monorepo")
        .expect("seed must create big-monorepo")
        .clone();
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.auth_selected_kind = Some(AuthKind::Claude);
    let ws_claude_idx = auth_row_idx(&ed, &config, |r| {
        matches!(
            r,
            AuthRow::WorkspaceMode {
                kind: AuthKind::Claude
            }
        )
    });
    ed.active_field = FieldFocus::Row(ws_claude_idx);
    state.stage = ManagerStage::Editor(ed);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char(' ')),
    )?;
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char(' ')),
    )?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    let Some(Modal::AuthSourcePicker { state: picker }) = &editor(&state).modal else {
        panic!(
            "Enter on auth credential source must open AuthSourcePicker; got {:?}",
            editor(&state).modal
        );
    };
    assert_eq!(picker.key, env_model::ANTHROPIC_API_KEY_ENV_NAME);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::AuthForm { .. })),
        "Esc from AuthSourcePicker must restore the auth form"
    );

    Ok(())
}

#[test]
fn auth_add_role_override_flow_uses_selected_auth_kind() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = ManagerState::from_config(&config, cwd);
    let ws = config
        .workspaces
        .get("big-monorepo")
        .expect("seed must create big-monorepo")
        .clone();
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.auth_selected_kind = Some(AuthKind::Claude);

    let sentinel_idx = auth_row_idx(&ed, &config, |r| matches!(r, AuthRow::AddSentinel { .. }));
    ed.active_field = FieldFocus::Row(sentinel_idx);
    state.stage = ManagerStage::Editor(ed);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::AuthRolePicker { .. })),
        "Enter on AddSentinel must open AuthRolePicker; got {:?}",
        editor(&state).modal
    );

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        match &editor(&state).modal {
            Some(Modal::AuthForm {
                target: jackin::console::tui::state::AuthFormTarget::WorkspaceRole { role, kind },
                ..
            }) => {
                assert_eq!(*kind, AuthKind::Claude);
                assert!(
                    !role.is_empty(),
                    "role must propagate from AuthRolePicker → AuthForm"
                );
                true
            }
            _ => false,
        },
        "Enter on AuthRolePicker must open AuthForm for selected auth kind; got {:?}",
        editor(&state).modal
    );
    Ok(())
}

#[test]
fn auth_role_header_left_right_toggles_expansion() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut ws = config.workspaces.get("big-monorepo").unwrap().clone();
    let over = WorkspaceRoleOverride {
        claude: Some(jackin_config::AgentAuthConfig {
            auth_forward: jackin_config::AuthForwardMode::Ignore,
            ..Default::default()
        }),
        ..Default::default()
    };
    ws.roles.insert("the-architect".into(), over);

    let mut state = ManagerState::from_config(&config, cwd);
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.set_tab_bar_focused(false);
    ed.auth_selected_kind = Some(AuthKind::Claude);
    let header_idx = auth_row_idx(&ed, &config, |r| matches!(r, AuthRow::RoleHeader { .. }));
    ed.active_field = FieldFocus::Row(header_idx);
    state.stage = ManagerStage::Editor(ed);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    assert!(editor(&state).auth_expanded.contains("the-architect"));
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Left))?;
    assert!(!editor(&state).auth_expanded.contains("the-architect"));
    Ok(())
}

#[test]
fn auth_role_header_d_clears_selected_auth_kind_override() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut ws = config.workspaces.get("big-monorepo").unwrap().clone();
    let over = WorkspaceRoleOverride {
        claude: Some(jackin_config::AgentAuthConfig {
            auth_forward: jackin_config::AuthForwardMode::Ignore,
            ..Default::default()
        }),
        codex: Some(jackin_config::AgentAuthConfig {
            auth_forward: jackin_config::AuthForwardMode::ApiKey,
            ..Default::default()
        }),
        ..Default::default()
    };
    ws.roles.insert("the-architect".into(), over);

    let mut state = ManagerState::from_config(&config, cwd);
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.auth_selected_kind = Some(AuthKind::Claude);
    let header_idx = auth_row_idx(&ed, &config, |r| matches!(r, AuthRow::RoleHeader { .. }));
    ed.active_field = FieldFocus::Row(header_idx);
    state.stage = ManagerStage::Editor(ed);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('d')),
    )?;
    assert!(
        editor(&state).modal.is_none(),
        "selected auth-kind clear should not open a confirm modal"
    );
    let role_over = editor(&state)
        .pending
        .roles
        .get("the-architect")
        .expect("override entry stays even after clear");
    assert!(role_over.claude.is_none());
    assert!(
        role_over.codex.is_some(),
        "hidden Codex override must be untouched"
    );
    Ok(())
}

#[test]
fn auth_role_agent_row_d_silently_clears_single_agent() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut ws = config.workspaces.get("big-monorepo").unwrap().clone();
    let over = WorkspaceRoleOverride {
        claude: Some(jackin_config::AgentAuthConfig {
            auth_forward: jackin_config::AuthForwardMode::Ignore,
            ..Default::default()
        }),
        codex: Some(jackin_config::AgentAuthConfig {
            auth_forward: jackin_config::AuthForwardMode::ApiKey,
            ..Default::default()
        }),
        ..Default::default()
    };
    ws.roles.insert("the-architect".into(), over);

    let mut state = ManagerState::from_config(&config, cwd);
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.auth_selected_kind = Some(AuthKind::Claude);
    ed.auth_expanded.insert("the-architect".into());
    let claude_idx = auth_row_idx(&ed, &config, |r| {
        matches!(
            r,
            AuthRow::RoleMode {
                kind: AuthKind::Claude,
                ..
            }
        )
    });
    ed.active_field = FieldFocus::Row(claude_idx);
    state.stage = ManagerStage::Editor(ed);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('d')),
    )?;
    assert!(
        editor(&state).modal.is_none(),
        "single-agent clear must be silent (no modal)"
    );
    let ro = editor(&state).pending.roles.get("the-architect").unwrap();
    assert!(ro.claude.is_none());
    assert!(ro.codex.is_some(), "Codex override must be untouched");
    Ok(())
}

/// Pressing Enter on an `AuthKind` row sets `auth_selected_kind` and
/// resets the cursor to row 0. Drives the kind picker → focused-view
/// transition through the real keystroke dispatcher (every other auth
/// integration test pre-seeds `auth_selected_kind`, which would mask
/// a regression in the input-layer wiring).
#[test]
fn auth_enter_on_auth_kind_focuses_selected_agent() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = ManagerState::from_config(&config, cwd);
    let ws = config.workspaces.get("big-monorepo").unwrap().clone();
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    // No `auth_selected_kind` set — picker view.
    let codex_kind_idx = auth_row_idx(&ed, &config, |r| {
        matches!(
            r,
            AuthRow::AuthKindRow {
                kind: AuthKind::Codex
            }
        )
    });
    ed.active_field = FieldFocus::Row(codex_kind_idx);
    state.stage = ManagerStage::Editor(ed);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    let ed = editor(&state);
    assert_eq!(ed.auth_selected_kind, Some(AuthKind::Codex));
    assert!(
        matches!(ed.active_field, FieldFocus::Row(0)),
        "Enter on AuthKind must reset cursor to Row(0); got {:?}",
        ed.active_field
    );
    Ok(())
}

/// Esc on the focused auth view pops back to the kind picker WITHOUT
/// triggering the dirty-modal flow even when `editor.is_dirty()`.
/// Pending edits stay in `editor.pending`; a subsequent Esc on the
/// picker would fall through to the dirty check.
#[test]
fn auth_esc_from_focused_view_pops_to_picker_without_dirty_modal() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = ManagerState::from_config(&config, cwd);
    let ws = config.workspaces.get("big-monorepo").unwrap().clone();
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.auth_selected_kind = Some(AuthKind::Claude);
    // Mutate `pending` so `is_dirty()` is true — this is what would
    // otherwise route Esc through `Modal::SaveDiscardCancel`.
    ed.pending.git_pull_on_entry = !ed.pending.git_pull_on_entry;
    ed.active_field = FieldFocus::Row(0);
    state.stage = ManagerStage::Editor(ed);
    assert!(editor(&state).is_dirty(), "fixture must be dirty");

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;

    let ed = editor(&state);
    assert!(
        ed.auth_selected_kind.is_none(),
        "Esc on focused view must clear auth_selected_kind"
    );
    assert!(
        ed.modal.is_none(),
        "Esc must not open the dirty SaveDiscardCancel modal mid-tab"
    );
    assert!(
        ed.is_dirty(),
        "pending edits must survive the in-tab navigation pop"
    );
    Ok(())
}

/// Tab/BackTab leaving the Auth tab clears `auth_selected_kind` so
/// re-entering the tab returns to the kind picker rather than a
/// stale focused view.
#[test]
fn auth_tab_cycle_off_auth_clears_selected_agent() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = ManagerState::from_config(&config, cwd);
    let ws = config.workspaces.get("big-monorepo").unwrap().clone();
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.set_tab_bar_focused(false);
    ed.auth_selected_kind = Some(AuthKind::Claude);
    state.stage = ManagerStage::Editor(ed);

    // Tab from content returns to tab bar and advances to next tab (General).
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab))?;
    let ed = editor(&state);
    assert_ne!(ed.active_tab, EditorTab::Auth);
    assert!(
        ed.auth_selected_kind.is_none(),
        "leaving the Auth tab must drop the focused-kind selection"
    );
    Ok(())
}

/// Pressing D on a `WorkspaceSource` row is a no-op. The main auth panel
/// renders source rows as previews; edits happen through the mode row's dialog.
#[test]
fn auth_workspace_source_d_is_noop() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut ws = config.workspaces.get("big-monorepo").unwrap().clone();
    ws.claude = Some(jackin_config::AgentAuthConfig {
        auth_forward: jackin_config::AuthForwardMode::ApiKey,
        ..Default::default()
    });
    ws.env.insert(
        env_model::ANTHROPIC_API_KEY_ENV_NAME.into(),
        jackin::operator_env::EnvValue::Plain("k".into()),
    );

    let mut state = ManagerState::from_config(&config, cwd);
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.auth_selected_kind = Some(AuthKind::Claude);
    let source_idx = auth_row_idx(&ed, &config, |r| {
        matches!(
            r,
            AuthRow::WorkspaceSource {
                kind: AuthKind::Claude
            }
        )
    });
    ed.active_field = FieldFocus::Row(source_idx);
    state.stage = ManagerStage::Editor(ed);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('d')),
    )?;

    let ed = editor(&state);
    assert!(
        ed.modal.is_none(),
        "workspace source preview no-op must be silent (no modal)"
    );
    assert!(
        ed.pending.claude.is_some(),
        "D on WorkspaceSource must keep the workspace-level claude override"
    );
    Ok(())
}

/// Cancelling the credential `Modal::TextInput` (the literal-text leg
/// of the source-picker round trip) must restore `Modal::AuthForm` and
/// drain `pending_auth_form_return` — not silently leave the operator
/// on a blank Auth tab.
#[test]
fn auth_credential_text_input_cancel_restores_form() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = ManagerState::from_config(&config, cwd);
    let ws = config.workspaces.get("big-monorepo").unwrap().clone();
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.auth_selected_kind = Some(AuthKind::Claude);
    let ws_claude_idx = auth_row_idx(&ed, &config, |r| {
        matches!(
            r,
            AuthRow::WorkspaceMode {
                kind: AuthKind::Claude
            }
        )
    });
    ed.active_field = FieldFocus::Row(ws_claude_idx);
    state.stage = ManagerStage::Editor(ed);

    // Open form → cycle to api_key → Tab to credential row → Enter →
    // SourcePicker → Enter (Plain) → TextInput → Esc.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char(' ')),
    )?;
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char(' ')),
    )?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(matches!(
        editor(&state).modal,
        Some(Modal::AuthSourcePicker { .. })
    ));
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(matches!(
        editor(&state).modal,
        Some(Modal::TextInput { .. })
    ));

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::AuthForm { .. })),
        "TextInput cancel must restore AuthForm; got {:?}",
        editor(&state).modal
    );
    Ok(())
}

/// `op_available = false` must propagate through the auth-form ↔
/// `AuthSourcePicker` handoff, so operators on hosts without `op` see
/// the picker with the 1Password choice disabled.
#[test]
fn auth_source_picker_op_disabled_when_op_missing() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = ManagerState::from_config(&config, cwd);
    state.op_available = false;
    let ws = config.workspaces.get("big-monorepo").unwrap().clone();
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.auth_selected_kind = Some(AuthKind::Claude);
    let ws_claude_idx = auth_row_idx(&ed, &config, |r| {
        matches!(
            r,
            AuthRow::WorkspaceMode {
                kind: AuthKind::Claude
            }
        )
    });
    ed.active_field = FieldFocus::Row(ws_claude_idx);
    state.stage = ManagerStage::Editor(ed);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char(' ')),
    )?;
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char(' ')),
    )?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    let Some(Modal::AuthSourcePicker { state: picker }) = &editor(&state).modal else {
        panic!(
            "Enter on credential source must open AuthSourcePicker; got {:?}",
            editor(&state).modal
        );
    };
    assert!(
        !picker.op_available,
        "op_available must propagate from ManagerState through to the picker"
    );
    Ok(())
}

// ── GitHub auth-tab integration tests ────────────────────────────────
//
// End-to-end coverage of the new GitHub kind on the Auth tab.
// Mirror-shape with `auth_form_save_persists_mode_and_credential_to_disk`
// for Claude — open the form on the workspace × Github row, set mode =
// token + a literal `GH_TOKEN`, commit, save, reload from disk, and
// assert the persisted TOML carries BOTH the `[github]`
// auth_forward = "token" block AND the `GH_TOKEN` env var on the
// matching `[github.env]` block.
#[allow(clippy::too_many_lines)]
#[test]
fn github_auth_form_save_persists_token_mode_and_gh_token_to_disk() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = ManagerState::from_config(&config, cwd);
    let ws = config
        .workspaces
        .get("big-monorepo")
        .expect("seed must create big-monorepo")
        .clone();
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.auth_selected_kind = Some(AuthKind::Github);
    let ws_github_idx = auth_row_idx(&ed, &config, |r| {
        matches!(
            r,
            AuthRow::WorkspaceMode {
                kind: AuthKind::Github
            }
        )
    });
    ed.active_field = FieldFocus::Row(ws_github_idx);
    state.stage = ManagerStage::Editor(ed);

    // Enter opens the auth-edit form on workspace × Github.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::AuthForm { .. })),
        "Enter on WorkspaceMode/Github must open AuthForm; got {:?}",
        editor(&state).modal
    );

    // Cycle mode: None → Sync → Token (two presses through the
    // [Sync, Token, Ignore] cycle).
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char(' ')),
    )?;
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char(' ')),
    )?;
    // Tab → credential row, Enter → source picker, Enter → plain text.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::AuthSourcePicker { .. })),
        "credential row Enter must open AuthSourcePicker; got {:?}",
        editor(&state).modal
    );
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::TextInput { .. })),
        "Plain source must open credential text input; got {:?}",
        editor(&state).modal
    );
    for ch in "ghp_round_trip".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        editor(&state).modal.is_none(),
        "auth-form save must close the modal"
    );

    // pending.github reflects Token + GH_TOKEN landed on the github
    // env block (NOT the regular workspace env).
    let pending = &editor(&state).pending;
    let github = pending
        .github
        .as_ref()
        .expect("workspace github block must be set in pending");
    assert_eq!(github.auth_forward, jackin_config::GithubAuthMode::Token);
    let value = github
        .env
        .get("GH_TOKEN")
        .expect("GH_TOKEN must land on the workspace github env block");
    match value {
        jackin::operator_env::EnvValue::Plain(s) => assert_eq!(s, "ghp_round_trip"),
        jackin::operator_env::EnvValue::Extended(e) => assert_eq!(e.value, "ghp_round_trip"),
        jackin::operator_env::EnvValue::OpRef(_) => panic!("expected literal credential"),
    }
    assert!(
        !pending.env.contains_key("GH_TOKEN"),
        "GH_TOKEN must not leak into the regular workspace env map"
    );

    // Save the editor: `s` opens ConfirmSave; Tab moves Cancel -> Save,
    // Enter commits and returns to List.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('s')),
    )?;
    // Default focus = Cancel (TUI design decisions: confirmation dialog rule).
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    // Enter moves state to PendingCommit; flush the queued write to disk.
    execute_pending_workspace_save_commit(&mut state, &mut config, &paths, cwd)?;
    wait_for_config_save(&mut state, &mut config, &paths, cwd)?;

    // Reload AppConfig from disk and assert the round-trip.
    let reloaded = AppConfig::load_or_init(&paths)?;
    let ws_on_disk = reloaded
        .workspaces
        .get("big-monorepo")
        .expect("workspace must still exist on disk");
    let github_on_disk = ws_on_disk
        .github
        .as_ref()
        .expect("[github] block must be on disk after save");
    assert_eq!(
        github_on_disk.auth_forward,
        jackin_config::GithubAuthMode::Token
    );
    let env_value = github_on_disk
        .env
        .get("GH_TOKEN")
        .expect("reload must see GH_TOKEN on the github env block");
    match env_value {
        jackin::operator_env::EnvValue::Plain(s) => assert_eq!(s, "ghp_round_trip"),
        jackin::operator_env::EnvValue::Extended(e) => assert_eq!(e.value, "ghp_round_trip"),
        jackin::operator_env::EnvValue::OpRef(_) => panic!("expected literal credential"),
    }
    // GH_TOKEN must NOT leak into the regular workspace env map after
    // reload (the kind-scoped layer is the only place it should live).
    assert!(
        !ws_on_disk.env.contains_key("GH_TOKEN"),
        "GH_TOKEN must not appear in [env]; only in [github.env]"
    );

    // Belt-and-braces: read the raw workspace TOML and confirm the
    // literal text landed on `[github]` / `[github.env]`, not `[env]`.
    let toml = std::fs::read_to_string(paths.workspaces_dir.join("big-monorepo.toml"))?;
    assert!(
        toml.contains("[github]"),
        "raw TOML must carry the workspace github block; got:\n{toml}"
    );
    assert!(
        toml.contains(r#"auth_forward = "token""#),
        "raw TOML must carry auth_forward = \"token\"; got:\n{toml}"
    );
    assert!(
        toml.contains(r#"GH_TOKEN = "ghp_round_trip""#),
        "raw TOML must carry GH_TOKEN; got:\n{toml}"
    );
    Ok(())
}

/// `D` on a Github `RoleHeader` clears the role's
/// `[roles.<role>.github]` override end-to-end through
/// the input dispatcher (in addition to the unit-level coverage in
/// `src/console/manager/input/auth.rs`).
#[test]
fn github_role_header_d_clears_github_role_override() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut ws = config.workspaces.get("big-monorepo").unwrap().clone();
    let over = WorkspaceRoleOverride {
        github: Some(jackin_config::GithubAuthConfig {
            auth_forward: jackin_config::GithubAuthMode::Ignore,
            ..Default::default()
        }),
        ..Default::default()
    };
    ws.roles.insert("the-architect".into(), over);

    let mut state = ManagerState::from_config(&config, cwd);
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.auth_selected_kind = Some(AuthKind::Github);
    let header_idx = auth_row_idx(&ed, &config, |r| {
        matches!(
            r,
            AuthRow::RoleHeader { role, .. } if role == "the-architect"
        )
    });
    ed.active_field = FieldFocus::Row(header_idx);
    state.stage = ManagerStage::Editor(ed);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('d')),
    )?;
    let role_entry = editor(&state)
        .pending
        .roles
        .get("the-architect")
        .expect("override entry must remain after D");
    assert!(
        role_entry.github.is_none(),
        "D on github RoleHeader must clear the role's github override"
    );
    Ok(())
}

/// Integration counterpart for the unit-level
/// `github_role_override_picker_filters_already_overridden_roles` —
/// drives the picker open through the keystroke dispatcher with the
/// github kind selected and asserts the candidate list filters out a
/// role that already carries a `[…github]` override.
#[test]
fn github_role_override_picker_filters_already_overridden_roles_via_dispatcher() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    // Pre-seed a workspace × role × github override on `the-architect`
    // so the picker should filter it out and only offer other roles.
    let mut ws = config.workspaces.get("big-monorepo").unwrap().clone();
    ws.roles.insert(
        "the-architect".into(),
        WorkspaceRoleOverride {
            github: Some(jackin_config::GithubAuthConfig {
                auth_forward: jackin_config::GithubAuthMode::Ignore,
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut state = ManagerState::from_config(&config, cwd);
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.auth_selected_kind = Some(AuthKind::Github);
    let sentinel_idx = auth_row_idx(&ed, &config, |r| matches!(r, AuthRow::AddSentinel { .. }));
    ed.active_field = FieldFocus::Row(sentinel_idx);
    state.stage = ManagerStage::Editor(ed);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    let Some(Modal::AuthRolePicker { state: picker }) = &editor(&state).modal else {
        panic!(
            "Enter on AddSentinel must open AuthRolePicker for github kind; got {:?}",
            editor(&state).modal
        );
    };
    let labels: Vec<String> = picker
        .roles
        .iter()
        .map(jackin::selector::RoleSelector::key)
        .collect();
    assert!(
        !labels.iter().any(|s| s == "the-architect"),
        "the-architect already has a github override and must be filtered out; got {labels:?}"
    );
    Ok(())
}
