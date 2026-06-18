//! Footer hint composition for root console screens and modals.

use crate::console::tui::layout::list::list_names_content_width;
use crate::console::tui::state::{ManagerListRow, ManagerState};
use jackin_config::AppConfig;
use jackin_console::tui::components::footer_hints::{
    WorkspaceFooterScrollFacts, WorkspaceListFooterFacts, selected_instance_snapshot_available,
    workspace_footer_scroll_axes, workspace_list_footer_items,
    workspace_list_footer_mode_for_facts, workspace_list_footer_row_facts,
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
    let row_facts = workspace_list_footer_row_facts(selected);
    let show_open_in_github = row_facts.selected_saved_workspace
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
        selected_instance: row_facts.selected_instance,
        preview_focused: state.preview_focused,
        selected_instance_has_snapshot: selected_instance_has_snapshot(state, selected),
        selected_saved_workspace: row_facts.selected_saved_workspace,
        selected_new_workspace: row_facts.selected_new_workspace,
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
    workspace_footer_scroll_axes(WorkspaceFooterScrollFacts {
        inline_agent_picker: state.inline_agent_picker.is_some(),
        inline_role_picker: state.inline_role_picker.is_some(),
        inline_picker_scroll_axes: inline_picker_scroll_axes(state),
        focused_block_scroll_axes: focused_block_scroll_axes(state, config, cwd),
        list_names_focused: state.list_names_focused(),
        list_names_scroll_axes: list_names_scroll_axes(state),
        show_expand,
        show_collapse,
    })
}

fn focused_block_scroll_axes(
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) -> Option<ScrollAxes> {
    let focus = state.list_scroll_focus()?;
    let body = jackin_console::tui::layout::list_body_area(state.cached_term_size);
    let columns = list_geometry::split_list_columns(body, state.list_split_pct);
    let areas = crate::console::tui::layout::list::selected_sidebar_scroll_areas(
        columns.preview,
        state,
        config,
        cwd,
    );
    Some(
        jackin_console::tui::sidebar_layout::focused_scroll_area_axes(focus.into(), areas.as_ref()),
    )
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
    selected_instance_snapshot_available(
        selected,
        |ws_idx, inst_idx| {
            state
                .workspace_active_instances(ws_idx)
                .get(inst_idx)
                .copied()
                .is_some_and(|entry| state.instance_snapshots.contains_key(&entry.container_base))
        },
        |inst_idx| {
            state
                .current_dir_active_instances()
                .get(inst_idx)
                .copied()
                .is_some_and(|entry| state.instance_snapshots.contains_key(&entry.container_base))
        },
    )
}
