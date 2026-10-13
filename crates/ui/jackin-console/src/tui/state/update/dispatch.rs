// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ManagerMessage` types and the `update_manager` reducer.

use super::{
    action_of, apply_op_commit_result, apply_settings_message, clear_settings_auth_kind,
    collapse_selected_tree, dismiss_settings_error_popup, enter_confirm_delete,
    enter_confirm_instance_purge, enter_create_editor, enter_settings_auth_kind,
    expand_selected_tree, move_editor_field_selection, move_editor_tab, move_list_selection,
    move_preview_pane, move_settings_auth_selection, move_settings_env_selection,
    move_settings_general_selection, move_settings_global_mounts_selection, move_settings_tab,
    move_settings_trust_selection, open_settings_error_popup, reload_from_config,
    request_poll_effect, scroll_editor_tab_horizontal, scroll_editor_workspace_mounts_horizontal,
    scroll_focused_mount_block_vertical, scroll_list_horizontal,
    scroll_settings_global_mounts_horizontal, scroll_settings_trust_horizontal,
    select_editor_mount_row, select_editor_tab, select_list_row, select_settings_tab,
    select_settings_trust_row, set_editor_secrets_role_expanded, set_editor_tab_bar_focus,
    set_settings_env_role_expanded, set_settings_tab_bar_focus, start_manager_action,
    toggle_editor_general_selected, toggle_editor_mount_readonly_selected,
    toggle_editor_secret_mask, toggle_settings_general_selected,
    toggle_settings_global_mount_readonly, toggle_settings_trust_selected,
};

use crate::tui::model::apply_manager_stage;

use super::super::{
    EditorState, EditorTab, ManagerConfigSaveResult, ManagerInstanceRefreshSnapshot, ManagerStage,
    ManagerState, MountScrollFocus, PendingDriftCheck, PendingFileBrowserCommit,
    PendingFileBrowserListing, PendingIsolationCleanup, PendingMountInfoRefresh, PendingRoleLoad,
    SecretsScopeTag, SettingsState, SettingsTab,
};
use crate::tui::screens::workspaces::update::{
    apply_preview_focus_plan, enter_preview_focus_plan, exit_preview_focus_plan,
};
use crate::tui::update::{
    InlinePickerDismissal, apply_drag_state_plan, apply_inline_picker_dismissal_plan,
    apply_list_modal_plan, apply_list_split_pct_plan, apply_status_overlay_plan,
    dismiss_list_modal_plan, dismiss_status_overlay_plan, drag_state_plan,
    inline_picker_dismissal_plan, list_names_focus_plan, list_scroll_focus_plan,
    list_split_pct_plan, open_container_info_modal_plan, open_error_popup_modal_plan,
    open_github_picker_modal_plan, open_status_overlay_plan,
};

pub type ManagerMessage = crate::tui::message::ConsoleManagerMessage<
    super::super::CreatePreludeState<'static>,
    EditorState<'static>,
    SettingsState<'static>,
    PendingFileBrowserCommit,
    PendingFileBrowserListing,
    ManagerInstanceRefreshSnapshot,
    PendingMountInfoRefresh,
    jackin_core::OpRef,
    jackin_config::AppConfig,
    jackin_config::WorkspaceConfig,
    EditorTab,
    SettingsTab,
    SecretsScopeTag,
    MountScrollFocus,
    super::super::DragState,
    crate::tui::components::container_info_surface::ContainerInfoState,
    crate::tui::components::github_picker::GithubPickerState,
>;

pub type ManagerBackgroundEvent = crate::tui::message::BackgroundEvent<
    ManagerMessage,
    PendingRoleLoad,
    PendingDriftCheck,
    jackin_core::DriftDetection,
    PendingIsolationCleanup,
    ManagerConfigSaveResult,
>;

// ── Reducer ───────────────────────────────────────────────────────────────

#[expect(
    clippy::too_many_lines,
    reason = "Manager-state reducer handles every ManagerMessage variant inline: \
              per-message-arm state mutation + per-stage emit + per-Update \
              branch. Inline shape preserves the per-message-arm state machine."
)]
pub fn update_manager(state: &mut ManagerState<'_>, message: ManagerMessage) {
    let action = action_of(&message);
    let action_guard = action.and_then(|name| start_manager_action(state, name));
    let action_span = action_guard.as_ref().map(|guard| guard.span().enter());
    match message {
        ManagerMessage::CollapseSelectedTree => collapse_selected_tree(state),
        ManagerMessage::EnterPreview => apply_preview_focus_plan(state, enter_preview_focus_plan()),
        ManagerMessage::EnterConfirmDelete { name } => enter_confirm_delete(state, name),
        ManagerMessage::EnterConfirmInstancePurge { container, label } => {
            enter_confirm_instance_purge(state, container, label);
        }
        ManagerMessage::EnterCreateEditor { name, workspace } => {
            enter_create_editor(state, name, workspace);
        }
        ManagerMessage::EnterCreatePrelude(prelude) => {
            apply_manager_stage(state, ManagerStage::CreatePrelude(prelude));
        }
        ManagerMessage::EnterEditor(editor) => {
            apply_manager_stage(state, ManagerStage::Editor(editor));
        }
        ManagerMessage::EnterSettings(settings) => {
            apply_manager_stage(state, ManagerStage::Settings(settings));
        }
        ManagerMessage::FileBrowserCommitValidated(result) => {
            crate::tui::file_browser::apply_file_browser_commit_result(state, result);
        }
        ManagerMessage::FileBrowserListingLoaded(result) => {
            crate::tui::file_browser::apply_file_browser_listing_result(state, result);
        }
        ManagerMessage::InstancesRefreshed(result) => state.apply_instance_refresh(result),
        ManagerMessage::MountInfoRefreshed(result) => {
            state.apply_mount_info_refresh(result);
        }
        ManagerMessage::OpCommitResolved {
            op_ref,
            result,
            is_settings,
        } => apply_op_commit_result(state, op_ref, result, is_settings),
        poll @ (ManagerMessage::PollPickerLoads | ManagerMessage::PollFileBrowserGitUrls) => {
            request_poll_effect(state, poll);
        }
        ManagerMessage::FocusEditorContent => set_editor_tab_bar_focus(state, false),
        ManagerMessage::FocusEditorTabBar => set_editor_tab_bar_focus(state, true),
        ManagerMessage::FocusSettingsContent => set_settings_tab_bar_focus(state, false),
        ManagerMessage::FocusSettingsTabBar => set_settings_tab_bar_focus(state, true),
        ManagerMessage::ExitPreview => apply_preview_focus_plan(state, exit_preview_focus_plan()),
        ManagerMessage::ExpandSelectedTree => expand_selected_tree(state),
        ManagerMessage::ClearSettingsAuthKind => clear_settings_auth_kind(state),
        ManagerMessage::DismissSettingsErrorPopup => dismiss_settings_error_popup(state),
        ManagerMessage::OpenSettingsErrorPopup { title, message } => {
            open_settings_error_popup(state, title, message);
        }
        ManagerMessage::EnterSettingsAuthKind => enter_settings_auth_kind(state),
        ManagerMessage::ScrollEditorTabHorizontal {
            delta,
            term_width,
            content_width,
        } => scroll_editor_tab_horizontal(state, delta, term_width, content_width),
        ManagerMessage::SelectEditorMountRow(row) => select_editor_mount_row(state, row),
        ManagerMessage::SelectEditorTab(tab) => select_editor_tab(state, tab),
        ManagerMessage::SelectListRow(row) => select_list_row(state, row),
        ManagerMessage::SelectSettingsTab(tab) => select_settings_tab(state, tab),
        ManagerMessage::SelectSettingsTrustRow(row) => select_settings_trust_row(state, row),
        ManagerMessage::ScrollEditorWorkspaceMountsHorizontal {
            delta,
            term_width,
            content_width,
        } => scroll_editor_workspace_mounts_horizontal(state, delta, term_width, content_width),
        ManagerMessage::ScrollSettingsGlobalMountsHorizontal {
            delta,
            term_width,
            content_width,
        } => scroll_settings_global_mounts_horizontal(state, delta, term_width, content_width),
        ManagerMessage::ScrollSettingsTrustHorizontal {
            delta,
            term_width,
            content_width,
        } => scroll_settings_trust_horizontal(state, delta, term_width, content_width),
        ManagerMessage::MoveSettingsGlobalMountsSelection {
            delta,
            term,
            footer_h,
        } => move_settings_global_mounts_selection(state, delta, term, footer_h),
        ManagerMessage::MoveSettingsEnvSelection {
            delta,
            term,
            footer_h,
        } => move_settings_env_selection(state, delta, term, footer_h),
        ManagerMessage::MoveSettingsTrustSelection {
            delta,
            term,
            footer_h,
        } => move_settings_trust_selection(state, delta, term, footer_h),
        ManagerMessage::MoveEditorTab {
            delta,
            focus_tab_bar,
        } => move_editor_tab(state, delta, focus_tab_bar),
        ManagerMessage::MoveEditorFieldSelection {
            delta,
            max_row,
            skipped_rows,
            term,
            footer_h,
        } => move_editor_field_selection(state, delta, max_row, &skipped_rows, term, footer_h),
        ManagerMessage::MoveSettingsTab {
            delta,
            focus_tab_bar,
        } => move_settings_tab(state, delta, focus_tab_bar),
        ManagerMessage::MoveSettingsGeneralSelection { delta } => {
            move_settings_general_selection(state, delta);
        }
        ManagerMessage::MoveSettingsAuthSelection { delta } => {
            move_settings_auth_selection(state, delta);
        }
        ManagerMessage::SetSettingsEnvRoleExpanded { role, expanded } => {
            set_settings_env_role_expanded(state, role, expanded);
        }
        ManagerMessage::SetEditorSecretsRoleExpanded { role, expanded } => {
            set_editor_secrets_role_expanded(state, role, expanded);
        }
        ManagerMessage::ToggleSettingsGlobalMountReadonly => {
            toggle_settings_global_mount_readonly(state);
        }
        ManagerMessage::ToggleEditorGeneralSelected => toggle_editor_general_selected(state),
        ManagerMessage::ToggleEditorMountReadonlySelected => {
            toggle_editor_mount_readonly_selected(state);
        }
        ManagerMessage::ToggleEditorSecretMask { scope, key } => {
            toggle_editor_secret_mask(state, scope, key);
        }
        ManagerMessage::ToggleSettingsGeneralSelected => toggle_settings_general_selected(state),
        ManagerMessage::ToggleSettingsTrustSelected => toggle_settings_trust_selected(state),
        ManagerMessage::MoveListSelection(delta) => move_list_selection(state, delta),
        ManagerMessage::MovePreviewPane { container, delta } => {
            move_preview_pane(state, &container, delta);
        }
        ManagerMessage::ReloadFromConfig { config, cwd } => {
            reload_from_config(state, &config, &cwd);
        }
        ManagerMessage::ReturnToList => apply_manager_stage(state, ManagerStage::List),
        ManagerMessage::ScrollListHorizontal(delta) => scroll_list_horizontal(state, delta),
        ManagerMessage::ScrollFocusedListBlockVertical(delta) => {
            scroll_focused_mount_block_vertical(state, delta);
        }
        ManagerMessage::SetListScrollFocus(focus) => {
            state.set_list_scroll_focus(list_scroll_focus_plan(focus));
        }
        ManagerMessage::SetListNamesFocused(focused) => {
            state.set_list_names_focused(list_names_focus_plan(focused));
        }
        ManagerMessage::SetDragState(drag) => {
            apply_drag_state_plan(state, drag_state_plan(drag));
        }
        ManagerMessage::SetListSplitPct(pct) => {
            apply_list_split_pct_plan(state, list_split_pct_plan(pct));
        }
        ManagerMessage::OpenListErrorPopup { title, message } => {
            apply_list_modal_plan(state, open_error_popup_modal_plan(title, message));
        }
        ManagerMessage::OpenStatusPopup { title, message } => {
            apply_status_overlay_plan(state, open_status_overlay_plan(title, message));
        }
        ManagerMessage::DismissStatusPopup => {
            apply_status_overlay_plan(state, dismiss_status_overlay_plan());
        }
        ManagerMessage::OpenListContainerInfo { state: info } => {
            apply_list_modal_plan(state, open_container_info_modal_plan(info));
        }
        ManagerMessage::OpenListGithubPicker { state: picker } => {
            apply_list_modal_plan(state, open_github_picker_modal_plan(picker));
        }
        ManagerMessage::DismissListModal => {
            apply_list_modal_plan(state, dismiss_list_modal_plan());
        }
        ManagerMessage::DismissInlineSessionPicker => {
            apply_inline_picker_dismissal_plan(
                state,
                inline_picker_dismissal_plan(InlinePickerDismissal::NewSession),
            );
        }
        ManagerMessage::DismissInlineRolePicker => {
            apply_inline_picker_dismissal_plan(
                state,
                inline_picker_dismissal_plan(InlinePickerDismissal::Role),
            );
        }
        ManagerMessage::DismissInlineAgentPicker => {
            apply_inline_picker_dismissal_plan(
                state,
                inline_picker_dismissal_plan(InlinePickerDismissal::Agent),
            );
        }
        ManagerMessage::DismissInlineAccountPicker => {
            apply_inline_picker_dismissal_plan(
                state,
                inline_picker_dismissal_plan(InlinePickerDismissal::Provider),
            );
        }
        ManagerMessage::DismissLaunchAccountPicker => {
            apply_inline_picker_dismissal_plan(
                state,
                inline_picker_dismissal_plan(InlinePickerDismissal::LaunchAccount),
            );
        }
        ManagerMessage::Settings(message) => apply_settings_message(state, message),
    }
    drop(action_span);
    if let Some(guard) = action_guard {
        jackin_telemetry::ui::remember_action_parent(guard);
    }
}
