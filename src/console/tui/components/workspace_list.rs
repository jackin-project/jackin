//! Root-console workspace-list display adapters.

use ratatui::{Frame, layout::Rect};

use crate::config::AppConfig;
use jackin_console::tui::screens::workspaces::view::{
    WorkspaceEnvRow, WorkspaceRoleRow, render_roles_subpanel,
};

pub(crate) fn workspace_env_rows(
    ws_config: Option<&crate::workspace::WorkspaceConfig>,
) -> Vec<WorkspaceEnvRow> {
    let mut rows = Vec::new();
    if let Some(ws) = ws_config {
        for (key, value) in &ws.env {
            rows.push(WorkspaceEnvRow {
                name: key.clone(),
                scope: None,
                is_op: matches!(value, crate::operator_env::EnvValue::OpRef(_)),
            });
        }
        for (role, overrides) in &ws.roles {
            for (key, value) in &overrides.env {
                rows.push(WorkspaceEnvRow {
                    name: key.clone(),
                    scope: Some(role.clone()),
                    is_op: matches!(value, crate::operator_env::EnvValue::OpRef(_)),
                });
            }
        }
    }
    rows
}

#[cfg(test)]
pub(crate) fn render_agents_subpanel(
    frame: &mut Frame,
    area: Rect,
    ws_config: Option<&crate::workspace::WorkspaceConfig>,
    config: &AppConfig,
) {
    render_agents_subpanel_scrollable(frame, area, ws_config, config, 0, 0, false);
}

pub(crate) fn render_agents_subpanel_scrollable(
    frame: &mut Frame,
    area: Rect,
    ws_config: Option<&crate::workspace::WorkspaceConfig>,
    config: &AppConfig,
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    let allowed = ws_config.map_or(&[][..], |w| w.allowed_roles.as_slice());
    let all_allowed = ws_config.is_none_or(jackin_console::workspace::allows_all_agents);
    let default = ws_config.and_then(|w| w.default_role.as_deref());

    let agent_names: Vec<&str> = if all_allowed {
        config.roles.keys().map(String::as_str).collect()
    } else {
        allowed.iter().map(String::as_str).collect()
    };
    let rows = agent_names
        .into_iter()
        .map(|role| WorkspaceRoleRow {
            name: role.to_string(),
            exists: config.roles.contains_key(role),
            is_default: Some(role) == default,
            scoped_mount_count: role_scoped_mount_count(config, role),
        })
        .collect();
    render_roles_subpanel(frame, area, default, rows, scroll_x, scroll_y, focused);
}

fn role_scoped_mount_count(config: &AppConfig, role: &str) -> usize {
    if let Ok(selector) = crate::selector::RoleSelector::parse(role) {
        config
            .resolve_mount_rows(&selector)
            .into_iter()
            .filter(|row| row.scope.is_some())
            .count()
    } else {
        0
    }
}
