// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! List and tab selection updates.

use ratatui::layout::Rect;

use super::super::{EditorTab, ManagerStage, ManagerState, SettingsTab};
use crate::tui::screens::editor::update::{editor_mount_row_select_plan, editor_tab_select_plan};
use crate::tui::screens::settings::update::{
    settings_env_selection_plan, settings_global_mounts_selection_plan, settings_tab_select_plan,
    settings_trust_row_select_plan, settings_trust_selection_plan,
};
use crate::tui::screens::workspaces::update::{
    apply_preview_pane_cursor_plan, apply_workspace_list_horizontal_scroll_plan,
    apply_workspace_list_selection_plan, apply_workspace_list_vertical_scroll_plan,
    apply_workspace_tree_disclosure_plan, collapse_selected_tree_plan, expand_selected_tree_plan,
    preview_pane_cursor_plan, workspace_list_horizontal_scroll_target_plan,
    workspace_list_move_selection_plan, workspace_list_select_row_plan,
    workspace_list_vertical_scroll_target_plan,
};
use crate::tui::update::{
    InlinePickerDismissal, apply_inline_picker_dismissal_plan, inline_picker_dismissal_plan,
};

pub(crate) fn move_settings_global_mounts_selection(
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
        settings.mounts.scroll.offset_y(),
        term.height,
        footer_h,
    );
    settings.mounts.apply_selection_plan(plan);
}

pub(crate) fn move_settings_env_selection(
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
        settings.env.scroll.offset_y(),
        term.height,
        footer_h,
    );
    settings.env.apply_selection_plan(plan);
}

pub(crate) fn move_settings_trust_selection(
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
        settings.trust.scroll.offset_y(),
        term.height,
        footer_h,
    );
    settings.trust.apply_selection_plan(plan);
}

pub(crate) fn collapse_selected_tree(state: &mut ManagerState<'_>) {
    apply_inline_picker_dismissal_plan(
        state,
        inline_picker_dismissal_plan(InlinePickerDismissal::NewSession),
    );
    apply_workspace_tree_disclosure_plan(state, collapse_selected_tree_plan(state.selected_row()));
}

pub(crate) fn expand_selected_tree(state: &mut ManagerState<'_>) {
    apply_inline_picker_dismissal_plan(
        state,
        inline_picker_dismissal_plan(InlinePickerDismissal::NewSession),
    );
    apply_workspace_tree_disclosure_plan(state, expand_selected_tree_plan(state.selected_row()));
}

pub(crate) fn move_list_selection(state: &mut ManagerState<'_>, delta: isize) {
    let plan = workspace_list_move_selection_plan(state.selected, state.row_count(), delta);
    apply_workspace_list_selection_plan(state, plan);
}

pub(crate) fn select_list_row(state: &mut ManagerState<'_>, selected: usize) {
    let plan = workspace_list_select_row_plan(state.selected, selected, state.row_count());
    apply_workspace_list_selection_plan(state, plan);
}

pub(crate) fn select_editor_tab(state: &mut ManagerState<'_>, tab: EditorTab) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let plan = editor_tab_select_plan(editor.active_tab, tab);
    editor.apply_tab_select_plan(plan);
}

pub(crate) fn select_editor_mount_row(state: &mut ManagerState<'_>, row: usize) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let plan = editor_mount_row_select_plan(row);
    editor.apply_mount_row_select_plan(plan);
}

pub(crate) fn select_settings_tab(state: &mut ManagerState<'_>, tab: SettingsTab) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let plan = settings_tab_select_plan(tab);
    settings.apply_tab_move_plan(plan);
}

pub(crate) fn select_settings_trust_row(state: &mut ManagerState<'_>, row: usize) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let plan = settings_trust_row_select_plan(row, settings.trust.pending.len());
    settings.apply_trust_row_select_plan(plan);
}

pub(crate) fn move_preview_pane(state: &mut ManagerState<'_>, container: &str, delta: isize) {
    let len = state.flattened_preview_panes(container).len();
    let plan = preview_pane_cursor_plan(
        len,
        state.preview_pane_cursor.get(container).copied(),
        delta,
    );
    apply_preview_pane_cursor_plan(state, container, plan);
}

pub(crate) fn scroll_list_horizontal(state: &mut ManagerState<'_>, delta: i16) {
    let plan = workspace_list_horizontal_scroll_target_plan(
        state.list_names_focused(),
        state.list_scroll_focus(),
    );
    apply_workspace_list_horizontal_scroll_plan(state, plan, delta);
}

pub(crate) fn scroll_focused_mount_block_vertical(state: &mut ManagerState<'_>, delta: i16) {
    let plan = workspace_list_vertical_scroll_target_plan(state.list_scroll_focus());
    apply_workspace_list_vertical_scroll_plan(state, plan, delta);
}
