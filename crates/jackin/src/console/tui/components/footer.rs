//! Footer hint composition for root console screens and modals.

use crate::config::AppConfig;
use crate::console::tui::layout::list::list_names_content_width;
use crate::console::tui::state::{ManagerListRow, ManagerState};
use jackin_console::tui::components::footer_hints::{
    WorkspaceListFooterFacts, workspace_list_footer_items, workspace_list_footer_mode_for_facts,
};
use jackin_tui::HintSpan;

pub(crate) mod editor;
pub(crate) mod modal;
pub(crate) mod settings;

pub(crate) fn workspace_list_footer_items_for_state(
    state: &ManagerState<'_>,
    config: &AppConfig,
) -> Vec<HintSpan<'static>> {
    workspace_list_footer_items(workspace_list_footer_mode_for_facts(
        workspace_list_footer_facts(state, config),
    ))
}

fn workspace_list_footer_facts(
    state: &ManagerState<'_>,
    config: &AppConfig,
) -> WorkspaceListFooterFacts {
    let scroll_focused = state.list_scroll_focus().is_some();
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
    let show_horizontal_scroll = state.list_names_focused()
        && !show_expand
        && !show_collapse
        && list_names_scrollable(state);

    WorkspaceListFooterFacts {
        scroll_focused,
        inline_agent_picker: state.inline_agent_picker.is_some(),
        inline_role_picker: state.inline_role_picker.is_some(),
        selected_instance,
        preview_focused: state.preview_focused,
        selected_instance_has_snapshot: selected_instance_has_snapshot(state, selected),
        selected_saved_workspace: is_saved,
        selected_new_workspace: matches!(selected, ManagerListRow::NewWorkspace),
        show_expand,
        show_collapse,
        show_horizontal_scroll,
        show_open_in_github,
    }
}

fn list_names_scrollable(state: &ManagerState<'_>) -> bool {
    let body = jackin_console::tui::layout::list_body_area(state.cached_term_size);
    let columns =
        jackin_console::tui::list_geometry::split_list_columns(body, state.list_split_pct);
    let viewport = jackin_console::tui::layout::scroll_viewport_width(columns.names);
    let content = list_names_content_width(state, viewport);
    jackin_tui::components::scrollable_panel::max_offset(content, viewport) > 0
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
