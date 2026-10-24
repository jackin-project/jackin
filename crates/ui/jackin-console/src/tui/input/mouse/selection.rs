// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    EditorTab, ManagerMessage, ManagerStage, ManagerState, MouseEvent, Rect, SettingsTab,
    dispatch_manager, editor_auth_row_index_at_position, editor_mount_index_at_position,
    editor_scroll_area, settings_trust_row_at_position,
};

pub fn try_select_editor_tab(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&jackin_config::AppConfig>,
) -> bool {
    let ManagerStage::Editor(editor) = &state.stage else {
        return false;
    };
    if editor.modal.is_some() {
        return false;
    }

    let regions = super::editor_tab_hit_regions(editor, term_size, config, state.op_available);
    let Some(tab_id) = super::tab_hit_id(&regions, mouse) else {
        return false;
    };
    let Some(tab) = EditorTab::ALL.get(tab_id).copied() else {
        return false;
    };

    dispatch_manager(state, ManagerMessage::SelectEditorTab(tab));
    true
}

pub fn try_select_settings_tab(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let ManagerStage::Settings(settings) = &state.stage else {
        return false;
    };
    if settings.mounts.modals.is_open() || settings.env.modals.is_open() {
        return false;
    }

    let regions = super::settings_tab_hit_regions(settings, term_size, state.op_available);
    let Some(tab_id) = super::tab_hit_id(&regions, mouse) else {
        return false;
    };
    let Some(tab) = SettingsTab::ALL.get(tab_id).copied() else {
        return false;
    };
    dispatch_manager(state, ManagerMessage::SelectSettingsTab(tab));
    true
}

pub fn try_select_settings_trust_row(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let ManagerStage::Settings(settings) = &state.stage else {
        return false;
    };
    if settings.active_tab != SettingsTab::Trust
        || settings.mounts.modals.is_open()
        || settings.env.modals.is_open()
        || settings.auth.modals.is_open()
        || settings.error_popup.is_some()
    {
        return false;
    }
    let area = settings.content_area(term_size);
    if !crate::tui::layout::point_in_rect(mouse.column, mouse.row, area) {
        return false;
    }
    if let Some(row) = settings_trust_row_at_position(
        area,
        mouse.column,
        mouse.row,
        settings.trust.scroll.offset_y(),
        settings.trust.pending.len(),
    ) {
        dispatch_manager(state, ManagerMessage::SelectSettingsTrustRow(row));
    } else {
        dispatch_manager(state, ManagerMessage::SelectSettingsTrustRow(usize::MAX));
    }
    true
}

pub fn editor_mount_index_at(
    editor: &crate::tui::state::EditorState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> Option<usize> {
    editor_mount_index_at_position(
        editor.active_tab,
        editor.modal.is_some(),
        editor_scroll_area(editor, term_size).area,
        mouse.column,
        mouse.row,
        editor.tab_scroll.offset_y(),
        editor.pending.mounts.as_slice(),
    )
}

pub fn try_select_editor_mount_row(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
) -> bool {
    let ManagerStage::Editor(editor) = &state.stage else {
        return false;
    };
    let Some(index) = editor_mount_index_at(editor, mouse, term_size) else {
        return false;
    };
    dispatch_manager(state, ManagerMessage::SelectEditorMountRow(index));
    true
}

pub fn try_select_editor_auth_row(
    state: &mut ManagerState<'_>,
    mouse: MouseEvent,
    term_size: Rect,
    config: Option<&jackin_config::AppConfig>,
) -> bool {
    let Some(config) = config else {
        return false;
    };
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return false;
    };
    let Some(index) = editor_auth_row_index_at(editor, config, mouse, term_size) else {
        return false;
    };
    editor.select_auth_row(index);
    true
}

pub fn editor_auth_row_index_at(
    editor: &crate::tui::state::EditorState<'_>,
    config: &jackin_config::AppConfig,
    mouse: MouseEvent,
    term_size: Rect,
) -> Option<usize> {
    let rows = editor.auth_flat_rows(config);
    editor_auth_row_index_at_position(
        editor.active_tab,
        editor.modal.is_some(),
        editor.content_area(term_size),
        mouse.column,
        mouse.row,
        editor.tab_scroll.offset_y(),
        &rows,
    )
}
