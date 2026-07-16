// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Concrete manager message type aliases and the `update_manager` reducer.
//!
//! All generic parameters are lower-crate-owned types, making this module
//! the canonical home for the concrete TUI update boundary.

use ratatui::layout::Rect;

use crate::tui::auth::AuthKind;
use crate::tui::model::apply_manager_stage;
use crate::tui::screens::editor::update::{
    clear_editor_auth_kind_plan, editor_field_selection_plan, editor_mount_row_select_plan,
    editor_tab_bar_focus_plan, editor_tab_horizontal_scroll_plan, editor_tab_move_plan,
    editor_tab_select_plan, editor_workspace_mounts_horizontal_scroll_plan,
    enter_editor_auth_kind_plan,
};
use crate::tui::screens::settings::update::{
    settings_env_selection_plan, settings_global_mounts_selection_plan,
    settings_horizontal_scroll_plan, settings_tab_bar_focus_plan, settings_tab_move_plan,
    settings_tab_select_plan, settings_trust_row_select_plan, settings_trust_selection_plan,
};
use crate::tui::screens::workspaces::update::{
    apply_preview_focus_plan, apply_preview_pane_cursor_plan,
    apply_workspace_list_horizontal_scroll_plan, apply_workspace_list_selection_plan,
    apply_workspace_list_vertical_scroll_plan, apply_workspace_tree_disclosure_plan,
    collapse_selected_tree_plan, enter_preview_focus_plan, exit_preview_focus_plan,
    expand_selected_tree_plan, instance_purge_confirm_plan, preview_pane_cursor_plan,
    workspace_delete_confirm_plan, workspace_list_horizontal_scroll_target_plan,
    workspace_list_move_selection_plan, workspace_list_select_row_plan,
    workspace_list_vertical_scroll_target_plan,
};
use crate::tui::update::{
    InlinePickerDismissal, apply_drag_state_plan, apply_inline_picker_dismissal_plan,
    apply_list_modal_plan, apply_list_split_pct_plan, apply_status_overlay_plan,
    dismiss_list_modal_plan, dismiss_status_overlay_plan, drag_state_plan,
    inline_picker_dismissal_plan, list_names_focus_plan, list_scroll_focus_plan,
    list_split_pct_plan, open_container_info_modal_plan, open_error_popup_modal_plan,
    open_github_picker_modal_plan, open_status_overlay_plan,
};

use super::{
    EditorState, EditorTab, FieldFocus, ManagerConfigSaveResult, ManagerEffect,
    ManagerInstanceRefreshSnapshot, ManagerStage, ManagerState, Modal, MountScrollFocus,
    PendingDriftCheck, PendingFileBrowserCommit, PendingFileBrowserListing,
    PendingIsolationCleanup, PendingMountInfoRefresh, PendingRoleLoad, SecretsScopeTag,
    SettingsState, SettingsTab,
};

// ── Concrete type aliases ──────────────────────────────────────────────────

pub type ManagerMessage = crate::tui::message::ConsoleManagerMessage<
    AuthKind,
    super::CreatePreludeState<'static>,
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
    super::DragState,
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

pub type ManagerUpdate = crate::tui::update::ConsoleUpdate<ManagerEffect>;

// ── Reducer ───────────────────────────────────────────────────────────────

#[expect(
    clippy::too_many_lines,
    reason = "Manager-state reducer handles every ManagerMessage variant inline: \
              per-message-arm state mutation + per-stage emit + per-Update \
              branch. Inline shape preserves the per-message-arm state machine."
)]
pub fn update_manager(state: &mut ManagerState<'_>, message: ManagerMessage) -> ManagerUpdate {
    let action = action_of(&message);
    let action_guard = action.and_then(|name| {
        let mut attrs = vec![jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::UI_ACTION_NAME,
            value: jackin_telemetry::Value::Str(name.as_str()),
        }];
        attrs.push(jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::APP_SCREEN_ID,
            value: jackin_telemetry::Value::Str(telemetry_screen(state).as_str()),
        });
        if let Some(widget) = telemetry_widget(state) {
            attrs.push(jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::APP_WIDGET_ID,
                value: jackin_telemetry::Value::Str(widget),
            });
        }
        jackin_telemetry::root_operation(&jackin_telemetry::operation::UI_ACTION, &attrs).ok()
    });
    let action_span = action_guard.as_ref().map(|guard| guard.span().enter());
    match message {
        ManagerMessage::CollapseSelectedTree => collapse_selected_tree(state),
        ManagerMessage::ClearEditorAuthKind => clear_editor_auth_kind(state),
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
        ManagerMessage::EnterEditorAuthKind { kind } => enter_editor_auth_kind(state, kind),
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
        ManagerMessage::PollPickerLoads => {
            state.request_effect(ManagerEffect::PollPickerLoads);
        }
        ManagerMessage::PollFileBrowserGitUrls => {
            state.request_effect(ManagerEffect::PollFileBrowserGitUrls);
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
        ManagerMessage::SetEditorAuthRoleExpanded { role, expanded } => {
            set_editor_auth_role_expanded(state, role, expanded);
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
        ManagerMessage::DismissInlineProviderPicker => {
            apply_inline_picker_dismissal_plan(
                state,
                inline_picker_dismissal_plan(InlinePickerDismissal::Provider),
            );
        }
        ManagerMessage::DismissLaunchProviderPicker => {
            apply_inline_picker_dismissal_plan(
                state,
                inline_picker_dismissal_plan(InlinePickerDismissal::LaunchProvider),
            );
        }
    }
    drop(action_span);
    if let (Some(guard), Some(action)) = (action_guard, action) {
        jackin_telemetry::ui::remember_action_parent(guard.span());
        guard.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
        let attrs = [jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::UI_ACTION_NAME,
            value: jackin_telemetry::Value::Str(action.as_str()),
        }];
        let _counter_result =
            jackin_telemetry::counter(&jackin_telemetry::metric::UI_ACTIONS).add(1, &attrs);
    }
    ManagerUpdate::redraw()
}

fn telemetry_screen(state: &ManagerState<'_>) -> jackin_telemetry::schema::enums::ScreenId {
    use jackin_telemetry::schema::enums::ScreenId;

    match state.stage {
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => ScreenId::WorkspaceList,
        ManagerStage::Editor(_) => ScreenId::WorkspaceEditor,
        ManagerStage::Settings(_) => ScreenId::Settings,
        ManagerStage::CreatePrelude(_) => ScreenId::WorkspaceCreate,
    }
}

fn telemetry_widget(state: &ManagerState<'_>) -> Option<&'static str> {
    match &state.stage {
        ManagerStage::Editor(editor) => Some(match editor.active_tab {
            EditorTab::General => "general",
            EditorTab::Mounts => "mounts",
            EditorTab::Roles => "roles",
            EditorTab::Secrets => "secrets_environments",
            EditorTab::Auth => "auth",
        }),
        ManagerStage::Settings(settings) => Some(match settings.active_tab {
            SettingsTab::General => "general",
            SettingsTab::Mounts => "mounts",
            SettingsTab::Environments => "environments",
            SettingsTab::Auth => "auth",
            SettingsTab::Trust => "trust",
        }),
        _ => None,
    }
}

pub(crate) fn record_manager_action(
    state: &ManagerState<'_>,
    action: jackin_telemetry::schema::enums::UiActionName,
) {
    jackin_telemetry::ui::record_action(action, telemetry_screen(state), telemetry_widget(state));
}

pub(crate) const fn action_of(
    message: &ManagerMessage,
) -> Option<jackin_telemetry::schema::enums::UiActionName> {
    use jackin_telemetry::schema::enums::UiActionName;

    match message {
        ManagerMessage::MoveEditorTab { .. }
        | ManagerMessage::SelectEditorTab(_)
        | ManagerMessage::MoveSettingsTab { .. }
        | ManagerMessage::SelectSettingsTab(_) => Some(UiActionName::TabSwitch),
        ManagerMessage::EnterCreatePrelude(_) => Some(UiActionName::WorkspaceCreate),
        ManagerMessage::EnterEditor(_) => Some(UiActionName::WorkspaceOpen),
        ManagerMessage::EnterSettings(_) => Some(UiActionName::SettingsOpen),
        ManagerMessage::ReturnToList => Some(UiActionName::ScreenBack),
        ManagerMessage::DismissSettingsErrorPopup
        | ManagerMessage::DismissStatusPopup
        | ManagerMessage::DismissListModal
        | ManagerMessage::DismissInlineSessionPicker
        | ManagerMessage::DismissInlineRolePicker
        | ManagerMessage::DismissInlineAgentPicker
        | ManagerMessage::DismissInlineProviderPicker
        | ManagerMessage::DismissLaunchProviderPicker => Some(UiActionName::DialogCancel),
        _ => None,
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn set_editor_tab_bar_focus(state: &mut ManagerState<'_>, focused: bool) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.apply_tab_bar_focus_plan(editor_tab_bar_focus_plan(focused));
}

fn set_settings_tab_bar_focus(state: &mut ManagerState<'_>, focused: bool) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.apply_tab_bar_focus_plan(settings_tab_bar_focus_plan(focused));
}

fn clear_editor_auth_kind(state: &mut ManagerState<'_>) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let plan = clear_editor_auth_kind_plan();
    editor.apply_auth_kind_plan(plan);
}

fn enter_editor_auth_kind(state: &mut ManagerState<'_>, kind: AuthKind) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let plan = enter_editor_auth_kind_plan(kind);
    editor.apply_auth_kind_plan(plan);
}

fn enter_confirm_delete(state: &mut ManagerState<'_>, name: String) {
    let plan = workspace_delete_confirm_plan(name);
    apply_manager_stage(
        state,
        ManagerStage::ConfirmDelete {
            state: plan.state,
            name: plan.name,
        },
    );
}

fn enter_confirm_instance_purge(state: &mut ManagerState<'_>, container: String, label: String) {
    let plan = instance_purge_confirm_plan(container, label);
    apply_manager_stage(
        state,
        ManagerStage::ConfirmInstancePurge {
            container: plan.container,
            state: plan.state,
            label: plan.label,
        },
    );
}

fn enter_create_editor(
    state: &mut ManagerState<'_>,
    name: String,
    workspace: jackin_config::WorkspaceConfig,
) {
    let editor = EditorState::new_create_with_workspace(name, workspace);
    apply_manager_stage(state, ManagerStage::Editor(editor));
}

fn reload_from_config(
    state: &mut ManagerState<'_>,
    config: &jackin_config::AppConfig,
    cwd: &std::path::Path,
) {
    let cache = std::rc::Rc::clone(&state.op_cache);
    let op_available = state.op_available;
    *state = ManagerState::from_config_with_cache_and_op(config, cwd, cache, op_available);
}

fn clear_settings_auth_kind(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.auth.clear_selected_kind();
}

fn dismiss_settings_error_popup(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.dismiss_error_popup();
}

fn open_settings_error_popup(
    state: &mut ManagerState<'_>,
    title: impl Into<String>,
    message: impl Into<String>,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.open_error_popup(title, message);
}

fn apply_op_commit_result(
    state: &mut ManagerState<'_>,
    op_ref: jackin_core::OpRef,
    result: anyhow::Result<()>,
    is_settings: bool,
) {
    if is_settings {
        match result {
            Ok(()) => state.apply_op_picker_op_ref_committed_for_settings(op_ref),
            Err(error) => state.apply_op_picker_commit_failed_for_settings(&error),
        }
        return;
    }
    match result {
        Ok(()) => state.apply_op_picker_op_ref_committed_for_editor(op_ref),
        Err(error) => state.apply_op_picker_commit_failed_for_editor(&error),
    }
}

fn enter_settings_auth_kind(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.auth.enter_selected_kind();
}

fn move_editor_tab(state: &mut ManagerState<'_>, delta: isize, focus_tab_bar: bool) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let plan = editor_tab_move_plan(editor.active_tab, delta, focus_tab_bar);
    editor.apply_tab_move_plan(plan);
}

fn move_editor_field_selection(
    state: &mut ManagerState<'_>,
    delta: isize,
    max_row: usize,
    skipped_rows: &[usize],
    term: Rect,
    footer_h: u16,
) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let FieldFocus::Row(row) = editor.active_field;
    let plan = editor_field_selection_plan(
        row,
        delta,
        max_row,
        skipped_rows,
        editor.tab_scroll_y,
        term.height,
        footer_h,
    );
    editor.apply_field_selection_plan(plan);
}

fn move_settings_tab(state: &mut ManagerState<'_>, delta: isize, focus_tab_bar: bool) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let plan = settings_tab_move_plan(settings.active_tab, delta, focus_tab_bar);
    settings.apply_tab_move_plan(plan);
}

fn move_settings_general_selection(state: &mut ManagerState<'_>, delta: isize) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.general.move_selection(delta);
}

fn toggle_settings_general_selected(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.general.toggle_selected();
}

fn set_editor_auth_role_expanded(state: &mut ManagerState<'_>, role: String, expanded: bool) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.set_auth_role_expanded(role, expanded);
}

fn set_editor_secrets_role_expanded(state: &mut ManagerState<'_>, role: String, expanded: bool) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.set_secrets_role_expanded(role, expanded);
}

fn toggle_editor_general_selected(state: &mut ManagerState<'_>) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.toggle_general_selected();
}

fn toggle_editor_mount_readonly_selected(state: &mut ManagerState<'_>) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.toggle_selected_mount_readonly();
}

fn toggle_editor_secret_mask(state: &mut ManagerState<'_>, scope: SecretsScopeTag, key: String) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.toggle_secret_mask(scope, key);
}

fn set_settings_env_role_expanded(state: &mut ManagerState<'_>, role: String, expanded: bool) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.env.set_role_expanded(role, expanded);
}

fn toggle_settings_global_mount_readonly(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.mounts.toggle_selected_readonly();
}

fn toggle_settings_trust_selected(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.trust.toggle_selected();
}

fn move_settings_auth_selection(state: &mut ManagerState<'_>, delta: isize) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.auth.move_selection(delta);
}

fn scroll_editor_tab_horizontal(
    state: &mut ManagerState<'_>,
    delta: i16,
    term_width: u16,
    content_width: usize,
) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let plan =
        editor_tab_horizontal_scroll_plan(editor.tab_scroll_x, delta, term_width, content_width);
    editor.apply_tab_horizontal_scroll_plan(plan);
}

fn scroll_editor_workspace_mounts_horizontal(
    state: &mut ManagerState<'_>,
    delta: i16,
    term_width: u16,
    content_width: usize,
) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let plan = editor_workspace_mounts_horizontal_scroll_plan(
        editor.workspace_mounts_scroll_x,
        delta,
        term_width,
        content_width,
    );
    editor.apply_workspace_mounts_horizontal_scroll_plan(plan);
}

fn scroll_settings_global_mounts_horizontal(
    state: &mut ManagerState<'_>,
    delta: i16,
    term_width: u16,
    content_width: usize,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let scroll_x =
        settings_horizontal_scroll_plan(settings.mounts.scroll_x, delta, term_width, content_width);
    settings.mounts.apply_horizontal_scroll(scroll_x);
}

fn scroll_settings_trust_horizontal(
    state: &mut ManagerState<'_>,
    delta: i16,
    term_width: u16,
    content_width: usize,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let scroll_x =
        settings_horizontal_scroll_plan(settings.trust.scroll_x, delta, term_width, content_width);
    settings.trust.apply_horizontal_scroll(scroll_x);
}

fn move_settings_global_mounts_selection(
    state: &mut ManagerState<'_>,
    delta: isize,
    term: Rect,
    footer_h: u16,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let plan = settings_global_mounts_selection_plan(
        settings.mounts.selected,
        settings.mounts.pending.len(),
        delta,
        settings.mounts.scroll_y,
        term.height,
        footer_h,
    );
    settings.mounts.apply_selection_plan(plan);
}

fn move_settings_env_selection(
    state: &mut ManagerState<'_>,
    delta: isize,
    term: Rect,
    footer_h: u16,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let rows = settings.env_flat_rows();
    let plan = settings_env_selection_plan(
        settings.env.selected,
        &rows,
        delta,
        settings.env.scroll_y,
        term.height,
        footer_h,
    );
    settings.env.apply_selection_plan(plan);
}

fn move_settings_trust_selection(
    state: &mut ManagerState<'_>,
    delta: isize,
    term: Rect,
    footer_h: u16,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let plan = settings_trust_selection_plan(
        settings.trust.selected,
        settings.trust.pending.len(),
        delta,
        settings.trust.scroll_y,
        term.height,
        footer_h,
    );
    settings.trust.apply_selection_plan(plan);
}

fn collapse_selected_tree(state: &mut ManagerState<'_>) {
    apply_inline_picker_dismissal_plan(
        state,
        inline_picker_dismissal_plan(InlinePickerDismissal::NewSession),
    );
    apply_workspace_tree_disclosure_plan(state, collapse_selected_tree_plan(state.selected_row()));
}

fn expand_selected_tree(state: &mut ManagerState<'_>) {
    apply_inline_picker_dismissal_plan(
        state,
        inline_picker_dismissal_plan(InlinePickerDismissal::NewSession),
    );
    apply_workspace_tree_disclosure_plan(state, expand_selected_tree_plan(state.selected_row()));
}

fn move_list_selection(state: &mut ManagerState<'_>, delta: isize) {
    let plan = workspace_list_move_selection_plan(state.selected, state.row_count(), delta);
    apply_workspace_list_selection_plan(state, plan);
}

fn select_list_row(state: &mut ManagerState<'_>, selected: usize) {
    let plan = workspace_list_select_row_plan(state.selected, selected, state.row_count());
    apply_workspace_list_selection_plan(state, plan);
}

fn select_editor_tab(state: &mut ManagerState<'_>, tab: EditorTab) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let plan = editor_tab_select_plan(editor.active_tab, tab);
    editor.apply_tab_select_plan(plan);
}

fn select_editor_mount_row(state: &mut ManagerState<'_>, row: usize) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let plan = editor_mount_row_select_plan(row);
    editor.apply_mount_row_select_plan(plan);
}

fn select_settings_tab(state: &mut ManagerState<'_>, tab: SettingsTab) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let plan = settings_tab_select_plan(tab);
    settings.apply_tab_move_plan(plan);
}

fn select_settings_trust_row(state: &mut ManagerState<'_>, row: usize) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let plan = settings_trust_row_select_plan(row, settings.trust.pending.len());
    settings.apply_trust_row_select_plan(plan);
}

fn move_preview_pane(state: &mut ManagerState<'_>, container: &str, delta: isize) {
    let len = state.flattened_preview_panes(container).len();
    let plan = preview_pane_cursor_plan(
        len,
        state.preview_pane_cursor.get(container).copied(),
        delta,
    );
    apply_preview_pane_cursor_plan(state, container, plan);
}

fn scroll_list_horizontal(state: &mut ManagerState<'_>, delta: i16) {
    let plan = workspace_list_horizontal_scroll_target_plan(
        state.list_names_focused(),
        state.list_scroll_focus(),
    );
    apply_workspace_list_horizontal_scroll_plan(state, plan, delta);
}

fn scroll_focused_mount_block_vertical(state: &mut ManagerState<'_>, delta: i16) {
    let plan = workspace_list_vertical_scroll_target_plan(state.list_scroll_focus());
    apply_workspace_list_vertical_scroll_plan(state, plan, delta);
}

/// Wire up an OP reference commit-validation subscription into manager state.
pub fn execute_op_commit_validation(
    state: &mut ManagerState<'_>,
    op_ref: jackin_core::OpRef,
    is_settings: bool,
) {
    let rx = crate::tui::op_picker::start_ref_validation(op_ref.clone());
    if is_settings {
        if let ManagerStage::Settings(settings) = &mut state.stage {
            settings
                .auth
                .set_pending_op_commit(super::PendingOpCommit::new(op_ref, rx));
        }
    } else if let ManagerStage::Editor(editor) = &mut state.stage {
        editor.pending_op_commit = Some(super::PendingOpCommit::new(op_ref, rx));
    }
}

/// Apply an async token-generate result (success or failure) to manager state.
pub fn apply_token_generate_result(
    state: &mut ManagerState<'_>,
    result: anyhow::Result<jackin_core::EnvValue>,
) {
    match result {
        Ok(env_value) => apply_generated_token(state, env_value),
        Err(error) => report_token_generate_error(state, error),
    }
}

fn apply_generated_token(state: &mut ManagerState<'_>, env_value: jackin_core::EnvValue) {
    if let jackin_core::EnvValue::OpRef(op_ref) = &env_value {
        crate::tui::op_picker::invalidate_cache_for_ref(&state.op_cache, op_ref);
    }

    match &mut state.stage {
        ManagerStage::Editor(editor) => match env_value {
            jackin_core::EnvValue::OpRef(op_ref) => {
                crate::tui::input::auth::apply_op_picker_to_auth_form_committed(editor, op_ref);
            }
            jackin_core::EnvValue::Plain(value) => {
                crate::tui::input::auth::apply_plain_text_to_auth_form(editor, &value);
            }
            jackin_core::EnvValue::Extended(e) => {
                crate::tui::input::auth::apply_plain_text_to_auth_form(editor, &e.value);
            }
        },
        ManagerStage::Settings(settings) => match env_value {
            jackin_core::EnvValue::OpRef(op_ref) => {
                crate::tui::input::apply_op_picker_to_settings_auth_form_committed(
                    &mut settings.auth,
                    op_ref,
                );
            }
            jackin_core::EnvValue::Plain(value) => {
                crate::tui::input::apply_plain_text_to_settings_auth_form(
                    &mut settings.auth,
                    &value,
                );
            }
            jackin_core::EnvValue::Extended(e) => {
                crate::tui::input::apply_plain_text_to_settings_auth_form(
                    &mut settings.auth,
                    &e.value,
                );
            }
        },
        _ => {}
    }
}

fn report_token_generate_error(state: &mut ManagerState<'_>, error: anyhow::Error) {
    use crate::tui::components::error_popup;
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            editor.modal = Some(Modal::ErrorPopup {
                state: error_popup::token_generation_failed_error_popup_state(error),
            });
        }
        ManagerStage::Settings(_) => {
            let _unused = update_manager(
                state,
                ManagerMessage::OpenSettingsErrorPopup {
                    title: error_popup::token_generation_failed_error_title().into(),
                    message: error.to_string(),
                },
            );
        }
        _ => {}
    }
}

/// Open a URL in the system browser; on failure route error popup to the active stage.
pub fn execute_open_url(state: &mut ManagerState<'_>, url: &str) -> bool {
    match crate::services::browser::open_url(url) {
        Ok(()) => false,
        Err(error) => {
            report_open_url_error(state, error);
            true
        }
    }
}

/// Apply a URL-open failure to manager state (opens an error popup).
pub fn report_open_url_error(state: &mut ManagerState<'_>, error: anyhow::Error) {
    use crate::tui::components::error_popup;
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            editor.modal = Some(Modal::ErrorPopup {
                state: error_popup::failed_to_open_url_error_popup_state(error),
            });
        }
        ManagerStage::Settings(_) => {
            let _unused = update_manager(
                state,
                ManagerMessage::OpenSettingsErrorPopup {
                    title: error_popup::failed_to_open_url_error_title().into(),
                    message: error.to_string(),
                },
            );
        }
        _ => {
            let _unused = update_manager(
                state,
                ManagerMessage::OpenListErrorPopup {
                    title: error_popup::failed_to_open_url_error_title().into(),
                    message: error.to_string(),
                },
            );
        }
    }
}

#[cfg(test)]
mod tests;
