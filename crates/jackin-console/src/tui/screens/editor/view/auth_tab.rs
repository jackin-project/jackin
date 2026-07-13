//! Auth tab lines, geometry, widths, `EditorAuthLineRow` and render helpers extracted
//! from the view coordinator. Items re-exported from parent to preserve `super::*`
//! call sites in tests and qualified calls from frame.rs (via `render_auth_tab` etc).

use ratatui::text::Line;

use crate::tui::components::editor_rows::{
    AuthLineRow, AuthSourceDisplay, AuthSourceValue, auth_line_width, auth_lines,
    auth_source_display_for_required_env,
};
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
    match row {
        AuthRow::AuthKindRow { kind } => AuthLineRow::AuthKind {
            label: kind.label().to_owned(),
        },
        AuthRow::WorkspaceMode { kind } => {
            let ws = synthesized.workspaces.get(workspace_name);
            let explicit =
                ws.and_then(|ws| crate::tui::auth_config::explicit_workspace_auth_mode(ws, *kind));
            let mode = explicit.unwrap_or_else(|| {
                crate::tui::auth_config::resolve_panel_mode(synthesized, *kind, workspace_name, "")
            });
            AuthLineRow::WorkspaceMode {
                mode_label: crate::tui::components::auth_panel::mode_str(mode).to_owned(),
                inherited: explicit.is_none(),
            }
        }
        AuthRow::WorkspaceSource { kind } => AuthLineRow::WorkspaceSource {
            display: editor_auth_source_display(synthesized, workspace_name, "", *kind),
        },
        AuthRow::WorkspaceSourceFolder { kind } => AuthLineRow::WorkspaceSourceFolder {
            display: crate::tui::auth_config::editor_source_folder_display(
                synthesized,
                workspace_name,
                "",
                *kind,
            ),
        },
        AuthRow::RoleHeader { role, expanded } => AuthLineRow::RoleHeader {
            role: role.clone(),
            expanded: *expanded,
        },
        AuthRow::RoleMode { role, kind } => {
            let mode = crate::tui::auth_config::resolve_panel_mode(
                synthesized,
                *kind,
                workspace_name,
                role,
            );
            AuthLineRow::RoleMode {
                mode_label: crate::tui::components::auth_panel::mode_str(mode).to_owned(),
            }
        }
        AuthRow::RoleSource { role, kind } => AuthLineRow::RoleSource {
            display: editor_auth_source_display(synthesized, workspace_name, role, *kind),
        },
        AuthRow::RoleSourceFolder { role, kind } => AuthLineRow::RoleSourceFolder {
            display: crate::tui::auth_config::editor_source_folder_display(
                synthesized,
                workspace_name,
                role,
                *kind,
            ),
        },
        AuthRow::AddSentinel { eligible } => AuthLineRow::AddSentinel {
            eligible: *eligible,
        },
        AuthRow::Spacer => AuthLineRow::Spacer,
    }
}

#[must_use]
pub(crate) fn auth_state_lines<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
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
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
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

fn editor_auth_source_display(
    synthesized: &jackin_config::AppConfig,
    workspace_name: &str,
    role: &str,
    kind: crate::tui::auth::AuthKind,
) -> AuthSourceDisplay {
    let mode = crate::tui::auth_config::resolve_panel_mode(synthesized, kind, workspace_name, role);
    let env_name = kind.required_env_var(mode);

    let value = env_name
        .and_then(|env_name| {
            crate::tui::auth_config::panel_auth_source_value(
                synthesized,
                workspace_name,
                role,
                env_name,
                kind,
            )
        })
        .map(|value| match value {
            jackin_core::EnvValue::OpRef(r) => AuthSourceValue::OpRefPath(r.path.clone()),
            jackin_core::EnvValue::Plain(s) => AuthSourceValue::Plain(s.clone()),
            jackin_core::EnvValue::Extended(e) => AuthSourceValue::Plain(e.value.clone()),
        });

    auth_source_display_for_required_env(
        env_name,
        value,
        crate::tui::components::auth_panel::mode_str(mode),
    )
}

#[must_use]
pub(crate) fn editor_auth_line_width(row: &EditorAuthLineRow) -> usize {
    auth_line_width(row)
}
