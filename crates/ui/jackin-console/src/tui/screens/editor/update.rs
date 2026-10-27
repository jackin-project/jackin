// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod auth;
mod cursor;
mod mounts_roles;
mod scroll;
mod secrets;
mod tabs;
pub use auth::{
    auth_focusable_index_at_visual_row, auth_row_is_focusable, editor_auth_row_index_at_position,
    resolve_auth_form_target,
};
pub use cursor::{
    auth_skipped_rows, editor_field_selection_plan, editor_secrets_selection_bounds,
    editor_selection_bounds, secrets_skipped_rows, step_cursor_down, step_cursor_up,
};
pub use mounts_roles::{
    EditorGeneralFieldModalPlan, add_role_to_workspace_editor, cycle_mount_isolation_at,
    editor_general_field_modal_plan, editor_max_row_for_tab, editor_mount_add_row_selected,
    editor_role_add_row_selected, toggle_allowed_role_at, toggle_default_role_at,
};
pub use scroll::{
    EditorFieldSelectionPlan, EditorHorizontalScrollPlan, EditorMountRowSelectPlan,
    EditorScrollFocusPlan, editor_mount_index_at_visual_row, editor_mount_row_select_plan,
    editor_scroll_focus_plan, editor_tab_horizontal_scroll_plan,
    editor_workspace_mounts_horizontal_scroll_plan,
};
pub use secrets::{
    forbidden_secret_keys, secret_add_target_for_row, secret_delete_target_for_row,
    secret_enter_plan_for_row, secret_picker_target_for_row, secret_unmask_target_for_row,
    secrets_flat_rows, set_secret_value,
};
pub use tabs::{
    EditorTabMovePlan, EditorTabSelectPlan, editor_mount_hover_target_at_position,
    editor_mount_index_at_position, editor_tab_bar_focus_plan, editor_tab_move_plan,
    editor_tab_select_plan, next_editor_tab, previous_editor_tab,
};
