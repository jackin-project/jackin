//! Tests for `update`.
use super::*;

#[test]
fn workspace_unclamped_scroll_plan_updates_offset() {
    assert_eq!(workspace_unclamped_scroll_plan(4, 3), 7);
    assert_eq!(workspace_unclamped_scroll_plan(4, -99), 0);
}

#[test]
fn workspace_list_selection_plans_clear_expected_pickers() {
    assert_eq!(
        workspace_list_move_selection_plan(0, 3, 1),
        WorkspaceListSelectionPlan {
            selected: 1,
            changed: true,
            clear_inline_role_picker: true,
            clear_inline_agent_picker: true,
            clear_inline_new_session_picker: true,
            clear_inline_provider_picker: false,
            clear_launch_provider_picker: false,
        }
    );
    assert_eq!(
        workspace_list_select_row_plan(0, 2, 3),
        WorkspaceListSelectionPlan {
            selected: 2,
            changed: true,
            clear_inline_role_picker: true,
            clear_inline_agent_picker: true,
            clear_inline_new_session_picker: true,
            clear_inline_provider_picker: true,
            clear_launch_provider_picker: true,
        }
    );
}

#[test]
fn workspace_list_scroll_focus_plan_routes_mouse_regions() {
    assert_eq!(
        workspace_list_scroll_focus_plan(true, true, true, true, true, true),
        WorkspaceListScrollFocusPlan {
            list_names_focused: true,
            scroll_focus: None,
        }
    );
    assert_eq!(
        workspace_list_scroll_focus_plan(false, false, true, false, false, false),
        WorkspaceListScrollFocusPlan {
            list_names_focused: false,
            scroll_focus: None,
        }
    );
    assert_eq!(
        workspace_list_scroll_focus_plan(false, true, false, true, false, false).scroll_focus,
        Some(crate::tui::focus::MountScrollFocus::Global)
    );
    assert_eq!(
        workspace_list_scroll_focus_plan(false, true, false, false, true, false).scroll_focus,
        Some(crate::tui::focus::MountScrollFocus::RoleGlobal)
    );
    assert_eq!(
        workspace_list_scroll_focus_plan(false, true, false, false, false, true).scroll_focus,
        Some(crate::tui::focus::MountScrollFocus::Roles)
    );
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
    assert!(purge_debug.contains("Cannot be undone"));
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

#[test]
fn instance_action_accepts_status_grid_smoke() {
    use WorkspaceInstanceAction as A;
    use WorkspaceInstanceStatus as S;

    assert!(instance_action_accepts_status(A::Stop, S::Running));
    assert!(!instance_action_accepts_status(A::Stop, S::CleanExited));
    assert!(!instance_action_accepts_status(A::Stop, S::Purged));
    assert!(instance_action_accepts_status(A::Purge, S::Running));
    assert!(instance_action_accepts_status(A::Purge, S::PreservedDirty));
    assert!(!instance_action_accepts_status(A::Purge, S::Purged));
    assert!(instance_action_accepts_status(A::Reconnect, S::Crashed));
    assert!(!instance_action_accepts_status(A::Reconnect, S::Purged));
}

#[test]
fn preview_pane_key_plan_routes_navigation() {
    assert_eq!(
        preview_pane_key_plan(KeyCode::Esc, 2),
        PreviewPaneKeyPlan::ExitPreview
    );
    assert_eq!(
        preview_pane_key_plan(KeyCode::Char('K'), 2),
        PreviewPaneKeyPlan::Move { delta: -1 }
    );
    assert_eq!(
        preview_pane_key_plan(KeyCode::Down, 2),
        PreviewPaneKeyPlan::Move { delta: 1 }
    );
    assert_eq!(
        preview_pane_key_plan(KeyCode::Enter, 2),
        PreviewPaneKeyPlan::ReconnectSelected
    );
    assert_eq!(
        preview_pane_key_plan(KeyCode::Tab, 2),
        PreviewPaneKeyPlan::Continue
    );
    assert_eq!(
        preview_pane_key_plan(KeyCode::Enter, 0),
        PreviewPaneKeyPlan::ExitPreview
    );
}

#[test]
fn preview_pane_cursor_plan_clamps_current_and_delta() {
    assert_eq!(preview_pane_cursor_plan(0, Some(4), 1), None);
    assert_eq!(preview_pane_cursor_plan(3, None, 1), Some(1));
    assert_eq!(preview_pane_cursor_plan(3, Some(9), 1), Some(2));
    assert_eq!(preview_pane_cursor_plan(3, Some(0), -9), Some(0));
}

#[test]
fn should_enter_preview_pane_requires_instance_row_key_and_panes() {
    assert!(should_enter_preview_pane(
        KeyCode::Tab,
        ManagerListRow::WorkspaceInstance(1, 0),
        2
    ));
    assert!(should_enter_preview_pane(
        KeyCode::Right,
        ManagerListRow::CurrentDirectoryInstance(0),
        1
    ));
    assert!(!should_enter_preview_pane(
        KeyCode::Tab,
        ManagerListRow::SavedWorkspace(1),
        2
    ));
    assert!(!should_enter_preview_pane(
        KeyCode::Down,
        ManagerListRow::WorkspaceInstance(1, 0),
        2
    ));
    assert!(!should_enter_preview_pane(
        KeyCode::Tab,
        ManagerListRow::WorkspaceInstance(1, 0),
        0
    ));
}

#[test]
fn destructive_confirm_plan_routes_commit_cancel_and_continue() {
    assert_eq!(
        destructive_confirm_plan(ModalOutcome::Commit(true)),
        DestructiveConfirmPlan::Commit
    );
    assert_eq!(
        destructive_confirm_plan(ModalOutcome::Commit(false)),
        DestructiveConfirmPlan::ReturnToList
    );
    assert_eq!(
        destructive_confirm_plan(ModalOutcome::Cancel),
        DestructiveConfirmPlan::ReturnToList
    );
    assert_eq!(
        destructive_confirm_plan(ModalOutcome::Continue),
        DestructiveConfirmPlan::Continue
    );
}
