//! Root bindings for the console-local auth panel component.

use crate::config::AppConfig;
use crate::console::domain::resolve_panel_mode;
use crate::console::tui::state::{
    AuthRow, EditorState, FieldFocus, SettingsState, auth_flat_rows,
    synthesize_appconfig_for_auth, workspace_name_for_panel,
};
use crate::operator_env::{EnvValue, OpRef};
use jackin_console::tui::components::editor_rows::{
    AuthSourceDisplay, AuthSourceValue, auth_source_display, auth_source_display_for_required_env,
};
use jackin_console::tui::screens::editor::view::EditorAuthLineRow;
use jackin_console::tui::screens::editor::view::auth_lines as editor_auth_lines;
use jackin_console::tui::screens::settings::view::{
    SettingsAuthLineRow, auth_lines as settings_auth_lines,
};

pub type AuthForm = jackin_console::tui::components::auth_panel::AuthForm<EnvValue>;

pub use jackin_console::tui::components::auth_panel::{CredentialInput, required_height};
pub(crate) use jackin_console::tui::components::auth_panel::mode_str;
pub use jackin_console::tui::components::auth_panel::render_form;

impl jackin_console::tui::components::auth_panel::AuthCredentialRef for OpRef {
    fn path(&self) -> &str {
        &self.path
    }

    fn is_empty(&self) -> bool {
        self.op.is_empty() || self.path.is_empty()
    }
}

impl jackin_console::tui::components::auth_panel::AuthCredential for EnvValue {
    type Ref = OpRef;

    fn into_credential_input(self) -> CredentialInput<Self::Ref> {
        match self {
            Self::Plain(value) => CredentialInput::Literal(value),
            Self::OpRef(value) => CredentialInput::OpRef(value),
        }
    }

    fn from_plain(value: String) -> Self {
        Self::Plain(value)
    }

    fn from_op_ref(value: Self::Ref) -> Self {
        Self::OpRef(value)
    }
}

pub(crate) fn editor_auth_display_row(
    row: &AuthRow,
    synthesized: &AppConfig,
    workspace_name: &str,
) -> EditorAuthLineRow {
    match row {
        AuthRow::AuthKindRow { kind } => EditorAuthLineRow::AuthKind {
            label: kind.label().to_string(),
        },
        AuthRow::WorkspaceMode { kind } => {
            let ws = synthesized.workspaces.get(workspace_name);
            let explicit = ws.and_then(|ws| explicit_workspace_mode(ws, *kind));
            let mode = explicit
                .unwrap_or_else(|| resolve_panel_mode(synthesized, *kind, workspace_name, ""));
            EditorAuthLineRow::WorkspaceMode {
                mode_label: mode_str(mode).to_string(),
                inherited: explicit.is_none(),
            }
        }
        AuthRow::WorkspaceSource { kind } => EditorAuthLineRow::WorkspaceSource {
            display: editor_auth_source_display(synthesized, workspace_name, "", *kind),
        },
        AuthRow::RoleHeader { role, expanded } => EditorAuthLineRow::RoleHeader {
            role: role.clone(),
            expanded: *expanded,
        },
        AuthRow::RoleMode { role, kind } => {
            let mode = resolve_panel_mode(synthesized, *kind, workspace_name, role);
            EditorAuthLineRow::RoleMode {
                mode_label: mode_str(mode).to_string(),
            }
        }
        AuthRow::RoleSource { role, kind } => EditorAuthLineRow::RoleSource {
            display: editor_auth_source_display(synthesized, workspace_name, role, *kind),
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
    let synthesized = synthesize_appconfig_for_auth(state, config);
    let workspace_name = workspace_name_for_panel(state);
    let rows = auth_flat_rows(state, config);

    let FieldFocus::Row(cursor) = state.active_field;
    let max_idx = rows.len().saturating_sub(1);
    let cursor_clamped = cursor.min(max_idx);
    let show_cursor =
        !state.tab_bar_focused && state.tab_content_scroll_focused && state.modal.is_none();

    let display_rows: Vec<EditorAuthLineRow> = rows
        .iter()
        .map(|row| editor_auth_display_row(row, &synthesized, &workspace_name))
        .collect();
    editor_auth_lines(&display_rows, cursor_clamped, show_cursor)
}

pub(crate) fn settings_auth_lines_for_state(state: &SettingsState<'_>) -> Vec<ratatui::text::Line<'static>> {
    let show_cursor =
        !state.tab_bar_focused && state.auth.scroll_focused && state.auth.modal.is_none();
    let Some(kind) = state.auth.selected_kind else {
        let rows: Vec<SettingsAuthLineRow> = state
            .auth
            .pending
            .iter()
            .map(|row| SettingsAuthLineRow::Kind {
                label: row.kind.label().to_string(),
            })
            .collect();
        return settings_auth_lines(&rows, state.auth.selected, show_cursor);
    };
    let Some(row) = state.auth.pending.iter().find(|row| row.kind == kind) else {
        return Vec::new();
    };
    let mut rows = vec![SettingsAuthLineRow::Mode {
        mode_label: mode_str(row.mode).to_string(),
    }];
    if let Some(env_name) = kind.required_env_var(row.mode) {
        rows.push(SettingsAuthLineRow::Source {
            display: settings_auth_source_display(state, kind, row.mode, env_name),
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
        settings_auth_source_value(state, kind, env_name).map(|value| match value {
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
    env_name: &str,
) -> Option<&'a EnvValue> {
    if kind == jackin_console::tui::auth::AuthKind::Github {
        state.auth.github_env.get(env_name)
    } else {
        state.env.pending.env.get(env_name)
    }
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
        .and_then(|env_name| auth_source_value(synthesized, workspace_name, role, env_name, kind))
        .map(|value| match value {
            EnvValue::OpRef(r) => AuthSourceValue::OpRefPath(r.path.clone()),
            EnvValue::Plain(s) => AuthSourceValue::Plain(s.clone()),
        });

    auth_source_display_for_required_env(env_name, value, mode_str(mode))
}

fn explicit_workspace_mode(
    ws: &crate::workspace::WorkspaceConfig,
    kind: jackin_console::tui::auth::AuthKind,
) -> Option<jackin_console::tui::auth::AuthMode> {
    use crate::console::domain::{auth_mode_from_auth_forward, auth_mode_from_github};
    use jackin_console::tui::auth::{AuthKind, AuthMode};
    match kind {
        AuthKind::Claude => ws
            .claude
            .as_ref()
            .map(|c| auth_mode_from_auth_forward(c.auth_forward)),
        AuthKind::Codex => ws
            .codex
            .as_ref()
            .map(|c| auth_mode_from_auth_forward(c.0.auth_forward)),
        AuthKind::Amp => ws
            .amp
            .as_ref()
            .map(|c| auth_mode_from_auth_forward(c.0.auth_forward)),
        AuthKind::Kimi => ws
            .kimi
            .as_ref()
            .map(|c| auth_mode_from_auth_forward(c.0.auth_forward)),
        AuthKind::Opencode => ws
            .opencode
            .as_ref()
            .map(|c| auth_mode_from_auth_forward(c.0.auth_forward)),
        AuthKind::Github => ws
            .github
            .as_ref()
            .map(|g| auth_mode_from_github(g.auth_forward)),
        AuthKind::Zai => {
            if ws.env.contains_key("ZAI_API_KEY") {
                Some(AuthMode::ApiKey)
            } else {
                None
            }
        }
    }
}

fn auth_source_value<'a>(
    synthesized: &'a AppConfig,
    workspace_name: &str,
    role: &str,
    env_name: &str,
    kind: jackin_console::tui::auth::AuthKind,
) -> Option<&'a EnvValue> {
    use jackin_console::tui::auth::AuthKind;
    match kind {
        AuthKind::Github => github_source_value(synthesized, workspace_name, role, env_name),
        AuthKind::Claude
        | AuthKind::Codex
        | AuthKind::Amp
        | AuthKind::Kimi
        | AuthKind::Opencode
        | AuthKind::Zai => agent_env_source_value(synthesized, workspace_name, role, env_name),
    }
}

fn agent_env_source_value<'a>(
    synthesized: &'a AppConfig,
    workspace_name: &str,
    role: &str,
    env_name: &str,
) -> Option<&'a EnvValue> {
    if !role.is_empty()
        && let Some(value) = synthesized
            .workspaces
            .get(workspace_name)
            .and_then(|ws| ws.roles.get(role))
            .and_then(|ro| ro.env.get(env_name))
    {
        return Some(value);
    }
    if let Some(value) = synthesized
        .workspaces
        .get(workspace_name)
        .and_then(|ws| ws.env.get(env_name))
    {
        return Some(value);
    }
    if !role.is_empty()
        && let Some(value) = synthesized
            .roles
            .get(role)
            .and_then(|r| r.env.get(env_name))
    {
        return Some(value);
    }
    synthesized.env.get(env_name)
}

fn github_source_value<'a>(
    synthesized: &'a AppConfig,
    workspace_name: &str,
    role: &str,
    env_name: &str,
) -> Option<&'a EnvValue> {
    if !role.is_empty()
        && let Some(value) = synthesized
            .workspaces
            .get(workspace_name)
            .and_then(|ws| ws.roles.get(role))
            .and_then(|ro| ro.github.as_ref())
            .and_then(|g| g.env.get(env_name))
    {
        return Some(value);
    }
    if let Some(value) = synthesized
        .workspaces
        .get(workspace_name)
        .and_then(|ws| ws.github.as_ref())
        .and_then(|g| g.env.get(env_name))
    {
        return Some(value);
    }
    synthesized
        .github
        .as_ref()
        .and_then(|g| g.env.get(env_name))
}
