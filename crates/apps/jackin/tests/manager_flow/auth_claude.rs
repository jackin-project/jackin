//! Account authorization survives the complete manager save/reload path.
use super::*;
use anyhow::Result;
use jackin::console::adapter::state::AuthRow;
use jackin_config::{AccountConfig, AccountCredential, AiProvider, AppConfig};
use jackin_core::{Agent, EnvValue, JackinPaths};
use tempfile::tempdir;

fn add_test_accounts(paths: &JackinPaths) -> Result<AppConfig> {
    let mut edit = ConfigEditor::open(paths)?;
    for id in ["personal-test", "work-test"] {
        edit.upsert_account(
            id,
            &AccountConfig {
                enabled: true,
                name: id.into(),
                provider: AiProvider::Anthropic,
                credential: AccountCredential::ApiKey {
                    value: EnvValue::from(format!("fixture-secret-{id}")),
                    base_url: None,
                    model: None,
                },
            },
        )?;
    }
    Ok(edit.save()?)
}

fn account_editor<'a>(config: &AppConfig, cwd: &std::path::Path) -> ManagerState<'a> {
    let mut state = ManagerState::from_config(config, cwd);
    let ws = config.workspaces["big-monorepo"].clone();
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.set_tab_bar_focused(false);
    state.stage = ManagerStage::Editor(ed);
    state
}

#[test]
fn account_assignment_and_selection_save_without_copying_secrets() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    seed_config(&paths, temp.path())?;
    let mut config = add_test_accounts(&paths)?;
    let mut state = account_editor(&config, temp.path());
    for id in ["personal-test", "work-test"] {
        let index = auth_common::auth_row_idx(
            editor(&state),
            &config,
            |row| matches!(row, AuthRow::Account { id: candidate } if candidate == id),
        );
        editor_mut(&mut state).active_field = FieldFocus::Row(index);
        handle_key(
            &mut state,
            &mut config,
            &paths,
            temp.path(),
            key(KeyCode::Enter),
        )?;
    }
    let index = auth_common::auth_row_idx(editor(&state), &config, |row| {
        matches!(
            row,
            AuthRow::Binding {
                agent: Agent::Claude,
                role: None
            }
        )
    });
    editor_mut(&mut state).active_field = FieldFocus::Row(index);
    handle_key(
        &mut state,
        &mut config,
        &paths,
        temp.path(),
        key(KeyCode::Enter),
    )?;
    assert_eq!(
        editor(&state)
            .pending
            .account_bindings
            .get(&Agent::Claude)
            .map(String::as_str),
        Some("personal-test")
    );
    handle_key(
        &mut state,
        &mut config,
        &paths,
        temp.path(),
        key(KeyCode::Char('s')),
    )?;
    handle_key(
        &mut state,
        &mut config,
        &paths,
        temp.path(),
        key(KeyCode::Tab),
    )?;
    handle_key(
        &mut state,
        &mut config,
        &paths,
        temp.path(),
        key(KeyCode::Enter),
    )?;
    mark_pending_save_drift_checked_for_test(&mut state);
    execute_pending_workspace_save_commit(&mut state, &mut config, &paths, temp.path())?;
    wait_for_config_save(&mut state, &mut config, &paths, temp.path())?;
    let reloaded = AppConfig::load_or_init(&paths)?;
    let workspace = &reloaded.workspaces["big-monorepo"];
    assert_eq!(workspace.accounts, ["personal-test", "work-test"]);
    assert_eq!(
        workspace
            .account_bindings
            .get(&Agent::Claude)
            .map(String::as_str),
        Some("personal-test")
    );
    let raw = std::fs::read_to_string(paths.workspaces_dir.join("big-monorepo.toml"))?;
    assert!(!raw.contains("fixture-secret"));
    assert!(!raw.contains("auth_forward"));
    Ok(())
}

#[test]
fn unassigning_account_clears_workspace_and_role_selection() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    seed_config(&paths, temp.path())?;
    let mut config = add_test_accounts(&paths)?;
    let workspace = config.workspaces.get_mut("big-monorepo").unwrap();
    workspace.accounts = vec!["personal-test".into()];
    workspace
        .account_bindings
        .insert(Agent::Claude, "personal-test".into());
    workspace
        .roles
        .entry("agent-smith".into())
        .or_default()
        .account_bindings
        .insert(Agent::Claude, "personal-test".into());
    let mut state = account_editor(&config, temp.path());
    let index = auth_common::auth_row_idx(
        editor(&state),
        &config,
        |row| matches!(row, AuthRow::Account { id } if id == "personal-test"),
    );
    editor_mut(&mut state).active_field = FieldFocus::Row(index);
    handle_key(
        &mut state,
        &mut config,
        &paths,
        temp.path(),
        key(KeyCode::Enter),
    )?;
    let pending = &editor(&state).pending;
    assert!(pending.accounts.is_empty());
    assert!(pending.account_bindings.is_empty());
    assert!(pending.roles["agent-smith"].account_bindings.is_empty());
    Ok(())
}
