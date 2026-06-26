//! Concrete footer adapters for workspace-style screens.
//!
//! Reads `ManagerState` fields and assembles the facts structs needed by
//! `footer_hints` so callers do not have to build them by hand.
use jackin_config::AppConfig;
use jackin_tui::{HintSpan, components::ScrollAxes};
use ratatui::layout::Rect;

use crate::tui::app::ConsoleManagerStage;
use crate::tui::components::footer_hints::{
    WorkspaceFooterScrollFacts, WorkspaceInlinePickerContentFacts, WorkspaceListFooterInputFacts,
    WorkspaceScreenFooterFacts, WorkspaceScreenFooterPlan, selected_instance_snapshot_available,
    workspace_footer_scroll_axes, workspace_inline_picker_content_height,
    workspace_list_footer_facts, workspace_list_footer_items, workspace_list_footer_mode_for_facts,
    workspace_list_open_github_visible, workspace_screen_footer_items,
    workspace_screen_footer_plan,
};
use crate::tui::layout;
use crate::tui::layout::list::{list_names_content_width, selected_sidebar_scroll_areas};
use crate::tui::list_geometry;
use crate::tui::screens::workspaces::model::ManagerListRow;
use crate::tui::screens::workspaces::update::{workspace_row_owns_left, workspace_row_owns_right};
use crate::tui::sidebar_layout;
use crate::tui::state::ManagerState;

/// Build the workspace list screen footer hints from a concrete `ManagerState`.
#[must_use]
pub fn workspace_list_footer_items_for_state(
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) -> Vec<HintSpan<'static>> {
    workspace_list_footer_items(workspace_list_footer_mode_for_facts(
        workspace_list_footer_facts_for_state(state, config, cwd),
    ))
}

fn workspace_list_footer_facts_for_state(
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) -> crate::tui::components::footer_hints::WorkspaceListFooterFacts {
    let selected = state.selected_row();
    let selected_workspace_has_github_mounts = state
        .selected_workspace_summary()
        .and_then(|s| config.workspaces.get(&s.name))
        .is_some_and(|ws| {
            !crate::github_mounts::resolve_for_workspace_from_cache(ws, &state.mount_info_cache)
                .is_empty()
        });
    let show_open_in_github =
        workspace_list_open_github_visible(selected, selected_workspace_has_github_mounts);
    let show_expand = workspace_row_owns_right(
        selected,
        state.current_dir_expanded,
        state.has_current_dir_visible_instances(),
        |idx| state.is_workspace_expanded(idx),
        |idx| state.has_visible_instances(idx),
    );
    let show_collapse = workspace_row_owns_left(
        selected,
        state.current_dir_expanded,
        state.has_current_dir_visible_instances(),
        |idx| state.is_workspace_expanded(idx),
    );
    let workspace_scroll_axes =
        workspace_scroll_axes(state, config, cwd, show_expand, show_collapse);

    workspace_list_footer_facts(WorkspaceListFooterInputFacts {
        selected_row: selected,
        inline_agent_picker: state.inline_agent_picker.is_some(),
        inline_role_picker: state.inline_role_picker.is_some(),
        preview_focused: state.preview_focused,
        selected_instance_has_snapshot: selected_instance_has_snapshot(state, selected),
        selected_instance_is_live: selected_instance_is_live(state, selected),
        show_expand,
        show_collapse,
        workspace_scroll_axes,
        show_open_in_github,
    })
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
    let body = layout::list_body_area(state.cached_term_size);
    let columns = list_geometry::split_list_columns(body, state.list_split_pct);
    let areas = selected_sidebar_scroll_areas(columns.preview, state, config, cwd);
    Some(sidebar_layout::focused_scroll_area_axes(
        focus.into(),
        areas.as_ref(),
    ))
}

fn inline_picker_scroll_axes(state: &ManagerState<'_>) -> ScrollAxes {
    let content = workspace_inline_picker_content_height(WorkspaceInlinePickerContentFacts {
        agent_picker_count: state
            .inline_agent_picker
            .as_ref()
            .map(|(_, picker)| picker.choices.len()),
        role_picker_count: state
            .inline_role_picker
            .as_ref()
            .map(|picker| picker.filtered.len()),
    });
    list_geometry::workspace_inline_picker_scroll_axes(
        content,
        state.cached_term_size,
        state.list_split_pct,
    )
}

fn list_names_scroll_axes(state: &ManagerState<'_>) -> ScrollAxes {
    let viewport = list_geometry::workspace_list_names_viewport_width(
        state.cached_term_size,
        state.list_split_pct,
    );
    let content = list_names_content_width(state, viewport);
    list_geometry::workspace_list_names_scroll_axes(
        content,
        state.cached_term_size,
        state.list_split_pct,
    )
}

fn selected_instance_has_snapshot(state: &ManagerState<'_>, selected: ManagerListRow) -> bool {
    selected_instance_snapshot_available(
        selected,
        |ws_idx, inst_idx| {
            state
                .workspace_visible_instances(ws_idx)
                .get(inst_idx)
                .copied()
                .is_some_and(|entry| state.instance_snapshots.contains_key(&entry.container_base))
        },
        |inst_idx| {
            state
                .current_dir_visible_instances()
                .get(inst_idx)
                .copied()
                .is_some_and(|entry| state.instance_snapshots.contains_key(&entry.container_base))
        },
    )
}

/// Whether the selected tree row is a live (`Active`/`Running`) instance. Drives
/// which instance-row footer actions to surface: live rows offer new-session /
/// shell / stop; failed/stopped rows offer restart instead (D15).
fn selected_instance_is_live(state: &ManagerState<'_>, selected: ManagerListRow) -> bool {
    let is_live = |entry: &jackin_core::instance::InstanceIndexEntry| {
        matches!(
            entry.status,
            jackin_core::instance::InstanceStatus::Active
                | jackin_core::instance::InstanceStatus::Running
        )
    };
    selected_instance_snapshot_available(
        selected,
        |ws_idx, inst_idx| {
            state
                .workspace_visible_instances(ws_idx)
                .get(inst_idx)
                .copied()
                .is_some_and(is_live)
        },
        |inst_idx| {
            state
                .current_dir_visible_instances()
                .get(inst_idx)
                .copied()
                .is_some_and(is_live)
        },
    )
}

#[cfg(test)]
mod tests;

/// Build the workspace-screen footer hints for any workspace-style screen
/// (list, create-prelude, destructive confirm) from a concrete `ManagerState`.
///
/// Delegates footer item assembly to `workspace_screen_footer_items`. The
/// `area` is passed through to modal footer adapters so scroll-sensitive
/// modals can measure their available height.
#[must_use]
pub fn workspace_screen_footer_items_for_state(
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
    area: Rect,
) -> Vec<HintSpan<'static>> {
    let facts = match workspace_screen_footer_plan(state.stage.route()) {
        WorkspaceScreenFooterPlan::List => WorkspaceScreenFooterFacts::List {
            list_items: workspace_list_footer_items_for_state(state, config, cwd),
            modal_items: state
                .list_modal
                .as_ref()
                .map(|modal| modal.footer_items_for_area(false, area)),
        },
        WorkspaceScreenFooterPlan::CreatePrelude => {
            let ConsoleManagerStage::CreatePrelude(prelude) = &state.stage else {
                unreachable!("CreatePrelude route must have CreatePrelude state")
            };
            WorkspaceScreenFooterFacts::CreatePrelude {
                modal_items: prelude
                    .modal
                    .as_ref()
                    .map(|modal| modal.footer_items(false)),
            }
        }
        WorkspaceScreenFooterPlan::DestructiveConfirm => {
            WorkspaceScreenFooterFacts::DestructiveConfirm
        }
        WorkspaceScreenFooterPlan::ScreenOwned => {
            unreachable!("Editor and Settings have their own render paths")
        }
    };
    workspace_screen_footer_items(facts)
}
