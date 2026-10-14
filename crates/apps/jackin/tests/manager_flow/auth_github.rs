use anyhow::Result;
use jackin::console::adapter::state::AuthRow;
use jackin::workspace::WorkspaceRoleOverride;
use jackin_config::AppConfig;
use jackin_console::tui::auth::AuthKind;
use jackin_core::JackinPaths;
use tempfile::tempdir;

use super::*;

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
    ed.set_tab_bar_focused(false);
    let ws_github_idx = auth_common::auth_row_idx(&ed, &config, |r| {
        matches!(
            r,
            AuthRow::WorkspaceMode {
                kind: AuthKind::Github
            }
        )
    });
    ed.active_field = FieldFocus::Row(ws_github_idx);
    state.stage = ManagerStage::Editor(ed);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::AuthForm { .. })),
        "Enter on WorkspaceMode/Github must open AuthForm; got {:?}",
        editor(&state).modal
    );

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
        jackin_core::EnvValue::Plain(s) => assert_eq!(s, "ghp_round_trip"),
        jackin_core::EnvValue::Extended(e) => assert_eq!(e.value, "ghp_round_trip"),
        jackin_core::EnvValue::OpRef(_) => panic!("expected literal credential"),
    }
    assert!(
        !pending.env.contains_key("GH_TOKEN"),
        "GH_TOKEN must not leak into the regular workspace env map"
    );

    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('s')),
    )?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    mark_pending_save_drift_checked_for_test(&mut state);
    execute_pending_workspace_save_commit(&mut state, &mut config, &paths, cwd)?;
    wait_for_config_save(&mut state, &mut config, &paths, cwd)?;

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
        jackin_core::EnvValue::Plain(s) => assert_eq!(s, "ghp_round_trip"),
        jackin_core::EnvValue::Extended(e) => assert_eq!(e.value, "ghp_round_trip"),
        jackin_core::EnvValue::OpRef(_) => panic!("expected literal credential"),
    }
    assert!(
        !ws_on_disk.env.contains_key("GH_TOKEN"),
        "GH_TOKEN must not appear in [env]; only in [github.env]"
    );

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

#[test]
fn github_role_mode_d_clears_github_role_override() -> Result<()> {
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
    ed.set_tab_bar_focused(false);
    let header_idx = auth_common::auth_row_idx(&ed, &config, |r| {
        matches!(
            r,
            AuthRow::RoleMode { role, .. } if role == "the-architect"
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
        "D on github RoleMode must clear the role's github override"
    );
    Ok(())
}

#[test]
fn github_role_override_opens_directly_from_role_row() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();
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
    ed.set_tab_bar_focused(false);
    let role_idx = auth_common::auth_row_idx(&ed, &config, |row| {
        matches!(
            row, AuthRow::RoleMode { role, kind: AuthKind::Github } if role == "the-architect"
        )
    });
    ed.active_field = FieldFocus::Row(role_idx);
    state.stage = ManagerStage::Editor(ed);
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(matches!(editor(&state).modal, Some(Modal::AuthForm { .. })));
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert_eq!(
        editor(&state).pending.roles["the-architect"]
            .github
            .as_ref()
            .unwrap()
            .auth_forward,
        jackin_config::GithubAuthMode::Ignore
    );
    assert!(editor(&state).modal.is_none());
    Ok(())
}
