use crate::config::AppConfig;
use crate::console::manager::auth_kind::{AuthKind, AuthMode};
use crate::console::manager::state::{AuthFormTarget, AuthRow, EditorMode, EditorState};

pub(crate) fn auth_flat_rows(editor: &EditorState<'_>, config: &AppConfig) -> Vec<AuthRow> {
    let synthesized = synthesize_appconfig_for_auth(editor, config);
    let ws_name = workspace_name_for_panel(editor);
    jackin_console::editor::update::auth_flat_rows(
        editor.auth_selected_kind,
        [
            AuthKind::Claude,
            AuthKind::Codex,
            AuthKind::Amp,
            AuthKind::Opencode,
            AuthKind::Github,
            AuthKind::Zai,
        ],
        &editor.pending.roles,
        editor.pending.allowed_roles.len(),
        &editor.auth_expanded,
        |kind, role| kind.role_override_present(role),
        |kind, role| effective_mode_needs_credential(&synthesized, &ws_name, role, *kind),
    )
}

fn effective_mode_needs_credential(
    synthesized: &AppConfig,
    ws_name: &str,
    role: &str,
    kind: AuthKind,
) -> bool {
    let mode = resolve_panel_mode(synthesized, kind, ws_name, role);
    kind.required_env_var(mode).is_some()
}

/// Resolve the effective auth mode for the panel via the kind-specific
/// resolver in `crate::config`. Agent kinds go through `resolve_mode`;
/// Github routes through `resolve_github_mode`.
pub(crate) fn resolve_panel_mode(
    cfg: &AppConfig,
    kind: AuthKind,
    workspace: &str,
    role: &str,
) -> AuthMode {
    match kind {
        AuthKind::Claude
        | AuthKind::Codex
        | AuthKind::Amp
        | AuthKind::Kimi
        | AuthKind::Opencode => {
            let Some(agent) = kind.agent() else {
                return AuthMode::Ignore;
            };
            let mode = crate::config::resolve_mode(cfg, agent, workspace, role);
            AuthMode::from_auth_forward(mode)
        }
        AuthKind::Github => {
            let mode = crate::config::resolve_github_mode(cfg, workspace, role);
            AuthMode::from_github(mode)
        }
        AuthKind::Zai => {
            let key_present = crate::operator_env::lookup_operator_env_raw(
                cfg,
                (!role.is_empty()).then_some(role),
                Some(workspace),
                "ZAI_API_KEY",
            )
            .is_some();
            if key_present {
                AuthMode::ApiKey
            } else {
                AuthMode::Ignore
            }
        }
    }
}

/// Mirrors launch-time semantics from
/// [`crate::app::context::eligible_roles_for_workspace`]. Roles
/// already carrying an override are NOT filtered — operators may add
/// more keys to an existing override.
pub(crate) fn eligible_agents_for_override(
    editor: &EditorState<'_>,
    config: &AppConfig,
) -> Vec<String> {
    if editor.pending.allowed_roles.is_empty() {
        config.roles.keys().cloned().collect()
    } else {
        editor.pending.allowed_roles.clone()
    }
}

/// Merge live global blocks with `editor.pending` for the active
/// workspace so the Auth panel renders pending edits before save.
pub(crate) fn synthesize_appconfig_for_auth(
    state: &EditorState<'_>,
    config: &AppConfig,
) -> AppConfig {
    let mut synthesized = AppConfig {
        claude: config.claude.clone(),
        codex: config.codex.clone(),
        amp: config.amp.clone(),
        opencode: config.opencode.clone(),
        github: config.github.clone(),
        env: config.env.clone(),
        roles: config.roles.clone(),
        ..AppConfig::default()
    };
    let ws_name = workspace_name_for_panel(state);
    synthesized
        .workspaces
        .insert(ws_name, state.pending.clone());
    synthesized
}

/// Resolve the workspace key used by the Auth panel. In Edit mode this is
/// the existing workspace name; in Create mode we use `pending_name` if set,
/// otherwise a stable placeholder ("(new workspace)") so the panel can still
/// render with the pending values populated.
pub(crate) fn workspace_name_for_panel(state: &EditorState<'_>) -> String {
    match &state.mode {
        EditorMode::Edit { name } => state.pending_name.clone().unwrap_or_else(|| name.clone()),
        EditorMode::Create => state
            .pending_name
            .clone()
            .unwrap_or_else(|| "(new workspace)".to_string()),
    }
}

/// Map a flattened auth row index (the cursor) into the
/// `AuthFormTarget` the form modal should be opened against. Returns
/// `None` for non-form rows (`AuthKindRow`, `RoleHeader`, `AddSentinel`,
/// `Spacer`) so callers can dispatch them separately.
pub(crate) fn resolve_auth_row_target(
    state: &EditorState<'_>,
    config: &AppConfig,
    row: usize,
) -> Option<AuthFormTarget> {
    let rows = auth_flat_rows(state, config);
    match rows.get(row)? {
        AuthRow::WorkspaceMode { kind } | AuthRow::WorkspaceSource { kind } => {
            Some(AuthFormTarget::Workspace { kind: *kind })
        }
        AuthRow::RoleMode { role, kind } | AuthRow::RoleSource { role, kind } => {
            Some(AuthFormTarget::WorkspaceRole {
                role: role.clone(),
                kind: *kind,
            })
        }
        _ => None,
    }
}
