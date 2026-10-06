// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Top-level console TUI update helpers.
mod confirm_plans;
mod modal_enums;
mod modal_plans;
mod overlays;
mod prerender;
mod resolvers;
mod shell;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use crate::tui::components::account_picker::AccountPickerState;
#[cfg(test)]
pub(crate) use crate::tui::components::agent_choice::AgentChoice;
#[cfg(test)]
pub(crate) use crate::tui::components::agent_choice::AgentChoiceState;
pub use confirm_plans::{
    bool_confirm_modal_plan, confirm_save_modal_plan, create_op_picker_plan,
    dismissible_modal_plan, list_github_picker_plan, list_role_picker_plan,
    save_discard_modal_plan, scope_picker_plan, source_picker_plan,
};
pub use modal_enums::{
    AuthSourceFolderPickerPlan, BoolConfirmModalPlan, ConfirmSaveModalPlan, CreateOpPickerPlan,
    DismissibleModalPlan, FileBrowserModalPlan, InlinePickerPlan, InlinePickerShellPlan,
    ListGithubPickerPlan, ListModalKeyTarget, ListModalScrollTarget, ListRolePickerPlan,
    MountDstChoicePlan, SaveDiscardModalPlan, ScopePickerPlan, SettingsModalScrollTarget,
    SharedModalScrollTarget, SourcePickerPlan,
};
pub use modal_plans::{
    auth_source_folder_picker_plan, file_browser_modal_plan, inline_account_followup_plan,
    inline_picker_plan, inline_picker_shell_plan, mount_dst_choice_plan, op_picker_inline_plan,
};
pub use overlays::{
    InlinePickerDismissal, InlinePickerDismissalState, ListModalPlan, ListModalState,
    StatusOverlayPlan, StatusOverlayState, apply_inline_picker_dismissal_plan,
    apply_list_modal_plan, apply_status_overlay_plan,
};
pub use prerender::{
    ConsoleMouseWheelPlan, InlineAccountFollowupPlan, InlineAccountPickerState,
    InlineNewSessionPickerState, ListPreRenderFacts, ListPreRenderFocusPlan, ListPreRenderPlan,
    ListPreRenderScrollResetPlan, apply_inline_account_picker_plan,
    apply_inline_new_session_picker_plan, list_names_focus_plan, list_scroll_focus_plan,
};
pub use resolvers::{
    console_mouse_wheel_plan, global_mount_modal_scroll_target, list_modal_key_target,
    list_modal_scroll_target, list_pre_render_facts_from_scroll_areas, list_pre_render_focus_plan,
    list_pre_render_plan, list_pre_render_scroll_reset_plan, settings_auth_modal_scroll_target,
    settings_env_modal_scroll_target, shared_modal_scroll_target,
};
pub use shell::{
    ListShellState, apply_drag_state_plan, apply_list_split_pct_plan, dismiss_list_modal_plan,
    dismiss_status_overlay_plan, drag_state_plan, inline_picker_dismissal_plan,
    list_split_pct_plan, open_container_info_modal_plan, open_error_popup_modal_plan,
    open_github_picker_modal_plan, open_status_overlay_plan, role_resolution_status_overlay_plan,
    selected_index_plan, selection_move_plan, term_width_scroll_plan, unclamped_scroll_plan,
};
