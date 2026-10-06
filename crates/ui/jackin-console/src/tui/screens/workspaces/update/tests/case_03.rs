// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn workspace_list_mouse_plan_routes_drag_update_end_and_modal_gate() {
    let rows = [Some(ManagerListRow::CurrentDirectory)];
    let term = Rect {
        x: 0,
        y: 0,
        width: 100,
        height: 20,
    };
    let drag = crate::tui::split::DragState {
        anchor_pct: 30,
        anchor_x: 30,
    };

    assert_eq!(
        workspace_list_mouse_plan(
            mouse(
                crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left),
                55,
                4,
            ),
            term,
            30,
            Some(drag),
            false,
            &rows,
            |_| true,
        ),
        WorkspaceListMousePlan::UpdateSplit(55)
    );
    assert_eq!(
        workspace_list_mouse_plan(
            mouse(
                crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                55,
                4,
            ),
            term,
            30,
            Some(drag),
            false,
            &rows,
            |_| true,
        ),
        WorkspaceListMousePlan::EndDrag
    );
    assert_eq!(
        workspace_list_mouse_plan(
            mouse(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                30,
                4,
            ),
            term,
            30,
            None,
            true,
            &rows,
            |_| true,
        ),
        WorkspaceListMousePlan::Continue
    );
}

#[test]
fn workspace_list_clickable_at_position_excludes_seam_spacers_and_modal() {
    let rows = [
        Some(ManagerListRow::CurrentDirectory),
        Some(ManagerListRow::SavedWorkspace(0)),
        None,
        Some(ManagerListRow::NewWorkspace),
    ];
    let term = Rect {
        x: 0,
        y: 0,
        width: 100,
        height: 20,
    };

    assert!(!workspace_list_clickable_at_position(
        30,
        4,
        term,
        30,
        false,
        &rows,
        |_| true,
    ));
    assert!(workspace_list_clickable_at_position(
        10,
        4,
        term,
        30,
        false,
        &rows,
        |_| true,
    ));
    assert!(!workspace_list_clickable_at_position(
        10,
        5,
        term,
        30,
        false,
        &rows,
        |_| true,
    ));
    assert!(!workspace_list_clickable_at_position(
        10,
        4,
        term,
        30,
        true,
        &rows,
        |_| true,
    ));
}

#[test]
fn workspace_list_clickable_at_position_respects_selectable_rows() {
    let rows = [
        Some(ManagerListRow::CurrentDirectory),
        Some(ManagerListRow::SavedWorkspace(0)),
    ];
    let term = Rect {
        x: 0,
        y: 0,
        width: 100,
        height: 20,
    };

    assert!(!workspace_list_clickable_at_position(
        10,
        4,
        term,
        30,
        false,
        &rows,
        |row| row != ManagerListRow::SavedWorkspace(0),
    ));
}

#[test]
fn workspace_visual_selected_index_skips_spacers() {
    let rows = [
        Some(ManagerListRow::CurrentDirectory),
        None,
        Some(ManagerListRow::SavedWorkspace(0)),
        Some(ManagerListRow::NewWorkspace),
    ];

    assert_eq!(
        workspace_visual_selected_index(&rows, ManagerListRow::SavedWorkspace(0)),
        Some(2)
    );
    assert_eq!(
        workspace_visual_selected_index(&rows, ManagerListRow::WorkspaceInstance(0, 0)),
        None
    );
}

#[test]
fn workspace_row_lookup_helpers_handle_selectable_and_visual_rows() {
    let rows = [
        ManagerListRow::CurrentDirectory,
        ManagerListRow::SavedWorkspace(0),
        ManagerListRow::NewWorkspace,
    ];
    let visual_rows = [
        Some(ManagerListRow::CurrentDirectory),
        None,
        Some(ManagerListRow::SavedWorkspace(0)),
        Some(ManagerListRow::NewWorkspace),
    ];

    assert_eq!(
        workspace_row_index(&rows, ManagerListRow::SavedWorkspace(0)),
        Some(1)
    );
    assert_eq!(
        workspace_row_at(&rows, 2),
        Some(ManagerListRow::NewWorkspace)
    );
    assert_eq!(workspace_row_at(&rows, 9), None);
    assert_eq!(
        workspace_selected_row(&rows, 9),
        ManagerListRow::CurrentDirectory
    );
    assert_eq!(workspace_row_at_visual_index(&visual_rows, 1), None);
    assert_eq!(
        workspace_row_at_visual_index(&visual_rows, 2),
        Some(ManagerListRow::SavedWorkspace(0))
    );
    assert_eq!(workspace_last_selectable_index(rows.len()), 2);
    assert_eq!(workspace_last_selectable_index(0), 0);
    assert_eq!(selected_index(9, rows.len()), 2);
    assert_eq!(selected_index(9, 0), 0);
}

#[test]
fn destructive_confirm_states_name_targets() {
    let delete = workspace_delete_confirm_plan("alpha".to_owned());
    let delete_debug = format!("{:?}", delete.state);
    assert_eq!(delete.name, "alpha");
    assert!(delete_debug.contains("Delete"));
    assert!(delete_debug.contains("alpha"));

    let purge = instance_purge_confirm_plan("abc123".to_owned(), "role/dev".to_owned());
    let purge_debug = format!("{:?}", purge.state);
    assert_eq!(purge.container, "abc123");
    assert_eq!(purge.label, "role/dev");
    assert!(purge_debug.contains("Purge"));
    assert!(purge_debug.contains("role/dev"));
    assert!(purge_debug.contains(
        "Removes the role container, DinD sidecar, volume, network, and local recovery state."
    ));
}

#[test]
fn tree_disclosure_plans_map_rows_to_actions() {
    assert_eq!(
        collapse_selected_tree_plan(ManagerListRow::WorkspaceInstance(2, 0)),
        WorkspaceTreeDisclosurePlan::CollapseWorkspace(2)
    );
    assert_eq!(
        collapse_selected_tree_plan(ManagerListRow::CurrentDirectoryInstance(0)),
        WorkspaceTreeDisclosurePlan::CollapseCurrentDir
    );
    assert_eq!(
        expand_selected_tree_plan(ManagerListRow::SavedWorkspace(1)),
        WorkspaceTreeDisclosurePlan::ExpandWorkspace(1)
    );
    assert_eq!(
        expand_selected_tree_plan(ManagerListRow::NewWorkspace),
        WorkspaceTreeDisclosurePlan::None
    );
}

#[test]
fn collapse_selection_plans_route_child_rows_to_parent() {
    assert_eq!(
        collapse_current_dir_selection_plan(ManagerListRow::CurrentDirectoryInstance(2)),
        WorkspaceCollapseSelectionPlan::Parent
    );
    assert_eq!(
        collapsed_current_dir_selected_index(ManagerListRow::CurrentDirectoryInstance(2)),
        Some(0)
    );
    assert_eq!(
        collapse_current_dir_selection_plan(ManagerListRow::SavedWorkspace(1)),
        WorkspaceCollapseSelectionPlan::Clamp
    );
    assert_eq!(
        collapsed_current_dir_selected_index(ManagerListRow::SavedWorkspace(1)),
        None
    );
    assert_eq!(
        collapse_workspace_selection_plan(ManagerListRow::WorkspaceInstance(3, 1), 3),
        WorkspaceCollapseSelectionPlan::Parent
    );
    assert_eq!(
        collapse_workspace_selection_plan(ManagerListRow::WorkspaceInstance(4, 1), 3),
        WorkspaceCollapseSelectionPlan::Clamp
    );
    assert_eq!(
        collapse_workspace_selection_plan(ManagerListRow::SavedWorkspace(3), 3),
        WorkspaceCollapseSelectionPlan::Clamp
    );
    let rows = [
        ManagerListRow::CurrentDirectory,
        ManagerListRow::SavedWorkspace(3),
        ManagerListRow::WorkspaceInstance(3, 0),
        ManagerListRow::NewWorkspace,
    ];
    assert_eq!(
        collapsed_workspace_selected_index(&rows, 2, ManagerListRow::WorkspaceInstance(3, 0), 3),
        Some(1)
    );
    assert_eq!(
        collapsed_workspace_selected_index(&rows, 99, ManagerListRow::SavedWorkspace(3), 3),
        Some(3)
    );
}

#[test]
fn workspace_row_ownership_routes_tree_arrows() {
    assert!(workspace_row_owns_left(
        ManagerListRow::CurrentDirectory,
        true,
        true,
        |_| false
    ));
    assert!(!workspace_row_owns_left(
        ManagerListRow::CurrentDirectory,
        true,
        false,
        |_| false
    ));
    assert!(workspace_row_owns_left(
        ManagerListRow::SavedWorkspace(1),
        false,
        false,
        |idx| idx == 1
    ));
    assert!(workspace_row_owns_right(
        ManagerListRow::CurrentDirectory,
        false,
        true,
        |_| false,
        |_| false
    ));
    assert!(workspace_row_owns_right(
        ManagerListRow::SavedWorkspace(1),
        false,
        false,
        |_| false,
        |idx| idx == 1
    ));
    assert!(!workspace_row_owns_right(
        ManagerListRow::WorkspaceInstance(1, 0),
        false,
        true,
        |_| false,
        |_| true
    ));
}

#[test]
fn workspace_list_horizontal_plan_routes_tree_or_scroll() {
    assert_eq!(
        workspace_list_horizontal_plan(
            ManagerListRow::CurrentDirectory,
            -8,
            true,
            true,
            |_| false,
            |_| false,
        ),
        WorkspaceListHorizontalPlan::CollapseTree
    );
    assert_eq!(
        workspace_list_horizontal_plan(
            ManagerListRow::SavedWorkspace(2),
            8,
            false,
            false,
            |_| false,
            |idx| idx == 2,
        ),
        WorkspaceListHorizontalPlan::ExpandTree
    );
    assert_eq!(
        workspace_list_horizontal_plan(
            ManagerListRow::NewWorkspace,
            8,
            false,
            false,
            |_| false,
            |_| false,
        ),
        WorkspaceListHorizontalPlan::Scroll(8)
    );
}

#[test]
fn preview_focus_plans_set_focus_state() {
    assert_eq!(
        enter_preview_focus_plan(),
        PreviewFocusPlan { focused: true }
    );
    assert_eq!(
        exit_preview_focus_plan(),
        PreviewFocusPlan { focused: false }
    );
}
