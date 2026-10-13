// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! List-stage dispatch: workspace-picker key handling and the
//! list-level modal (`GithubPicker`).
mod actions;
mod keys;
mod modals;
mod nav;
mod pickers;
#[cfg(test)]
mod tests;
pub(crate) use actions::{
    clamp_list_scroll_after_key, confirm_purge_outcome, console_instance_action_and_empty_message,
    dispatch_manager, dispatch_workspace_list_delete, dispatch_workspace_list_edit,
    dispatch_workspace_list_settings, instance_action_outcome, open_new_session_picker,
};
pub(crate) use keys::ConcreteInstanceAction;
pub use keys::handle_list_key;
pub use modals::handle_list_modal;
pub(crate) use modals::handle_list_open_in_github;
pub(crate) use nav::{
    handle_list_left_right, handle_preview_focused_key, selected_instance_container,
};
pub use pickers::{
    handle_inline_account_picker, handle_inline_agent_picker, handle_inline_role_picker,
    handle_launch_account_picker, handle_new_session_picker,
};
