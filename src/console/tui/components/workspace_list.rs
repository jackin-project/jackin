//! Root-console workspace-list display adapters.

use ratatui::{Frame, layout::Rect, text::Line};

use crate::config::AppConfig;
use crate::console::tui::state::{ManagerListRow, ManagerState};
use jackin_console::tui::screens::workspaces::view::{
    WorkspaceEnvRow, WorkspaceListDisplayRow, WorkspaceListRowTone, WorkspaceRoleRow,
    list_name_lines as workspace_list_name_lines, render_roles_subpanel,
};

pub(crate) fn list_name_lines(
    state: &ManagerState<'_>,
    viewport: usize,
) -> (Vec<Line<'static>>, usize) {
    let visual_rows = state.visual_rows_vec();
    let visual_selected = state.visual_selected();
    let hovered_row = state.hovered_list_row;
    let display_rows: Vec<Option<WorkspaceListDisplayRow>> = visual_rows
        .iter()
        .enumerate()
        .map(|(idx, visual_row)| {
            visual_row.as_ref().and_then(|row| {
                workspace_list_display_row(
                    state,
                    row,
                    idx == visual_selected,
                    hovered_row == Some(*row),
                )
            })
        })
        .collect();
    workspace_list_name_lines(&display_rows, viewport, state.list_names_focused)
}

fn workspace_list_display_row(
    state: &ManagerState<'_>,
    row: &ManagerListRow,
    selected: bool,
    hovered: bool,
) -> Option<WorkspaceListDisplayRow> {
    match row {
        ManagerListRow::CurrentDirectory => Some(WorkspaceListDisplayRow {
            label: "Current directory".to_string(),
            tone: WorkspaceListRowTone::White,
            expanded: state.current_dir_expanded,
            has_instances: state.has_current_dir_active_instances(),
            selected,
            hovered,
        }),
        ManagerListRow::CurrentDirectoryInstance(inst_idx) => state
            .current_dir_active_instances()
            .get(*inst_idx)
            .map(|entry| instance_display_row(&entry.instance_id, &entry.role_key, selected, hovered)),
        ManagerListRow::SavedWorkspace(i) => {
            let ws = state.workspaces.get(*i)?;
            Some(WorkspaceListDisplayRow {
                label: ws.name.clone(),
                tone: WorkspaceListRowTone::Workspace,
                expanded: state.is_workspace_expanded(*i),
                has_instances: state.has_active_instances(*i),
                selected,
                hovered,
            })
        }
        ManagerListRow::WorkspaceInstance(ws_idx, inst_idx) => state
            .workspace_active_instances(*ws_idx)
            .get(*inst_idx)
            .map(|entry| instance_display_row(&entry.instance_id, &entry.role_key, selected, hovered)),
        ManagerListRow::NewWorkspace => Some(WorkspaceListDisplayRow {
            label: "+ New workspace".to_string(),
            tone: WorkspaceListRowTone::White,
            expanded: false,
            has_instances: false,
            selected,
            hovered,
        }),
    }
}

fn instance_display_row(
    instance_id: &str,
    role_key: &str,
    selected: bool,
    hovered: bool,
) -> WorkspaceListDisplayRow {
    WorkspaceListDisplayRow {
        label: format!("{instance_id}  {role_key}"),
        tone: WorkspaceListRowTone::Instance,
        expanded: false,
        has_instances: false,
        selected,
        hovered,
    }
}

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
