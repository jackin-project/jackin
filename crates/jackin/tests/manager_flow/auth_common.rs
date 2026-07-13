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

//! Auth-kind and role-override integration tests for the manager TUI.
//!
//! Extracted from `manager_flow.rs` to keep each test binary under the
//! 1500-line file-size cap. Covers the auth-row helpers + the
//! role-header / role-agent / workspace-source edit flows that are
//! shared across both Claude and GitHub auth kinds.

use anyhow::Result;
use jackin::console::tui::state::AuthRow;
use jackin_config::AppConfig;
use jackin_console::tui::auth::AuthKind;
use jackin_core::JackinPaths;
use jackin_core::env_model;
use tempfile::tempdir;

use super::*;

/// Return the flat-row index of the first `AuthRow` that matches `pred`.
pub(super) fn auth_row_idx(
    ed: &EditorState<'_>,
    config: &AppConfig,
    pred: impl Fn(&AuthRow) -> bool,
) -> usize {
    ed.auth_flat_rows(config)
        .iter()
        .position(pred)
        .expect("required auth row not found")
}

#[test]
fn auth_add_role_override_flow_uses_selected_auth_kind() -> Result<()> {
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

    let sentinel_idx = auth_row_idx(&ed, &config, |r| matches!(r, AuthRow::AddSentinel { .. }));
    ed.active_field = FieldFocus::Row(sentinel_idx);
    state.stage = ManagerStage::Editor(ed);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::AuthRolePicker { .. })),
        "Enter on AddSentinel must open AuthRolePicker; got {:?}",
        editor(&state).modal
    );

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        match &editor(&state).modal {
            Some(Modal::AuthForm {
                target: jackin::console::tui::state::AuthFormTarget::WorkspaceRole { role, kind },
                ..
            }) => {
                assert_eq!(*kind, AuthKind::Claude);
                assert!(
                    !role.is_empty(),
                    "role must propagate from AuthRolePicker → AuthForm"
                );
                true
            }
            _ => false,
        },
        "Enter on AuthRolePicker must open AuthForm for selected auth kind; got {:?}",
        editor(&state).modal
    );
    Ok(())
}

#[test]
fn auth_role_header_left_right_toggles_expansion() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut ws = config.workspaces.get("big-monorepo").unwrap().clone();
    let over = WorkspaceRoleOverride {
        claude: Some(jackin_config::AgentAuthConfig {
            auth_forward: jackin_config::AuthForwardMode::Ignore,
            ..Default::default()
        }),
        ..Default::default()
    };
    ws.roles.insert("the-architect".into(), over);

    let mut state = ManagerState::from_config(&config, cwd);
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.set_tab_bar_focused(false);
    ed.auth_selected_kind = Some(AuthKind::Claude);
    let header_idx = auth_row_idx(&ed, &config, |r| matches!(r, AuthRow::RoleHeader { .. }));
    ed.active_field = FieldFocus::Row(header_idx);
    state.stage = ManagerStage::Editor(ed);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    assert!(editor(&state).auth_expanded.contains("the-architect"));
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Left))?;
    assert!(!editor(&state).auth_expanded.contains("the-architect"));
    Ok(())
}

#[test]
fn auth_role_header_d_clears_selected_auth_kind_override() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut ws = config.workspaces.get("big-monorepo").unwrap().clone();
    let over = WorkspaceRoleOverride {
        claude: Some(jackin_config::AgentAuthConfig {
            auth_forward: jackin_config::AuthForwardMode::Ignore,
            ..Default::default()
        }),
        codex: Some(jackin_config::AgentAuthConfig {
            auth_forward: jackin_config::AuthForwardMode::ApiKey,
            ..Default::default()
        }),
        ..Default::default()
    };
    ws.roles.insert("the-architect".into(), over);

    let mut state = ManagerState::from_config(&config, cwd);
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.auth_selected_kind = Some(AuthKind::Claude);
    let header_idx = auth_row_idx(&ed, &config, |r| matches!(r, AuthRow::RoleHeader { .. }));
    ed.active_field = FieldFocus::Row(header_idx);
    state.stage = ManagerStage::Editor(ed);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('d')),
    )?;
    assert!(
        editor(&state).modal.is_none(),
        "selected auth-kind clear should not open a confirm modal"
    );
    let role_over = editor(&state)
        .pending
        .roles
        .get("the-architect")
        .expect("override entry stays even after clear");
    assert!(role_over.claude.is_none());
    assert!(
        role_over.codex.is_some(),
        "hidden Codex override must be untouched"
    );
    Ok(())
}

#[test]
fn auth_role_agent_row_d_silently_clears_single_agent() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut ws = config.workspaces.get("big-monorepo").unwrap().clone();
    let over = WorkspaceRoleOverride {
        claude: Some(jackin_config::AgentAuthConfig {
            auth_forward: jackin_config::AuthForwardMode::Ignore,
            ..Default::default()
        }),
        codex: Some(jackin_config::AgentAuthConfig {
            auth_forward: jackin_config::AuthForwardMode::ApiKey,
            ..Default::default()
        }),
        ..Default::default()
    };
    ws.roles.insert("the-architect".into(), over);

    let mut state = ManagerState::from_config(&config, cwd);
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.auth_selected_kind = Some(AuthKind::Claude);
    ed.auth_expanded.insert("the-architect".into());
    let claude_idx = auth_row_idx(&ed, &config, |r| {
        matches!(
            r,
            AuthRow::RoleMode {
                kind: AuthKind::Claude,
                ..
            }
        )
    });
    ed.active_field = FieldFocus::Row(claude_idx);
    state.stage = ManagerStage::Editor(ed);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('d')),
    )?;
    assert!(
        editor(&state).modal.is_none(),
        "single-agent clear must be silent (no modal)"
    );
    let ro = editor(&state).pending.roles.get("the-architect").unwrap();
    assert!(ro.claude.is_none());
    assert!(ro.codex.is_some(), "Codex override must be untouched");
    Ok(())
}

/// Pressing Enter on an `AuthKind` row sets `auth_selected_kind` and
/// resets the cursor to row 0. Drives the kind picker → focused-view
/// transition through the real keystroke dispatcher (every other auth
/// integration test pre-seeds `auth_selected_kind`, which would mask
/// a regression in the input-layer wiring).
#[test]
fn auth_enter_on_auth_kind_focuses_selected_agent() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = ManagerState::from_config(&config, cwd);
    let ws = config.workspaces.get("big-monorepo").unwrap().clone();
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    // No `auth_selected_kind` set — picker view.
    let codex_kind_idx = auth_row_idx(&ed, &config, |r| {
        matches!(
            r,
            AuthRow::AuthKindRow {
                kind: AuthKind::Codex
            }
        )
    });
    ed.active_field = FieldFocus::Row(codex_kind_idx);
    state.stage = ManagerStage::Editor(ed);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    let ed = editor(&state);
    assert_eq!(ed.auth_selected_kind, Some(AuthKind::Codex));
    assert!(
        matches!(ed.active_field, FieldFocus::Row(0)),
        "Enter on AuthKind must reset cursor to Row(0); got {:?}",
        ed.active_field
    );
    Ok(())
}

/// Esc on the focused auth view pops back to the kind picker WITHOUT
/// triggering the dirty-modal flow even when `editor.is_dirty()`.
/// Pending edits stay in `editor.pending`; a subsequent Esc on the
/// picker would fall through to the dirty check.
#[test]
fn auth_esc_from_focused_view_pops_to_picker_without_dirty_modal() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = ManagerState::from_config(&config, cwd);
    let ws = config.workspaces.get("big-monorepo").unwrap().clone();
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.auth_selected_kind = Some(AuthKind::Claude);
    // Mutate `pending` so `is_dirty()` is true — this is what would
    // otherwise route Esc through `Modal::SaveDiscardCancel`.
    ed.pending.git_pull_on_entry = !ed.pending.git_pull_on_entry;
    ed.active_field = FieldFocus::Row(0);
    state.stage = ManagerStage::Editor(ed);
    assert!(editor(&state).is_dirty(), "fixture must be dirty");

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;

    let ed = editor(&state);
    assert!(
        ed.auth_selected_kind.is_none(),
        "Esc on focused view must clear auth_selected_kind"
    );
    assert!(
        ed.modal.is_none(),
        "Esc must not open the dirty SaveDiscardCancel modal mid-tab"
    );
    assert!(
        ed.is_dirty(),
        "pending edits must survive the in-tab navigation pop"
    );
    Ok(())
}

/// Tab/BackTab leaving the Auth tab clears `auth_selected_kind` so
/// re-entering the tab returns to the kind picker rather than a
/// stale focused view.
#[test]
fn auth_tab_cycle_off_auth_clears_selected_agent() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut state = ManagerState::from_config(&config, cwd);
    let ws = config.workspaces.get("big-monorepo").unwrap().clone();
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.set_tab_bar_focused(false);
    ed.auth_selected_kind = Some(AuthKind::Claude);
    state.stage = ManagerStage::Editor(ed);

    // Tab from content returns to tab bar and advances to next tab (General).
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab))?;
    let ed = editor(&state);
    assert_ne!(ed.active_tab, EditorTab::Auth);
    assert!(
        ed.auth_selected_kind.is_none(),
        "leaving the Auth tab must drop the focused-kind selection"
    );
    Ok(())
}

/// Pressing D on a `WorkspaceSource` row is a no-op. The main auth panel
/// renders source rows as previews; edits happen through the mode row's dialog.
#[test]
fn auth_workspace_source_d_is_noop() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config(&paths, temp.path())?;
    let cwd = temp.path();

    let mut ws = config.workspaces.get("big-monorepo").unwrap().clone();
    ws.claude = Some(jackin_config::AgentAuthConfig {
        auth_forward: jackin_config::AuthForwardMode::ApiKey,
        ..Default::default()
    });
    ws.env.insert(
        env_model::ANTHROPIC_API_KEY_ENV_NAME.into(),
        jackin_core::EnvValue::Plain("k".into()),
    );

    let mut state = ManagerState::from_config(&config, cwd);
    let mut ed = EditorState::new_edit("big-monorepo".into(), ws);
    ed.active_tab = EditorTab::Auth;
    ed.auth_selected_kind = Some(AuthKind::Claude);
    let source_idx = auth_row_idx(&ed, &config, |r| {
        matches!(
            r,
            AuthRow::WorkspaceSource {
                kind: AuthKind::Claude
            }
        )
    });
    ed.active_field = FieldFocus::Row(source_idx);
    state.stage = ManagerStage::Editor(ed);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('d')),
    )?;

    let ed = editor(&state);
    assert!(
        ed.modal.is_none(),
        "workspace source preview no-op must be silent (no modal)"
    );
    assert!(
        ed.pending.claude.is_some(),
        "D on WorkspaceSource must keep the workspace-level claude override"
    );
    Ok(())
}
