use super::*;
use jackin::console::adapter::state::{SecretsPickerTarget, TextInputTarget};
use jackin_core::WorkspaceName;

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

    editor_mut(&mut state).unmasked_rows.insert((
        jackin::console::adapter::state::SecretsScopeTag::Workspace,
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

#[test]
fn secrets_edit_value_saves_to_disk() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_env(&paths, temp.path(), vec![("DB_URL", "old-value")])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    assert!(
        matches!(editor(&state).active_field, FieldFocus::Row(0)),
        "cursor must open on the first key row"
    );

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

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

    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('S')),
    )?;

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

#[test]
fn secrets_delete_key_saves_to_disk() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_env(&paths, temp.path(), vec![("DB_URL", "doomed")])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('D')),
    )?;

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

    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('S')),
    )?;

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

#[test]
fn secrets_masking_m_toggle() -> Result<()> {
    use jackin::console::adapter::state::SecretsScopeTag;
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

#[test]
fn secrets_agent_section_expand_collapse() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs()?;
    let host_path = temp.path().display().to_string();

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
            account_bindings: std::collections::BTreeMap::default(),
            github: None,
            default_launch: None,
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

    editor_mut(&mut state).unmasked_rows.insert((
        jackin::console::adapter::state::SecretsScopeTag::Role("agent-smith".into()),
        "LOG_LEVEL".into(),
    ));

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    assert!(
        matches!(editor(&state).active_field, FieldFocus::Row(2)),
        "cursor must skip SectionSpacer and land on the role header row; \
         got {:?}",
        editor(&state).active_field
    );

    let dump_collapsed = render_to_dump(&state, &config, cwd);
    assert!(
        !dump_collapsed.contains("LOG_LEVEL"),
        "collapsed section must not render role keys; got:\n{dump_collapsed}"
    );

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

#[test]
fn secrets_add_new_key_flow() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('A')),
    )?;

    for ch in "API_KEY".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::SourcePicker { .. })),
        "EnvKey commit must open SourcePicker; got {:?}",
        editor(&state).modal
    );

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

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

#[test]
fn op_picker_opens_on_p_from_secrets_key_row() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_env(&paths, temp.path(), vec![("DB_URL", "postgres")])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    state.op_available = true;

    assert!(matches!(editor(&state).active_field, FieldFocus::Row(0)));

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

#[test]
fn op_picker_cancel_closes_modal() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_env(&paths, temp.path(), vec![("DB_URL", "untouched")])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    state.op_available = true;

    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('p')),
    )?;
    assert!(matches!(editor(&state).modal, Some(Modal::OpPicker { .. })));

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert!(
        editor(&state).modal.is_none(),
        "Esc-cancel must close the picker entirely; got {:?}",
        editor(&state).modal
    );

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

    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('p')),
    )?;

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
                    section_id: None,
                    label: "password".into(),
                    field_type: "concealed".into(),
                    concealed: true,
                    reference: String::new(),
                }];
                picker.field_list_state.set_active(Some(0));
                picker.stage = OpPickerStage::Field;
                picker.load_state = OpLoadState::Ready;

                picker.cancel_in_flight_load();
            }
            other => panic!("expected OpPicker modal; got {other:?}"),
        }
    }

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

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

#[test]
fn op_picker_sentinel_p_flow() -> Result<()> {
    use jackin_console::tui::components::op_picker::{OpLoadState, OpPickerStage};
    use jackin_env::{OpField, OpItem, OpVault};

    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());

    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    state.op_available = true;

    assert!(matches!(editor(&state).active_field, FieldFocus::Row(0)));

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
                    section_id: None,
                    label: "credential".into(),
                    field_type: "concealed".into(),
                    concealed: true,
                    reference: String::new(),
                }];
                picker.field_list_state.set_active(Some(0));
                picker.stage = OpPickerStage::Field;
                picker.load_state = OpLoadState::Ready;

                picker.cancel_in_flight_load();
            }
            other => panic!("expected OpPicker modal; got {other:?}"),
        }
    }
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

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

fn drive_to_source_picker<'a>(
    config: &mut AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    key_name: &str,
) -> Result<ManagerState<'a>> {
    let mut state = manager_on_secrets_tab(config, cwd);

    handle_key(&mut state, config, paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, config, paths, cwd, key(KeyCode::Enter))?;

    for ch in key_name.chars() {
        handle_key(&mut state, config, paths, cwd, key(KeyCode::Char(ch)))?;
    }

    handle_key(&mut state, config, paths, cwd, key(KeyCode::Enter))?;
    Ok(state)
}

#[test]
fn enter_on_sentinel_opens_envkey_then_sourcepicker_then_value_modal() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = drive_to_source_picker(&mut config, &paths, cwd, "API_KEY")?;

    assert!(
        matches!(editor(&state).modal, Some(Modal::SourcePicker { .. })),
        "EnvKey commit on a sentinel must open SourcePicker; got {:?}",
        editor(&state).modal
    );

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

#[test]
fn enter_on_sentinel_path_to_op_picker() -> Result<()> {
    use jackin_console::tui::components::source_picker::SourcePickerState;
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = manager_on_secrets_tab(&config, cwd);

    {
        let editor_state = editor_mut(&mut state);
        editor_state.modal = Some(Modal::SourcePicker {
            env_key: Some((
                jackin::console::adapter::state::SecretsScopeTag::Workspace,
                "API_KEY".into(),
            )),
            state: SourcePickerState::new("API_KEY".into(), true),
        });
    }

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
                jackin::console::adapter::state::SecretsScopeTag::Workspace,
                "API_KEY".into(),
            )),
            state: SourcePickerState::new("API_KEY".into(), false),
        });
    }

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

#[test]
fn source_picker_esc_clears_pending_state() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = drive_to_source_picker(&mut config, &paths, cwd, "API_KEY")?;

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

#[test]
fn op_picker_multi_account_flow() -> Result<()> {
    use jackin_console::tui::components::op_picker::{OpLoadState, OpPickerStage};
    use jackin_env::{OpAccount, OpField, OpItem, OpVault};

    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_env(&paths, temp.path(), vec![("DB_URL", "old")])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    state.op_available = true;

    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('p')),
    )?;

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
                picker.account_list_state.set_active(Some(1));
                picker.selected_account = None;
                picker.stage = OpPickerStage::Account;
                picker.load_state = OpLoadState::Ready;

                picker.cancel_in_flight_load();
            }
            other => panic!("expected OpPicker modal; got {other:?}"),
        }
    }

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
                    section_id: None,
                    label: "password".into(),
                    field_type: "concealed".into(),
                    concealed: true,
                    reference: String::new(),
                }];
                picker.field_list_state.set_active(Some(0));
                picker.stage = OpPickerStage::Field;
                picker.load_state = OpLoadState::Ready;

                picker.cancel_in_flight_load();
            }
            other => panic!("expected OpPicker modal; got {other:?}"),
        }
    }

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

#[path = "secrets/agent_picker.rs"]
mod agent_picker;

#[path = "secrets/env_key.rs"]
mod env_key;

#[path = "secrets/overrides.rs"]
mod overrides;
