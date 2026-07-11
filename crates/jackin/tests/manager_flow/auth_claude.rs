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

//! Claude-kind auth-form integration tests for the manager TUI.
//!
//! Extracted from `manager_flow.rs` to keep each test binary under the
//! 1500-line file-size cap. Covers the Claude auth-form save path,
//! source picker round trip, credential text input cancel, and the
//! `op_available = false` propagation through the picker.

use anyhow::Result;
use jackin::console::tui::state::AuthRow;
use jackin_console::tui::auth::AuthKind;
use jackin_core::JackinPaths;
use jackin_core::env_model;
use tempfile::tempdir;

use super::*;

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
    let ws_claude_idx = auth_common::auth_row_idx(&ed, &config, |r| {
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
        jackin_core::EnvValue::Plain(s) => assert_eq!(s, "sk-ant-test"),
        jackin_core::EnvValue::Extended(e) => assert_eq!(e.value, "sk-ant-test"),
        jackin_core::EnvValue::OpRef(_) => {
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
    let ws_claude_idx = auth_common::auth_row_idx(&ed, &config, |r| {
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

/// Cancelling the credential `Modal::TextInput` (the literal-text leg
/// of the source-picker round trip) must restore `Modal::AuthForm` and
/// drain the modal parent auth form — not silently leave the operator
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
    let ws_claude_idx = auth_common::auth_row_idx(&ed, &config, |r| {
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
    let ws_claude_idx = auth_common::auth_row_idx(&ed, &config, |r| {
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
