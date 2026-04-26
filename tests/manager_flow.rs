//! End-to-end integration test for the workspace manager TUI.
//! Drives `manager::handle_key` with a scripted key stream — no live
//! terminal.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use jackin::{
    config::{AppConfig, ConfigEditor},
    console::{
        ConsoleStage, ConsoleState,
        manager::{
            ManagerStage, ManagerState, handle_key,
            state::{EditorState, EditorTab, FieldFocus, Modal, TextInputTarget},
        },
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
fn render_to_dump(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) -> String {
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

    // Unmask so the literal value reaches the dump.
    editor_mut(&mut state).unmasked_rows.insert((
        jackin::console::manager::state::SecretsScopeTag::Workspace,
        "DB_URL".into(),
    ));

    let dump = render_to_dump(&mut state, &config, cwd);
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
/// the on-disk TOML reflects the new value. The cursor opens directly on
/// the key row (the Secrets tab no longer renders a preamble label —
/// row 0 is the first navigable row).
#[test]
fn secrets_edit_value_saves_to_disk() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_env(&paths, temp.path(), vec![("DB_URL", "old-value")])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Secrets flat rows on this fixture (no focusable header):
    //   0 WorkspaceKeyRow("DB_URL")
    //   1 WorkspaceAddSentinel
    // Cursor is already on row 0 — no nav needed.
    assert!(
        matches!(editor(&state).active_field, FieldFocus::Row(0)),
        "cursor must open on the first key row"
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

    // Cursor opens on row 0 (the key row — no focusable header above it).
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

/// `M` toggles per-row mask state. The cursor opens on the first key
/// row (`DB_URL`) — pressing M adds it to `unmasked_rows`; pressing M
/// again removes it.
#[test]
fn secrets_masking_m_toggle() -> Result<()> {
    use jackin::console::manager::state::SecretsScopeTag;
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_env(&paths, temp.path(), vec![("DB_URL", "v")])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    assert!(
        editor(&state).unmasked_rows.is_empty(),
        "default must have unmasked_rows empty"
    );
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('m')),
    )?;
    assert!(
        editor(&state)
            .unmasked_rows
            .contains(&(SecretsScopeTag::Workspace, "DB_URL".into())),
        "M must insert focused row into unmasked_rows"
    );
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('m')),
    )?;
    assert!(
        editor(&state).unmasked_rows.is_empty(),
        "second M must remove the focused row from unmasked_rows"
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
        agents,
        ..Default::default()
    };
    let mut ce = ConfigEditor::open(&paths)?;
    ce.create_workspace("big-monorepo", ws)?;
    let mut config = ce.save()?;

    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    // Unmask so the key value (`debug`) would show up in the dump if
    // the section were expanded.
    editor_mut(&mut state).unmasked_rows.insert((
        jackin::console::manager::state::SecretsScopeTag::Agent("agent-smith".into()),
        "LOG_LEVEL".into(),
    ));

    // Secrets flat rows on this fixture (no workspace-level keys, one
    // collapsed agent section; no preamble rows are rendered):
    //   0 WorkspaceAddSentinel
    //   1 SectionSpacer
    //   2 AgentHeader { agent: "agent-smith", expanded: false }
    // Pressing `↓` once skips the SectionSpacer at row 1 and lands on
    // the agent header at row 2.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    assert!(
        matches!(editor(&state).active_field, FieldFocus::Row(2)),
        "cursor must skip SectionSpacer and land on the agent header row; \
         got {:?}",
        editor(&state).active_field
    );

    // Before expansion: LOG_LEVEL must not appear in the render.
    let dump_collapsed = render_to_dump(&mut state, &config, cwd);
    assert!(
        !dump_collapsed.contains("LOG_LEVEL"),
        "collapsed section must not render agent keys; got:\n{dump_collapsed}"
    );

    // `→` on a collapsed agent header expands the section, symmetric with
    // `←` collapsing an expanded header (verified below). Enter on the
    // header also expands; Right is exercised here because it's the binding
    // most likely to be eaten by the tab-advance handler if the guard ever
    // regresses.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    assert!(
        editor(&state).secrets_expanded.contains("agent-smith"),
        "→ on collapsed header must expand the agent-smith section"
    );
    let dump_expanded = render_to_dump(&mut state, &config, cwd);
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
    let dump_recollapsed = render_to_dump(&mut state, &config, cwd);
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

/// Three-step Add flow: `A` opens the EnvKey modal, typing + Enter
/// stashes the key and opens the SourcePicker; Enter on the (default)
/// Plain choice opens the EnvValue modal; typing + Enter commits the
/// value into `pending.env`.
#[test]
fn secrets_add_new_key_flow() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // `A` opens the EnvKey modal. The default cursor is on row 0
    // (the WorkspaceAddSentinel — a Workspace-scope row), exactly the
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
    // Commit — stashes the key and opens the SourcePicker.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::SourcePicker { .. })),
        "EnvKey commit must open SourcePicker; got {:?}",
        editor(&state).modal
    );
    // Default focus is Plain — Enter commits Plain and opens EnvValue.
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
        "pending.env must contain the new key after the three-step add"
    );
    Ok(())
}

// ── 1Password picker integration tests ────────────────────────────

/// Picker may load or fall into a fatal state depending on `op` on
/// `$PATH`; either way the modal variant must be `OpPicker` and
/// `pending_picker_target` must record (scope, key).
#[test]
fn op_picker_opens_on_p_from_secrets_key_row() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_env(&paths, temp.path(), vec![("DB_URL", "postgres")])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    state.op_available = true;

    // Cursor opens directly on row 0 (the WorkspaceKeyRow for DB_URL —
    // no focusable header above it).
    assert!(matches!(editor(&state).active_field, FieldFocus::Row(0)));

    // Press P directly on the key row — no Enter into the text modal first.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('p')),
    )?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::OpPicker { .. })),
        "P on a key row must open the OpPicker modal directly"
    );
    // `pending_picker_target` records the focused key so the commit
    // handler can write straight into pending.env.
    match &editor(&state).pending_picker_target {
        Some((_, Some(key))) => {
            assert_eq!(key, "DB_URL", "key-row P must stash the focused key");
        }
        other => panic!(
            "expected pending_picker_target = Some((scope, Some(\"DB_URL\"))), got {other:?}"
        ),
    }
    Ok(())
}

/// Esc on the `OpPicker` (vault pane / fatal state / loading) closes
/// the modal entirely — the picker is no longer a sub-mode of the
/// EnvValue text modal, so cancel returns the operator to the editor
/// list view with `pending.env` unchanged.
#[test]
fn op_picker_cancel_closes_modal() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_env(&paths, temp.path(), vec![("DB_URL", "untouched")])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    state.op_available = true;

    // Cursor opens on the key row (row 0, no focusable header above);
    // press P to open the picker.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('p')),
    )?;
    assert!(matches!(editor(&state).modal, Some(Modal::OpPicker { .. })));

    // Esc on the OpPicker closes the modal and clears the picker target.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert!(
        editor(&state).modal.is_none(),
        "Esc-cancel must close the picker entirely; got {:?}",
        editor(&state).modal
    );
    assert!(
        editor(&state).pending_picker_target.is_none(),
        "Esc-cancel must clear pending_picker_target"
    );
    assert!(
        editor(&state).pending_picker_value.is_none(),
        "Esc-cancel must clear pending_picker_value"
    );
    // Cancel is a pure UI action — the on-pending env value is unchanged.
    assert_eq!(
        editor(&state).pending.env.get("DB_URL").map(String::as_str),
        Some("untouched"),
        "Esc-cancel must not mutate pending.env"
    );
    Ok(())
}

/// `P` on an existing key row, picker drives to a Field commit — the
/// `op://Vault/Item/Field` reference must land directly in
/// `pending.env[key]` and the modal must close. No follow-up text modal.
#[test]
fn op_picker_commit_writes_value_directly_to_pending() -> Result<()> {
    use jackin::console::widgets::op_picker::{OpLoadState, OpPickerStage};
    use jackin::operator_env::{OpField, OpItem, OpVault};

    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_env(&paths, temp.path(), vec![("DB_URL", "old-value")])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    state.op_available = true;

    // Cursor opens on row 0 (DB_URL key row — no focusable header above);
    // P opens the picker.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('p')),
    )?;

    // Drive the picker straight to the Field stage with seeded vault +
    // item + fields, mirroring what `poll_load` would do after the
    // background loads complete. Same Option-Z direct-field approach
    // the prior commit-7 picker tests use.
    {
        let editor_state = editor_mut(&mut state);
        match &mut editor_state.modal {
            Some(Modal::OpPicker { state: picker }) => {
                picker.vaults = vec![OpVault {
                    id: "v1".into(),
                    name: "Personal".into(),
                }];
                picker.selected_vault = Some(OpVault {
                    id: "v1".into(),
                    name: "Personal".into(),
                });
                picker.items = vec![OpItem {
                    id: "i1".into(),
                    name: "Database".into(),
                    subtitle: String::new(),
                }];
                picker.selected_item = Some(OpItem {
                    id: "i1".into(),
                    name: "Database".into(),
                    subtitle: String::new(),
                });
                picker.fields = vec![OpField {
                    id: "password".into(),
                    label: "password".into(),
                    field_type: "concealed".into(),
                    concealed: true,
                    reference: String::new(),
                }];
                picker.field_list_state.select(Some(0));
                picker.stage = OpPickerStage::Field;
                picker.load_state = OpLoadState::Ready;
            }
            other => panic!("expected OpPicker modal; got {other:?}"),
        }
    }

    // Enter on the Field pane commits the `op://...` reference.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    // The reference must land directly in pending.env (no text modal
    // intermediate).
    assert_eq!(
        editor(&state).pending.env.get("DB_URL").map(String::as_str),
        Some("op://Personal/Database/password"),
        "picker commit must write the op:// reference straight into pending.env[key]"
    );
    assert!(
        editor(&state).modal.is_none(),
        "modal must close after key-row picker commit; got {:?}",
        editor(&state).modal
    );
    assert!(editor(&state).pending_picker_target.is_none());
    assert!(editor(&state).pending_picker_value.is_none());
    Ok(())
}

/// `P` on the `+ Add environment variable` sentinel: the picker commits
/// a path before the operator has named the key. The `EnvKey` modal opens
/// next, the path is held on `pending_picker_value`, and committing the
/// key name writes both into pending.env at once.
#[test]
fn op_picker_sentinel_p_flow() -> Result<()> {
    use jackin::console::widgets::op_picker::{OpLoadState, OpPickerStage};
    use jackin::operator_env::{OpField, OpItem, OpVault};

    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    // Empty env so the only navigable row is WorkspaceAddSentinel at
    // index 0 (no preamble label is rendered).
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    state.op_available = true;

    // Cursor opens on row 0 (WorkspaceAddSentinel).
    assert!(matches!(editor(&state).active_field, FieldFocus::Row(0)));

    // P on the sentinel opens the picker with `pending_picker_target =
    // Some((Workspace, None))`.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('p')),
    )?;
    assert!(matches!(editor(&state).modal, Some(Modal::OpPicker { .. })));
    match &editor(&state).pending_picker_target {
        Some((_, None)) => {}
        other => panic!("sentinel P must stash (scope, None); got pending_picker_target={other:?}"),
    }

    // Drive the picker to a Field commit.
    {
        let editor_state = editor_mut(&mut state);
        match &mut editor_state.modal {
            Some(Modal::OpPicker { state: picker }) => {
                picker.vaults = vec![OpVault {
                    id: "v1".into(),
                    name: "Personal".into(),
                }];
                picker.selected_vault = Some(OpVault {
                    id: "v1".into(),
                    name: "Personal".into(),
                });
                picker.items = vec![OpItem {
                    id: "i1".into(),
                    name: "API Keys".into(),
                    subtitle: String::new(),
                }];
                picker.selected_item = Some(OpItem {
                    id: "i1".into(),
                    name: "API Keys".into(),
                    subtitle: String::new(),
                });
                picker.fields = vec![OpField {
                    id: "credential".into(),
                    label: "credential".into(),
                    field_type: "concealed".into(),
                    concealed: true,
                    reference: String::new(),
                }];
                picker.field_list_state.select(Some(0));
                picker.stage = OpPickerStage::Field;
                picker.load_state = OpLoadState::Ready;
            }
            other => panic!("expected OpPicker modal; got {other:?}"),
        }
    }
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    // After the picker commits: EnvKey modal is open and the path is
    // stashed on pending_picker_value.
    match &editor(&state).modal {
        Some(Modal::TextInput {
            target: TextInputTarget::EnvKey { .. },
            ..
        }) => {}
        other => panic!("expected TextInput(EnvKey) modal; got {other:?}"),
    }
    assert_eq!(
        editor(&state).pending_picker_value.as_deref(),
        Some("op://Personal/API Keys/credential"),
        "picker commit must stash the op:// reference for the EnvKey commit"
    );

    // Type the new key name and Enter — the EnvKey commit handler must
    // consume `pending_picker_value` and write the pair into pending.env.
    for ch in "API_KEY".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    assert_eq!(
        editor(&state)
            .pending
            .env
            .get("API_KEY")
            .map(String::as_str),
        Some("op://Personal/API Keys/credential"),
        "EnvKey commit must write the stashed picker value into pending.env"
    );
    assert!(
        editor(&state).pending_picker_value.is_none(),
        "EnvKey commit must clear pending_picker_value"
    );
    assert!(
        editor(&state).modal.is_none(),
        "EnvKey commit on the picker fast-path must close the modal; got {:?}",
        editor(&state).modal
    );
    Ok(())
}

// ── SourcePicker integration tests ────────────────────────────────

/// Drives Enter-on-sentinel → ScopePicker(All agents) → EnvKey
/// commit, leaving SourcePicker open. Default `op_available = false`.
fn drive_to_source_picker<'a>(
    config: &mut AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    key_name: &str,
) -> Result<ManagerState<'a>> {
    // Build a manager state on the Secrets tab with op_available
    // baked in via the test-only constructor (false by default).
    let mut state = manager_on_secrets_tab(config, cwd);
    // Enter on the workspace sentinel opens the ScopePicker modal —
    // default focus is `AllAgents`, so a second Enter commits the
    // workspace path and opens the EnvKey modal.
    handle_key(&mut state, config, paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, config, paths, cwd, key(KeyCode::Enter))?;
    // Type the key.
    for ch in key_name.chars() {
        handle_key(&mut state, config, paths, cwd, key(KeyCode::Char(ch)))?;
    }
    // Commit the EnvKey — opens SourcePicker.
    handle_key(&mut state, config, paths, cwd, key(KeyCode::Enter))?;
    Ok(state)
}

/// `Enter` on `+ Add` walks: EnvKey → SourcePicker → EnvValue → commit.
/// The SourcePicker is the new step between the existing two text
/// modals. Verifies the `Plain` branch lands the typed value in
/// `pending.env`.
#[test]
fn enter_on_sentinel_opens_envkey_then_sourcepicker_then_value_modal() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = drive_to_source_picker(&mut config, &paths, cwd, "API_KEY")?;
    // SourcePicker is the active modal.
    assert!(
        matches!(editor(&state).modal, Some(Modal::SourcePicker { .. })),
        "EnvKey commit on a sentinel must open SourcePicker; got {:?}",
        editor(&state).modal
    );

    // Default focus is Plain — Enter opens the EnvValue text modal.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    match &editor(&state).modal {
        Some(Modal::TextInput {
            target: TextInputTarget::EnvValue { key, .. },
            ..
        }) => {
            assert_eq!(key, "API_KEY", "EnvValue modal must carry the typed key");
        }
        other => panic!("expected TextInput(EnvValue); got {other:?}"),
    }

    // Type the value and commit.
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
        "Plain-text source path must land the typed value in pending.env"
    );
    Ok(())
}

/// POSIX `VAR=""` differs from `unset VAR`, so EnvValue must commit
/// the empty string into `pending.env`.
#[test]
fn env_value_modal_allows_empty_commit() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = drive_to_source_picker(&mut config, &paths, cwd, "EMPTY_OK")?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::SourcePicker { .. })),
        "expected SourcePicker; got {:?}",
        editor(&state).modal
    );

    // Plain branch → EnvValue modal opens.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(
            editor(&state).modal,
            Some(Modal::TextInput {
                target: TextInputTarget::EnvValue { .. },
                ..
            })
        ),
        "expected EnvValue modal; got {:?}",
        editor(&state).modal
    );

    // Press Enter immediately on an empty textarea — must commit `""`
    // into pending.env. With the previous global non-empty validity
    // rule this Enter was swallowed.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    assert_eq!(
        editor(&state)
            .pending
            .env
            .get("EMPTY_OK")
            .map(String::as_str),
        Some(""),
        "EnvValue modal must allow committing an empty string \
         (POSIX VAR=\"\" semantics)"
    );
    assert!(
        editor(&state).modal.is_none(),
        "EnvValue commit must close the modal; got {:?}",
        editor(&state).modal
    );
    Ok(())
}

/// SourcePicker → 1Password branch: when op is available and the
/// operator picks the Op choice, the OpPicker modal opens with
/// `pending_picker_target = (scope, Some(key))` so its commit handler
/// can write the `op://...` reference straight into the named key.
#[test]
fn enter_on_sentinel_path_to_op_picker() -> Result<()> {
    use jackin::console::widgets::source_picker::SourcePickerState;
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    // Build the state and force `op_available = true` on the
    // SourcePicker so the Op branch is selectable. (The default
    // `from_config` constructor used by `manager_on_secrets_tab` gives
    // op_available=false; this test fakes the available state by
    // injecting the modal directly with op_available=true.)
    let mut state = manager_on_secrets_tab(&config, cwd);
    // Pretend the operator already typed "API_KEY" and committed via
    // the sentinel-add path: stash pending_env_key + open SourcePicker
    // with op_available = true.
    {
        let editor_state = editor_mut(&mut state);
        editor_state.pending_env_key = Some((
            jackin::console::manager::state::SecretsScopeTag::Workspace,
            "API_KEY".into(),
        ));
        editor_state.modal = Some(Modal::SourcePicker {
            state: SourcePickerState::new("API_KEY".into(), true),
        });
    }

    // Right arrow → focus moves to Op. Enter commits Op and opens the
    // OpPicker modal.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    assert!(
        matches!(editor(&state).modal, Some(Modal::OpPicker { .. })),
        "Op branch on SourcePicker must open the OpPicker; got {:?}",
        editor(&state).modal
    );
    match &editor(&state).pending_picker_target {
        Some((_, Some(name))) => {
            assert_eq!(name, "API_KEY", "OpPicker target must carry the typed key");
        }
        other => {
            panic!("expected pending_picker_target = (scope, Some(\"API_KEY\")); got {other:?}")
        }
    }
    // pending_env_key is consumed once OpPicker takes ownership of the
    // (scope, key) pair via pending_picker_target.
    assert!(
        editor(&state).pending_env_key.is_none(),
        "Op branch must consume pending_env_key into pending_picker_target"
    );
    Ok(())
}

/// When `op_available = false`, the Op button on the SourcePicker is
/// disabled: `→`/`Tab` cycling skips it, focus stays on Plain, and the
/// `O` direct hotkey is inert. The picker commits Plain regardless of
/// key flailing.
#[test]
fn source_picker_op_disabled_when_op_missing() -> Result<()> {
    use jackin::console::widgets::source_picker::{SourceChoice, SourcePickerState};
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = manager_on_secrets_tab(&config, cwd);
    {
        let editor_state = editor_mut(&mut state);
        editor_state.pending_env_key = Some((
            jackin::console::manager::state::SecretsScopeTag::Workspace,
            "API_KEY".into(),
        ));
        editor_state.modal = Some(Modal::SourcePicker {
            state: SourcePickerState::new("API_KEY".into(), false),
        });
    }

    // Press Right arrow + Tab — neither must advance focus to Op.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab))?;
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('O')),
    )?;
    match &editor(&state).modal {
        Some(Modal::SourcePicker { state }) => {
            assert_eq!(
                state.focused,
                SourceChoice::Plain,
                "focus must stay on Plain when op_available is false"
            );
        }
        other => panic!("expected SourcePicker still active; got {other:?}"),
    }
    // Enter on the still-Plain focus commits Plain and opens EnvValue.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(
            editor(&state).modal,
            Some(Modal::TextInput {
                target: TextInputTarget::EnvValue { .. },
                ..
            })
        ),
        "Enter on Plain (only available choice) must commit and open EnvValue; got {:?}",
        editor(&state).modal
    );
    Ok(())
}

/// `Esc` on the `SourcePicker` closes the modal and clears
/// `pending_env_key`, `pending_picker_value`. Operator returns to the
/// Secrets tab with no entry added.
#[test]
fn source_picker_esc_clears_pending_state() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = drive_to_source_picker(&mut config, &paths, cwd, "API_KEY")?;
    assert!(matches!(
        editor(&state).modal,
        Some(Modal::SourcePicker { .. })
    ));
    assert!(
        editor(&state).pending_env_key.is_some(),
        "EnvKey commit must stash (scope, key)"
    );

    // Esc cancels.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert!(
        editor(&state).modal.is_none(),
        "Esc on SourcePicker must close the modal; got {:?}",
        editor(&state).modal
    );
    assert!(
        editor(&state).pending_env_key.is_none(),
        "Esc on SourcePicker must clear pending_env_key"
    );
    assert!(
        editor(&state).pending_picker_value.is_none(),
        "Esc on SourcePicker must clear pending_picker_value"
    );
    assert!(
        !editor(&state).pending.env.contains_key("API_KEY"),
        "Esc on SourcePicker must not write any env entry; got {:?}",
        editor(&state).pending.env
    );
    Ok(())
}

/// Drives Account → Vault → Item → Field. Verifies committed `op://`
/// is the bare `op://Vault/Item/Field` form (account scope is not
/// encoded in the path).
#[test]
fn op_picker_multi_account_flow() -> Result<()> {
    use jackin::console::widgets::op_picker::{OpLoadState, OpPickerStage};
    use jackin::operator_env::{OpAccount, OpField, OpItem, OpVault};

    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_env(&paths, temp.path(), vec![("DB_URL", "old")])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    state.op_available = true;

    // Cursor opens on row 0 (DB_URL key row); P opens the picker.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('p')),
    )?;

    // Seed the picker as if the constructor's probe had returned two
    // accounts: stage = Account, accounts populated, none selected yet.
    {
        let editor_state = editor_mut(&mut state);
        match &mut editor_state.modal {
            Some(Modal::OpPicker { state: picker }) => {
                picker.accounts = vec![
                    OpAccount {
                        id: "ACCT_A".into(),
                        email: "alice@example.com".into(),
                        url: "alpha.1password.com".into(),
                    },
                    OpAccount {
                        id: "ACCT_B".into(),
                        email: "bob@example.com".into(),
                        url: "beta.1password.com".into(),
                    },
                ];
                picker.account_list_state.select(Some(1)); // Bob
                picker.selected_account = None;
                picker.stage = OpPickerStage::Account;
                picker.load_state = OpLoadState::Ready;
            }
            other => panic!("expected OpPicker modal; got {other:?}"),
        }
    }

    // Enter on the Account pane — selects Bob, advances to Vault.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    match &editor(&state).modal {
        Some(Modal::OpPicker { state: picker }) => {
            assert_eq!(picker.stage, OpPickerStage::Vault);
            assert_eq!(
                picker.selected_account.as_ref().map(|a| a.id.as_str()),
                Some("ACCT_B"),
                "Enter on Account must capture the selection"
            );
        }
        other => panic!("expected OpPicker modal; got {other:?}"),
    }

    // Seed Vault → Item → Field straight to the commit point.
    {
        let editor_state = editor_mut(&mut state);
        match &mut editor_state.modal {
            Some(Modal::OpPicker { state: picker }) => {
                picker.vaults = vec![OpVault {
                    id: "v1".into(),
                    name: "Shared".into(),
                }];
                picker.selected_vault = Some(OpVault {
                    id: "v1".into(),
                    name: "Shared".into(),
                });
                picker.items = vec![OpItem {
                    id: "i1".into(),
                    name: "Database".into(),
                    subtitle: String::new(),
                }];
                picker.selected_item = Some(OpItem {
                    id: "i1".into(),
                    name: "Database".into(),
                    subtitle: String::new(),
                });
                picker.fields = vec![OpField {
                    id: "password".into(),
                    label: "password".into(),
                    field_type: "concealed".into(),
                    concealed: true,
                    reference: String::new(),
                }];
                picker.field_list_state.select(Some(0));
                picker.stage = OpPickerStage::Field;
                picker.load_state = OpLoadState::Ready;
            }
            other => panic!("expected OpPicker modal; got {other:?}"),
        }
    }

    // Enter on the Field pane commits the path. The committed reference
    // is the simple `op://Vault/Item/Field` form — account scoping is
    // not (yet) embedded in the URL; the launch-time resolver uses the
    // operator's default `op` account context.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert_eq!(
        editor(&state).pending.env.get("DB_URL").map(String::as_str),
        Some("op://Shared/Database/password"),
        "multi-account picker commit must produce a Vault/Item/Field path"
    );
    assert!(editor(&state).modal.is_none());
    Ok(())
}

// ── Modal::AgentPicker dispatch tests ─────────────────────────────

/// Seed a config with a workspace that has `agent_keys.len()` allowed
/// agents (each registered as `config.agents`), optionally a named
/// `default_agent`. Returns the saved `AppConfig`.
fn seed_config_with_agents(
    paths: &JackinPaths,
    temp_dir: &std::path::Path,
    agent_keys: &[&str],
    default_agent: Option<&str>,
) -> Result<AppConfig> {
    paths.ensure_base_dirs()?;
    let host_path = temp_dir.display().to_string();
    let mut config = AppConfig::default();
    for key in agent_keys {
        config.agents.insert(
            (*key).to_string(),
            jackin::config::AgentSource {
                git: format!("https://example.invalid/jackin-{key}.git"),
                trusted: true,
                claude: None,
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
        }],
        allowed_agents: agent_keys.iter().map(|s| (*s).to_string()).collect(),
        default_agent: default_agent.map(String::from),
        ..Default::default()
    };
    let mut ce = ConfigEditor::open(paths)?;
    ce.create_workspace("multi-agent-ws", ws)?;
    ce.save()
}

/// `Enter` on a workspace row with two eligible agents and no default
/// must open `Modal::AgentPicker` overlaid on the manager list — not
/// short-circuit to a launch outcome.
#[test]
fn agent_picker_opens_when_multiple_agents_available() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let config = seed_config_with_agents(
        &paths,
        temp.path(),
        &["chainargos/agent-smith", "chainargos/agent-brown"],
        None,
    )?;
    let cwd = temp.path();
    let mut state = ConsoleState::new(&config, cwd)?;

    let outcome = state.dispatch_launch_for_workspace(
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("multi-agent-ws".into()),
    )?;
    assert!(
        outcome.is_none(),
        "multi-agent dispatch must stay in the run-loop (Ok(None)); got {outcome:?}"
    );
    let ConsoleStage::Manager(ms) = &state.stage;
    match &ms.list_modal {
        Some(Modal::AgentPicker { state: picker }) => {
            assert_eq!(picker.agents.len(), 2);
            assert_eq!(picker.filtered.len(), 2);
        }
        other => panic!("expected Modal::AgentPicker on list_modal; got {other:?}"),
    }
    Ok(())
}

/// `default_agent` set on the workspace must short-circuit the picker
/// and produce an `Ok(Some(_))` direct launch outcome — no modal opens.
#[test]
fn agent_picker_skipped_when_default_agent_set() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let config = seed_config_with_agents(
        &paths,
        temp.path(),
        &["chainargos/agent-smith", "chainargos/agent-brown"],
        Some("chainargos/agent-smith"),
    )?;
    let cwd = temp.path();
    let mut state = ConsoleState::new(&config, cwd)?;

    let outcome = state.dispatch_launch_for_workspace(
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("multi-agent-ws".into()),
    )?;
    let (agent, _ws) = outcome.expect("default_agent must short-circuit to a direct launch");
    assert_eq!(agent.key(), "chainargos/agent-smith");
    let ConsoleStage::Manager(ms) = &state.stage;
    assert!(
        ms.list_modal.is_none(),
        "default_agent dispatch must NOT open the picker; got {:?}",
        ms.list_modal
    );
    Ok(())
}

/// Exactly one eligible agent must short-circuit the picker — same
/// direct-launch outcome as the default-agent path.
#[test]
fn agent_picker_skipped_when_single_eligible_agent() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let config = seed_config_with_agents(&paths, temp.path(), &["chainargos/agent-smith"], None)?;
    let cwd = temp.path();
    let mut state = ConsoleState::new(&config, cwd)?;

    let outcome = state.dispatch_launch_for_workspace(
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("multi-agent-ws".into()),
    )?;
    let (agent, _ws) =
        outcome.expect("single eligible agent must short-circuit to a direct launch");
    assert_eq!(agent.key(), "chainargos/agent-smith");
    let ConsoleStage::Manager(ms) = &state.stage;
    assert!(ms.list_modal.is_none());
    Ok(())
}

/// `Enter` on the picker commits the selected agent — `manager::handle_key`
/// returns `InputOutcome::LaunchWithAgent(agent)` so `run_console` can
/// resolve the workspace and break the event loop.
#[test]
fn agent_picker_enter_commits_launch() -> Result<()> {
    use jackin::console::manager::InputOutcome;

    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_agents(
        &paths,
        temp.path(),
        &["chainargos/agent-smith", "chainargos/agent-brown"],
        None,
    )?;
    let cwd = temp.path();
    let mut state = ConsoleState::new(&config, cwd)?;

    state.dispatch_launch_for_workspace(
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("multi-agent-ws".into()),
    )?;
    let ConsoleStage::Manager(ms) = &mut state.stage;
    assert!(matches!(ms.list_modal, Some(Modal::AgentPicker { .. })));

    // Enter on the picker — selection defaults to index 0
    // (BTreeMap ordering of agent keys).
    let outcome = handle_key(ms, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    match outcome {
        InputOutcome::LaunchWithAgent(agent) => {
            assert!(
                agent.key() == "chainargos/agent-brown" || agent.key() == "chainargos/agent-smith",
                "picker commit must surface one of the two seeded agents; got {}",
                agent.key()
            );
        }
        other => panic!("expected LaunchWithAgent outcome; got {other:?}"),
    }
    assert!(
        ms.list_modal.is_none(),
        "picker commit must close the modal"
    );
    Ok(())
}

/// `Esc` on the picker closes the modal and returns `Continue` — the
/// operator stays on the manager list with an unchanged selection.
#[test]
fn agent_picker_esc_closes_modal() -> Result<()> {
    use jackin::console::manager::InputOutcome;

    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_agents(
        &paths,
        temp.path(),
        &["chainargos/agent-smith", "chainargos/agent-brown"],
        None,
    )?;
    let cwd = temp.path();
    let mut state = ConsoleState::new(&config, cwd)?;

    state.dispatch_launch_for_workspace(
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("multi-agent-ws".into()),
    )?;
    let ConsoleStage::Manager(ms) = &mut state.stage;
    assert!(matches!(ms.list_modal, Some(Modal::AgentPicker { .. })));

    let outcome = handle_key(ms, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert!(
        matches!(outcome, InputOutcome::Continue),
        "Esc on the picker must produce Continue; got {outcome:?}"
    );
    assert!(
        ms.list_modal.is_none(),
        "Esc must close the picker modal; got {:?}",
        ms.list_modal
    );
    Ok(())
}

// ── Duplicate-env-key live-validation tests ──────────────────────────

/// Operator opens the `EnvKey` modal on a workspace where `EXISTING`
/// already lives in `pending.env` and types the same name. Enter must
/// be swallowed (modal stays open, `EnvKey` target intact) and the
/// pre-existing value in `pending.env["EXISTING"]` must be unchanged.
#[test]
fn env_key_modal_blocks_duplicate_workspace_key() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_env(&paths, temp.path(), vec![("EXISTING", "kept-value")])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Rows: 0 WorkspaceKeyRow("EXISTING"), 1 WorkspaceAddSentinel.
    // Navigate to the sentinel; Enter opens the ScopePicker, then a
    // second Enter (default focus = AllAgents) opens the EnvKey modal.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(
            editor(&state).modal,
            Some(Modal::TextInput {
                target: TextInputTarget::EnvKey { .. },
                ..
            })
        ),
        "ScopePicker(AllAgents) commit must open the EnvKey modal; got {:?}",
        editor(&state).modal
    );

    // Type the colliding key.
    for ch in "EXISTING".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    let env_size_before = editor(&state).pending.env.len();
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    // Modal must still be the EnvKey TextInput (Enter was swallowed).
    assert!(
        matches!(
            editor(&state).modal,
            Some(Modal::TextInput {
                target: TextInputTarget::EnvKey { .. },
                ..
            })
        ),
        "Enter on a duplicate key must leave the EnvKey modal open; got {:?}",
        editor(&state).modal
    );
    assert_eq!(
        editor(&state).pending.env.len(),
        env_size_before,
        "duplicate Enter must not add an env entry"
    );
    assert_eq!(
        editor(&state)
            .pending
            .env
            .get("EXISTING")
            .map(String::as_str),
        Some("kept-value"),
        "the pre-existing value must remain untouched"
    );
    Ok(())
}

/// Same guard, agent-override scope. Seed a workspace with one agent
/// override `LOG_LEVEL`, expand the section, navigate to its `+ Add`
/// sentinel, and type the colliding key.
#[test]
fn env_key_modal_blocks_duplicate_agent_key() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs()?;
    let host_path = temp.path().display().to_string();

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
        agents,
        ..Default::default()
    };
    let mut ce = ConfigEditor::open(&paths)?;
    ce.create_workspace("big-monorepo", ws)?;
    let mut config = ce.save()?;

    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Rows on this fixture (no workspace keys, one collapsed agent section):
    //   0 WorkspaceAddSentinel
    //   1 SectionSpacer  (skipped by ↑/↓)
    //   2 AgentHeader { agent: "agent-smith", expanded: false }
    // Expand the agent section so the AgentAddSentinel row exists.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    assert!(editor(&state).secrets_expanded.contains("agent-smith"));

    // After expansion:
    //   0 WorkspaceAddSentinel
    //   1 SectionSpacer
    //   2 AgentHeader (expanded)
    //   3 AgentKeyRow { agent: "agent-smith", key: "LOG_LEVEL" }
    //   4 AgentAddSentinel("agent-smith")
    // Navigate to row 4.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;

    // Enter to open the EnvKey modal for the agent scope.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(matches!(
        editor(&state).modal,
        Some(Modal::TextInput {
            target: TextInputTarget::EnvKey { .. },
            ..
        })
    ));

    for ch in "LOG_LEVEL".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    // The modal must still be open, and the agent's env unchanged.
    assert!(
        matches!(
            editor(&state).modal,
            Some(Modal::TextInput {
                target: TextInputTarget::EnvKey { .. },
                ..
            })
        ),
        "Enter on a duplicate agent key must leave the EnvKey modal open; got {:?}",
        editor(&state).modal
    );
    let agent_entry = editor(&state)
        .pending
        .agents
        .get("agent-smith")
        .expect("agent override must survive");
    assert_eq!(agent_entry.env.len(), 1);
    assert_eq!(
        agent_entry.env.get("LOG_LEVEL").map(String::as_str),
        Some("debug"),
        "pre-existing agent value must remain untouched"
    );
    Ok(())
}

/// Regression: the duplicate guard does not block a unique name. Type
/// `NEW_KEY` at a workspace whose `pending.env` already contains
/// `EXISTING` and confirm the `EnvKey` modal commits (transitions to
/// `SourcePicker`, the next stage in the unified add flow).
#[test]
fn env_key_modal_allows_unique_name() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_env(&paths, temp.path(), vec![("EXISTING", "kept-value")])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Sentinel is row 1 here (row 0 is the existing key row). The
    // first Enter opens the ScopePicker; the second commits AllAgents
    // and opens the EnvKey modal.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    for ch in "NEW_KEY".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    // Unique name commits — flow advances to the SourcePicker.
    assert!(
        matches!(editor(&state).modal, Some(Modal::SourcePicker { .. })),
        "unique key must commit and open the SourcePicker; got {:?}",
        editor(&state).modal
    );
    Ok(())
}

// ── ScopePicker + AgentOverridePicker integration tests ──────────────

/// Seed the on-disk config + workspace for the override-picker tests.
/// Adds a workspace with the given `allowed_agents` (each also
/// registered as an `AgentSource`), and pre-populates `pending` per-
/// agent overrides for any names in `with_overrides`.
fn seed_override_picker_workspace(
    paths: &JackinPaths,
    temp_dir: &std::path::Path,
    allowed: &[&str],
    with_overrides: &[&str],
) -> Result<AppConfig> {
    paths.ensure_base_dirs()?;
    let host_path = temp_dir.display().to_string();
    let mut config = AppConfig::default();
    for name in allowed {
        config.agents.insert(
            (*name).to_string(),
            jackin::config::AgentSource {
                git: format!("https://example.invalid/{name}.git"),
                trusted: true,
                claude: None,
                env: std::collections::BTreeMap::new(),
            },
        );
    }
    let toml = toml::to_string(&config)?;
    std::fs::write(&paths.config_file, toml)?;

    let mut agents_map = std::collections::BTreeMap::new();
    for name in with_overrides {
        let mut env = std::collections::BTreeMap::new();
        env.insert("LOG_LEVEL".into(), "debug".into());
        agents_map.insert((*name).into(), WorkspaceAgentOverride { env });
    }

    let ws = WorkspaceConfig {
        workdir: host_path.clone(),
        mounts: vec![MountConfig {
            src: host_path.clone(),
            dst: host_path,
            readonly: false,
        }],
        allowed_agents: allowed.iter().map(|s| (*s).to_string()).collect(),
        agents: agents_map,
        ..Default::default()
    };
    let mut ce = ConfigEditor::open(paths)?;
    ce.create_workspace("big-monorepo", ws)?;
    ce.save()
}

/// Press `Enter` on the workspace-level `+ Add environment variable`
/// sentinel: the `Modal::ScopePicker` opens on the editor stage so the
/// operator can pick "All agents" or "Specific agent" before falling
/// into the rest of the add flow.
#[test]
fn sentinel_enter_opens_scope_picker() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Rows: [WorkspaceAddSentinel]. Cursor opens on row 0.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::ScopePicker { .. })),
        "Enter on the workspace sentinel must open the ScopePicker; got {:?}",
        editor(&state).modal
    );
    Ok(())
}

/// ScopePicker → AllAgents path: pick the default-focused choice and
/// confirm the EnvKey modal opens with `Workspace` scope.
#[test]
fn scope_picker_all_path_to_workspace_envkey() -> Result<()> {
    use jackin::console::manager::state::SecretsScopeTag;
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Open the ScopePicker; default focus is AllAgents — Enter commits.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    match &editor(&state).modal {
        Some(Modal::TextInput { target, .. }) => match target {
            TextInputTarget::EnvKey { scope } => {
                assert_eq!(
                    scope,
                    &SecretsScopeTag::Workspace,
                    "AllAgents commit must open the EnvKey modal with Workspace scope"
                );
            }
            other => panic!("expected TextInputTarget::EnvKey; got {other:?}"),
        },
        other => panic!("expected Modal::TextInput; got {other:?}"),
    }
    Ok(())
}

/// ScopePicker → SpecificAgent → AgentPicker → EnvKey path. Verifies
/// each transition: ScopePicker right-arrow → SpecificAgent focus,
/// Enter → AgentOverridePicker; picker Enter → EnvKey with `Agent`
/// scope. `pending.agents` is NOT mutated — the section materialises
/// organically once the first key/value commits.
#[test]
fn scope_picker_specific_path_to_agent_picker_then_envkey() -> Result<()> {
    use jackin::console::manager::state::SecretsScopeTag;
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Open the ScopePicker, advance focus to SpecificAgent, commit.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    // The agent-override picker is now open with the eligible set.
    match &editor(&state).modal {
        Some(Modal::AgentOverridePicker { state: picker }) => {
            assert_eq!(picker.agents.len(), 1);
            assert_eq!(picker.agents[0].key(), "agent-smith");
        }
        other => panic!("expected Modal::AgentOverridePicker; got {other:?}"),
    }

    // Commit the only eligible agent — the EnvKey modal opens with
    // `Agent(<name>)` scope and `pending.agents` stays empty.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        editor(&state).pending.agents.is_empty(),
        "picker commit must not create an override entry; got pending.agents keys={:?}",
        editor(&state).pending.agents.keys().collect::<Vec<_>>()
    );
    assert!(
        editor(&state).secrets_expanded.is_empty(),
        "picker commit must not pre-expand any section"
    );
    match &editor(&state).modal {
        Some(Modal::TextInput { target, .. }) => match target {
            TextInputTarget::EnvKey { scope } => {
                assert_eq!(
                    scope,
                    &SecretsScopeTag::Agent("agent-smith".into()),
                    "EnvKey modal must scope to the picked agent"
                );
            }
            other => panic!("expected TextInputTarget::EnvKey; got {other:?}"),
        },
        other => panic!("expected Modal::TextInput; got {other:?}"),
    }
    Ok(())
}

/// Agents already carrying an override are not filtered out — the
/// operator may want to add more keys.
#[test]
fn agent_picker_lists_all_allowed_agents_not_filtered_by_existing_overrides() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(
        &paths,
        temp.path(),
        &["agent-smith", "agent-brown"],
        &["agent-smith"],
    )?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Workspace sentinel → ScopePicker → SpecificAgent → AgentPicker.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    match &editor(&state).modal {
        Some(Modal::AgentOverridePicker { state: picker }) => {
            let mut keys: Vec<String> = picker.agents.iter().map(|a| a.key()).collect();
            keys.sort();
            assert_eq!(
                keys,
                vec!["agent-brown".to_string(), "agent-smith".to_string()],
                "agent-smith already has an override section but must still appear so the operator can add another key"
            );
        }
        other => panic!("expected Modal::AgentOverridePicker; got {other:?}"),
    }
    Ok(())
}

/// Esc on the ScopePicker closes the modal and leaves
/// `pending.agents` and `pending.env` untouched — backing out is a
/// pure no-op.
#[test]
fn cancel_from_scope_picker_returns_to_secrets_tab() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(matches!(
        editor(&state).modal,
        Some(Modal::ScopePicker { .. })
    ));

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert!(
        editor(&state).modal.is_none(),
        "Esc on the ScopePicker must close the modal; got {:?}",
        editor(&state).modal
    );
    assert!(
        editor(&state).pending.agents.is_empty(),
        "Esc must not create an override entry"
    );
    assert!(
        editor(&state).pending.env.is_empty(),
        "Esc must not write any env entry"
    );
    Ok(())
}

/// Esc on the EnvKey modal that opens after the picker commit must
/// leave `pending.agents` untouched — no orphan empty section.
#[test]
fn cancel_from_envkey_after_agent_pick_does_not_create_section() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Workspace sentinel → ScopePicker → SpecificAgent → AgentPicker
    // → commit → EnvKey modal opens.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(matches!(
        editor(&state).modal,
        Some(Modal::TextInput {
            target: TextInputTarget::EnvKey { .. },
            ..
        })
    ));

    // Esc on the EnvKey modal.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert!(
        editor(&state).modal.is_none(),
        "Esc must close the EnvKey modal; got {:?}",
        editor(&state).modal
    );
    assert!(
        editor(&state).pending.agents.is_empty(),
        "Esc on EnvKey must not have created an override entry; got keys={:?}",
        editor(&state).pending.agents.keys().collect::<Vec<_>>()
    );
    assert!(
        editor(&state).secrets_expanded.is_empty(),
        "Esc on EnvKey must not have expanded any section"
    );
    Ok(())
}

/// Esc on the SourcePicker modal that opens after a valid EnvKey commit
/// must also leave `pending.agents` untouched. Mirrors the EnvKey
/// cancel test one step deeper in the chain.
#[test]
fn cancel_from_sourcepicker_after_agent_pick_does_not_create_section() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Workspace sentinel → ScopePicker → SpecificAgent → AgentPicker
    // → commit → EnvKey modal.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    // Type a valid key name and commit → SourcePicker modal opens.
    for ch in "API_TOKEN".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::SourcePicker { .. })),
        "EnvKey commit must open SourcePicker; got {:?}",
        editor(&state).modal
    );

    // Esc on the SourcePicker.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert!(
        editor(&state).modal.is_none(),
        "Esc must close the SourcePicker; got {:?}",
        editor(&state).modal
    );
    assert!(
        editor(&state).pending.agents.is_empty(),
        "Esc on SourcePicker must not have created an override entry; got keys={:?}",
        editor(&state).pending.agents.keys().collect::<Vec<_>>()
    );
    assert!(
        editor(&state).secrets_expanded.is_empty(),
        "Esc on SourcePicker must not have expanded any section"
    );
    Ok(())
}

/// Drive the full chain — workspace sentinel → ScopePicker(Specific) →
/// AgentPicker → EnvKey → SourcePicker(Plain) → EnvValue → commit. The
/// override section materialises only on this final commit, with the
/// key/value present and the section expanded.
#[test]
fn completing_value_after_agent_pick_creates_section_with_one_var() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Workspace sentinel → ScopePicker → SpecificAgent → AgentPicker
    // → commit → EnvKey modal.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    // Type the key, commit → SourcePicker.
    for ch in "API_TOKEN".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(matches!(
        editor(&state).modal,
        Some(Modal::SourcePicker { .. })
    ));

    // SourcePicker default selection is Plain — Enter commits → EnvValue.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(
            editor(&state).modal,
            Some(Modal::TextInput {
                target: TextInputTarget::EnvValue { .. },
                ..
            })
        ),
        "SourcePicker(Plain) commit must open EnvValue; got {:?}",
        editor(&state).modal
    );

    // Type the value and commit.
    for ch in "secret".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    // The override section now exists with the single key/value.
    let agents = &editor(&state).pending.agents;
    assert!(
        agents.contains_key("agent-smith"),
        "pending.agents must contain the chosen agent; got keys={:?}",
        agents.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        agents.get("agent-smith").unwrap().env.get("API_TOKEN"),
        Some(&"secret".to_string()),
        "the committed key/value must land in the agent's env map"
    );
    // The section must be auto-expanded.
    assert!(
        editor(&state).secrets_expanded.contains("agent-smith"),
        "value commit must auto-expand the agent's section"
    );
    // All modals closed.
    assert!(
        editor(&state).modal.is_none(),
        "value commit must close every modal; got {:?}",
        editor(&state).modal
    );
    Ok(())
}

/// Esc on the override picker (reachable through the ScopePicker's
/// SpecificAgent path) closes the modal and leaves `pending.agents`
/// untouched — symmetric with the ScopePicker-cancel test, one step
/// deeper.
#[test]
fn cancel_from_agent_override_picker_after_scope_pick_does_not_create_section() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Open the picker via ScopePicker → SpecificAgent.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(matches!(
        editor(&state).modal,
        Some(Modal::AgentOverridePicker { .. })
    ));

    // Esc — the modal closes and pending.agents stays empty.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert!(
        editor(&state).modal.is_none(),
        "Esc must close the override picker; got {:?}",
        editor(&state).modal
    );
    assert!(
        editor(&state).pending.agents.is_empty(),
        "Esc must not create an override entry; got pending.agents keys={:?}",
        editor(&state).pending.agents.keys().collect::<Vec<_>>()
    );
    assert!(
        editor(&state).secrets_expanded.is_empty(),
        "Esc must not expand any section"
    );
    Ok(())
}

/// Once a key has landed in an agent's section, the in-section
/// `+ Add <agent> environment variable` sentinel must remain a direct
/// fast-path to the EnvKey modal — the ScopePicker only intercedes at
/// the workspace-level sentinel.
#[test]
fn in_section_agent_sentinel_skips_scope_picker() -> Result<()> {
    use jackin::console::manager::state::SecretsScopeTag;
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config =
        seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &["agent-smith"])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    // Pre-expand the agent so its in-section sentinel is reachable.
    editor_mut(&mut state)
        .secrets_expanded
        .insert("agent-smith".into());

    // Rows now: [WorkspaceAddSentinel, SectionSpacer,
    //            AgentHeader(expanded), AgentKeyRow(LOG_LEVEL),
    //            AgentAddSentinel]. The first `↓` skips the
    //            non-focusable SectionSpacer at row 1 and lands on the
    //            AgentHeader at row 2; subsequent `↓` presses walk to
    //            the AgentAddSentinel at row 4.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;

    // Enter — the EnvKey modal opens directly with `Agent` scope. No
    // ScopePicker intercedes here.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    match &editor(&state).modal {
        Some(Modal::TextInput { target, .. }) => match target {
            TextInputTarget::EnvKey { scope } => {
                assert_eq!(
                    scope,
                    &SecretsScopeTag::Agent("agent-smith".into()),
                    "in-section sentinel must open EnvKey with the contextual Agent scope"
                );
            }
            other => panic!("expected TextInputTarget::EnvKey; got {other:?}"),
        },
        other => panic!(
            "expected Modal::TextInput on in-section sentinel; got {other:?} (ScopePicker must not intercede)"
        ),
    }
    Ok(())
}

// ── Agent-section header absorbs ←/→ regardless of expanded state ──
//
// Bug: pressing → on an *already-expanded* header (or ← on a *collapsed*
// one) used to fall through to the tab-cycle handler — operators saw the
// active editor tab change unexpectedly when arrowing on
// `▼ Agent: <name>` / `▶ Agent: <name>`. The header now absorbs ←/→ in
// both states; semantics codified in RULES.md
// § "TUI Keybindings → Contextual key absorption".

/// `→` on a collapsed agent header expands the section AND leaves the
/// active tab on Secrets. Already covered by
/// `secrets_agent_section_expand_collapse`, but pinned here as a focused
/// regression guard for the contextual-absorption rule.
#[test]
fn right_on_collapsed_agent_header_expands_does_not_change_tab() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config =
        seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &["agent-smith"])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    // ↓ from row 0 (WorkspaceAddSentinel) skips the SectionSpacer and
    // lands on the AgentHeader at row 2.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    assert!(matches!(editor(&state).active_field, FieldFocus::Row(2)));
    assert!(
        !editor(&state).secrets_expanded.contains("agent-smith"),
        "section must start collapsed"
    );

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;

    assert!(
        editor(&state).secrets_expanded.contains("agent-smith"),
        "→ on collapsed header must expand the section"
    );
    assert_eq!(
        editor(&state).active_tab,
        EditorTab::Secrets,
        "→ on header must NOT advance the active tab"
    );
    Ok(())
}

/// `→` on an *already-expanded* agent header is a no-op — but it must
/// still be absorbed. The active tab must not change. This is the exact
/// regression the operator reported.
#[test]
fn right_on_expanded_agent_header_does_nothing_does_not_change_tab() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config =
        seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &["agent-smith"])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    // Pre-expand the section so we exercise the "already expanded" path.
    editor_mut(&mut state)
        .secrets_expanded
        .insert("agent-smith".into());

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;

    assert!(
        editor(&state).secrets_expanded.contains("agent-smith"),
        "→ on expanded header must leave the section expanded (no-op on the section)"
    );
    assert_eq!(
        editor(&state).active_tab,
        EditorTab::Secrets,
        "→ on expanded header must NOT advance the active tab"
    );
    Ok(())
}

/// `←` on an expanded agent header collapses the section. Active tab
/// stays put.
#[test]
fn left_on_expanded_agent_header_collapses_does_not_change_tab() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config =
        seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &["agent-smith"])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    editor_mut(&mut state)
        .secrets_expanded
        .insert("agent-smith".into());

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Left))?;

    assert!(
        !editor(&state).secrets_expanded.contains("agent-smith"),
        "← on expanded header must collapse the section"
    );
    assert_eq!(
        editor(&state).active_tab,
        EditorTab::Secrets,
        "← on header must NOT rewind the active tab"
    );
    Ok(())
}

/// `←` on an *already-collapsed* agent header is a no-op — but it must
/// still be absorbed. The active tab must not change. Mirror of the
/// `→`-on-expanded case.
#[test]
fn left_on_collapsed_agent_header_does_nothing_does_not_change_tab() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config =
        seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &["agent-smith"])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    // Section starts collapsed; don't expand it.
    assert!(!editor(&state).secrets_expanded.contains("agent-smith"));

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Left))?;

    assert!(
        !editor(&state).secrets_expanded.contains("agent-smith"),
        "← on collapsed header must leave the section collapsed"
    );
    assert_eq!(
        editor(&state).active_tab,
        EditorTab::Secrets,
        "← on collapsed header must NOT rewind the active tab"
    );
    Ok(())
}

/// Regression: `Tab` is intentionally *not* absorbed. Even when focused
/// on an `AgentHeader`, pressing `Tab` advances to the next editor tab.
/// Only `←` and `→` are owned by the header — `Tab` is the canonical
/// tab-cycle key and never participates in contextual absorption.
#[test]
fn tab_on_agent_header_advances_tab_normally() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config =
        seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &["agent-smith"])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    // Cursor on AgentHeader (row 2 — ↓ from row 0 skips the spacer).
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    assert!(matches!(editor(&state).active_field, FieldFocus::Row(2)));

    // `Tab` from Secrets must wrap to General regardless of focus.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab))?;
    assert_eq!(
        editor(&state).active_tab,
        EditorTab::General,
        "Tab on a header must still advance the active tab"
    );
    Ok(())
}

// ── Launch-routing regression tests for commit 53 ────────────────────
//
// PR #171 review (Codex, Section 1: High): `ConsoleState.workspaces`
// was a snapshot built once at console startup. Manager edits
// (create / rename / edit / delete) rebuilt `ManagerState.workspaces`
// after each save but never refreshed the console-level snapshot, so
// the launch dispatcher kept reading stale data.
//
// Fix (Option B): drop the snapshot entirely and have
// `dispatch_launch_for_workspace` build a fresh `WorkspaceChoice` from
// the current `AppConfig` per call (`build_workspace_choice` in
// `console::state`). These tests pin the four operator-visible failure
// modes the bug produced — each would have failed against the
// pre-fix code.

/// A workspace created via `ConfigEditor` after the `ConsoleState` has
/// been built must be resolvable by the launch dispatcher in the same
/// session. Under the stale-snapshot bug, the new name was absent from
/// `ConsoleState.workspaces` and the dispatcher returned `Ok(None)`.
#[test]
fn launch_after_create_workspace_uses_fresh_data() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config =
        seed_config_with_agents(&paths, temp.path(), &["chainargos/agent-smith"], None)?;
    let cwd = temp.path();

    // Build the console state BEFORE the new workspace exists.
    let mut state = ConsoleState::new(&config, cwd)?;

    // Now create a second workspace via ConfigEditor — same code path
    // the manager save flow uses (no CLI subprocess, no UX detour).
    let host_path = cwd.display().to_string();
    let new_ws = WorkspaceConfig {
        workdir: host_path.clone(),
        mounts: vec![MountConfig {
            src: host_path.clone(),
            dst: host_path,
            readonly: false,
        }],
        allowed_agents: vec!["chainargos/agent-smith".to_string()],
        default_agent: Some("chainargos/agent-smith".to_string()),
        ..Default::default()
    };
    {
        let mut ce = ConfigEditor::open(&paths)?;
        ce.create_workspace("freshly-created", new_ws)?;
        config = ce.save()?;
    }

    // Dispatch a launch against the freshly-created name. With the bug,
    // `ConsoleState.workspaces` would not contain "freshly-created" and
    // the dispatcher would return Ok(None). With the fix, the dispatcher
    // builds the choice from the current `config` and short-circuits on
    // the single eligible agent.
    let outcome = state.dispatch_launch_for_workspace(
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("freshly-created".into()),
    )?;
    let (agent, _ws) = outcome.expect(
        "freshly-created workspace must resolve through the dispatcher; under the bug, \
         ConsoleState.workspaces was a startup snapshot and didn't include the new name",
    );
    assert_eq!(agent.key(), "chainargos/agent-smith");
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

    let mut state = ConsoleState::new(&config, cwd)?;

    // Rename "multi-agent-ws" → "renamed-ws" via ConfigEditor.
    {
        let mut ce = ConfigEditor::open(&paths)?;
        ce.rename_workspace("multi-agent-ws", "renamed-ws")?;
        config = ce.save()?;
    }

    // Dispatch against the new name — must resolve and short-circuit
    // (single eligible agent + default_agent set).
    let outcome = state.dispatch_launch_for_workspace(
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("renamed-ws".into()),
    )?;
    let (agent, _ws) = outcome.expect("renamed workspace must resolve under the new name");
    assert_eq!(agent.key(), "chainargos/agent-smith");

    // OLD name must not resolve — under the snapshot bug it did.
    let stale_outcome = state.dispatch_launch_for_workspace(
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("multi-agent-ws".into()),
    )?;
    assert!(
        stale_outcome.is_none(),
        "the old (renamed-away) name must not resolve to a launch outcome; got {stale_outcome:?}"
    );
    Ok(())
}

/// Post-edit `default_agent` must short-circuit dispatch from picker
/// to direct launch.
#[test]
fn launch_after_default_agent_change_uses_new_default() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_agents(
        &paths,
        temp.path(),
        &["chainargos/agent-smith", "chainargos/agent-brown"],
        // No default_agent — two eligible agents → picker would open.
        None,
    )?;
    let cwd = temp.path();

    let mut state = ConsoleState::new(&config, cwd)?;

    // Confirm baseline: dispatch against the seeded workspace opens the
    // picker (no short-circuit).
    let baseline = state.dispatch_launch_for_workspace(
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("multi-agent-ws".into()),
    )?;
    assert!(
        baseline.is_none(),
        "baseline (no default_agent) must open the picker, not direct-launch"
    );
    {
        let ConsoleStage::Manager(ms) = &mut state.stage;
        // Close the picker so the next dispatch can reopen / short-circuit.
        ms.list_modal = None;
    }
    state.pending_launch = None;

    // Now set default_agent via ConfigEditor (same path the manager's
    // save flow drives via WorkspaceEdit { default_agent: Some(_), .. }).
    {
        let mut ce = ConfigEditor::open(&paths)?;
        let mut edit = jackin::workspace::WorkspaceEdit::default();
        edit.default_agent = Some(Some("chainargos/agent-smith".to_string()));
        ce.edit_workspace("multi-agent-ws", edit)?;
        config = ce.save()?;
    }

    // Dispatch again — with the new default_agent in config, the
    // dispatcher must short-circuit to a direct launch outcome.
    let after = state.dispatch_launch_for_workspace(
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("multi-agent-ws".into()),
    )?;
    let (agent, _ws) = after.expect(
        "after default_agent is set, dispatch must short-circuit to a direct launch outcome; \
         under the bug, the snapshot's default_agent: None forced the picker open",
    );
    assert_eq!(agent.key(), "chainargos/agent-smith");
    let ConsoleStage::Manager(ms) = &state.stage;
    assert!(
        ms.list_modal.is_none(),
        "post-default direct-launch must NOT have opened the picker; got {:?}",
        ms.list_modal
    );
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
        }],
        allowed_agents: vec!["chainargos/agent-smith".to_string()],
        default_agent: Some("chainargos/agent-smith".to_string()),
        ..Default::default()
    };
    let mut config = {
        let mut ce = ConfigEditor::open(&paths)?;
        ce.create_workspace("survivor-ws", second)?;
        ce.save()?
    };
    let cwd = temp.path();

    let mut state = ConsoleState::new(&config, cwd)?;

    // Delete the first workspace via ConfigEditor.
    {
        let mut ce = ConfigEditor::open(&paths)?;
        ce.remove_workspace("multi-agent-ws")?;
        config = ce.save()?;
    }

    // Attempt a launch against the deleted name — must no-op.
    let outcome = state.dispatch_launch_for_workspace(
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("multi-agent-ws".into()),
    )?;
    assert!(
        outcome.is_none(),
        "deleted workspace must not resolve to a launch outcome; under the bug, \
         the snapshot retained it and the dispatcher would have launched a ghost; \
         got {outcome:?}"
    );

    // Sanity: the surviving workspace still resolves.
    let alive = state.dispatch_launch_for_workspace(
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("survivor-ws".into()),
    )?;
    let (agent, _ws) = alive.expect("survivor-ws must still resolve");
    assert_eq!(agent.key(), "chainargos/agent-smith");
    Ok(())
}
