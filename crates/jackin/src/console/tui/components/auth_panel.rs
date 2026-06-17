//! Root bindings for the console-local auth panel component.

use crate::config::AppConfig;
use crate::console::tui::state::{AuthRow, EditorState, FieldFocus, SettingsState, SettingsTab};
use crate::operator_env::EnvValue;
use jackin_console::tui::auth_config::{
    editor_source_folder_display, explicit_workspace_auth_mode, panel_auth_source_value,
    resolve_panel_mode, settings_auth_env_value, settings_source_folder_display,
};
use jackin_console::tui::components::editor_rows::{
    AuthSourceDisplay, AuthSourceValue, auth_source_display, auth_source_display_for_required_env,
};
use jackin_console::tui::screens::editor::view::EditorAuthLineRow;
use jackin_console::tui::screens::editor::view::auth_lines as editor_auth_lines;
use jackin_console::tui::screens::settings::view::{
    SettingsAuthLineRow, auth_lines as settings_auth_lines,
};

pub(crate) type AuthForm = jackin_console::tui::components::auth_panel::AuthForm<EnvValue>;

pub(crate) use jackin_console::tui::components::auth_panel::mode_str;
pub(crate) use jackin_console::tui::components::auth_panel::render_form;

pub(crate) fn editor_auth_display_row(
    row: &AuthRow,
    synthesized: &AppConfig,
    workspace_name: &str,
) -> EditorAuthLineRow {
    match row {
        AuthRow::AuthKindRow { kind } => EditorAuthLineRow::AuthKind {
            label: kind.label().to_owned(),
        },
        AuthRow::WorkspaceMode { kind } => {
            let ws = synthesized.workspaces.get(workspace_name);
            let explicit = ws.and_then(|ws| explicit_workspace_auth_mode(ws, *kind));
            let mode = explicit
                .unwrap_or_else(|| resolve_panel_mode(synthesized, *kind, workspace_name, ""));
            EditorAuthLineRow::WorkspaceMode {
                mode_label: mode_str(mode).to_owned(),
                inherited: explicit.is_none(),
            }
        }
        AuthRow::WorkspaceSource { kind } => EditorAuthLineRow::WorkspaceSource {
            display: editor_auth_source_display(synthesized, workspace_name, "", *kind),
        },
        AuthRow::WorkspaceSourceFolder { kind } => EditorAuthLineRow::WorkspaceSourceFolder {
            display: editor_source_folder_display(synthesized, workspace_name, "", *kind),
        },
        AuthRow::RoleHeader { role, expanded } => EditorAuthLineRow::RoleHeader {
            role: role.clone(),
            expanded: *expanded,
        },
        AuthRow::RoleMode { role, kind } => {
            let mode = resolve_panel_mode(synthesized, *kind, workspace_name, role);
            EditorAuthLineRow::RoleMode {
                mode_label: mode_str(mode).to_owned(),
            }
        }
        AuthRow::RoleSource { role, kind } => EditorAuthLineRow::RoleSource {
            display: editor_auth_source_display(synthesized, workspace_name, role, *kind),
        },
        AuthRow::RoleSourceFolder { role, kind } => EditorAuthLineRow::RoleSourceFolder {
            display: editor_source_folder_display(synthesized, workspace_name, role, *kind),
        },
        AuthRow::AddSentinel { eligible } => EditorAuthLineRow::AddSentinel {
            eligible: *eligible,
        },
        AuthRow::Spacer => EditorAuthLineRow::Spacer,
    }
}

pub(crate) fn editor_auth_lines_for_state(
    state: &EditorState<'_>,
    config: &AppConfig,
) -> Vec<ratatui::text::Line<'static>> {
    let synthesized = state.synthesize_app_config_for_auth(config);
    let workspace_name = state.workspace_name_for_panel();
    let rows = state.auth_flat_rows(config);

    let FieldFocus::Row(cursor) = state.active_field;
    let max_idx = rows.len().saturating_sub(1);
    let cursor_clamped = cursor.min(max_idx);
    let show_cursor =
        !state.tab_bar_focused() && state.tab_content_scroll_focused() && state.modal.is_none();

    let display_rows: Vec<EditorAuthLineRow> = rows
        .iter()
        .map(|row| editor_auth_display_row(row, &synthesized, &workspace_name))
        .collect();
    editor_auth_lines(&display_rows, cursor_clamped, show_cursor)
}

pub(crate) fn settings_auth_lines_for_state(
    state: &SettingsState<'_>,
) -> Vec<ratatui::text::Line<'static>> {
    let show_cursor = state.content_focused(SettingsTab::Auth) && state.auth.modal.is_none();
    let Some(kind) = state.auth.selected_kind else {
        let rows: Vec<SettingsAuthLineRow> = state
            .auth
            .pending
            .iter()
            .map(|row| SettingsAuthLineRow::Kind {
                label: row.kind.label().to_owned(),
            })
            .collect();
        return settings_auth_lines(&rows, state.auth.selected, show_cursor);
    };
    let Some(row) = state.auth.pending.iter().find(|row| row.kind == kind) else {
        return Vec::new();
    };
    let mut rows = vec![SettingsAuthLineRow::Mode {
        mode_label: mode_str(row.mode).to_owned(),
    }];
    if let Some(env_name) = kind.required_env_var(row.mode) {
        rows.push(SettingsAuthLineRow::Source {
            display: settings_auth_source_display(state, kind, row.mode, env_name),
        });
    }
    if jackin_console::tui::auth::auth_mode_supports_source_folder(kind, row.mode) {
        rows.push(SettingsAuthLineRow::SourceFolder {
            display: settings_source_folder_display(row),
        });
    }
    rows.push(SettingsAuthLineRow::Spacer);
    settings_auth_lines(&rows, state.auth.selected, show_cursor)
}

fn settings_auth_source_display(
    state: &SettingsState<'_>,
    kind: jackin_console::tui::auth::AuthKind,
    mode: jackin_console::tui::auth::AuthMode,
    env_name: &str,
) -> AuthSourceDisplay {
    auth_source_display(
        settings_auth_source_value(state, kind, mode).map(|value| match value {
            EnvValue::Plain(value) => AuthSourceValue::Plain(value.clone()),
            EnvValue::OpRef(op_ref) => AuthSourceValue::OpRefPath(op_ref.path.clone()),
        }),
        env_name,
        mode_str(mode),
    )
}

fn settings_auth_source_value<'a>(
    state: &'a SettingsState<'_>,
    kind: jackin_console::tui::auth::AuthKind,
    mode: jackin_console::tui::auth::AuthMode,
) -> Option<&'a EnvValue> {
    settings_auth_env_value(kind, mode, &state.auth.github_env, &state.env.pending.env)
}

fn editor_auth_source_display(
    synthesized: &AppConfig,
    workspace_name: &str,
    role: &str,
    kind: jackin_console::tui::auth::AuthKind,
) -> AuthSourceDisplay {
    let mode = resolve_panel_mode(synthesized, kind, workspace_name, role);
    let env_name = kind.required_env_var(mode);

    let value = env_name
        .and_then(|env_name| {
            panel_auth_source_value(synthesized, workspace_name, role, env_name, kind)
        })
        .map(|value| match value {
            EnvValue::OpRef(r) => AuthSourceValue::OpRefPath(r.path.clone()),
            EnvValue::Plain(s) => AuthSourceValue::Plain(s.clone()),
        });

    auth_source_display_for_required_env(env_name, value, mode_str(mode))
}
