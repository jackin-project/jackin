//! Root-console workspace-list display adapters.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::Line,
};

use crate::config::AppConfig;
use crate::console::tui::components::mount_display::format_mount_rows_with_cache;
use crate::console::tui::layout::list::{
    SidebarInputs, SidebarLayout, compute_sidebar_layout, sidebar_inputs_for_current_dir,
    sidebar_inputs_for_workspace, split_global_mount_rows,
};
use crate::console::tui::state::{
    ManagerListRow, ManagerState, MountInfoCache, MountScrollFocus, WorkspaceSummary,
};
use jackin_console::tui::screens::workspaces::view::{
    WorkspaceEnvRow, WorkspaceInstancePane, WorkspaceInstancePaneContent, WorkspaceInstanceSessionRow,
    WorkspaceInstanceTab, WorkspaceInstanceTabPane,
    WorkspaceListDisplayRow, WorkspaceListRowTone, WorkspaceRoleRow,
    current_directory_display_row, current_directory_workspace_title, global_mounts_title,
    instance_sessions_empty_message, list_name_lines as workspace_list_name_lines,
    new_workspace_display_row, picker_sidebar_title, provider_picker_title,
    render_compact_instances_summary, render_environments_subpanel, render_general_subpanel,
    render_global_mounts_subpanel, render_mounts_subpanel as render_workspace_mounts_panel,
    render_instance_details_pane as render_workspace_instance_details_pane,
    render_list_names_block, render_picker_sidebar, render_roles_subpanel,
    render_sentinel_description_pane, role_global_mounts_title, workspace_instance_list_label,
    workspace_instance_pane_agent_label,
};

#[allow(clippy::too_many_lines)]
pub(crate) fn render_list_body(
    frame: &mut Frame,
    area: Rect,
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    // See ManagerListRow docs for row layout.
    // Split driven by `state.list_split_pct` (default 30), adjustable via
    // mouse-drag on the seam column. Keeps the right pane visible on every
    // row. Row-specific right-pane renderers:
    //   CurrentDirectory  → current-dir details
    //   SavedWorkspace(i) → saved-workspace details
    //   NewWorkspace      → description-of-what-a-workspace-is pane
    let left_pct = state.list_split_pct;
    let right_pct = 100u16.saturating_sub(left_pct);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(right_pct),
        ])
        .split(area);
    let list_area = columns[0];

    match state.selected_row() {
        ManagerListRow::CurrentDirectory => {
            render_current_dir_details_pane(frame, columns[1], cwd, config, state);
        }
        ManagerListRow::NewWorkspace => {
            render_sentinel_description_pane(frame, columns[1]);
        }
        ManagerListRow::SavedWorkspace(i) => {
            if let Some(ws) = state.workspaces.get(i).cloned() {
                render_details_pane(frame, columns[1], &ws, config, state);
            }
        }
        ManagerListRow::WorkspaceInstance(ws_idx, inst_idx) => {
            let instances = state.workspace_active_instances(ws_idx);
            if let Some(entry) = instances.get(inst_idx).copied() {
                let sessions = state.sessions_for_instance(&entry.container_base);
                let session_load_error = state.has_session_load_error(&entry.container_base);
                let snapshot = state.snapshot_for_instance(&entry.container_base);
                let selected_pane = if state.preview_focused {
                    state
                        .preview_selected_pane(&entry.container_base)
                        .map(|(_, id)| id)
                } else {
                    None
                };
                render_instance_details_pane(
                    frame,
                    columns[1],
                    entry,
                    sessions,
                    session_load_error,
                    snapshot,
                    selected_pane,
                    state.preview_focused,
                );
            }
        }
        ManagerListRow::CurrentDirectoryInstance(inst_idx) => {
            let instances = state.current_dir_active_instances();
            if let Some(entry) = instances.get(inst_idx).copied() {
                let sessions = state.sessions_for_instance(&entry.container_base);
                let session_load_error = state.has_session_load_error(&entry.container_base);
                let snapshot = state.snapshot_for_instance(&entry.container_base);
                let selected_pane = if state.preview_focused {
                    state
                        .preview_selected_pane(&entry.container_base)
                        .map(|(_, id)| id)
                } else {
                    None
                };
                render_instance_details_pane(
                    frame,
                    columns[1],
                    entry,
                    sessions,
                    session_load_error,
                    snapshot,
                    selected_pane,
                    state.preview_focused,
                );
            }
        }
    }

    render_list_sidebar(frame, list_area, state);
}

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
        ManagerListRow::CurrentDirectory => Some(current_directory_display_row(
            state.current_dir_expanded,
            state.has_current_dir_active_instances(),
            selected,
            hovered,
        )),
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
        ManagerListRow::NewWorkspace => Some(new_workspace_display_row(selected, hovered)),
    }
}

fn instance_display_row(
    instance_id: &str,
    role_key: &str,
    selected: bool,
    hovered: bool,
) -> WorkspaceListDisplayRow {
    WorkspaceListDisplayRow {
        label: workspace_instance_list_label(instance_id, role_key),
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

pub(crate) fn instance_details_pane(
    entry: &crate::instance::InstanceIndexEntry,
    sessions: &[crate::instance::SessionRecord],
    session_load_error: bool,
    snapshot: Option<&crate::runtime::snapshot::InstanceSnapshot>,
    selected_pane: Option<u64>,
    preview_focused: bool,
) -> WorkspaceInstancePane {
    WorkspaceInstancePane {
        instance_id: entry.instance_id.clone(),
        focused: preview_focused,
        content: instance_details_content(sessions, session_load_error, snapshot, selected_pane),
    }
}

fn instance_details_content(
    sessions: &[crate::instance::SessionRecord],
    session_load_error: bool,
    snapshot: Option<&crate::runtime::snapshot::InstanceSnapshot>,
    selected_pane: Option<u64>,
) -> WorkspaceInstancePaneContent {
    if let Some(snapshot) = snapshot {
        return WorkspaceInstancePaneContent::Live {
            tabs: snapshot
                .tabs
                .iter()
                .enumerate()
                .map(|(tab_idx, tab)| WorkspaceInstanceTab {
                    index: tab_idx,
                    label: tab.label.clone(),
                    active: tab_idx == snapshot.active_tab as usize,
                    panes: tab
                        .panes
                        .iter()
                        .map(|pane| WorkspaceInstanceTabPane {
                            label: pane.label.clone(),
                            agent_label: workspace_instance_pane_agent_label(pane.agent.as_deref()),
                            state_label: pane.state.label().to_string(),
                            focused: pane.session_id == tab.focused_pane,
                            selected: selected_pane == Some(pane.session_id),
                        })
                        .collect(),
                })
                .collect(),
        };
    }
    if sessions.is_empty() {
        return WorkspaceInstancePaneContent::Empty {
            message: instance_sessions_empty_message(session_load_error).to_string(),
        };
    }
    WorkspaceInstancePaneContent::Sessions {
        rows: sessions
            .iter()
            .map(|session| WorkspaceInstanceSessionRow {
                name: session.tmux_name.clone(),
                agent_runtime: session.agent_runtime.clone(),
            })
            .collect(),
    }
}

pub(crate) fn render_list_sidebar(frame: &mut Frame, area: Rect, state: &ManagerState<'_>) {
    if let Some(picker) = state.inline_provider_picker.as_ref() {
        let short_id = crate::instance::naming::instance_id_from_container_base(&picker.context)
            .unwrap_or(picker.context.as_str());
        render_provider_picker_sidebar(
            frame,
            area,
            Some(short_id),
            picker.providers(),
            picker.selected(),
        );
    } else if let Some(picker) = state.launch_provider_picker.as_ref() {
        render_provider_picker_sidebar(frame, area, None, picker.providers(), picker.selected());
    } else if let Some((container, picker, _providers)) = state.inline_new_session_picker.as_ref() {
        let short_id = crate::instance::naming::instance_id_from_container_base(container)
            .unwrap_or(container);
        render_agent_picker_sidebar(frame, area, short_id, picker, state.list_names_focused);
    } else if let Some((role, picker)) = state.inline_agent_picker.as_ref() {
        render_agent_picker_sidebar(frame, area, &role.key(), picker, state.list_names_focused);
    } else if let Some(picker) = state.inline_role_picker.as_ref() {
        let title = state
            .selected_workspace_summary()
            .map_or(current_directory_workspace_title(), |summary| summary.name.as_str());
        render_role_picker_sidebar(frame, area, title, picker, state.list_names_focused);
    } else {
        let (list_lines, content_width) = list_name_lines(
            state,
            jackin_tui::components::scrollable_panel::viewport_width(area),
        );
        render_list_names_block(
            frame,
            area,
            list_lines,
            content_width,
            state.list_names_focused,
            state.list_names_scroll_x,
        );
    }
}

pub(crate) fn render_details_pane(
    frame: &mut Frame,
    area: Rect,
    ws: &WorkspaceSummary,
    config: &AppConfig,
    state: &ManagerState<'_>,
) {
    let inputs = sidebar_inputs_for_workspace(ws, config, state);
    let layout = compute_sidebar_layout(area, &inputs);
    render_sidebar_body(frame, &layout, &inputs, config, state);
}

pub(crate) fn render_current_dir_details_pane(
    frame: &mut Frame,
    area: Rect,
    cwd: &std::path::Path,
    config: &AppConfig,
    state: &ManagerState<'_>,
) {
    let cwd_str = cwd.display().to_string();
    let mounts = [crate::console::domain::current_dir_mount_config(&cwd_str)];
    let inputs = sidebar_inputs_for_current_dir(&cwd_str, &mounts, config, state);
    let layout = compute_sidebar_layout(area, &inputs);
    render_sidebar_body(frame, &layout, &inputs, config, state);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_instance_details_pane(
    frame: &mut Frame,
    area: Rect,
    entry: &crate::instance::InstanceIndexEntry,
    sessions: &[crate::instance::SessionRecord],
    session_load_error: bool,
    snapshot: Option<&crate::runtime::snapshot::InstanceSnapshot>,
    selected_pane: Option<u64>,
    preview_focused: bool,
) {
    let pane = instance_details_pane(
        entry,
        sessions,
        session_load_error,
        snapshot,
        selected_pane,
        preview_focused,
    );
    render_workspace_instance_details_pane(frame, area, &pane);
}

pub(crate) fn render_provider_picker_sidebar(
    frame: &mut Frame,
    area: Rect,
    container_id: Option<&str>,
    providers: &[jackin_protocol::Provider],
    selected: usize,
) {
    let title = provider_picker_title(container_id);
    let labels = providers
        .iter()
        .map(|provider| provider.label().to_string())
        .collect();
    render_picker_sidebar(frame, area, &title, labels, Some(selected), false);
}

pub(crate) fn render_role_picker_sidebar(
    frame: &mut Frame,
    area: Rect,
    workspace_name: &str,
    picker: &crate::selector::RolePickerState,
    focused: bool,
) {
    let title = picker_sidebar_title(workspace_name);
    let labels = picker.filtered.iter().map(|role| role.key()).collect();
    render_picker_sidebar(frame, area, &title, labels, picker.list_state.selected, focused);
}

pub(crate) fn render_agent_picker_sidebar(
    frame: &mut Frame,
    area: Rect,
    role_name: &str,
    picker: &crate::agent::AgentChoiceState,
    focused: bool,
) {
    let title = picker_sidebar_title(role_name);
    let labels = picker
        .choices
        .iter()
        .map(|agent| {
            jackin_console::tui::components::agent_choice::agent_picker_label(*agent).to_string()
        })
        .collect();
    let selected =
        picker
            .choices
            .iter()
            .position(|agent| *agent == picker.focused);
    render_picker_sidebar(frame, area, &title, labels, selected, focused);
}

pub(crate) fn render_mounts_subpanel(
    frame: &mut Frame,
    area: Rect,
    mounts: &[crate::workspace::MountConfig],
    cache: &MountInfoCache,
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    let rows = format_mount_rows_with_cache(mounts, cache);
    render_workspace_mounts_panel(frame, area, &rows, scroll_x, scroll_y, focused);
}

pub(crate) fn render_global_mount_rows_section(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    rows: &[&crate::config::GlobalMountRow],
    cache: &MountInfoCache,
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    let mounts: Vec<crate::workspace::MountConfig> =
        rows.iter().map(|row| row.mount.clone()).collect();
    let display_rows = format_mount_rows_with_cache(&mounts, cache);
    render_global_mounts_subpanel(frame, area, title, &display_rows, scroll_x, scroll_y, focused);
}

pub(crate) fn render_sidebar_body(
    frame: &mut Frame,
    layout: &SidebarLayout,
    inputs: &SidebarInputs<'_>,
    config: &AppConfig,
    state: &ManagerState<'_>,
) {
    if let Some(area) = layout.instances {
        render_compact_instances_summary(
            frame,
            area,
            inputs.instance_count,
            inputs.instance_expanded,
        );
    }
    render_general_subpanel(
        frame,
        layout.general,
        &crate::tui::shorten_home(inputs.workdir),
    );
    let ws_focused = state.list_scroll_focus == Some(MountScrollFocus::Workspace);
    render_mounts_subpanel(
        frame,
        layout.mounts,
        inputs.mounts,
        &inputs.mount_info_cache,
        state.list_mounts_scroll_x,
        state.list_mounts_scroll_y,
        ws_focused,
    );
    if layout.global.is_some() || layout.role_global.is_some() {
        let global_focused = state.list_scroll_focus;
        let (global_rows, role_global_rows) = split_global_mount_rows(&inputs.global_rows);
        if let Some(area) = layout.global {
            render_global_mount_rows_section(
                frame,
                area,
                global_mounts_title(),
                &global_rows,
                &inputs.mount_info_cache,
                state.list_global_mounts_scroll_x,
                state.list_global_mounts_scroll_y,
                global_focused == Some(MountScrollFocus::Global),
            );
        }
        if let Some(area) = layout.role_global {
            let title = role_global_mounts_title(&inputs.picker_role_label);
            render_global_mount_rows_section(
                frame,
                area,
                &title,
                &role_global_rows,
                &inputs.mount_info_cache,
                state.list_role_global_mounts_scroll_x,
                state.list_role_global_mounts_scroll_y,
                global_focused == Some(MountScrollFocus::RoleGlobal),
            );
        }
    }
    if let Some(area) = layout.env {
        render_environments_subpanel(frame, area, workspace_env_rows(inputs.ws_config));
    }
    if let Some(area) = layout.roles {
        let roles_focused = state.list_scroll_focus == Some(MountScrollFocus::Roles);
        render_agents_subpanel_scrollable(
            frame,
            area,
            inputs.ws_config,
            config,
            state.list_roles_scroll_x,
            state.list_roles_scroll_y,
            roles_focused,
        );
    }
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
