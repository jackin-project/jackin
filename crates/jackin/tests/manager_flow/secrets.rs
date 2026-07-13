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

use super::*;
use jackin::console::tui::state::{SecretsPickerTarget, TextInputTarget};
use jackin_core::WorkspaceName;

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
        jackin::console::tui::state::SecretsScopeTag::Workspace,
        "DB_URL".into(),
    ));

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

/// Edit an existing env value via the `TextInput` modal, save, and verify
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
        editor(&state)
            .pending
            .env
            .get("DB_URL")
            .map(jackin_core::EnvValue::as_persisted_str),
        Some("new-value"),
        "pending.env must reflect the edit"
    );

    // Kick off the save: `S` opens ConfirmSave; Tab moves Cancel -> Save,
    // Enter commits.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('S')),
    )?;
    // Default focus = Cancel (TUI design decisions: confirmation dialog rule).
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    mark_pending_save_drift_checked_for_test(&mut state);
    execute_pending_workspace_save_commit(&mut state, &mut config, &paths, cwd)?;
    wait_for_config_save(&mut state, &mut config, &paths, cwd)?;

    let reloaded = AppConfig::load_or_init(&paths)?;
    let ws = reloaded
        .workspaces
        .get("big-monorepo")
        .expect("workspace must still exist");
    assert_eq!(
        ws.env
            .get("DB_URL")
            .map(jackin_core::EnvValue::as_persisted_str),
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

    // `S` opens ConfirmSave; Tab moves Cancel -> Save, Enter commits.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('S')),
    )?;
    // Default focus = Cancel (TUI design decisions: confirmation dialog rule).
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    mark_pending_save_drift_checked_for_test(&mut state);
    execute_pending_workspace_save_commit(&mut state, &mut config, &paths, cwd)?;
    wait_for_config_save(&mut state, &mut config, &paths, cwd)?;

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
    use jackin::console::tui::state::SecretsScopeTag;
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

/// Role-override section starts collapsed; `→` on the header expands it
/// and the role's env key becomes visible; `←` collapses it back and
/// the key disappears from the buffer.
#[test]
fn secrets_agent_section_expand_collapse() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs()?;
    let host_path = temp.path().display().to_string();

    // Seed a workspace with one role override. Using `ConfigEditor`
    // keeps the test aligned with the real save path.
    let mut role_env = std::collections::BTreeMap::new();
    role_env.insert(
        "LOG_LEVEL".into(),
        jackin_core::EnvValue::Plain("debug".into()),
    );
    let mut roles = std::collections::BTreeMap::new();
    roles.insert(
        "agent-smith".into(),
        WorkspaceRoleOverride {
            env: role_env,
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            grok: None,
            github: None,
        },
    );
    let ws = WorkspaceConfig {
        workdir: host_path.clone(),
        mounts: vec![MountConfig {
            src: host_path.clone(),
            dst: host_path,
            readonly: false,
            isolation: jackin_core::MountIsolation::Shared,
        }],
        roles,
        ..Default::default()
    };
    let mut ce = ConfigEditor::open(&paths)?;
    ce.create_workspace(&WorkspaceName::parse("big-monorepo").unwrap(), ws)?;
    let mut config = ce.save()?;

    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    // Unmask so the key value (`debug`) would show up in the dump if
    // the section were expanded.
    editor_mut(&mut state).unmasked_rows.insert((
        jackin::console::tui::state::SecretsScopeTag::Role("agent-smith".into()),
        "LOG_LEVEL".into(),
    ));

    // Secrets flat rows on this fixture (no workspace-level keys, one
    // collapsed role section; no preamble rows are rendered):
    //   0 WorkspaceAddSentinel
    //   1 SectionSpacer
    //   2 AgentHeader { role: "agent-smith", expanded: false }
    // Pressing `↓` once skips the SectionSpacer at row 1 and lands on
    // the role header at row 2.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    assert!(
        matches!(editor(&state).active_field, FieldFocus::Row(2)),
        "cursor must skip SectionSpacer and land on the role header row; \
         got {:?}",
        editor(&state).active_field
    );

    // Before expansion: LOG_LEVEL must not appear in the render.
    let dump_collapsed = render_to_dump(&state, &config, cwd);
    assert!(
        !dump_collapsed.contains("LOG_LEVEL"),
        "collapsed section must not render role keys; got:\n{dump_collapsed}"
    );

    // `→` on a collapsed role header expands the section, symmetric with
    // `←` collapsing an expanded header (verified below). Enter on the
    // header also expands; Right is exercised here because it's the binding
    // most likely to be eaten by the tab-advance handler if the guard ever
    // regresses.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    assert!(
        editor(&state).secrets_expanded.contains("agent-smith"),
        "→ on collapsed header must expand the agent-smith section"
    );
    let dump_expanded = render_to_dump(&state, &config, cwd);
    assert!(
        dump_expanded.contains("LOG_LEVEL"),
        "expanded section must render role keys; got:\n{dump_expanded}"
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
        "recollapsed section must hide role keys; got:\n{dump_recollapsed}"
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
        .insert("NEW_KEY".into(), jackin_core::EnvValue::Plain("v".into()));

    assert!(editor(&state).is_dirty(), "env add must flip is_dirty");
    assert!(
        editor(&state).change_count() >= 1,
        "env add must bump change_count"
    );
    Ok(())
}

/// Three-step Add flow: `A` opens the `EnvKey` modal, typing + Enter
/// stashes the key and opens the `SourcePicker`; Enter on the (default)
/// Plain choice opens the `EnvValue` modal; typing + Enter commits the
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
            .map(jackin_core::EnvValue::as_persisted_str),
        Some("s3cret"),
        "pending.env must contain the new key after the three-step add"
    );
    Ok(())
}

// ── 1Password picker integration tests ────────────────────────────

/// Picker may load or fall into a fatal state depending on `op` on
/// `$PATH`; either way the modal variant must be `OpPicker` and
/// the modal must carry the selected secrets key target.
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
    // The active modal carries the focused key so the commit handler
    // can write straight into pending.env.
    match &editor(&state).modal {
        Some(Modal::OpPicker {
            secrets_target: Some(SecretsPickerTarget::Existing { key, .. }),
            ..
        }) => {
            assert_eq!(key, "DB_URL", "key-row P must stash the focused key");
        }
        other => panic!("expected OpPicker target for DB_URL; got {other:?}"),
    }
    Ok(())
}

/// Esc on the `OpPicker` (vault pane / fatal state / loading) closes
/// the modal entirely — the picker is no longer a sub-mode of the
/// `EnvValue` text modal, so cancel returns the operator to the editor
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

    // Esc on the OpPicker closes the modal and drops its target context.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert!(
        editor(&state).modal.is_none(),
        "Esc-cancel must close the picker entirely; got {:?}",
        editor(&state).modal
    );
    // Cancel is a pure UI action — the on-pending env value is unchanged.
    assert_eq!(
        editor(&state)
            .pending
            .env
            .get("DB_URL")
            .map(jackin_core::EnvValue::as_persisted_str),
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
    use jackin_console::tui::components::op_picker::{OpLoadState, OpPickerStage};
    use jackin_env::{OpField, OpItem, OpVault};

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
            Some(Modal::OpPicker { state: picker, .. }) => {
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
                // Drop any in-flight probe result so handle_key's
                // leading poll_load doesn't race the test by draining
                // a stale `Err(...)` from the constructor's account
                // probe and routing into the Fatal-error guard.
                picker.cancel_in_flight_load();
            }
            other => panic!("expected OpPicker modal; got {other:?}"),
        }
    }

    // Enter on the Field pane commits the `op://...` reference.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    // The reference must land directly in pending.env (no text modal
    // intermediate). `as_persisted_str()` on an `OpRef` returns the
    // UUID-form `op` field (vault uuid=v1, item uuid=i1, field=password).
    assert_eq!(
        editor(&state)
            .pending
            .env
            .get("DB_URL")
            .map(jackin_core::EnvValue::as_persisted_str),
        Some("op://v1/i1/password"),
        "picker commit must write the UUID-form op:// reference straight into pending.env[key]"
    );
    assert!(
        editor(&state).modal.is_none(),
        "modal must close after key-row picker commit; got {:?}",
        editor(&state).modal
    );
    Ok(())
}

/// `P` on the `+ Add environment variable` sentinel: the picker commits
/// a path before the operator has named the key. The `EnvKeyWithValue` modal
/// opens next, and committing the key name writes both into pending.env at once.
#[test]
fn op_picker_sentinel_p_flow() -> Result<()> {
    use jackin_console::tui::components::op_picker::{OpLoadState, OpPickerStage};
    use jackin_env::{OpField, OpItem, OpVault};

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

    // P on the sentinel opens the picker with a NewKey target.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('p')),
    )?;
    assert!(matches!(editor(&state).modal, Some(Modal::OpPicker { .. })));
    match &editor(&state).modal {
        Some(Modal::OpPicker {
            secrets_target: Some(SecretsPickerTarget::NewKey { .. }),
            ..
        }) => {}
        other => panic!("sentinel P must open OpPicker with NewKey target; got {other:?}"),
    }

    // Drive the picker to a Field commit.
    {
        let editor_state = editor_mut(&mut state);
        match &mut editor_state.modal {
            Some(Modal::OpPicker { state: picker, .. }) => {
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
                // Drop any in-flight probe result so handle_key's
                // leading poll_load doesn't race the test by draining
                // a stale `Err(...)` from the constructor's account
                // probe and routing into the Fatal-error guard.
                picker.cancel_in_flight_load();
            }
            other => panic!("expected OpPicker modal; got {other:?}"),
        }
    }
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    // After the picker commits: EnvKeyWithValue modal is open and carries the OpRef.
    match &editor(&state).modal {
        Some(Modal::TextInput {
            target:
                TextInputTarget::EnvKeyWithValue {
                    value: carried_value,
                    ..
                },
            ..
        }) => {
            assert_eq!(
                carried_value.as_persisted_str(),
                "op://v1/i1/credential",
                "picker commit must carry the UUID-form op:// reference for the EnvKey commit"
            );
        }
        other => panic!("expected TextInput(EnvKey) modal; got {other:?}"),
    }

    // Type the new key name and Enter — the EnvKey commit handler must
    // consume the carried value and write the pair into pending.env.
    for ch in "API_KEY".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    assert_eq!(
        editor(&state)
            .pending
            .env
            .get("API_KEY")
            .map(jackin_core::EnvValue::as_persisted_str),
        Some("op://v1/i1/credential"),
        "EnvKey commit must write the stashed UUID-form OpRef into pending.env"
    );
    assert!(
        editor(&state).modal.is_none(),
        "EnvKey commit on the picker fast-path must close the modal; got {:?}",
        editor(&state).modal
    );
    Ok(())
}

// ── SourcePicker integration tests ────────────────────────────────

/// Drives Enter-on-sentinel → ScopePicker(All roles) → `EnvKey`
/// commit, leaving `SourcePicker` open. Default `op_available = false`.
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

/// `Enter` on `+ Add` walks: `EnvKey` → `SourcePicker` → `EnvValue` → commit.
/// The `SourcePicker` is the new step between the existing two text
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
            .map(jackin_core::EnvValue::as_persisted_str),
        Some("s3cret"),
        "Plain-text source path must land the typed value in pending.env"
    );
    Ok(())
}

/// POSIX `VAR=""` differs from `unset VAR`, so `EnvValue` must commit
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
            .map(jackin_core::EnvValue::as_persisted_str),
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

/// `SourcePicker` → 1Password branch: when op is available and the
/// operator picks the Op choice, the `OpPicker` modal opens with
/// a secrets target for the typed key so its commit handler
/// can write the `op://...` reference straight into the named key.
#[test]
fn enter_on_sentinel_path_to_op_picker() -> Result<()> {
    use jackin_console::tui::components::source_picker::SourcePickerState;
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
    // the sentinel-add path: env_key is embedded in SourcePicker modal.
    {
        let editor_state = editor_mut(&mut state);
        editor_state.modal = Some(Modal::SourcePicker {
            env_key: Some((
                jackin::console::tui::state::SecretsScopeTag::Workspace,
                "API_KEY".into(),
            )),
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
    match &editor(&state).modal {
        Some(Modal::OpPicker {
            secrets_target: Some(SecretsPickerTarget::Existing { key: name, .. }),
            ..
        }) => {
            assert_eq!(name, "API_KEY", "OpPicker target must carry the typed key");
        }
        other => panic!("expected OpPicker target for API_KEY; got {other:?}"),
    }
    Ok(())
}

/// When `op_available = false`, the Op button on the `SourcePicker` is
/// disabled: `→`/`Tab` cycling skips it, focus stays on Plain, and the
/// `O` direct hotkey is inert. The picker commits Plain regardless of
/// key flailing.
#[test]
fn source_picker_op_disabled_when_op_missing() -> Result<()> {
    use jackin_console::tui::components::source_picker::{SourceChoice, SourcePickerState};
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = manager_on_secrets_tab(&config, cwd);
    {
        let editor_state = editor_mut(&mut state);
        editor_state.modal = Some(Modal::SourcePicker {
            env_key: Some((
                jackin::console::tui::state::SecretsScopeTag::Workspace,
                "API_KEY".into(),
            )),
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
        Some(Modal::SourcePicker { state, .. }) => {
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

/// `Esc` on the `SourcePicker` walks back to the key input and clears
/// source-picker scratch state. Operator can edit the key name without an
/// env entry being added.
#[test]
fn source_picker_esc_clears_pending_state() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = drive_to_source_picker(&mut config, &paths, cwd, "API_KEY")?;
    // env_key context travels inside the SourcePicker modal itself.
    assert!(
        matches!(
            editor(&state).modal,
            Some(Modal::SourcePicker {
                env_key: Some(_),
                ..
            })
        ),
        "EnvKey commit must embed (scope, key) in SourcePicker modal"
    );

    // Esc walks back one dialog frame.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert!(
        matches!(
            editor(&state).modal,
            Some(Modal::TextInput {
                target: TextInputTarget::EnvKey { .. },
                ..
            })
        ),
        "Esc on SourcePicker must restore EnvKey input; got {:?}",
        editor(&state).modal
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
#[allow(
    clippy::too_many_lines,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
fn op_picker_multi_account_flow() -> Result<()> {
    use jackin_console::tui::components::op_picker::{OpLoadState, OpPickerStage};
    use jackin_env::{OpAccount, OpField, OpItem, OpVault};

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
            Some(Modal::OpPicker { state: picker, .. }) => {
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
                // Drop any in-flight probe result so handle_key's
                // leading poll_load doesn't race the test by draining
                // a stale `Err(...)` from the constructor's account
                // probe and routing into the Fatal-error guard.
                picker.cancel_in_flight_load();
            }
            other => panic!("expected OpPicker modal; got {other:?}"),
        }
    }

    // Enter on the Account pane — selects Bob, advances to Vault.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    match &editor(&state).modal {
        Some(Modal::OpPicker { state: picker, .. }) => {
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
            Some(Modal::OpPicker { state: picker, .. }) => {
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
                // Drop any in-flight probe result so handle_key's
                // leading poll_load doesn't race the test by draining
                // a stale `Err(...)` from the constructor's account
                // probe and routing into the Fatal-error guard.
                picker.cancel_in_flight_load();
            }
            other => panic!("expected OpPicker modal; got {other:?}"),
        }
    }

    // Enter on the Field pane commits the OpRef. The `op` field uses
    // UUID-form identifiers (vault=v1, item=i1, field=password).
    // Account scoping is not embedded in the URL; the launch-time
    // resolver uses the operator's default `op` account context.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert_eq!(
        editor(&state)
            .pending
            .env
            .get("DB_URL")
            .map(jackin_core::EnvValue::as_persisted_str),
        Some("op://v1/i1/password"),
        "multi-account picker commit must produce a UUID-form op:// reference"
    );
    assert!(editor(&state).modal.is_none());
    Ok(())
}

// ── Modal::RolePicker dispatch tests ─────────────────────────────

#[path = "secrets/agent_picker.rs"]
mod agent_picker;

#[path = "secrets/env_key.rs"]
mod env_key;

// ── ScopePicker + AgentOverridePicker integration tests ──────────────

#[path = "secrets/overrides.rs"]
mod overrides;
