use super::*;
use jackin_core::WorkspaceName;

#[test]
fn env_key_modal_blocks_duplicate_workspace_key() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_env(&paths, temp.path(), vec![("EXISTING", "kept-value")])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

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

    for ch in "EXISTING".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    let env_size_before = editor(&state).pending.env.len();
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

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
            .map(jackin_core::EnvValue::as_persisted_str),
        Some("kept-value"),
        "the pre-existing value must remain untouched"
    );
    Ok(())
}

#[test]
fn env_key_modal_blocks_duplicate_agent_key() -> Result<()> {
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

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    assert!(editor(&state).secrets_expanded.contains("agent-smith"));

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;

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

    assert!(
        matches!(
            editor(&state).modal,
            Some(Modal::TextInput {
                target: TextInputTarget::EnvKey { .. },
                ..
            })
        ),
        "Enter on a duplicate role key must leave the EnvKey modal open; got {:?}",
        editor(&state).modal
    );
    let agent_entry = editor(&state)
        .pending
        .roles
        .get("agent-smith")
        .expect("role override must survive");
    assert_eq!(agent_entry.env.len(), 1);
    assert_eq!(
        agent_entry
            .env
            .get("LOG_LEVEL")
            .map(jackin_core::EnvValue::as_persisted_str),
        Some("debug"),
        "pre-existing role value must remain untouched"
    );
    Ok(())
}

#[test]
fn env_key_modal_allows_unique_name() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_env(&paths, temp.path(), vec![("EXISTING", "kept-value")])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    for ch in "NEW_KEY".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    assert!(
        matches!(editor(&state).modal, Some(Modal::SourcePicker { .. })),
        "unique key must commit and open the SourcePicker; got {:?}",
        editor(&state).modal
    );
    Ok(())
}
