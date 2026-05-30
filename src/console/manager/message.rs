//! Workspace-manager message/update boundary.
//!
//! This starts the Model/Update/View migration with state-only list messages.
//! Input handlers should increasingly translate terminal events into these
//! messages instead of mutating `ManagerState` inline.

use super::auth_kind::AuthKind;
use super::render::global_mounts::{SettingsEnvRow, settings_env_flat_rows};
use super::state::{
    EditorTab, FieldFocus, ManagerListRow, ManagerStage, ManagerState, SecretsScopeTag,
    SettingsTab,
};
use jackin_tui::runtime::Dirty;
use ratatui::layout::Rect;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagerMessage {
    CollapseSelectedTree,
    ClearEditorAuthKind,
    EnterPreview,
    EnterEditorAuthKind {
        kind: AuthKind,
    },
    FocusEditorContent,
    FocusEditorTabBar,
    FocusSettingsContent,
    FocusSettingsTabBar,
    ExitPreview,
    ExpandSelectedTree,
    ClearSettingsAuthKind,
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
    ScrollListHorizontal(i16),
    ScrollFocusedListBlockVertical(i16),
}

pub fn update_manager(state: &mut ManagerState<'_>, message: ManagerMessage) -> Dirty {
    match message {
        ManagerMessage::CollapseSelectedTree => collapse_selected_tree(state),
        ManagerMessage::ClearEditorAuthKind => clear_editor_auth_kind(state),
        ManagerMessage::EnterPreview => state.preview_focused = true,
        ManagerMessage::EnterEditorAuthKind { kind } => enter_editor_auth_kind(state, kind),
        ManagerMessage::FocusEditorContent => set_editor_tab_bar_focus(state, false),
        ManagerMessage::FocusEditorTabBar => set_editor_tab_bar_focus(state, true),
        ManagerMessage::FocusSettingsContent => set_settings_tab_bar_focus(state, false),
        ManagerMessage::FocusSettingsTabBar => set_settings_tab_bar_focus(state, true),
        ManagerMessage::ExitPreview => state.preview_focused = false,
        ManagerMessage::ExpandSelectedTree => expand_selected_tree(state),
        ManagerMessage::ClearSettingsAuthKind => clear_settings_auth_kind(state),
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
        ManagerMessage::ScrollListHorizontal(delta) => scroll_list_horizontal(state, delta),
        ManagerMessage::ScrollFocusedListBlockVertical(delta) => {
            scroll_focused_mount_block_vertical(state, delta);
        }
    }
    Dirty::Redraw
}

fn set_editor_tab_bar_focus(state: &mut ManagerState<'_>, focused: bool) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.tab_bar_focused = focused;
}

fn set_settings_tab_bar_focus(state: &mut ManagerState<'_>, focused: bool) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.tab_bar_focused = focused;
}

fn clear_editor_auth_kind(state: &mut ManagerState<'_>) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.auth_selected_kind = None;
    editor.active_field = FieldFocus::Row(0);
    editor.tab_scroll_x = 0;
    editor.tab_scroll_y = 0;
}

fn enter_editor_auth_kind(state: &mut ManagerState<'_>, kind: AuthKind) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.auth_selected_kind = Some(kind);
    editor.active_field = FieldFocus::Row(0);
    editor.tab_scroll_x = 0;
    editor.tab_scroll_y = 0;
}

fn clear_settings_auth_kind(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.auth.selected_kind = None;
    settings.auth.selected = 0;
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
    let candidate = if delta.is_negative() {
        row.saturating_sub(delta.unsigned_abs())
    } else {
        row.saturating_add(delta as usize).min(max_row)
    };
    let next = if delta.is_negative() {
        step_cursor_up(skipped_rows, candidate)
    } else {
        step_cursor_down(skipped_rows, candidate, max_row)
    };
    editor.active_field = FieldFocus::Row(next);
    editor.tab_scroll_y =
        super::render::cursor_scroll_for_panel(next, editor.tab_scroll_y, term, footer_h);
}

fn step_cursor_down(skipped_rows: &[usize], candidate: usize, max_row: usize) -> usize {
    let mut idx = candidate;
    while idx <= max_row {
        if skipped_rows.contains(&idx) {
            idx += 1;
        } else {
            return idx;
        }
    }
    candidate
}

fn step_cursor_up(skipped_rows: &[usize], candidate: usize) -> usize {
    let mut idx = candidate;
    loop {
        if skipped_rows.contains(&idx) {
            if idx == 0 {
                return 0;
            }
            idx -= 1;
        } else {
            return idx;
        }
    }
}

fn move_settings_tab(state: &mut ManagerState<'_>, delta: isize, focus_tab_bar: bool) {
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
    settings.general.selected = if delta.is_negative() {
        settings.general.selected.saturating_sub(delta.unsigned_abs())
    } else {
        settings.general.selected.saturating_add(delta as usize).min(1)
    };
}

fn toggle_settings_general_selected(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    match settings.general.selected {
        0 => {
            settings.general.pending_coauthor_trailer =
                !settings.general.pending_coauthor_trailer;
        }
        1 => {
            settings.general.pending_dco = !settings.general.pending_dco;
        }
        _ => {}
    }
}

fn set_editor_auth_role_expanded(state: &mut ManagerState<'_>, role: String, expanded: bool) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    if expanded {
        editor.auth_expanded.insert(role);
    } else {
        editor.auth_expanded.remove(&role);
    }
}

fn set_editor_secrets_role_expanded(state: &mut ManagerState<'_>, role: String, expanded: bool) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    if expanded {
        editor.secrets_expanded.insert(role);
    } else {
        editor.secrets_expanded.remove(&role);
    }
}

fn toggle_editor_general_selected(state: &mut ManagerState<'_>) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let FieldFocus::Row(row) = editor.active_field;
    match row {
        2 => editor.pending.keep_awake.enabled = !editor.pending.keep_awake.enabled,
        3 => editor.pending.git_pull_on_entry = !editor.pending.git_pull_on_entry,
        _ => {}
    }
}

fn toggle_editor_mount_readonly_selected(state: &mut ManagerState<'_>) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let FieldFocus::Row(row) = editor.active_field;
    if let Some(mount) = editor.pending.mounts.get_mut(row) {
        mount.readonly = !mount.readonly;
    }
}

fn toggle_editor_secret_mask(state: &mut ManagerState<'_>, scope: SecretsScopeTag, key: String) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let entry = (scope, key);
    if !editor.unmasked_rows.remove(&entry) {
        editor.unmasked_rows.insert(entry);
    }
}

fn set_settings_env_role_expanded(state: &mut ManagerState<'_>, role: String, expanded: bool) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    if expanded {
        settings.env.expanded.insert(role);
    } else {
        settings.env.expanded.remove(&role);
    }
}

fn toggle_settings_global_mount_readonly(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    if let Some(row) = settings.mounts.pending.get_mut(settings.mounts.selected) {
        row.mount.readonly = !row.mount.readonly;
    }
}

fn toggle_settings_trust_selected(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    if let Some(row) = settings.trust.pending.get_mut(settings.trust.selected) {
        row.trusted = !row.trusted;
    }
}

fn move_settings_auth_selection(state: &mut ManagerState<'_>, delta: isize) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let max = settings.auth.row_count().saturating_sub(1);
    settings.auth.selected = if delta.is_negative() {
        settings.auth.selected.saturating_sub(delta.unsigned_abs())
    } else {
        settings.auth.selected.saturating_add(delta as usize).min(max)
    };
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
    settings.mounts.scroll_y = super::render::cursor_scroll_for_panel(
        settings.mounts.selected,
        settings.mounts.scroll_y,
        term,
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
    let rows = settings_env_flat_rows(settings);
    let max = rows.len().saturating_sub(1);
    let candidate = if delta.is_negative() {
        settings.env.selected.saturating_sub(delta.unsigned_abs())
    } else {
        settings.env.selected.saturating_add(delta as usize).min(max)
    };
    settings.env.selected = if delta.is_negative() {
        step_settings_env_cursor_up(&rows, candidate)
    } else {
        step_settings_env_cursor_down(&rows, candidate, max)
    };
    settings.env.scroll_y = super::render::cursor_scroll_for_panel(
        settings.env.selected,
        settings.env.scroll_y,
        term,
        footer_h,
    );
}

fn step_settings_env_cursor_down(rows: &[SettingsEnvRow], candidate: usize, max: usize) -> usize {
    let mut idx = candidate;
    while idx <= max {
        match rows.get(idx) {
            Some(SettingsEnvRow::SectionSpacer) => idx += 1,
            _ => return idx,
        }
    }
    candidate
}

fn step_settings_env_cursor_up(rows: &[SettingsEnvRow], candidate: usize) -> usize {
    let mut idx = candidate;
    loop {
        match rows.get(idx) {
            Some(SettingsEnvRow::SectionSpacer) => {
                if idx == 0 {
                    return 0;
                }
                idx -= 1;
            }
            _ => return idx,
        }
    }
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
    let max = settings.trust.pending.len().saturating_sub(1);
    settings.trust.selected = if delta.is_negative() {
        settings
            .trust
            .selected
            .saturating_sub(delta.unsigned_abs())
    } else {
        settings
            .trust
            .selected
            .saturating_add(delta as usize)
            .min(max)
    };
    settings.trust.scroll_y = super::render::cursor_scroll_for_panel(
        settings.trust.selected,
        settings.trust.scroll_y,
        term,
        footer_h,
    );
}

const fn previous_editor_tab(tab: EditorTab) -> EditorTab {
    match tab {
        EditorTab::General => EditorTab::Auth,
        EditorTab::Mounts => EditorTab::General,
        EditorTab::Roles => EditorTab::Mounts,
        EditorTab::Secrets => EditorTab::Roles,
        EditorTab::Auth => EditorTab::Secrets,
    }
}

const fn next_editor_tab(tab: EditorTab) -> EditorTab {
    match tab {
        EditorTab::General => EditorTab::Mounts,
        EditorTab::Mounts => EditorTab::Roles,
        EditorTab::Roles => EditorTab::Secrets,
        EditorTab::Secrets => EditorTab::Auth,
        EditorTab::Auth => EditorTab::General,
    }
}

const fn previous_settings_tab(tab: SettingsTab) -> SettingsTab {
    match tab {
        SettingsTab::General => SettingsTab::Trust,
        SettingsTab::Mounts => SettingsTab::General,
        SettingsTab::Environments => SettingsTab::Mounts,
        SettingsTab::Auth => SettingsTab::Environments,
        SettingsTab::Trust => SettingsTab::Auth,
    }
}

const fn next_settings_tab(tab: SettingsTab) -> SettingsTab {
    match tab {
        SettingsTab::General => SettingsTab::Mounts,
        SettingsTab::Mounts => SettingsTab::Environments,
        SettingsTab::Environments => SettingsTab::Auth,
        SettingsTab::Auth => SettingsTab::Trust,
        SettingsTab::Trust => SettingsTab::General,
    }
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
    let last = state.row_count().saturating_sub(1);
    let selected = if delta.is_negative() {
        state.selected.saturating_sub(delta.unsigned_abs())
    } else {
        state.selected.saturating_add(delta as usize).min(last)
    };
    if selected != state.selected {
        state.reset_list_scroll();
        state.selected = selected;
    }
}

fn select_list_row(state: &mut ManagerState<'_>, selected: usize) {
    state.inline_role_picker = None;
    let selected = selected.min(state.row_count().saturating_sub(1));
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

fn select_editor_mount_row(state: &mut ManagerState<'_>, row: usize) {
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
    settings.active_tab = tab;
}

fn select_settings_trust_row(state: &mut ManagerState<'_>, row: usize) {
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
    let next = if delta.is_negative() {
        cursor.saturating_sub(delta.unsigned_abs())
    } else {
        cursor.saturating_add(delta as usize).min(len - 1)
    };
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
    use super::{ManagerMessage, update_manager};
    use crate::console::manager::state::{
        EditorState, EditorTab, FieldFocus, ManagerStage, ManagerState, MountScrollFocus,
        SettingsState, SettingsTab,
    };

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

        assert!(update_manager(&mut state, ManagerMessage::SelectEditorTab(EditorTab::Mounts)).is_dirty());
        assert!(update_manager(&mut state, ManagerMessage::SelectEditorMountRow(2)).is_dirty());

        let ManagerStage::Editor(editor) = &state.stage else {
            panic!("expected editor stage");
        };
        assert_eq!(editor.active_tab, EditorTab::Mounts);
        assert_eq!(editor.active_field, FieldFocus::Row(2));
        assert!(editor.workspace_mounts_scroll_focused);
        assert!(editor.secrets_expanded.is_empty());
        assert!(editor.unmasked_rows.is_empty());

        state.stage =
            ManagerStage::Settings(SettingsState::from_config(&crate::config::AppConfig::default()));
        assert!(
            update_manager(&mut state, ManagerMessage::SelectSettingsTab(SettingsTab::Trust))
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
    fn expand_and_collapse_selected_tree_updates_current_dir() {
        let mut state = state_with_saved_count(1);

        assert!(update_manager(&mut state, ManagerMessage::ExpandSelectedTree).is_dirty());
        assert!(state.current_dir_expanded);

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
            update_manager(&mut state, ManagerMessage::ToggleEditorMountReadonlySelected)
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
        assert!(
            editor
                .unmasked_rows
                .contains(&(crate::console::manager::state::SecretsScopeTag::Workspace, "TOKEN".into()))
        );
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
        assert!(update_manager(&mut state, ManagerMessage::ToggleSettingsGeneralSelected).is_dirty());

        let ManagerStage::Settings(settings) = state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.general.selected, 1);
        assert!(settings.general.pending_dco);
    }

    #[test]
    fn settings_auth_selection_and_kind_entry_update_state() {
        let mut state = state_with_saved_count(0);
        state.stage =
            ManagerStage::Settings(SettingsState::from_config(&crate::config::AppConfig::default()));

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
        state.stage =
            ManagerStage::Settings(SettingsState::from_config(&crate::config::AppConfig::default()));

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
        settings
            .env
            .pending
            .env
            .insert("ALPHA".into(), crate::operator_env::EnvValue::Plain("one".into()));
        settings
            .env
            .pending
            .env
            .insert("BETA".into(), crate::operator_env::EnvValue::Plain("two".into()));
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
        state.stage =
            ManagerStage::Settings(SettingsState::from_config(&crate::config::AppConfig::default()));

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
            update_manager(&mut state, ManagerMessage::ToggleSettingsGlobalMountReadonly)
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
}
