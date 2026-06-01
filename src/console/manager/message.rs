//! Workspace-manager message/update boundary.
//!
//! This starts the Model/Update/View migration with state-only list messages.
//! Input handlers should increasingly translate terminal events into these
//! messages instead of mutating `ManagerState` inline.

use super::auth_kind::AuthKind;
use super::state::{
    CreatePreludeState, DragState, EditorState, EditorTab, FieldFocus, GlobalMountModal,
    InstanceRefreshSnapshot, ManagerListRow, ManagerStage, ManagerState, Modal, MountScrollFocus,
    PendingDriftCheck, PendingIsolationCleanup, PendingMountInfoRefresh, SecretsScopeTag,
    SettingsState, SettingsTab,
};
use crate::config::AppConfig;
use jackin_console::focus::moved_selection;
pub(crate) use jackin_console::tui::effect::ConsoleEffect as ManagerEffect;
use jackin_console::tui::screens::editor::update::{
    next_editor_tab, previous_editor_tab, set_role_expanded as set_editor_role_expanded,
    step_cursor_down, step_cursor_up, toggle_general_selected as toggle_editor_general_row,
    toggle_mount_readonly as toggle_editor_mount_readonly,
    toggle_secret_mask as toggle_editor_secret_mask_row,
};
use jackin_console::tui::screens::settings::model::SettingsEnvRow;
use jackin_console::tui::screens::settings::update::{
    move_general_selection, move_trust_selection, next_settings_tab, previous_settings_tab,
    set_role_expanded as set_settings_role_expanded, settings_env_flat_rows, step_cursor_down_by,
    step_cursor_up_by, toggle_general_selected, toggle_readonly as toggle_settings_readonly,
    toggle_trust_selected,
};
use ratatui::layout::Rect;
use std::path::PathBuf;
use jackin_tui::runtime::spawn_blocking_subscription;

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

pub(crate) type ManagerUpdate = jackin_console::tui::update::ConsoleUpdate<ManagerEffect>;

pub(crate) fn execute_manager_effect(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &crate::paths::JackinPaths,
    effect: ManagerEffect,
) {
    match effect {
        ManagerEffect::RequestActiveMountInfoRefresh => {
            if state.mount_info_refresh_in_flight() {
                return;
            }
            let Some((target, sources)) = state.active_mount_info_sources(config) else {
                return;
            };
            if tokio::runtime::Handle::try_current().is_err() {
                let entries = jackin_console::services::mount_info::inspect_entries(sources);
                let _ = update_manager(
                    state,
                    ManagerMessage::MountInfoRefreshed(PendingMountInfoRefresh {
                        target,
                        entries,
                    }),
                );
                return;
            }
            let rx = spawn_blocking_subscription(move || {
                let entries = jackin_console::services::mount_info::inspect_entries(sources);
                PendingMountInfoRefresh { target, entries }
            });
            state.begin_mount_info_refresh(rx);
        }
        ManagerEffect::RequestInstanceRefresh => {
            state.request_instance_refresh(paths);
        }
    }
}

pub(crate) type ManagerBackgroundEvent = jackin_console::tui::message::BackgroundEvent<
    ManagerMessage,
    super::state::PendingRoleLoad,
    PendingDriftCheck,
    crate::config::DriftDetection,
    PendingIsolationCleanup,
>;

pub(crate) fn poll_background_messages(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &crate::paths::JackinPaths,
) -> Vec<ManagerBackgroundEvent> {
    let mut messages = vec![
        ManagerBackgroundEvent::Message(ManagerMessage::PollFileBrowserGitUrls),
        ManagerBackgroundEvent::Message(ManagerMessage::PollPickerLoads),
    ];
    if let ManagerStage::Editor(editor) = &mut state.stage {
        if let Some((load, result)) = super::input::editor::poll_role_load_completion(editor) {
            messages.push(ManagerBackgroundEvent::RoleLoadFinished { load, result });
        }
    }
    execute_manager_effect(
        state,
        config,
        paths,
        ManagerEffect::RequestActiveMountInfoRefresh,
    );
    if let Some(result) = state.poll_mount_info_refresh() {
        messages.push(ManagerBackgroundEvent::Message(
            ManagerMessage::MountInfoRefreshed(result),
        ));
    }
    if let Some(result) = state.poll_instance_refresh() {
        messages.push(ManagerBackgroundEvent::Message(
            ManagerMessage::InstancesRefreshed(result),
        ));
    }
    execute_manager_effect(state, config, paths, ManagerEffect::RequestInstanceRefresh);
    if let Some((op_ref, result, is_settings)) = state.poll_pending_op_commit() {
        messages.push(ManagerBackgroundEvent::Message(
            ManagerMessage::OpCommitResolved {
                op_ref,
                result,
                is_settings,
            },
        ));
    }
    if let Some((check, detection)) = state.poll_pending_drift_check() {
        messages.push(ManagerBackgroundEvent::DriftCheckFinished { check, detection });
    }
    if let Some((cleanup, result)) = state.poll_pending_isolation_cleanup() {
        messages.push(ManagerBackgroundEvent::IsolationCleanupFinished { cleanup, result });
    }
    messages
}

fn poll_file_browser_git_urls(state: &mut ManagerState<'_>) -> bool {
    let mut dirty = false;
    if let Some(modal) = state.list_modal.as_mut() {
        dirty |= poll_modal_file_browser_git_url(modal);
    }
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            if let Some(modal) = editor.modal.as_mut() {
                dirty |= poll_modal_file_browser_git_url(modal);
            }
            for modal in &mut editor.modal_parents {
                dirty |= poll_modal_file_browser_git_url(modal);
            }
        }
        ManagerStage::CreatePrelude(prelude) => {
            if let Some(modal) = prelude.modal.as_mut() {
                dirty |= poll_modal_file_browser_git_url(modal);
            }
        }
        ManagerStage::Settings(settings) => {
            if let Some(modal) = settings.mounts.modal.as_mut() {
                dirty |= poll_global_mount_file_browser_git_url(modal);
            }
            for modal in &mut settings.mounts.modal_parents {
                dirty |= poll_global_mount_file_browser_git_url(modal);
            }
        }
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => {}
    }
    dirty
}

fn poll_modal_file_browser_git_url(modal: &mut Modal<'_>) -> bool {
    match modal {
        Modal::FileBrowser { state, .. } => state.poll_git_url_resolution(),
        _ => false,
    }
}

fn poll_global_mount_file_browser_git_url(modal: &mut GlobalMountModal<'_>) -> bool {
    match modal {
        GlobalMountModal::FileBrowser { state } => state.poll_git_url_resolution(),
        _ => false,
    }
}

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
            if state.poll_picker_loads() {
                return ManagerUpdate::redraw();
            }
        }
        ManagerMessage::PollFileBrowserGitUrls => {
            if poll_file_browser_git_urls(state) {
                return ManagerUpdate::redraw();
            }
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
            state.status_overlay = Some(jackin_tui::components::StatusPopupState::new(
                title, message,
            ));
        }
        ManagerMessage::DismissStatusPopup => {
            state.status_overlay = None;
        }
        ManagerMessage::OpenListContainerInfo { state: info } => {
            state.list_modal = Some(super::state::Modal::ContainerInfo { state: info });
        }
        ManagerMessage::OpenListGithubPicker { state: picker } => {
            state.list_modal = Some(super::state::Modal::GithubPicker { state: picker });
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
    editor.auth_selected_kind = None;
    editor.active_field = FieldFocus::Row(0);
    editor.tab_scroll_x = 0;
    editor.tab_scroll_y = 0;
}

const fn enter_editor_auth_kind(state: &mut ManagerState<'_>, kind: AuthKind) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.auth_selected_kind = Some(kind);
    editor.active_field = FieldFocus::Row(0);
    editor.tab_scroll_x = 0;
    editor.tab_scroll_y = 0;
}

fn enter_confirm_delete(state: &mut ManagerState<'_>, name: String) {
    state.stage = ManagerStage::ConfirmDelete {
        state: jackin_tui::components::ConfirmState::new(format!("Delete \"{name}\"?")),
        name,
    };
}

fn enter_confirm_instance_purge(state: &mut ManagerState<'_>, container: String, label: String) {
    let prompt = format!(
        "Purge \"{label}\"?\nThis removes the role container, DinD sidecar, volume, network, AND local recovery state. Cannot be undone."
    );
    state.stage = ManagerStage::ConfirmInstancePurge {
        container,
        label,
        state: jackin_tui::components::ConfirmState::new(prompt),
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
    settings.auth.selected_kind = None;
    settings.auth.selected = 0;
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
    settings.error_popup = Some(jackin_tui::components::ErrorPopupState::new(
        title.into(),
        message.into(),
    ));
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
                crate::console::manager::input::apply_op_picker_to_settings_auth_form_committed(
                    &mut settings.auth,
                    op_ref,
                );
            }
            Err(error) => {
                crate::console::manager::input::apply_op_picker_settings_commit_failed(
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
            crate::console::manager::input::auth::apply_op_picker_to_auth_form_committed(
                editor, op_ref,
            );
        }
        Err(error) => {
            crate::console::manager::input::auth::apply_op_picker_commit_failed(editor, &error);
        }
    }
}

fn enter_settings_auth_kind(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    if let Some(row) = settings.auth.pending.get(settings.auth.selected) {
        settings.auth.selected_kind = Some(row.kind);
        settings.auth.selected = 0;
    }
}

fn move_editor_tab(state: &mut ManagerState<'_>, delta: isize, focus_tab_bar: bool) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let was_secrets = editor.active_tab == EditorTab::Secrets;
    editor.active_tab = if delta.is_negative() {
        previous_editor_tab(editor.active_tab)
    } else {
        next_editor_tab(editor.active_tab)
    };
    editor.tab_bar_focused = focus_tab_bar;
    editor.active_field = FieldFocus::Row(0);
    editor.tab_scroll_x = 0;
    editor.tab_scroll_y = 0;
    if editor.active_tab != EditorTab::Auth {
        editor.auth_selected_kind = None;
    }
    if was_secrets {
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
    let candidate = moved_selection(row, max_row.saturating_add(1), delta);
    let next = if delta.is_negative() {
        step_cursor_up(skipped_rows, candidate)
    } else {
        step_cursor_down(skipped_rows, candidate, max_row)
    };
    editor.active_field = FieldFocus::Row(next);
    editor.tab_scroll_y = jackin_console::focus::cursor_scroll_for_panel(
        next,
        editor.tab_scroll_y,
        term.height,
        footer_h,
    );
}

const fn move_settings_tab(state: &mut ManagerState<'_>, delta: isize, focus_tab_bar: bool) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.active_tab = if delta.is_negative() {
        previous_settings_tab(settings.active_tab)
    } else {
        next_settings_tab(settings.active_tab)
    };
    settings.tab_bar_focused = focus_tab_bar;
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
        moved_selection(settings.auth.selected, settings.auth.row_count(), delta);
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
    jackin_tui::components::apply_term_width_scroll_delta(
        &mut editor.tab_scroll_x,
        delta,
        term_width,
        content_width,
    );
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
    jackin_tui::components::apply_term_width_scroll_delta(
        &mut editor.workspace_mounts_scroll_x,
        delta,
        term_width,
        content_width,
    );
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
    jackin_tui::components::apply_term_width_scroll_delta(
        &mut settings.mounts.scroll_x,
        delta,
        term_width,
        content_width,
    );
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
    jackin_tui::components::apply_term_width_scroll_delta(
        &mut settings.trust.scroll_x,
        delta,
        term_width,
        content_width,
    );
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
    let max = settings.mounts.pending.len();
    settings.mounts.selected = if delta.is_negative() {
        settings
            .mounts
            .selected
            .saturating_sub(delta.unsigned_abs())
    } else {
        settings
            .mounts
            .selected
            .saturating_add(delta as usize)
            .min(max)
    };
    settings.mounts.scroll_y = jackin_console::focus::cursor_scroll_for_panel(
        settings.mounts.selected,
        settings.mounts.scroll_y,
        term.height,
        footer_h,
    );
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
    let rows = settings_env_flat_rows(&settings.env.pending, &settings.env.expanded);
    let max = rows.len().saturating_sub(1);
    let candidate = if delta.is_negative() {
        settings.env.selected.saturating_sub(delta.unsigned_abs())
    } else {
        settings
            .env
            .selected
            .saturating_add(delta as usize)
            .min(max)
    };
    settings.env.selected = if delta.is_negative() {
        step_cursor_up_by(candidate, |idx| {
            matches!(rows.get(idx), Some(SettingsEnvRow::SectionSpacer))
        })
    } else {
        step_cursor_down_by(candidate, max, |idx| {
            matches!(rows.get(idx), Some(SettingsEnvRow::SectionSpacer))
        })
    };
    settings.env.scroll_y = jackin_console::focus::cursor_scroll_for_panel(
        settings.env.selected,
        settings.env.scroll_y,
        term.height,
        footer_h,
    );
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
    move_trust_selection(&mut settings.trust, delta);
    settings.trust.scroll_y = jackin_console::focus::cursor_scroll_for_panel(
        settings.trust.selected,
        settings.trust.scroll_y,
        term.height,
        footer_h,
    );
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
    let selected = jackin_console::tui::screens::workspaces::update::moved_selection(
        state.selected,
        state.row_count(),
        delta,
    );
    if selected != state.selected {
        state.reset_list_scroll();
        state.selected = selected;
    }
}

fn select_list_row(state: &mut ManagerState<'_>, selected: usize) {
    state.inline_role_picker = None;
    let selected = jackin_console::tui::screens::workspaces::update::selected_index(
        selected,
        state.row_count(),
    );
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
    let was_secrets = editor.active_tab == EditorTab::Secrets;
    editor.active_tab = tab;
    editor.tab_bar_focused = true;
    editor.active_field = FieldFocus::Row(0);
    editor.workspace_mounts_scroll_focused = false;
    if editor.active_tab != EditorTab::Auth {
        editor.auth_selected_kind = None;
    }
    if was_secrets && editor.active_tab != EditorTab::Secrets {
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

const fn select_settings_tab(state: &mut ManagerState<'_>, tab: SettingsTab) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.active_tab = tab;
    settings.tab_bar_focused = true;
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
    if len == 0 {
        state.preview_focused = false;
        return;
    }
    let cursor = state
        .preview_pane_cursor
        .get(container)
        .copied()
        .unwrap_or(0)
        .min(len - 1);
    let next = moved_selection(cursor, len, delta);
    state.preview_pane_cursor.insert(container.to_owned(), next);
}

const fn scroll_list_horizontal(state: &mut ManagerState<'_>, delta: i16) {
    if state.list_names_focused {
        jackin_tui::components::apply_scroll_delta_unclamped(&mut state.list_names_scroll_x, delta);
    } else {
        scroll_focused_mount_block(state, delta);
    }
}

const fn scroll_focused_mount_block(state: &mut ManagerState<'_>, delta: i16) {
    let Some(focus) = state.list_scroll_focus else {
        return;
    };
    let value = state.list_scroll_x_mut(focus);
    jackin_tui::components::apply_scroll_delta_unclamped(value, delta);
}

const fn scroll_focused_mount_block_vertical(state: &mut ManagerState<'_>, delta: i16) {
    let Some(focus) = state.list_scroll_focus else {
        return;
    };
    let value = state.list_scroll_y_mut(focus);
    jackin_tui::components::apply_scroll_delta_unclamped(value, delta);
}

#[cfg(test)]
mod tests {
    use super::{
        ManagerBackgroundEvent, ManagerEffect, ManagerMessage, execute_manager_effect,
        poll_background_messages, update_manager,
    };
    use crate::console::manager::auth_kind::AuthKind;
    use crate::console::manager::state::{
        AuthFormFocus, AuthFormTarget, CreatePreludeState, DragState, EditorState, EditorTab,
        FieldFocus, ManagerStage, ManagerState, MountScrollFocus, SettingsAuthModal, SettingsState,
        SettingsTab,
    };
    use crate::console::widgets::auth_panel::AuthForm;
    use jackin_tui::components::ErrorPopupState;
    use ratatui::layout::Rect;

    fn state_with_saved_count(count: usize) -> ManagerState<'static> {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path();
        let mut config = crate::config::AppConfig::default();
        for idx in 0..count {
            config.workspaces.insert(
                format!("workspace-{idx}"),
                crate::workspace::WorkspaceConfig {
                    workdir: format!("/tmp/workspace-{idx}"),
                    ..crate::workspace::WorkspaceConfig::default()
                },
            );
        }
        ManagerState::from_config(&config, cwd)
    }

    #[test]
    fn move_list_selection_clamps() {
        let mut state = state_with_saved_count(2);
        state.selected = 1;

        assert!(update_manager(&mut state, ManagerMessage::MoveListSelection(99)).is_dirty());

        assert_eq!(state.selected, state.row_count() - 1);
    }

    #[test]
    fn select_list_row_resets_selection_local_state() {
        let mut state = state_with_saved_count(2);
        state.selected = 0;
        state.list_mounts_scroll_x = 4;

        assert!(update_manager(&mut state, ManagerMessage::SelectListRow(1)).is_dirty());

        assert_eq!(state.selected, 1);
        assert_eq!(state.list_mounts_scroll_x, 0);
    }

    #[test]
    fn mouse_selection_messages_update_tabs_and_rows() {
        let mut state = state_with_saved_count(0);
        let mut editor = EditorState::new_edit(
            "workspace".into(),
            crate::workspace::WorkspaceConfig::default(),
        );
        editor.active_tab = EditorTab::Secrets;
        editor.secrets_expanded.insert("smith".into());
        editor.unmasked_rows.insert((
            crate::console::manager::state::SecretsScopeTag::Workspace,
            "TOKEN".into(),
        ));
        state.stage = ManagerStage::Editor(editor);

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::SelectEditorTab(EditorTab::Mounts)
            )
            .is_dirty()
        );
        assert!(update_manager(&mut state, ManagerMessage::SelectEditorMountRow(2)).is_dirty());

        let ManagerStage::Editor(editor) = &state.stage else {
            panic!("expected editor stage");
        };
        assert_eq!(editor.active_tab, EditorTab::Mounts);
        assert_eq!(editor.active_field, FieldFocus::Row(2));
        assert!(editor.workspace_mounts_scroll_focused);
        assert!(editor.secrets_expanded.is_empty());
        assert!(editor.unmasked_rows.is_empty());

        state.stage = ManagerStage::Settings(SettingsState::from_config(
            &crate::config::AppConfig::default(),
        ));
        assert!(
            update_manager(
                &mut state,
                ManagerMessage::SelectSettingsTab(SettingsTab::Trust)
            )
            .is_dirty()
        );
        assert!(update_manager(&mut state, ManagerMessage::SelectSettingsTrustRow(0)).is_dirty());

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.active_tab, SettingsTab::Trust);
        assert!(settings.trust.scroll_focused);
    }

    #[test]
    fn scroll_focused_list_block_updates_selected_axis() {
        let mut state = state_with_saved_count(1);
        state.list_scroll_focus = Some(MountScrollFocus::Workspace);

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::ScrollFocusedListBlockVertical(3),
            )
            .is_dirty()
        );

        assert_eq!(state.list_mounts_scroll_y, 3);
    }

    #[test]
    fn current_dir_tree_messages_respect_instance_gate() {
        let mut state = state_with_saved_count(1);

        assert!(update_manager(&mut state, ManagerMessage::ExpandSelectedTree).is_dirty());
        assert!(!state.current_dir_expanded);

        state.current_dir_expanded = true;
        assert!(update_manager(&mut state, ManagerMessage::CollapseSelectedTree).is_dirty());
        assert!(!state.current_dir_expanded);
    }

    #[test]
    fn move_editor_tab_resets_tab_local_view_state() {
        let mut state = state_with_saved_count(0);
        let mut editor = EditorState::new_edit(
            "workspace".into(),
            crate::workspace::WorkspaceConfig::default(),
        );
        editor.active_tab = EditorTab::Secrets;
        editor.tab_bar_focused = false;
        editor.active_field = FieldFocus::Row(7);
        editor.tab_scroll_x = 4;
        editor.tab_scroll_y = 5;
        editor.secrets_expanded.insert("role".into());
        state.stage = ManagerStage::Editor(editor);

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::MoveEditorTab {
                    delta: 1,
                    focus_tab_bar: true,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Editor(editor) = state.stage else {
            panic!("expected editor stage");
        };
        assert_eq!(editor.active_tab, EditorTab::Auth);
        assert!(editor.tab_bar_focused);
        assert_eq!(editor.active_field, FieldFocus::Row(0));
        assert_eq!(editor.tab_scroll_x, 0);
        assert_eq!(editor.tab_scroll_y, 0);
        assert!(editor.secrets_expanded.is_empty());
    }

    #[test]
    fn editor_auth_kind_messages_reset_local_view_state() {
        let mut state = state_with_saved_count(0);
        let mut editor = EditorState::new_edit(
            "workspace".into(),
            crate::workspace::WorkspaceConfig::default(),
        );
        editor.active_field = FieldFocus::Row(5);
        editor.tab_scroll_x = 9;
        editor.tab_scroll_y = 7;
        state.stage = ManagerStage::Editor(editor);

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::EnterEditorAuthKind {
                    kind: crate::console::manager::auth_kind::AuthKind::Claude,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Editor(editor) = &state.stage else {
            panic!("expected editor stage");
        };
        assert_eq!(
            editor.auth_selected_kind,
            Some(crate::console::manager::auth_kind::AuthKind::Claude)
        );
        assert_eq!(editor.active_field, FieldFocus::Row(0));
        assert_eq!(editor.tab_scroll_x, 0);
        assert_eq!(editor.tab_scroll_y, 0);

        assert!(update_manager(&mut state, ManagerMessage::ClearEditorAuthKind).is_dirty());

        let ManagerStage::Editor(editor) = state.stage else {
            panic!("expected editor stage");
        };
        assert!(editor.auth_selected_kind.is_none());
        assert_eq!(editor.active_field, FieldFocus::Row(0));
    }

    #[test]
    fn editor_role_header_messages_set_expansion() {
        let mut state = state_with_saved_count(0);
        state.stage = ManagerStage::Editor(EditorState::new_edit(
            "workspace".into(),
            crate::workspace::WorkspaceConfig::default(),
        ));

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::SetEditorSecretsRoleExpanded {
                    role: "smith".into(),
                    expanded: true,
                },
            )
            .is_dirty()
        );
        assert!(
            update_manager(
                &mut state,
                ManagerMessage::SetEditorAuthRoleExpanded {
                    role: "smith".into(),
                    expanded: true,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Editor(editor) = state.stage else {
            panic!("expected editor stage");
        };
        assert!(editor.secrets_expanded.contains("smith"));
        assert!(editor.auth_expanded.contains("smith"));
    }

    #[test]
    fn move_editor_field_selection_skips_rows_and_scrolls() {
        let mut state = state_with_saved_count(0);
        let mut editor = EditorState::new_edit(
            "workspace".into(),
            crate::workspace::WorkspaceConfig::default(),
        );
        editor.active_field = FieldFocus::Row(1);
        state.stage = ManagerStage::Editor(editor);

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::MoveEditorFieldSelection {
                    delta: 1,
                    max_row: 4,
                    skipped_rows: vec![2],
                    term: Rect::new(0, 0, 80, 24),
                    footer_h: 1,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Editor(editor) = state.stage else {
            panic!("expected editor stage");
        };
        assert_eq!(editor.active_field, FieldFocus::Row(3));
    }

    #[test]
    fn editor_toggle_messages_update_selected_content() {
        let mut state = state_with_saved_count(0);
        let mut editor = EditorState::new_edit(
            "workspace".into(),
            crate::workspace::WorkspaceConfig::default(),
        );
        editor.active_field = FieldFocus::Row(2);
        editor.pending.keep_awake.enabled = false;
        editor.pending.mounts.push(crate::workspace::MountConfig {
            src: "/tmp/cache".into(),
            dst: "/home/agent/.cache".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        });
        state.stage = ManagerStage::Editor(editor);

        assert!(update_manager(&mut state, ManagerMessage::ToggleEditorGeneralSelected).is_dirty());

        let ManagerStage::Editor(editor) = &mut state.stage else {
            panic!("expected editor stage");
        };
        assert!(editor.pending.keep_awake.enabled);
        editor.active_field = FieldFocus::Row(0);

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::ToggleEditorMountReadonlySelected
            )
            .is_dirty()
        );
        assert!(
            update_manager(
                &mut state,
                ManagerMessage::ToggleEditorSecretMask {
                    scope: crate::console::manager::state::SecretsScopeTag::Workspace,
                    key: "TOKEN".into(),
                },
            )
            .is_dirty()
        );

        let ManagerStage::Editor(editor) = state.stage else {
            panic!("expected editor stage");
        };
        assert!(editor.pending.mounts[0].readonly);
        assert!(editor.unmasked_rows.contains(&(
            crate::console::manager::state::SecretsScopeTag::Workspace,
            "TOKEN".into()
        )));
    }

    #[test]
    fn move_settings_tab_cycles_and_sets_focus() {
        let mut state = state_with_saved_count(0);
        let mut settings = SettingsState::from_config(&crate::config::AppConfig::default());
        settings.active_tab = SettingsTab::Trust;
        settings.tab_bar_focused = false;
        state.stage = ManagerStage::Settings(settings);

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::MoveSettingsTab {
                    delta: 1,
                    focus_tab_bar: true,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.active_tab, SettingsTab::General);
        assert!(settings.tab_bar_focused);
    }

    #[test]
    fn settings_general_selection_and_toggle_update_state() {
        let mut state = state_with_saved_count(0);
        let mut settings = SettingsState::from_config(&crate::config::AppConfig::default());
        settings.general.pending_dco = false;
        state.stage = ManagerStage::Settings(settings);

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::MoveSettingsGeneralSelection { delta: 1 },
            )
            .is_dirty()
        );
        assert!(
            update_manager(&mut state, ManagerMessage::ToggleSettingsGeneralSelected).is_dirty()
        );

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.general.selected, 1);
        assert!(settings.general.pending_dco);
    }

    #[test]
    fn settings_auth_selection_and_kind_entry_update_state() {
        let mut state = state_with_saved_count(0);
        state.stage = ManagerStage::Settings(SettingsState::from_config(
            &crate::config::AppConfig::default(),
        ));

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::MoveSettingsAuthSelection { delta: 99 },
            )
            .is_dirty()
        );
        assert!(update_manager(&mut state, ManagerMessage::EnterSettingsAuthKind).is_dirty());

        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.auth.selected, 0);
        assert!(settings.auth.selected_kind.is_some());

        assert!(update_manager(&mut state, ManagerMessage::ClearSettingsAuthKind).is_dirty());

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.auth.selected, 0);
        assert!(settings.auth.selected_kind.is_none());
    }

    #[test]
    fn dismiss_settings_error_popup_restores_pending_auth_form() {
        let mut state = state_with_saved_count(0);
        let mut settings = SettingsState::from_config(&crate::config::AppConfig::default());
        settings.error_popup = Some(ErrorPopupState::new("Token mint failed", "op item missing"));
        settings
            .auth
            .modal_parents
            .push(SettingsAuthModal::AuthForm {
                target: AuthFormTarget::Workspace {
                    kind: AuthKind::Claude,
                },
                state: Box::new(AuthForm::new(AuthKind::Claude)),
                focus: AuthFormFocus::Save,
                literal_buffer: "token".into(),
            });
        state.stage = ManagerStage::Settings(settings);

        assert!(update_manager(&mut state, ManagerMessage::DismissSettingsErrorPopup).is_dirty());

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert!(settings.error_popup.is_none());
        assert!(settings.auth.modal_parents.is_empty());
        let Some(SettingsAuthModal::AuthForm {
            target,
            focus,
            literal_buffer,
            ..
        }) = settings.auth.modal
        else {
            panic!("expected auth form to be restored");
        };
        assert_eq!(
            target,
            AuthFormTarget::Workspace {
                kind: AuthKind::Claude
            }
        );
        assert_eq!(focus, AuthFormFocus::Save);
        assert_eq!(literal_buffer, "token");
    }

    #[test]
    fn return_to_list_closes_confirm_stages() {
        let mut state = state_with_saved_count(0);
        state.stage = ManagerStage::ConfirmDelete {
            name: "workspace".into(),
            state: jackin_tui::components::ConfirmState::new("delete?"),
        };

        assert!(update_manager(&mut state, ManagerMessage::ReturnToList).is_dirty());

        assert!(matches!(state.stage, ManagerStage::List));
    }

    #[test]
    fn reload_from_config_preserves_session_cache_and_rebuilds_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path();
        let mut state = state_with_saved_count(0);
        state.op_available = true;
        state.stage = ManagerStage::Settings(SettingsState::from_config(
            &crate::config::AppConfig::default(),
        ));
        let cache = state.op_cache.clone();
        let mut config = crate::config::AppConfig::default();
        config.workspaces.insert(
            "reloaded".into(),
            crate::workspace::WorkspaceConfig {
                workdir: cwd.display().to_string(),
                ..crate::workspace::WorkspaceConfig::default()
            },
        );

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::ReloadFromConfig {
                    config: Box::new(config),
                    cwd: cwd.to_path_buf(),
                },
            )
            .is_dirty()
        );

        assert!(std::rc::Rc::ptr_eq(&state.op_cache, &cache));
        assert!(state.op_available);
        assert!(matches!(state.stage, ManagerStage::List));
        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.workspaces[0].name, "reloaded");
    }

    #[test]
    fn stage_entry_messages_open_requested_stage() {
        let mut state = state_with_saved_count(0);

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::EnterSettings(SettingsState::from_config(
                    &crate::config::AppConfig::default(),
                )),
            )
            .is_dirty()
        );
        assert!(matches!(state.stage, ManagerStage::Settings(_)));

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::EnterEditor(EditorState::new_edit(
                    "workspace".into(),
                    crate::workspace::WorkspaceConfig::default(),
                )),
            )
            .is_dirty()
        );
        assert!(matches!(state.stage, ManagerStage::Editor(_)));

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::EnterCreateEditor {
                    name: "new-workspace".into(),
                    workspace: crate::workspace::WorkspaceConfig::default(),
                },
            )
            .is_dirty()
        );
        let ManagerStage::Editor(editor) = &state.stage else {
            panic!("expected editor stage");
        };
        assert_eq!(editor.pending_name.as_deref(), Some("new-workspace"));

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::EnterCreatePrelude(CreatePreludeState::new()),
            )
            .is_dirty()
        );
        assert!(matches!(state.stage, ManagerStage::CreatePrelude(_)));

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::EnterConfirmDelete {
                    name: "workspace".into(),
                },
            )
            .is_dirty()
        );
        assert!(matches!(state.stage, ManagerStage::ConfirmDelete { .. }));

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::EnterConfirmInstancePurge {
                    container: "jk-test".into(),
                    label: "jk-test (rust)".into(),
                },
            )
            .is_dirty()
        );
        assert!(matches!(
            state.stage,
            ManagerStage::ConfirmInstancePurge { .. }
        ));
    }

    #[test]
    fn scroll_editor_tab_marks_panel_focus_and_updates_offset() {
        let mut state = state_with_saved_count(0);
        state.stage = ManagerStage::Editor(EditorState::new_edit(
            "workspace".into(),
            crate::workspace::WorkspaceConfig::default(),
        ));

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::ScrollEditorTabHorizontal {
                    delta: 8,
                    term_width: 10,
                    content_width: 40,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Editor(editor) = state.stage else {
            panic!("expected editor stage");
        };
        assert!(editor.tab_content_scroll_focused);
        assert_eq!(editor.tab_scroll_x, 8);
    }

    #[test]
    fn scroll_editor_workspace_mounts_marks_mounts_focus_and_updates_offset() {
        let mut state = state_with_saved_count(0);
        state.stage = ManagerStage::Editor(EditorState::new_edit(
            "workspace".into(),
            crate::workspace::WorkspaceConfig::default(),
        ));

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::ScrollEditorWorkspaceMountsHorizontal {
                    delta: 8,
                    term_width: 10,
                    content_width: 40,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Editor(editor) = state.stage else {
            panic!("expected editor stage");
        };
        assert!(editor.workspace_mounts_scroll_focused);
        assert_eq!(editor.workspace_mounts_scroll_x, 8);
    }

    #[test]
    fn scroll_settings_global_mounts_updates_offset() {
        let mut state = state_with_saved_count(0);
        state.stage = ManagerStage::Settings(SettingsState::from_config(
            &crate::config::AppConfig::default(),
        ));

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::ScrollSettingsGlobalMountsHorizontal {
                    delta: 8,
                    term_width: 10,
                    content_width: 40,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.mounts.scroll_x, 8);
    }

    #[test]
    fn move_settings_global_mounts_selection_clamps_to_add_row() {
        let mut state = state_with_saved_count(0);
        let mut settings = SettingsState::from_config(&crate::config::AppConfig::default());
        settings.mounts.pending.push(crate::config::GlobalMountRow {
            scope: None,
            name: "cache".into(),
            mount: crate::workspace::MountConfig {
                src: "/tmp/cache".into(),
                dst: "/home/agent/.cache".into(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            },
        });
        state.stage = ManagerStage::Settings(settings);

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::MoveSettingsGlobalMountsSelection {
                    delta: 99,
                    term: Rect::new(0, 0, 80, 24),
                    footer_h: 1,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.mounts.selected, settings.mounts.pending.len());
    }

    #[test]
    fn move_settings_env_selection_skips_section_spacers() {
        let mut state = state_with_saved_count(0);
        let mut settings = SettingsState::from_config(&crate::config::AppConfig::default());
        settings.env.pending.env.insert(
            "ALPHA".into(),
            crate::operator_env::EnvValue::Plain("one".into()),
        );
        settings.env.pending.env.insert(
            "BETA".into(),
            crate::operator_env::EnvValue::Plain("two".into()),
        );
        settings.env.selected = 1;
        state.stage = ManagerStage::Settings(settings);

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::MoveSettingsEnvSelection {
                    delta: 1,
                    term: Rect::new(0, 0, 80, 24),
                    footer_h: 1,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.env.selected, 3);
    }

    #[test]
    fn settings_env_role_header_message_sets_expansion() {
        let mut state = state_with_saved_count(0);
        state.stage = ManagerStage::Settings(SettingsState::from_config(
            &crate::config::AppConfig::default(),
        ));

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::SetSettingsEnvRoleExpanded {
                    role: "smith".into(),
                    expanded: true,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert!(settings.env.expanded.contains("smith"));
    }

    #[test]
    fn settings_mount_and_trust_toggle_messages_update_selected_rows() {
        let mut state = state_with_saved_count(0);
        let mut config = crate::config::AppConfig::default();
        config.roles.insert(
            "chainargos/agent-smith".into(),
            crate::config::RoleSource {
                git: "https://github.com/chainargos/agent-smith".into(),
                trusted: false,
                ..crate::config::RoleSource::default()
            },
        );
        let mut settings = SettingsState::from_config(&config);
        settings.mounts.pending.push(crate::config::GlobalMountRow {
            scope: None,
            name: "cache".into(),
            mount: crate::workspace::MountConfig {
                src: "/tmp/cache".into(),
                dst: "/home/agent/.cache".into(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            },
        });
        state.stage = ManagerStage::Settings(settings);

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::ToggleSettingsGlobalMountReadonly
            )
            .is_dirty()
        );
        assert!(update_manager(&mut state, ManagerMessage::ToggleSettingsTrustSelected).is_dirty());

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert!(settings.mounts.pending[0].mount.readonly);
        assert!(settings.trust.pending[0].trusted);
    }

    #[test]
    fn scroll_settings_trust_updates_offset() {
        let mut state = state_with_saved_count(0);
        let mut config = crate::config::AppConfig::default();
        config.roles.insert(
            "chainargos/agent-smith".into(),
            crate::config::RoleSource {
                git: "https://github.com/chainargos/agent-smith".into(),
                trusted: true,
                ..crate::config::RoleSource::default()
            },
        );
        state.stage = ManagerStage::Settings(SettingsState::from_config(&config));

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::ScrollSettingsTrustHorizontal {
                    delta: 8,
                    term_width: 10,
                    content_width: 40,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.trust.scroll_x, 8);
    }

    #[test]
    fn move_settings_trust_selection_clamps_to_role_rows() {
        let mut state = state_with_saved_count(0);
        let mut config = crate::config::AppConfig::default();
        config.roles.insert(
            "chainargos/agent-a".into(),
            crate::config::RoleSource {
                git: "https://github.com/chainargos/agent-a".into(),
                trusted: false,
                ..crate::config::RoleSource::default()
            },
        );
        config.roles.insert(
            "chainargos/agent-b".into(),
            crate::config::RoleSource {
                git: "https://github.com/chainargos/agent-b".into(),
                trusted: true,
                ..crate::config::RoleSource::default()
            },
        );
        state.stage = ManagerStage::Settings(SettingsState::from_config(&config));

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::MoveSettingsTrustSelection {
                    delta: 99,
                    term: Rect::new(0, 0, 80, 24),
                    footer_h: 1,
                },
            )
            .is_dirty()
        );

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.trust.selected, settings.trust.pending.len() - 1);
    }

    #[test]
    fn set_list_scroll_focus_stores_focus() {
        let cwd = std::path::Path::new("/");
        let config = crate::config::AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);
        assert!(state.list_scroll_focus.is_none());

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::SetListScrollFocus(Some(MountScrollFocus::Workspace))
            )
            .is_dirty()
        );
        assert_eq!(state.list_scroll_focus, Some(MountScrollFocus::Workspace));

        assert!(update_manager(&mut state, ManagerMessage::SetListScrollFocus(None)).is_dirty());
        assert!(state.list_scroll_focus.is_none());
    }

    #[test]
    fn set_list_names_focused_stores_flag() {
        let cwd = std::path::Path::new("/");
        let config = crate::config::AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);

        assert!(update_manager(&mut state, ManagerMessage::SetListNamesFocused(true)).is_dirty());
        assert!(state.list_names_focused);
        assert!(update_manager(&mut state, ManagerMessage::SetListNamesFocused(false)).is_dirty());
        assert!(!state.list_names_focused);
    }

    #[test]
    fn set_drag_state_stores_and_clears() {
        let cwd = std::path::Path::new("/");
        let config = crate::config::AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);
        assert!(state.drag_state.is_none());

        let drag = DragState {
            anchor_pct: 50,
            anchor_x: 40,
        };
        assert!(update_manager(&mut state, ManagerMessage::SetDragState(Some(drag))).is_dirty());
        assert!(state.drag_state.is_some());
        assert!(update_manager(&mut state, ManagerMessage::SetDragState(None)).is_dirty());
        assert!(state.drag_state.is_none());
    }

    #[test]
    fn set_list_split_pct_stores_value() {
        let cwd = std::path::Path::new("/");
        let config = crate::config::AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);
        let original = state.list_split_pct;

        assert!(update_manager(&mut state, ManagerMessage::SetListSplitPct(75)).is_dirty());
        assert_eq!(state.list_split_pct, 75);
        assert_ne!(state.list_split_pct, original);
    }

    #[test]
    fn open_list_error_popup_sets_error_modal() {
        let cwd = std::path::Path::new("/");
        let config = crate::config::AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);
        assert!(state.list_modal.is_none());

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::OpenListErrorPopup {
                    title: "Test error".into(),
                    message: "Something went wrong.".into(),
                }
            )
            .is_dirty()
        );
        assert!(matches!(
            state.list_modal,
            Some(super::super::state::Modal::ErrorPopup { .. })
        ));
    }

    #[test]
    fn status_popup_messages_open_and_dismiss_overlay() {
        let cwd = std::path::Path::new("/");
        let config = crate::config::AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);
        assert!(state.status_overlay.is_none());

        assert!(
            update_manager(
                &mut state,
                ManagerMessage::OpenStatusPopup {
                    title: "Stopping".into(),
                    message: "Stopping capsule-a...".into(),
                }
            )
            .is_dirty()
        );
        assert!(state.status_overlay.is_some());

        assert!(update_manager(&mut state, ManagerMessage::DismissStatusPopup).is_dirty());
        assert!(state.status_overlay.is_none());
    }

    #[tokio::test]
    async fn poll_background_messages_routes_file_browser_poll_through_message() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = crate::paths::JackinPaths::for_tests(tmp.path());
        let cwd = tmp.path();
        let mut config = crate::config::AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);

        let events = poll_background_messages(&mut state, &mut config, &paths);

        assert!(events.iter().any(|event| matches!(
            event,
            ManagerBackgroundEvent::Message(ManagerMessage::PollFileBrowserGitUrls)
        )));
    }

    #[tokio::test]
    async fn execute_manager_effect_requests_instance_refresh() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = crate::paths::JackinPaths::for_tests(tmp.path());
        let cwd = tmp.path();
        let mut config = crate::config::AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);

        execute_manager_effect(
            &mut state,
            &mut config,
            &paths,
            ManagerEffect::RequestInstanceRefresh,
        );

        assert!(
            state.instance_refresh_in_flight(),
            "instance refresh effect should spawn a worker"
        );
    }

    #[test]
    fn dismiss_list_modal_clears_modal() {
        let cwd = std::path::Path::new("/");
        let config = crate::config::AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);
        let _ = update_manager(
            &mut state,
            ManagerMessage::OpenListErrorPopup {
                title: "x".into(),
                message: "y".into(),
            },
        );
        assert!(state.list_modal.is_some());

        assert!(update_manager(&mut state, ManagerMessage::DismissListModal).is_dirty());
        assert!(state.list_modal.is_none());
    }
}
