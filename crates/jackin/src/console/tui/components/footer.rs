//! Footer hint composition for root console screens and modals.

use crate::config::AppConfig;
use crate::console::tui::layout::list::list_names_content_width;
use crate::console::tui::state::{ManagerListRow, ManagerState};
use jackin_console::tui::components::footer_hints::{
    WorkspaceListFooterFacts, workspace_list_footer_items, workspace_list_footer_mode_for_facts,
};
use jackin_tui::{HintSpan, components::ScrollAxes};

pub(crate) mod editor;
pub(crate) mod settings;

pub(crate) fn workspace_list_footer_items_for_state(
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) -> Vec<HintSpan<'static>> {
    workspace_list_footer_items(workspace_list_footer_mode_for_facts(
        workspace_list_footer_facts(state, config, cwd),
    ))
}

fn workspace_list_footer_facts(
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) -> WorkspaceListFooterFacts {
    let selected = state.selected_row();
    let selected_instance = matches!(
        selected,
        ManagerListRow::WorkspaceInstance(_, _) | ManagerListRow::CurrentDirectoryInstance(_)
    );
    let is_saved = matches!(selected, ManagerListRow::SavedWorkspace(_));
    let show_open_in_github = is_saved
        && state
            .selected_workspace_summary()
            .and_then(|s| config.workspaces.get(&s.name))
            .is_some_and(|ws| {
                !jackin_console::github_mounts::resolve_for_workspace_from_cache(
                    ws,
                    &state.mount_info_cache,
                )
                .is_empty()
            });
    let show_expand = match selected {
        ManagerListRow::CurrentDirectory => {
            state.has_current_dir_active_instances() && !state.current_dir_expanded
        }
        ManagerListRow::SavedWorkspace(i) => {
            !state.workspace_active_instances(i).is_empty() && !state.is_workspace_expanded(i)
        }
        ManagerListRow::CurrentDirectoryInstance(_)
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::NewWorkspace => false,
    };
    let show_collapse = match selected {
        ManagerListRow::CurrentDirectory => {
            state.has_current_dir_active_instances() && state.current_dir_expanded
        }
        ManagerListRow::SavedWorkspace(i) => state.is_workspace_expanded(i),
        ManagerListRow::CurrentDirectoryInstance(_) | ManagerListRow::WorkspaceInstance(_, _) => {
            true
        }
        ManagerListRow::NewWorkspace => false,
    };
    let workspace_scroll_axes =
        workspace_scroll_axes(state, config, cwd, show_expand, show_collapse);

    WorkspaceListFooterFacts {
        inline_agent_picker: state.inline_agent_picker.is_some(),
        inline_role_picker: state.inline_role_picker.is_some(),
        selected_instance,
        preview_focused: state.preview_focused,
        selected_instance_has_snapshot: selected_instance_has_snapshot(state, selected),
        selected_saved_workspace: is_saved,
        selected_new_workspace: matches!(selected, ManagerListRow::NewWorkspace),
        show_expand,
        show_collapse,
        workspace_scroll_axes,
        show_open_in_github,
    }
}

fn workspace_scroll_axes(
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
    show_expand: bool,
    show_collapse: bool,
) -> ScrollAxes {
    if state.inline_agent_picker.is_some() || state.inline_role_picker.is_some() {
        return inline_picker_scroll_axes(state);
    }
    if let Some(focus) = state.list_scroll_focus() {
        let body = jackin_console::tui::layout::list_body_area(state.cached_term_size);
        let columns =
            jackin_console::tui::list_geometry::split_list_columns(body, state.list_split_pct);
        let areas = crate::console::tui::layout::list::selected_sidebar_scroll_areas(
            columns.preview,
            state,
            config,
            cwd,
        );
        return focused_sidebar_scroll_axes(focus.into(), areas.as_ref());
    }
    if state.list_names_focused() && !show_expand && !show_collapse {
        return list_names_scroll_axes(state);
    }
    ScrollAxes::none()
}

fn inline_picker_scroll_axes(state: &ManagerState<'_>) -> ScrollAxes {
    let body = jackin_console::tui::layout::list_body_area(state.cached_term_size);
    let columns =
        jackin_console::tui::list_geometry::split_list_columns(body, state.list_split_pct);
    let viewport = jackin_tui::components::scrollable_panel::viewport_height(columns.names);
    let content = state
        .inline_agent_picker
        .as_ref()
        .map(|(_, picker)| picker.choices.len())
        .or_else(|| {
            state
                .inline_role_picker
                .as_ref()
                .map(|picker| picker.filtered.len())
        })
        .unwrap_or(0);
    ScrollAxes {
        vertical: jackin_tui::components::scrollable_panel::is_scrollable(content, viewport),
        horizontal: false,
    }
}

fn focused_sidebar_scroll_axes(
    focus: jackin_console::tui::sidebar_layout::SidebarScrollFocus,
    areas: Option<&jackin_console::tui::sidebar_layout::SidebarScrollAreas>,
) -> ScrollAxes {
    let Some(areas) = areas else {
        return ScrollAxes::none();
    };
    match focus {
        jackin_console::tui::sidebar_layout::SidebarScrollFocus::Workspace => {
            jackin_console::tui::sidebar_layout::scroll_area_axes(areas.workspace)
        }
        jackin_console::tui::sidebar_layout::SidebarScrollFocus::Global => {
            jackin_console::tui::sidebar_layout::scroll_area_axes(areas.global)
        }
        jackin_console::tui::sidebar_layout::SidebarScrollFocus::RoleGlobal => {
            areas.role_global.map_or_else(
                ScrollAxes::none,
                jackin_console::tui::sidebar_layout::scroll_area_axes,
            )
        }
        jackin_console::tui::sidebar_layout::SidebarScrollFocus::Roles => areas.roles.map_or_else(
            ScrollAxes::none,
            jackin_console::tui::sidebar_layout::scroll_area_axes,
        ),
    }
}

fn list_names_scroll_axes(state: &ManagerState<'_>) -> ScrollAxes {
    let body = jackin_console::tui::layout::list_body_area(state.cached_term_size);
    let columns =
        jackin_console::tui::list_geometry::split_list_columns(body, state.list_split_pct);
    let viewport = jackin_console::tui::layout::scroll_viewport_width(columns.names);
    let content = list_names_content_width(state, viewport);
    ScrollAxes {
        horizontal: jackin_tui::components::scrollable_panel::max_offset(content, viewport) > 0,
        vertical: false,
    }
}

fn selected_instance_has_snapshot(state: &ManagerState<'_>, selected: ManagerListRow) -> bool {
    match selected {
        ManagerListRow::WorkspaceInstance(ws_idx, inst_idx) => state
            .workspace_active_instances(ws_idx)
            .get(inst_idx)
            .copied()
            .is_some_and(|entry| state.instance_snapshots.contains_key(&entry.container_base)),
        ManagerListRow::CurrentDirectoryInstance(inst_idx) => state
            .current_dir_active_instances()
            .get(inst_idx)
            .copied()
            .is_some_and(|entry| state.instance_snapshots.contains_key(&entry.container_base)),
        _ => false,
    }
}
