//! Workspace-manager message/update boundary.
//!
//! This starts the Model/Update/View migration with state-only list messages.
//! Input handlers should increasingly translate terminal events into these
//! messages instead of mutating `ManagerState` inline.

use super::effect::ManagerEffect;
use crate::console::domain::InstanceRefreshSnapshot;
use crate::console::tui::state::{
    CreatePreludeState, DragState, EditorState, EditorTab, FieldFocus, ManagerStage, ManagerState,
    MountScrollFocus, PendingDriftCheck, PendingIsolationCleanup, PendingMountInfoRefresh,
    PendingRoleLoad, SecretsScopeTag, SettingsState, SettingsTab,
};
use jackin_config::AppConfig;
use jackin_console::tui::auth::AuthKind;
use jackin_console::tui::screens::editor::update::{
    clear_editor_auth_kind_plan, editor_field_selection_plan, editor_mount_row_select_plan,
    editor_tab_bar_focus_plan, editor_tab_horizontal_scroll_plan, editor_tab_move_plan,
    editor_tab_select_plan, editor_workspace_mounts_horizontal_scroll_plan,
    enter_editor_auth_kind_plan, set_role_expanded as set_editor_role_expanded,
    toggle_general_selected as toggle_editor_general_row,
    toggle_mount_readonly as toggle_editor_mount_readonly,
    toggle_secret_mask as toggle_editor_secret_mask_row,
};
use jackin_console::tui::screens::settings::update::{
    settings_env_selection_plan, settings_global_mounts_selection_plan,
    settings_horizontal_scroll_plan, settings_tab_bar_focus_plan, settings_tab_move_plan,
    settings_tab_select_plan, settings_trust_row_select_plan, settings_trust_selection_plan,
    toggle_readonly as toggle_settings_readonly,
};
use jackin_console::tui::screens::workspaces::update::{
    PreviewFocusPlan, WorkspaceListScrollTargetPlan, apply_workspace_list_selection_plan,
    apply_workspace_tree_disclosure_plan, collapse_selected_tree_plan, enter_preview_focus_plan,
    exit_preview_focus_plan, expand_selected_tree_plan, instance_purge_confirm_plan,
    preview_pane_cursor_plan, workspace_delete_confirm_plan,
    workspace_list_horizontal_scroll_target_plan, workspace_list_move_selection_plan,
    workspace_list_select_row_plan, workspace_list_vertical_scroll_target_plan,
    workspace_unclamped_scroll_plan,
};
use jackin_console::tui::update::{
    InlinePickerDismissal, apply_inline_picker_dismissal_plan, apply_list_modal_plan,
    apply_status_overlay_plan, dismiss_list_modal_plan, dismiss_status_overlay_plan,
    drag_state_plan, inline_picker_dismissal_plan, list_names_focus_plan, list_scroll_focus_plan,
    list_split_pct_plan, open_container_info_modal_plan, open_github_picker_modal_plan,
    open_status_overlay_plan,
};
use ratatui::layout::Rect;

pub(crate) type ManagerMessage = jackin_console::tui::message::ConsoleManagerMessage<
    AuthKind,
    CreatePreludeState<'static>,
    EditorState<'static>,
    SettingsState<'static>,
    InstanceRefreshSnapshot,
    PendingMountInfoRefresh,
    jackin_core::OpRef,
    AppConfig,
    jackin_config::WorkspaceConfig,
    EditorTab,
    SettingsTab,
    SecretsScopeTag,
    MountScrollFocus,
    DragState,
    jackin_tui::components::ContainerInfoState,
    jackin_console::tui::components::github_picker::GithubPickerState,
>;

pub(crate) type ManagerBackgroundEvent = jackin_console::tui::message::BackgroundEvent<
    ManagerMessage,
    PendingRoleLoad,
    PendingDriftCheck,
    crate::runtime::drift::DriftDetection,
    PendingIsolationCleanup,
>;

pub(crate) type ManagerUpdate = jackin_console::tui::update::ConsoleUpdate<ManagerEffect>;

#[expect(
    clippy::too_many_lines,
    reason = "pending extraction — tracked in codebase-readability roadmap"
)]
pub(crate) fn update_manager(
    state: &mut ManagerState<'_>,
    message: ManagerMessage,
) -> ManagerUpdate {
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
            state.stage = ManagerStage::CreatePrelude(prelude);
        }
        ManagerMessage::EnterEditor(editor) => {
            state.stage = ManagerStage::Editor(editor);
        }
        ManagerMessage::EnterEditorAuthKind { kind } => enter_editor_auth_kind(state, kind),
        ManagerMessage::EnterSettings(settings) => {
            state.stage = ManagerStage::Settings(settings);
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
        ManagerMessage::ReturnToList => state.stage = ManagerStage::List,
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
            state.drag_state = drag_state_plan(drag);
        }
        ManagerMessage::SetListSplitPct(pct) => {
            state.list_split_pct = list_split_pct_plan(pct);
        }
        ManagerMessage::OpenListErrorPopup { title, message } => {
            state.open_list_error_popup(title, message);
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
    ManagerUpdate::redraw()
}

const fn apply_preview_focus_plan(state: &mut ManagerState<'_>, plan: PreviewFocusPlan) {
    state.preview_focused = plan.focused;
}

fn set_editor_tab_bar_focus(state: &mut ManagerState<'_>, focused: bool) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.set_tab_bar_focused(editor_tab_bar_focus_plan(focused));
}

fn set_settings_tab_bar_focus(state: &mut ManagerState<'_>, focused: bool) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.set_tab_bar_focused(settings_tab_bar_focus_plan(focused));
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
    state.stage = ManagerStage::ConfirmDelete {
        state: plan.state,
        name: plan.name,
    };
}

fn enter_confirm_instance_purge(state: &mut ManagerState<'_>, container: String, label: String) {
    let plan = instance_purge_confirm_plan(container, label);
    state.stage = ManagerStage::ConfirmInstancePurge {
        container: plan.container,
        state: plan.state,
        label: plan.label,
    };
}

fn enter_create_editor(
    state: &mut ManagerState<'_>,
    name: String,
    workspace: jackin_config::WorkspaceConfig,
) {
    let mut editor = EditorState::new_create();
    editor.pending = workspace;
    editor.pending_name = Some(name);
    state.stage = ManagerStage::Editor(editor);
}

fn reload_from_config(state: &mut ManagerState<'_>, config: &AppConfig, cwd: &std::path::Path) {
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
    settings.error_popup = None;
    settings.auth.restore_pending_auth_form();
}

fn open_settings_error_popup(
    state: &mut ManagerState<'_>,
    title: impl Into<String>,
    message: impl Into<String>,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.error_popup =
        Some(jackin_console::tui::components::error_popup::error_popup_state(title, message));
}

fn apply_op_commit_result(
    state: &mut ManagerState<'_>,
    op_ref: jackin_core::OpRef,
    result: anyhow::Result<()>,
    is_settings: bool,
) {
    if is_settings {
        let ManagerStage::Settings(settings) = &mut state.stage else {
            return;
        };
        match result {
            Ok(()) => {
                crate::console::tui::input::apply_op_picker_to_settings_auth_form_committed(
                    &mut settings.auth,
                    op_ref,
                );
            }
            Err(error) => {
                crate::console::tui::input::apply_op_picker_settings_commit_failed(
                    &mut settings.auth,
                    &error,
                );
            }
        }
        return;
    }

    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    match result {
        Ok(()) => {
            crate::console::tui::input::auth::apply_op_picker_to_auth_form_committed(
                editor, op_ref,
            );
        }
        Err(error) => {
            crate::console::tui::input::auth::apply_op_picker_commit_failed(editor, &error);
        }
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
    set_editor_role_expanded(&mut editor.auth_expanded, role, expanded);
}

fn set_editor_secrets_role_expanded(state: &mut ManagerState<'_>, role: String, expanded: bool) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    set_editor_role_expanded(&mut editor.secrets_expanded, role, expanded);
}

fn toggle_editor_general_selected(state: &mut ManagerState<'_>) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let FieldFocus::Row(row) = editor.active_field;
    toggle_editor_general_row(
        row,
        &mut editor.pending.keep_awake.enabled,
        &mut editor.pending.git_pull_on_entry,
    );
}

fn toggle_editor_mount_readonly_selected(state: &mut ManagerState<'_>) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let FieldFocus::Row(row) = editor.active_field;
    if let Some(mount) = editor.pending.mounts.get_mut(row) {
        toggle_editor_mount_readonly(&mut mount.readonly);
    }
}

fn toggle_editor_secret_mask(state: &mut ManagerState<'_>, scope: SecretsScopeTag, key: String) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    toggle_editor_secret_mask_row(&mut editor.unmasked_rows, scope, key);
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
    if let Some(row) = settings.mounts.pending.get_mut(settings.mounts.selected) {
        toggle_settings_readonly(&mut row.mount.readonly);
    }
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
    state.inline_new_session_picker = None;
    apply_workspace_tree_disclosure_plan(state, collapse_selected_tree_plan(state.selected_row()));
}

fn expand_selected_tree(state: &mut ManagerState<'_>) {
    state.inline_new_session_picker = None;
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

#[allow(clippy::missing_const_for_fn)]
fn select_settings_trust_row(state: &mut ManagerState<'_>, row: usize) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let plan = settings_trust_row_select_plan(row, settings.trust.pending.len());
    let content_focused = settings.trust.apply_row_select_plan(plan);
    settings.set_content_focused(SettingsTab::Trust, content_focused);
}

fn move_preview_pane(state: &mut ManagerState<'_>, container: &str, delta: isize) {
    let len = state.flattened_preview_panes(container).len();
    let next = preview_pane_cursor_plan(
        len,
        state.preview_pane_cursor.get(container).copied(),
        delta,
    );
    let Some(next) = next else {
        state.preview_focused = false;
        return;
    };
    state.preview_pane_cursor.insert(container.to_owned(), next);
}

const fn scroll_list_horizontal(state: &mut ManagerState<'_>, delta: i16) {
    match workspace_list_horizontal_scroll_target_plan(
        state.list_names_focused(),
        state.list_scroll_focus(),
    ) {
        WorkspaceListScrollTargetPlan::ListNames => {
            state.list_names_scroll_x =
                workspace_unclamped_scroll_plan(state.list_names_scroll_x, delta);
        }
        WorkspaceListScrollTargetPlan::FocusedBlock(focus) => {
            scroll_focused_mount_block(state, focus, delta);
        }
        WorkspaceListScrollTargetPlan::None => {}
    }
}

const fn scroll_focused_mount_block(
    state: &mut ManagerState<'_>,
    focus: MountScrollFocus,
    delta: i16,
) {
    let value = state.list_scroll_x_mut(focus);
    *value = workspace_unclamped_scroll_plan(*value, delta);
}

const fn scroll_focused_mount_block_vertical(state: &mut ManagerState<'_>, delta: i16) {
    match workspace_list_vertical_scroll_target_plan(state.list_scroll_focus()) {
        WorkspaceListScrollTargetPlan::FocusedBlock(focus) => {
            let value = state.list_scroll_y_mut(focus);
            *value = workspace_unclamped_scroll_plan(*value, delta);
        }
        WorkspaceListScrollTargetPlan::ListNames | WorkspaceListScrollTargetPlan::None => {}
    }
}

#[cfg(test)]
mod tests;
