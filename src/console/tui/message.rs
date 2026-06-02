//! Workspace-manager message/update boundary.
//!
//! This starts the Model/Update/View migration with state-only list messages.
//! Input handlers should increasingly translate terminal events into these
//! messages instead of mutating `ManagerState` inline.

use jackin_console::tui::auth::AuthKind;
use super::effect::ManagerEffect;
use crate::console::tui::state::{
    CreatePreludeState, DragState, EditorState, EditorTab, FieldFocus, ManagerListRow,
    ManagerStage, ManagerState, MountScrollFocus, PendingDriftCheck, PendingIsolationCleanup,
    PendingMountInfoRefresh, PendingRoleLoad, SecretsScopeTag, SettingsState, SettingsTab,
    settings_env_flat_rows,
};
use crate::config::AppConfig;
use crate::console::domain::InstanceRefreshSnapshot;
use jackin_console::tui::screens::editor::update::{
    clear_editor_auth_kind_plan, editor_field_selection_plan, editor_tab_move_plan,
    editor_tab_select_plan, enter_editor_auth_kind_plan,
    set_role_expanded as set_editor_role_expanded,
    toggle_general_selected as toggle_editor_general_row,
    toggle_mount_readonly as toggle_editor_mount_readonly,
    toggle_secret_mask as toggle_editor_secret_mask_row,
};
use jackin_console::tui::screens::settings::update::{
    clear_settings_auth_kind_plan, enter_settings_auth_kind_plan, move_general_selection,
    set_role_expanded as set_settings_role_expanded, settings_auth_selection_plan,
    settings_env_selection_plan, settings_global_mounts_selection_plan, settings_tab_move_plan,
    settings_tab_select_plan, settings_trust_selection_plan, toggle_general_selected,
    toggle_readonly as toggle_settings_readonly, toggle_trust_selected,
};
use jackin_console::tui::screens::workspaces::view::{
    instance_purge_confirm_state, workspace_delete_confirm_state,
};
use jackin_console::tui::screens::workspaces::update::preview_pane_cursor_plan;
use jackin_console::tui::update::{
    selected_index_plan, selection_move_plan, term_width_scroll_plan, unclamped_scroll_plan,
};
use ratatui::layout::Rect;
use std::path::PathBuf;

#[derive(Debug)]
pub(crate) enum ManagerMessage {
    CollapseSelectedTree,
    ClearEditorAuthKind,
    EnterPreview,
    EnterConfirmDelete {
        name: String,
    },
    EnterConfirmInstancePurge {
        container: String,
        label: String,
    },
    EnterCreateEditor {
        name: String,
        workspace: crate::workspace::WorkspaceConfig,
    },
    EnterCreatePrelude(CreatePreludeState<'static>),
    EnterEditor(EditorState<'static>),
    EnterEditorAuthKind {
        kind: AuthKind,
    },
    EnterSettings(SettingsState<'static>),
    InstancesRefreshed(Result<InstanceRefreshSnapshot, String>),
    MountInfoRefreshed(PendingMountInfoRefresh),
    OpCommitResolved {
        op_ref: crate::operator_env::OpRef,
        result: anyhow::Result<()>,
        is_settings: bool,
    },
    PollFileBrowserGitUrls,
    PollPickerLoads,
    FocusEditorContent,
    FocusEditorTabBar,
    FocusSettingsContent,
    FocusSettingsTabBar,
    ExitPreview,
    ExpandSelectedTree,
    ClearSettingsAuthKind,
    DismissSettingsErrorPopup,
    OpenSettingsErrorPopup {
        title: String,
        message: String,
    },
    EnterSettingsAuthKind,
    ScrollEditorTabHorizontal {
        delta: i16,
        term_width: u16,
        content_width: usize,
    },
    SelectEditorMountRow(usize),
    SelectEditorTab(EditorTab),
    SelectListRow(usize),
    SelectSettingsTab(SettingsTab),
    SelectSettingsTrustRow(usize),
    ScrollEditorWorkspaceMountsHorizontal {
        delta: i16,
        term_width: u16,
        content_width: usize,
    },
    ScrollSettingsGlobalMountsHorizontal {
        delta: i16,
        term_width: u16,
        content_width: usize,
    },
    ScrollSettingsTrustHorizontal {
        delta: i16,
        term_width: u16,
        content_width: usize,
    },
    MoveSettingsGlobalMountsSelection {
        delta: isize,
        term: Rect,
        footer_h: u16,
    },
    MoveSettingsEnvSelection {
        delta: isize,
        term: Rect,
        footer_h: u16,
    },
    MoveSettingsTrustSelection {
        delta: isize,
        term: Rect,
        footer_h: u16,
    },
    MoveEditorTab {
        delta: isize,
        focus_tab_bar: bool,
    },
    MoveEditorFieldSelection {
        delta: isize,
        max_row: usize,
        skipped_rows: Vec<usize>,
        term: Rect,
        footer_h: u16,
    },
    MoveSettingsTab {
        delta: isize,
        focus_tab_bar: bool,
    },
    MoveSettingsGeneralSelection {
        delta: isize,
    },
    MoveSettingsAuthSelection {
        delta: isize,
    },
    SetSettingsEnvRoleExpanded {
        role: String,
        expanded: bool,
    },
    SetEditorAuthRoleExpanded {
        role: String,
        expanded: bool,
    },
    SetEditorSecretsRoleExpanded {
        role: String,
        expanded: bool,
    },
    ToggleSettingsGlobalMountReadonly,
    ToggleEditorGeneralSelected,
    ToggleEditorMountReadonlySelected,
    ToggleEditorSecretMask {
        scope: SecretsScopeTag,
        key: String,
    },
    ToggleSettingsGeneralSelected,
    ToggleSettingsTrustSelected,
    MoveListSelection(isize),
    MovePreviewPane {
        container: String,
        delta: isize,
    },
    ReloadFromConfig {
        config: Box<AppConfig>,
        cwd: PathBuf,
    },
    ReturnToList,
    ScrollListHorizontal(i16),
    ScrollFocusedListBlockVertical(i16),
    SetListScrollFocus(Option<MountScrollFocus>),
    SetListNamesFocused(bool),
    SetDragState(Option<DragState>),
    SetListSplitPct(u16),
    OpenListErrorPopup {
        title: String,
        message: String,
    },
    OpenStatusPopup {
        title: String,
        message: String,
    },
    DismissStatusPopup,
    OpenListContainerInfo {
        state: jackin_tui::components::ContainerInfoState,
    },
    OpenListGithubPicker {
        state: jackin_console::tui::components::github_picker::GithubPickerState,
    },
    DismissListModal,
    DismissInlineSessionPicker,
    DismissInlineRolePicker,
    DismissInlineAgentPicker,
    DismissInlineProviderPicker,
    DismissLaunchProviderPicker,
}

pub(crate) type ManagerBackgroundEvent = jackin_console::tui::message::BackgroundEvent<
    ManagerMessage,
    PendingRoleLoad,
    PendingDriftCheck,
    crate::config::DriftDetection,
    PendingIsolationCleanup,
>;

pub(crate) type ManagerUpdate = jackin_console::tui::update::ConsoleUpdate<ManagerEffect>;

#[allow(clippy::too_many_lines)]
pub(crate) fn update_manager(
    state: &mut ManagerState<'_>,
    message: ManagerMessage,
) -> ManagerUpdate {
    match message {
        ManagerMessage::CollapseSelectedTree => collapse_selected_tree(state),
        ManagerMessage::ClearEditorAuthKind => clear_editor_auth_kind(state),
        ManagerMessage::EnterPreview => state.preview_focused = true,
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
        ManagerMessage::ExitPreview => state.preview_focused = false,
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
            state.list_scroll_focus = focus;
        }
        ManagerMessage::SetListNamesFocused(focused) => {
            state.list_names_focused = focused;
        }
        ManagerMessage::SetDragState(drag) => {
            state.drag_state = drag;
        }
        ManagerMessage::SetListSplitPct(pct) => {
            state.list_split_pct = pct;
        }
        ManagerMessage::OpenListErrorPopup { title, message } => {
            state.open_list_error_popup(title, message);
        }
        ManagerMessage::OpenStatusPopup { title, message } => {
            state.status_overlay = Some(
                jackin_console::tui::components::status_popup::status_popup_state(title, message),
            );
        }
        ManagerMessage::DismissStatusPopup => {
            state.status_overlay = None;
        }
        ManagerMessage::OpenListContainerInfo { state: info } => {
            state.list_modal = Some(crate::console::tui::state::Modal::ContainerInfo { state: info });
        }
        ManagerMessage::OpenListGithubPicker { state: picker } => {
            state.list_modal = Some(crate::console::tui::state::Modal::GithubPicker { state: picker });
        }
        ManagerMessage::DismissListModal => {
            state.list_modal = None;
        }
        ManagerMessage::DismissInlineSessionPicker => {
            state.inline_new_session_picker = None;
        }
        ManagerMessage::DismissInlineRolePicker => {
            state.inline_role_picker = None;
        }
        ManagerMessage::DismissInlineAgentPicker => {
            state.inline_agent_picker = None;
        }
        ManagerMessage::DismissInlineProviderPicker => {
            state.inline_provider_picker = None;
        }
        ManagerMessage::DismissLaunchProviderPicker => {
            state.launch_provider_picker = None;
        }
    }
    ManagerUpdate::redraw()
}

const fn set_editor_tab_bar_focus(state: &mut ManagerState<'_>, focused: bool) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.tab_bar_focused = focused;
}

const fn set_settings_tab_bar_focus(state: &mut ManagerState<'_>, focused: bool) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.tab_bar_focused = focused;
}

const fn clear_editor_auth_kind(state: &mut ManagerState<'_>) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let plan = clear_editor_auth_kind_plan();
    editor.auth_selected_kind = plan.selected_kind;
    editor.active_field = FieldFocus::Row(plan.active_row);
    editor.tab_scroll_x = plan.tab_scroll_x;
    editor.tab_scroll_y = plan.tab_scroll_y;
}

fn enter_editor_auth_kind(state: &mut ManagerState<'_>, kind: AuthKind) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let plan = enter_editor_auth_kind_plan(kind);
    editor.auth_selected_kind = plan.selected_kind;
    editor.active_field = FieldFocus::Row(plan.active_row);
    editor.tab_scroll_x = plan.tab_scroll_x;
    editor.tab_scroll_y = plan.tab_scroll_y;
}

fn enter_confirm_delete(state: &mut ManagerState<'_>, name: String) {
    state.stage = ManagerStage::ConfirmDelete {
        state: workspace_delete_confirm_state(&name),
        name,
    };
}

fn enter_confirm_instance_purge(state: &mut ManagerState<'_>, container: String, label: String) {
    state.stage = ManagerStage::ConfirmInstancePurge {
        container,
        state: instance_purge_confirm_state(&label),
        label,
    };
}

fn enter_create_editor(
    state: &mut ManagerState<'_>,
    name: String,
    workspace: crate::workspace::WorkspaceConfig,
) {
    let mut editor = EditorState::new_create();
    editor.pending = workspace;
    editor.pending_name = Some(name);
    state.stage = ManagerStage::Editor(editor);
}

fn reload_from_config(state: &mut ManagerState<'_>, config: &AppConfig, cwd: &std::path::Path) {
    let cache = state.op_cache.clone();
    let op_available = state.op_available;
    *state = ManagerState::from_config_with_cache_and_op(config, cwd, cache, op_available);
}

const fn clear_settings_auth_kind(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let plan = clear_settings_auth_kind_plan();
    settings.auth.selected_kind = plan.selected_kind;
    settings.auth.selected = plan.selected;
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
    settings.error_popup = Some(
        jackin_console::tui::components::error_popup::error_popup_state(title, message),
    );
}

fn apply_op_commit_result(
    state: &mut ManagerState<'_>,
    op_ref: crate::operator_env::OpRef,
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
    let selected_kind = settings
        .auth
        .pending
        .get(settings.auth.selected)
        .map(|row| row.kind);
    if let Some(plan) = enter_settings_auth_kind_plan(selected_kind) {
        settings.auth.selected_kind = plan.selected_kind;
        settings.auth.selected = plan.selected;
    }
}

fn move_editor_tab(state: &mut ManagerState<'_>, delta: isize, focus_tab_bar: bool) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let plan = editor_tab_move_plan(editor.active_tab, delta, focus_tab_bar);
    editor.active_tab = plan.active_tab;
    editor.tab_bar_focused = plan.tab_bar_focused;
    editor.active_field = FieldFocus::Row(plan.active_row);
    editor.tab_scroll_x = plan.tab_scroll_x;
    editor.tab_scroll_y = plan.tab_scroll_y;
    if plan.clear_auth_kind {
        editor.auth_selected_kind = None;
    }
    if plan.clear_secret_view_state {
        editor.unmasked_rows.clear();
        editor.secrets_expanded.clear();
    }
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
    editor.active_field = FieldFocus::Row(plan.active_row);
    editor.tab_scroll_y = plan.tab_scroll_y;
}

const fn move_settings_tab(state: &mut ManagerState<'_>, delta: isize, focus_tab_bar: bool) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let plan = settings_tab_move_plan(settings.active_tab, delta, focus_tab_bar);
    settings.active_tab = plan.active_tab;
    settings.tab_bar_focused = plan.tab_bar_focused;
}

fn move_settings_general_selection(state: &mut ManagerState<'_>, delta: isize) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    move_general_selection(&mut settings.general, delta);
}

fn toggle_settings_general_selected(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    toggle_general_selected(&mut settings.general);
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
    set_settings_role_expanded(&mut settings.env.expanded, role, expanded);
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
    toggle_trust_selected(&mut settings.trust);
}

fn move_settings_auth_selection(state: &mut ManagerState<'_>, delta: isize) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.auth.selected =
        settings_auth_selection_plan(settings.auth.selected, settings.auth.row_count(), delta);
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
    editor.tab_content_scroll_focused = true;
    editor.tab_scroll_x =
        term_width_scroll_plan(editor.tab_scroll_x, delta, term_width, content_width);
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
    editor.workspace_mounts_scroll_focused = true;
    editor.workspace_mounts_scroll_x =
        term_width_scroll_plan(editor.workspace_mounts_scroll_x, delta, term_width, content_width);
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
    settings.mounts.scroll_x =
        term_width_scroll_plan(settings.mounts.scroll_x, delta, term_width, content_width);
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
    settings.trust.scroll_x =
        term_width_scroll_plan(settings.trust.scroll_x, delta, term_width, content_width);
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
    settings.mounts.selected = plan.selected;
    settings.mounts.scroll_y = plan.scroll_y;
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
    let rows = settings_env_flat_rows(settings);
    let plan = settings_env_selection_plan(
        settings.env.selected,
        &rows,
        delta,
        settings.env.scroll_y,
        term.height,
        footer_h,
    );
    settings.env.selected = plan.selected;
    settings.env.scroll_y = plan.scroll_y;
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
    settings.trust.selected = plan.selected;
    settings.trust.scroll_y = plan.scroll_y;
}

fn collapse_selected_tree(state: &mut ManagerState<'_>) {
    state.inline_new_session_picker = None;
    match state.selected_row() {
        ManagerListRow::SavedWorkspace(i) => {
            state.collapse_workspace(i);
        }
        ManagerListRow::WorkspaceInstance(ws_idx, _) => {
            state.collapse_workspace(ws_idx);
        }
        ManagerListRow::CurrentDirectory | ManagerListRow::CurrentDirectoryInstance(_) => {
            state.collapse_current_dir();
        }
        ManagerListRow::NewWorkspace => {}
    }
}

fn expand_selected_tree(state: &mut ManagerState<'_>) {
    state.inline_new_session_picker = None;
    match state.selected_row() {
        ManagerListRow::SavedWorkspace(i) => state.expand_workspace(i),
        ManagerListRow::CurrentDirectory => state.expand_current_dir(),
        ManagerListRow::CurrentDirectoryInstance(_)
        | ManagerListRow::WorkspaceInstance(_, _)
        | ManagerListRow::NewWorkspace => {}
    }
}

fn move_list_selection(state: &mut ManagerState<'_>, delta: isize) {
    state.inline_role_picker = None;
    state.inline_agent_picker = None;
    state.inline_new_session_picker = None;
    let selected = selection_move_plan(state.selected, state.row_count(), delta);
    if selected != state.selected {
        state.reset_list_scroll();
        state.selected = selected;
    }
}

fn select_list_row(state: &mut ManagerState<'_>, selected: usize) {
    state.inline_role_picker = None;
    let selected = selected_index_plan(selected, state.row_count());
    if selected != state.selected {
        state.reset_list_scroll();
        state.selected = selected;
        state.inline_agent_picker = None;
        state.inline_new_session_picker = None;
        state.inline_provider_picker = None;
        state.launch_provider_picker = None;
    }
}

fn select_editor_tab(state: &mut ManagerState<'_>, tab: EditorTab) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let plan = editor_tab_select_plan(editor.active_tab, tab);
    editor.active_tab = plan.active_tab;
    editor.tab_bar_focused = plan.tab_bar_focused;
    editor.active_field = FieldFocus::Row(plan.active_row);
    editor.workspace_mounts_scroll_focused = plan.workspace_mounts_scroll_focused;
    if plan.clear_auth_kind {
        editor.auth_selected_kind = None;
    }
    if plan.clear_secret_view_state {
        editor.unmasked_rows.clear();
        editor.secrets_expanded.clear();
    }
}

const fn select_editor_mount_row(state: &mut ManagerState<'_>, row: usize) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.active_field = FieldFocus::Row(row);
    editor.workspace_mounts_scroll_focused = true;
}

fn select_settings_tab(state: &mut ManagerState<'_>, tab: SettingsTab) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let plan = settings_tab_select_plan(tab);
    settings.active_tab = plan.active_tab;
    settings.tab_bar_focused = plan.tab_bar_focused;
}

const fn select_settings_trust_row(state: &mut ManagerState<'_>, row: usize) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    if row < settings.trust.pending.len() {
        settings.trust.selected = row;
    }
    settings.trust.scroll_focused = true;
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
    if state.list_names_focused {
        state.list_names_scroll_x = unclamped_scroll_plan(state.list_names_scroll_x, delta);
    } else {
        scroll_focused_mount_block(state, delta);
    }
}

const fn scroll_focused_mount_block(state: &mut ManagerState<'_>, delta: i16) {
    let Some(focus) = state.list_scroll_focus else {
        return;
    };
    let value = state.list_scroll_x_mut(focus);
    *value = unclamped_scroll_plan(*value, delta);
}

const fn scroll_focused_mount_block_vertical(state: &mut ManagerState<'_>, delta: i16) {
    let Some(focus) = state.list_scroll_focus else {
        return;
    };
    let value = state.list_scroll_y_mut(focus);
    *value = unclamped_scroll_plan(*value, delta);
}
