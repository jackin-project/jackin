//! End-to-end integration test for the workspace manager TUI.
//! Drives `manager::handle_key` with a scripted key stream — no live
//! terminal.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use jackin::{
    config::{AppConfig, ConfigEditor},
    console::manager::{
        ManagerStage, ManagerState, handle_key,
        state::{EditorState, EditorTab, FieldFocus},
    },
    paths::JackinPaths,
    workspace::{MountConfig, WorkspaceAgentOverride, WorkspaceConfig},
};
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

/// Ctrl-modified key event — used by the Secrets tab's `Ctrl+M` mask
/// toggle tests.
const fn ctrl_key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn seed_config(paths: &JackinPaths, temp_dir: &std::path::Path) -> Result<AppConfig> {
    paths.ensure_base_dirs()?;

    // Use a host path that actually exists (the tempdir) so
    // WorkspaceConfig's workdir-must-equal-or-be-covered-by-mount-dst
    // validation passes.
    let host_path = temp_dir.display().to_string();
    let ws = WorkspaceConfig {
        workdir: host_path.clone(),
        mounts: vec![MountConfig {
            src: host_path.clone(),
            dst: host_path,
            readonly: false,
        }],
        allowed_agents: vec![],
        default_agent: None,
        last_agent: None,
        env: std::collections::BTreeMap::new(),
        agents: std::collections::BTreeMap::new(),
    };

    let mut ce = ConfigEditor::open(paths)?;
    ce.create_workspace("big-monorepo", ws)?;
    ce.save()
}

/// Seed a workspace with pre-populated `env` keys. Used by several Secrets
/// integration tests that need existing env rows to navigate over.
fn seed_config_with_env(
    paths: &JackinPaths,
    temp_dir: &std::path::Path,
    env: Vec<(&str, &str)>,
) -> Result<AppConfig> {
    paths.ensure_base_dirs()?;
    let host_path = temp_dir.display().to_string();
    let env_map: std::collections::BTreeMap<String, String> = env
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let ws = WorkspaceConfig {
        workdir: host_path.clone(),
        mounts: vec![MountConfig {
            src: host_path.clone(),
            dst: host_path,
            readonly: false,
        }],
        allowed_agents: vec![],
        default_agent: None,
        last_agent: None,
        env: env_map,
        agents: std::collections::BTreeMap::new(),
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
        jackin::console::manager::render(f, state, config, cwd);
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
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('y')),
    )?;

    // Config on disk should no longer have big-monorepo.
    let reloaded = AppConfig::load_or_init(&paths)?;
    assert!(
        !reloaded.workspaces.contains_key("big-monorepo"),
        "workspace should be deleted from disk"
    );

    // In-memory state: returned to List, empty workspace list.
    assert!(matches!(state.stage, ManagerStage::List));
    assert!(
        state.workspaces.is_empty(),
        "in-memory list should be empty"
    );

    Ok(())
}

// ── Secrets tab integration tests ─────────────────────────────────

/// Seed a workspace with a `DB_URL` env key, open the Secrets tab, and
/// assert the key is visible in the rendered buffer. Toggles
/// `secrets_masked = false` first so the literal key label shows up
/// cleanly (the label is independent of masking, but we also want the
/// value confirmed visible to pin the render path).
#[test]
fn secrets_tab_shows_existing_env() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let config = seed_config_with_env(
        &paths,
        temp.path(),
        vec![("DB_URL", "postgres://localhost")],
    )?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Unmask so both the key label and literal value are visible in the
    // dump (the masked default would still show `DB_URL` but would hide
    // the value; unmasking makes the assertion more specific).
    editor_mut(&mut state).secrets_masked = false;

    let dump = render_to_dump(&state, &config, cwd);
    assert!(
        dump.contains("DB_URL"),
        "existing env key must be visible on Secrets tab; got:\n{dump}"
    );
    assert!(
        dump.contains("postgres://localhost"),
        "unmasked value must be visible; got:\n{dump}"
    );
    Ok(())
}

/// Edit an existing env value via the TextInput modal, save, and verify
/// the on-disk TOML reflects the new value. The test navigates from the
/// Workspace-env header (row 0) down to the key row (row 1), commits an
/// edit, then drives the `S` → `Enter` save-confirm sequence.
#[test]
fn secrets_edit_value_saves_to_disk() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_env(&paths, temp.path(), vec![("DB_URL", "old-value")])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Secrets flat rows on this fixture:
    //   0 WorkspaceHeader
    //   1 WorkspaceKeyRow("DB_URL")
    //   2 WorkspaceAddSentinel
    // Navigate to row 1.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    assert!(
        matches!(editor(&state).active_field, FieldFocus::Row(1)),
        "cursor must land on the key row after one Down"
    );

    // Enter opens the EnvValue modal pre-filled with the old value.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    // Clear the pre-filled "old-value" (9 chars) and type "new-value".
    for _ in 0..9 {
        handle_key(
            &mut state,
            &mut config,
            &paths,
            cwd,
            key(KeyCode::Backspace),
        )?;
    }
    for ch in "new-value".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    // Commit the TextInput.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    assert_eq!(
        editor(&state).pending.env.get("DB_URL").map(String::as_str),
        Some("new-value"),
        "pending.env must reflect the edit"
    );

    // Kick off the save: `S` opens ConfirmSave, Enter commits.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('S')),
    )?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    let reloaded = AppConfig::load_or_init(&paths)?;
    let ws = reloaded
        .workspaces
        .get("big-monorepo")
        .expect("workspace must still exist");
    assert_eq!(
        ws.env.get("DB_URL").map(String::as_str),
        Some("new-value"),
        "on-disk env must reflect the edit"
    );
    Ok(())
}

/// Delete an existing env key via `D` → `Y`, save, and verify the key is
/// absent on disk.
#[test]
fn secrets_delete_key_saves_to_disk() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_env(&paths, temp.path(), vec![("DB_URL", "doomed")])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Cursor → row 1 (key row).
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    // `D` opens the DeleteEnvVar Confirm modal.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('D')),
    )?;
    // `Y` commits the delete.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('y')),
    )?;

    assert!(
        !editor(&state).pending.env.contains_key("DB_URL"),
        "pending.env must no longer contain the deleted key"
    );

    // `S` → Enter writes through to disk.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('S')),
    )?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    let reloaded = AppConfig::load_or_init(&paths)?;
    let ws = reloaded
        .workspaces
        .get("big-monorepo")
        .expect("workspace must still exist");
    assert!(
        !ws.env.contains_key("DB_URL"),
        "on-disk env must no longer contain the deleted key"
    );
    Ok(())
}

/// `Ctrl+M` flips `secrets_masked` on the editor state. Verifies the
/// default (true), the first toggle (false), and the second toggle
/// (back to true).
#[test]
fn secrets_masking_ctrl_m_toggle() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_env(&paths, temp.path(), vec![("DB_URL", "v")])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    assert!(editor(&state).secrets_masked, "default must be masked=true");
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        ctrl_key(KeyCode::Char('m')),
    )?;
    assert!(
        !editor(&state).secrets_masked,
        "Ctrl+M must flip masked to false"
    );
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        ctrl_key(KeyCode::Char('m')),
    )?;
    assert!(
        editor(&state).secrets_masked,
        "second Ctrl+M must flip masked back to true"
    );
    Ok(())
}

/// Agent-override section starts collapsed; `→` on the header expands it
/// and the agent's env key becomes visible; `←` collapses it back and
/// the key disappears from the buffer.
#[test]
fn secrets_agent_section_expand_collapse() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs()?;
    let host_path = temp.path().display().to_string();

    // Seed a workspace with one agent override. Using `ConfigEditor`
    // keeps the test aligned with the real save path.
    let mut agent_env = std::collections::BTreeMap::new();
    agent_env.insert("LOG_LEVEL".into(), "debug".into());
    let mut agents = std::collections::BTreeMap::new();
    agents.insert(
        "agent-smith".into(),
        WorkspaceAgentOverride { env: agent_env },
    );
    let ws = WorkspaceConfig {
        workdir: host_path.clone(),
        mounts: vec![MountConfig {
            src: host_path.clone(),
            dst: host_path,
            readonly: false,
        }],
        allowed_agents: vec![],
        default_agent: None,
        last_agent: None,
        env: std::collections::BTreeMap::new(),
        agents,
    };
    let mut ce = ConfigEditor::open(&paths)?;
    ce.create_workspace("big-monorepo", ws)?;
    let mut config = ce.save()?;

    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    // Unmask so the key value (`debug`) would show up in the dump if
    // the section were expanded.
    editor_mut(&mut state).secrets_masked = false;

    // Secrets flat rows on this fixture (no workspace-level keys, one
    // collapsed agent section):
    //   0 WorkspaceHeader
    //   1 WorkspaceAddSentinel
    //   2 AgentHeader { agent: "agent-smith", expanded: false }
    // Navigate to row 2.
    for _ in 0..2 {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    }
    assert!(
        matches!(editor(&state).active_field, FieldFocus::Row(2)),
        "cursor must land on the agent header row"
    );

    // Before expansion: LOG_LEVEL must not appear in the render.
    let dump_collapsed = render_to_dump(&state, &config, cwd);
    assert!(
        !dump_collapsed.contains("LOG_LEVEL"),
        "collapsed section must not render agent keys; got:\n{dump_collapsed}"
    );

    // Enter on the agent header expands the section. The plan's key-map
    // lists `→` as an alias for expand here, but Commit 2 only wired
    // Enter (Right-arrow is eaten by the tab-advance handler before the
    // header arm can see it). Verify via Enter, which is the canonical
    // binding that landed.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        editor(&state).secrets_expanded.contains("agent-smith"),
        "Enter on header must expand the agent-smith section"
    );
    let dump_expanded = render_to_dump(&state, &config, cwd);
    assert!(
        dump_expanded.contains("LOG_LEVEL"),
        "expanded section must render agent keys; got:\n{dump_expanded}"
    );

    // Cursor is still on the AgentHeader row (expanded). `←` collapses.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Left))?;
    assert!(
        !editor(&state).secrets_expanded.contains("agent-smith"),
        "← on expanded header must collapse the section"
    );
    let dump_recollapsed = render_to_dump(&state, &config, cwd);
    assert!(
        !dump_recollapsed.contains("LOG_LEVEL"),
        "recollapsed section must hide agent keys; got:\n{dump_recollapsed}"
    );
    Ok(())
}

/// Adding a workspace-level env key via direct mutation flips
/// `is_dirty()` and bumps `change_count()` by 1. Exercises the
/// diff-based state layer that `change_count` relies on (covered here
/// end-to-end alongside the editor plumbing).
#[test]
fn secrets_dirty_detection_and_change_count() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    assert!(!editor(&state).is_dirty());
    assert_eq!(editor(&state).change_count(), 0);

    editor_mut(&mut state)
        .pending
        .env
        .insert("NEW_KEY".into(), "v".into());

    assert!(editor(&state).is_dirty(), "env add must flip is_dirty");
    assert!(
        editor(&state).change_count() >= 1,
        "env add must bump change_count"
    );
    Ok(())
}

/// Two-step Add flow: `A` opens the EnvKey modal, typing + Enter stashes
/// the key and opens the EnvValue modal; typing + Enter commits the
/// value into `pending.env`.
#[test]
fn secrets_add_new_key_flow() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // `A` opens the EnvKey modal. The default cursor is on row 0
    // (WorkspaceHeader) which is a Workspace-scope row — exactly the
    // scope we want for this assertion.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('A')),
    )?;

    // Type "API_KEY" into the EnvKey TextInput.
    for ch in "API_KEY".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    // Commit — stashes the key and opens the EnvValue modal.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    // Type "s3cret" into the EnvValue TextInput and commit.
    for ch in "s3cret".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    assert_eq!(
        editor(&state)
            .pending
            .env
            .get("API_KEY")
            .map(String::as_str),
        Some("s3cret"),
        "pending.env must contain the new key after the two-step add"
    );
    Ok(())
}
