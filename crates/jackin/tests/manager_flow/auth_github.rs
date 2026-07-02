//! GitHub-kind auth-form integration tests for the manager TUI.
//!
//! Extracted from `manager_flow.rs` to keep each test binary under the
//! 1500-line file-size cap. Covers the GitHub token save path, the
//! role-header github override clear, and the picker filter for
//! roles that already carry a github override.

use anyhow::Result;
use jackin::console::tui::state::AuthRow;
use jackin::workspace::WorkspaceRoleOverride;
use jackin_config::AppConfig;
use jackin_console::tui::auth::AuthKind;
use jackin_core::JackinPaths;
use tempfile::tempdir;

use super::*;

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
        jackin_core::EnvValue::Plain(s) => assert_eq!(s, "ghp_round_trip"),
        jackin_core::EnvValue::Extended(e) => assert_eq!(e.value, "ghp_round_trip"),
        jackin_core::EnvValue::OpRef(_) => panic!("expected literal credential"),
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
        jackin_core::EnvValue::Plain(s) => assert_eq!(s, "ghp_round_trip"),
        jackin_core::EnvValue::Extended(e) => assert_eq!(e.value, "ghp_round_trip"),
        jackin_core::EnvValue::OpRef(_) => panic!("expected literal credential"),
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
    let header_idx = auth_common::auth_row_idx(&ed, &config, |r| {
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
    let sentinel_idx =
        auth_common::auth_row_idx(&ed, &config, |r| matches!(r, AuthRow::AddSentinel { .. }));
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
        .map(jackin_core::RoleSelector::key)
        .collect();
    assert!(
        !labels.iter().any(|s| s == "the-architect"),
        "the-architect already has a github override and must be filtered out; got {labels:?}"
    );
    Ok(())
}
