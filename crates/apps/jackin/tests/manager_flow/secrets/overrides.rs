#![expect(
    clippy::unwrap_used,
    reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
)]
use super::*;
use jackin_core::WorkspaceName;

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
        config.roles.insert(
            (*name).to_owned(),
            jackin_config::RoleSource {
                git: format!("https://example.invalid/{name}.git"),
                trusted: true,
                env: std::collections::BTreeMap::new(),
            },
        );
    }
    let toml = toml::to_string(&config)?;
    std::fs::write(&paths.config_file, toml)?;

    let mut roles_map = std::collections::BTreeMap::new();
    for name in with_overrides {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "LOG_LEVEL".into(),
            jackin_core::EnvValue::Plain("debug".into()),
        );
        roles_map.insert(
            (*name).into(),
            WorkspaceRoleOverride {
                env,
                account_bindings: std::collections::BTreeMap::default(),
                github: None,
                default_launch: None,
            },
        );
    }

    let ws = WorkspaceConfig {
        workdir: host_path.clone(),
        mounts: vec![MountConfig {
            src: host_path.clone(),
            dst: host_path,
            readonly: false,
            isolation: jackin_core::MountIsolation::Shared,
        }],
        allowed_roles: allowed.iter().map(|s| (*s).to_owned()).collect(),
        roles: roles_map,
        ..Default::default()
    };
    let mut ce = ConfigEditor::open(paths)?;
    ce.create_workspace(&WorkspaceName::parse("big-monorepo").unwrap(), ws)?;
    Ok(ce.save()?)
}

#[test]
fn sentinel_enter_opens_scope_picker() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::ScopePicker { .. })),
        "Enter on the workspace sentinel must open the ScopePicker; got {:?}",
        editor(&state).modal
    );
    Ok(())
}

#[test]
fn scope_picker_all_path_to_workspace_envkey() -> Result<()> {
    use jackin::console::adapter::state::SecretsScopeTag;
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

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

#[test]
fn scope_picker_specific_path_to_agent_picker_then_envkey() -> Result<()> {
    use jackin::console::adapter::state::SecretsScopeTag;
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    match &editor(&state).modal {
        Some(Modal::RoleOverridePicker { state: picker }) => {
            assert_eq!(picker.roles.len(), 1);
            assert_eq!(picker.roles[0].key(), "agent-smith");
        }
        other => panic!("expected Modal::RoleOverridePicker; got {other:?}"),
    }

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        editor(&state).pending.roles.is_empty(),
        "picker commit must not create an override entry; got pending.roles keys={:?}",
        editor(&state).pending.roles.keys().collect::<Vec<_>>()
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
                    &SecretsScopeTag::Role("agent-smith".into()),
                    "EnvKey modal must scope to the picked role"
                );
            }
            other => panic!("expected TextInputTarget::EnvKey; got {other:?}"),
        },
        other => panic!("expected Modal::TextInput; got {other:?}"),
    }
    Ok(())
}

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

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    match &editor(&state).modal {
        Some(Modal::RoleOverridePicker { state: picker }) => {
            let mut keys: Vec<String> = picker
                .roles
                .iter()
                .map(jackin_core::RoleSelector::key)
                .collect();
            keys.sort();
            assert_eq!(
                keys,
                vec!["agent-brown".to_owned(), "agent-smith".to_owned()],
                "agent-smith already has an override section but must still appear so the operator can add another key"
            );
        }
        other => panic!("expected Modal::RoleOverridePicker; got {other:?}"),
    }
    Ok(())
}

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
        editor(&state).pending.roles.is_empty(),
        "Esc must not create an override entry"
    );
    assert!(
        editor(&state).pending.env.is_empty(),
        "Esc must not write any env entry"
    );
    Ok(())
}

#[test]
fn cancel_from_envkey_after_agent_pick_does_not_create_section() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

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

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::RoleOverridePicker { .. })),
        "Esc must restore the override picker; got {:?}",
        editor(&state).modal
    );
    assert!(
        editor(&state).pending.roles.is_empty(),
        "Esc on EnvKey must not have created an override entry; got keys={:?}",
        editor(&state).pending.roles.keys().collect::<Vec<_>>()
    );
    assert!(
        editor(&state).secrets_expanded.is_empty(),
        "Esc on EnvKey must not have expanded any section"
    );
    Ok(())
}

#[test]
fn cancel_from_sourcepicker_after_agent_pick_does_not_create_section() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    for ch in "API_TOKEN".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::SourcePicker { .. })),
        "EnvKey commit must open SourcePicker; got {:?}",
        editor(&state).modal
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
        "Esc must restore the EnvKey modal; got {:?}",
        editor(&state).modal
    );
    assert!(
        editor(&state).pending.roles.is_empty(),
        "Esc on SourcePicker must not have created an override entry; got keys={:?}",
        editor(&state).pending.roles.keys().collect::<Vec<_>>()
    );
    assert!(
        editor(&state).secrets_expanded.is_empty(),
        "Esc on SourcePicker must not have expanded any section"
    );
    Ok(())
}

#[test]
fn completing_value_after_agent_pick_creates_section_with_one_var() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    for ch in "API_TOKEN".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(matches!(
        editor(&state).modal,
        Some(Modal::SourcePicker { .. })
    ));

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

    for ch in "secret".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    let roles = &editor(&state).pending.roles;
    assert!(
        roles.contains_key("agent-smith"),
        "pending.roles must contain the chosen role; got keys={:?}",
        roles.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        roles
            .get("agent-smith")
            .unwrap()
            .env
            .get("API_TOKEN")
            .map(jackin_core::EnvValue::as_persisted_str),
        Some("secret"),
        "the committed key/value must land in the role's env map"
    );
    assert!(
        editor(&state).secrets_expanded.contains("agent-smith"),
        "value commit must auto-expand the role's section"
    );
    assert!(
        editor(&state).modal.is_none(),
        "value commit must close every modal; got {:?}",
        editor(&state).modal
    );
    Ok(())
}

#[test]
fn cancel_from_agent_override_picker_after_scope_pick_does_not_create_section() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(matches!(
        editor(&state).modal,
        Some(Modal::RoleOverridePicker { .. })
    ));

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::ScopePicker { .. })),
        "Esc must restore the ScopePicker; got {:?}",
        editor(&state).modal
    );
    assert!(
        editor(&state).pending.roles.is_empty(),
        "Esc must not create an override entry; got pending.roles keys={:?}",
        editor(&state).pending.roles.keys().collect::<Vec<_>>()
    );
    assert!(
        editor(&state).secrets_expanded.is_empty(),
        "Esc must not expand any section"
    );
    Ok(())
}

#[test]
fn in_section_agent_sentinel_skips_scope_picker() -> Result<()> {
    use jackin::console::adapter::state::SecretsScopeTag;
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config =
        seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &["agent-smith"])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    editor_mut(&mut state)
        .secrets_expanded
        .insert("agent-smith".into());

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    match &editor(&state).modal {
        Some(Modal::TextInput { target, .. }) => match target {
            TextInputTarget::EnvKey { scope } => {
                assert_eq!(
                    scope,
                    &SecretsScopeTag::Role("agent-smith".into()),
                    "in-section sentinel must open EnvKey with the contextual Role scope"
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

#[test]
fn right_on_collapsed_agent_header_expands_does_not_change_tab() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config =
        seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &["agent-smith"])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
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

#[test]
fn right_on_expanded_agent_header_does_nothing_does_not_change_tab() -> Result<()> {
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

#[test]
fn left_on_collapsed_agent_header_does_nothing_does_not_change_tab() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config =
        seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &["agent-smith"])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
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

#[test]
fn tab_on_agent_header_advances_tab_normally() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config =
        seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &["agent-smith"])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    assert!(matches!(editor(&state).active_field, FieldFocus::Row(2)));

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab))?;
    assert_eq!(
        editor(&state).active_tab,
        EditorTab::Auth,
        "Tab on a header must still advance the active tab"
    );
    Ok(())
}
