// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Concrete manager message type aliases and the `update_manager` reducer.
//!
//! All generic parameters are lower-crate-owned types, making this module
//! the canonical home for the concrete TUI update boundary.
mod actions;
mod dispatch;
mod execute;
mod screens;
mod scroll;
mod selection;
mod settings;
#[cfg(test)]
mod tests;
pub(crate) use actions::{
    action_of, apply_settings_message, record_manager_action, request_poll_effect,
    start_manager_action,
};
pub use dispatch::{ManagerBackgroundEvent, ManagerMessage, update_manager};
pub use execute::{execute_op_commit_validation, execute_open_url, report_open_url_error};
pub(crate) use screens::{
    apply_op_commit_result, clear_settings_auth_kind, dismiss_settings_error_popup,
    enter_confirm_delete, enter_confirm_instance_purge, enter_create_editor,
    enter_settings_auth_kind, move_editor_field_selection, move_editor_tab,
    open_settings_error_popup, reload_from_config, set_editor_tab_bar_focus,
    set_settings_tab_bar_focus,
};
pub(crate) use scroll::{
    scroll_editor_tab_horizontal, scroll_editor_workspace_mounts_horizontal,
    scroll_settings_global_mounts_horizontal, scroll_settings_trust_horizontal,
};
pub(crate) use selection::{
    collapse_selected_tree, expand_selected_tree, move_list_selection, move_preview_pane,
    move_settings_env_selection, move_settings_global_mounts_selection,
    move_settings_trust_selection, scroll_focused_mount_block_vertical, scroll_list_horizontal,
    select_editor_mount_row, select_editor_tab, select_list_row, select_settings_tab,
    select_settings_trust_row,
};
pub(crate) use settings::{
    move_settings_auth_selection, move_settings_general_selection, move_settings_tab,
    set_editor_secrets_role_expanded, set_settings_env_role_expanded,
    toggle_editor_general_selected, toggle_editor_mount_readonly_selected,
    toggle_editor_secret_mask, toggle_settings_general_selected,
    toggle_settings_global_mount_readonly, toggle_settings_trust_selected,
};
