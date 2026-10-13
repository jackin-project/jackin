// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Auth tab lines, geometry, widths, `EditorAuthLineRow` and render helpers extracted
//! from the view coordinator. Items re-exported from parent to preserve `super::*`
//! call sites in tests and qualified calls from frame.rs (via `render_auth_tab` etc).

use ratatui::text::Line;

use crate::tui::components::editor_rows::{AuthLineRow, auth_line_width, auth_lines};
use crate::tui::screens::editor::model::{AuthRow, FieldFocus};

use super::WorkspaceEditorState;

// Structural exception: editor rows are form/table rows with labels, values,
// disclosures, masked secrets, and action sentinels, so they cannot use the
// flat picker renderer even though they share its focus-gated cursor contract.
pub(crate) type EditorAuthLineRow = AuthLineRow;

#[must_use]
pub(crate) fn auth_display_row(
    row: &AuthRow<crate::tui::auth::AuthKind>,
    synthesized: &jackin_config::AppConfig,
    workspace_name: &str,
) -> EditorAuthLineRow {
    let workspace = synthesized.workspaces.get(workspace_name);
    let label = match row {
        AuthRow::Account { id } => {
            let enabled = workspace.is_some_and(|ws| ws.accounts.contains(id));
            let detail = synthesized.accounts.get(id).map_or_else(
                || id.clone(),
                |account| {
                    format!(
                        "{} ({id}, {}){}",
                        account.name,
                        account.provider.slug(),
                        if account.enabled { "" } else { " · disabled" }
                    )
                },
            );
            format!("[{}] {detail}", if enabled { "x" } else { " " })
        }
        AuthRow::Binding { agent, role } => {
            let binding = workspace.and_then(|ws| match role {
                Some(role) => ws
                    .roles
                    .get(role)
                    .and_then(|entry| entry.account_bindings.get(agent)),
                None => ws.account_bindings.get(agent),
            });
            let scope = role.as_deref().unwrap_or("Workspace");
            format!(
                "{scope} / {}: {}",
                agent.label(),
                binding.map_or("automatic", String::as_str)
            )
        }
        AuthRow::WorkspaceMode {
            kind: crate::tui::auth::AuthKind::Github,
        } => {
            let mode = workspace
                .and_then(|ws| ws.github.as_ref())
                .map(|github| github.auth_forward);
            format!(
                "Workspace / GitHub: {}",
                mode.map_or_else(
                    || "inherited".to_owned(),
                    |mode| format!("{mode:?}").to_lowercase()
                )
            )
        }
        AuthRow::RoleMode {
            role,
            kind: crate::tui::auth::AuthKind::Github,
        } => {
            let mode = workspace
                .and_then(|ws| ws.roles.get(role))
                .and_then(|entry| entry.github.as_ref())
                .map(|github| github.auth_forward);
            format!(
                "{role} / GitHub: {}",
                mode.map_or_else(
                    || "inherited".to_owned(),
                    |mode| format!("{mode:?}").to_lowercase()
                )
            )
        }
        _ => String::new(),
    };
    AuthLineRow::AuthKind { label }
}

#[must_use]
pub(crate) fn auth_state_lines<
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    let synthesized = state.synthesize_app_config_for_auth(config);
    let workspace_name = state.workspace_name_for_panel();
    let rows = state.auth_flat_rows(config);

    let FieldFocus::Row(cursor) = state.active_field;
    let max_idx = rows.len().saturating_sub(1);
    let cursor_clamped = cursor.min(max_idx);

    let display_rows: Vec<AuthLineRow> = rows
        .iter()
        .map(|row| auth_display_row(row, &synthesized, &workspace_name))
        .collect();
    auth_lines(&display_rows, cursor_clamped, show_cursor)
}

#[must_use]
pub(crate) fn auth_state_geometry<
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
) -> super::EditorTabContentGeometry {
    let rows = state.auth_flat_rows(config);
    let synthesized = state.synthesize_app_config_for_auth(config);
    let workspace_name = state.workspace_name_for_panel();
    let content_width = rows
        .iter()
        .map(|row| {
            let display_row = auth_display_row(row, &synthesized, &workspace_name);
            editor_auth_line_width(&display_row)
        })
        .max()
        .unwrap_or(0);
    super::EditorTabContentGeometry {
        content_width,
        content_height: rows.len(),
    }
}

#[must_use]
pub(crate) fn editor_auth_line_width(row: &EditorAuthLineRow) -> usize {
    auth_line_width(row)
}
