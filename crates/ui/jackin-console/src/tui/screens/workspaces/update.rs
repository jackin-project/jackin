// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workspace list screen update logic: handle keyboard events and produce
//! effects for launch, reconnect, stop, purge, and navigation actions.
//!
//! Not responsible for: rendering (see `view`) or state definitions (see
//! `model`).
mod apply;
mod confirms;
mod destructive;
mod destructive_keys;
mod instance_focus;
mod list_keys;
mod mouse;
mod preview;
mod resolvers;
mod rows;
mod scroll_plans;
mod selection;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use super::model::ManagerHoverTarget;
#[cfg(test)]
pub(crate) use super::model::ManagerListRow;
#[cfg(test)]
pub(crate) use crate::mount_info_cache::MountInfoCache;
pub use apply::{
    WorkspaceListHoverState, WorkspaceListScrollFocusPlan, WorkspaceListScrollState,
    WorkspaceListScrollTargetPlan, WorkspaceListSelectionPlan, WorkspaceListSelectionState,
    apply_workspace_list_horizontal_scroll_plan, apply_workspace_list_hover_target,
    apply_workspace_list_selection_plan, apply_workspace_list_vertical_scroll_plan, selected_index,
    workspace_list_hover_row_at_position,
};
pub use confirms::{
    collapse_selected_tree_plan, expand_selected_tree_plan, instance_purge_confirm_plan,
    instance_purge_confirm_state, is_preview_pane_entry_target, should_enter_preview_pane,
    workspace_delete_confirm_plan, workspace_delete_confirm_state, workspace_list_horizontal_plan,
    workspace_row_owns_left, workspace_row_owns_right, workspace_unclamped_scroll_plan,
};
#[cfg(test)]
pub(crate) use crossterm::event::KeyCode;
#[cfg(test)]
pub(crate) use crossterm::event::MouseEvent;
pub use destructive::{
    DestructiveConfirmPlan, InstancePurgeConfirmPlan, InstancePurgeKeyPlan,
    SelectedInstanceActionPlan, SelectedInstancePurgeConfirmPlan, WorkspaceCollapseSelectionPlan,
    WorkspaceDeleteConfirmPlan, WorkspaceDeleteKeyPlan, WorkspaceListEnterPlan,
    WorkspaceListHorizontalPlan, WorkspaceTreeDisclosurePlan, WorkspaceTreeDisclosureState,
    apply_workspace_tree_disclosure_plan,
};
pub use destructive_keys::{
    destructive_confirm_plan, instance_action_accepts_status, instance_purge_key_plan,
    selected_instance_action_plan, selected_instance_purge_confirm_plan, workspace_delete_key_plan,
};
pub use instance_focus::{
    PreviewFocusPlan, PreviewFocusState, PreviewPaneActionPlan, PreviewPaneCursorState,
    PreviewPaneKeyPlan, WorkspaceInstanceAction, WorkspaceInstanceStatus, WorkspaceListFocusOwner,
    apply_preview_focus_plan, apply_preview_pane_cursor_plan, workspace_instance_empty_message,
    workspace_list_focus_head, workspace_list_focus_next, workspace_list_focus_order,
};
#[cfg(test)]
pub(crate) use jackin_oppicker::ModalOutcome;
pub use list_keys::{
    WorkspaceInstanceLookupEntry, WorkspaceInstanceLookupScope, WorkspaceInstanceScopePlan,
    WorkspaceListDeletePlan, WorkspaceListEditPlan, WorkspaceListKeyPlan,
    WorkspaceListNewSessionOpenPlan, WorkspaceListNewSessionPlan,
    WorkspaceListSelectedInstancePlan, WorkspaceListSettingsPlan, WorkspaceListTopLevelKeyPlan,
};
pub use mouse::{
    WorkspaceListMousePlan, workspace_list_clickable_at_position, workspace_list_mouse_plan,
};
pub use preview::{
    enter_preview_focus_plan, exit_preview_focus_plan, preview_pane_action_plan,
    preview_pane_cursor_plan, preview_pane_key_plan, preview_pane_selected_index,
    workspace_list_top_level_key_plan,
};
pub use resolvers::{
    instance_lookup_entry_matches_scope, selected_instance_container_for_action,
    selected_instance_plan, selected_instance_scope_plan,
    workspace_list_current_directory_selected, workspace_list_delete_plan,
    workspace_list_edit_plan, workspace_list_enter_plan, workspace_list_github_open_plan,
    workspace_list_key_plan, workspace_list_new_session_open_plan, workspace_list_new_session_plan,
    workspace_list_new_workspace_selected, workspace_list_prewarm_plan,
    workspace_list_settings_plan,
};
pub use rows::{
    WorkspaceRowLayout, selectable_rows, visual_rows, workspace_last_selectable_index,
    workspace_row_at, workspace_row_at_visual_index, workspace_row_index, workspace_selected_row,
    workspace_visual_selected_index,
};
pub use scroll_plans::{
    workspace_list_horizontal_scroll_target_plan, workspace_list_move_selection_plan,
    workspace_list_scroll_focus_plan, workspace_list_select_row_plan,
    workspace_list_vertical_scroll_target_plan,
};
pub use selection::{
    collapse_current_dir_selection_plan, collapse_workspace_selection_plan,
    collapsed_current_dir_selected_index, collapsed_workspace_selected_index,
    initial_workspace_selected_index, saved_workspace_selected_index,
    workspace_list_saved_workspace_index, workspace_list_settings_available,
};
