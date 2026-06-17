//! Footer hint composition for root console screens and modals.

use crate::config::AppConfig;
use crate::console::tui::layout::list::list_names_content_width;
use crate::console::tui::state::{ManagerListRow, ManagerState};
use jackin_console::tui::components::footer_hints::{
    WorkspaceListFooterFacts, workspace_list_footer_items, workspace_list_footer_mode_for_facts,
};
use jackin_console::tui::list_geometry;
use jackin_console::tui::screens::workspaces::update::{
    workspace_row_owns_left, workspace_row_owns_right,
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
    let show_expand = workspace_row_owns_right(
        selected,
        state.current_dir_expanded,
        state.has_current_dir_active_instances(),
        |idx| state.is_workspace_expanded(idx),
        |idx| !state.workspace_active_instances(idx).is_empty(),
    );
    let show_collapse = workspace_row_owns_left(
        selected,
        state.current_dir_expanded,
        state.has_current_dir_active_instances(),
        |idx| state.is_workspace_expanded(idx),
    );
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
        let columns = list_geometry::split_list_columns(body, state.list_split_pct);
        let areas = crate::console::tui::layout::list::selected_sidebar_scroll_areas(
            columns.preview,
            state,
            config,
            cwd,
        );
        return jackin_console::tui::sidebar_layout::focused_scroll_area_axes(
            focus.into(),
            areas.as_ref(),
        );
    }
    if state.list_names_focused() && !show_expand && !show_collapse {
        return list_names_scroll_axes(state);
    }
    ScrollAxes::none()
}

fn inline_picker_scroll_axes(state: &ManagerState<'_>) -> ScrollAxes {
    let body = jackin_console::tui::layout::list_body_area(state.cached_term_size);
    let columns = list_geometry::split_list_columns(body, state.list_split_pct);
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
    list_geometry::vertical_scroll_axes(content, columns.names)
}

fn list_names_scroll_axes(state: &ManagerState<'_>) -> ScrollAxes {
    let body = jackin_console::tui::layout::list_body_area(state.cached_term_size);
    let columns = list_geometry::split_list_columns(body, state.list_split_pct);
    let viewport = jackin_console::tui::layout::scroll_viewport_width(columns.names);
    let content = list_names_content_width(state, viewport);
    list_geometry::list_names_scroll_axes(content, columns.names)
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
