// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Screen focus and entry transitions.

use crate::tui::model::apply_manager_stage;
use crate::tui::screens::editor::update::{
    editor_field_selection_plan, editor_tab_bar_focus_plan, editor_tab_move_plan,
};
use crate::tui::screens::settings::update::settings_tab_bar_focus_plan;
use crate::tui::screens::workspaces::update::{
    instance_purge_confirm_plan, workspace_delete_confirm_plan,
};
use ratatui::layout::Rect;

use super::super::{EditorState, FieldFocus, ManagerStage, ManagerState};

pub(crate) fn set_editor_tab_bar_focus(state: &mut ManagerState<'_>, focused: bool) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.apply_tab_bar_focus_plan(editor_tab_bar_focus_plan(focused));
}

pub(crate) fn set_settings_tab_bar_focus(state: &mut ManagerState<'_>, focused: bool) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.apply_tab_bar_focus_plan(settings_tab_bar_focus_plan(focused));
}

pub(crate) fn enter_confirm_delete(state: &mut ManagerState<'_>, name: String) {
    let plan = workspace_delete_confirm_plan(name);
    apply_manager_stage(
        state,
        ManagerStage::ConfirmDelete {
            state: plan.state,
            name: plan.name,
        },
    );
}

pub(crate) fn enter_confirm_instance_purge(
    state: &mut ManagerState<'_>,
    container: String,
    label: String,
) {
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

pub(crate) fn enter_create_editor(
    state: &mut ManagerState<'_>,
    name: String,
    workspace: jackin_config::WorkspaceConfig,
) {
    let editor = EditorState::new_create_with_workspace(name, workspace);
    apply_manager_stage(state, ManagerStage::Editor(editor));
}

pub(crate) fn reload_from_config(
    state: &mut ManagerState<'_>,
    config: &jackin_config::AppConfig,
    cwd: &std::path::Path,
) {
    let cache = std::rc::Rc::clone(&state.op_cache);
    let op_available = state.op_available;
    *state = ManagerState::from_config_with_cache_and_op(config, cwd, cache, op_available);
}

pub(crate) fn clear_settings_auth_kind(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.auth.clear_selected_kind();
}

pub(crate) fn dismiss_settings_error_popup(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.dismiss_error_popup();
}

pub(crate) fn open_settings_error_popup(
    state: &mut ManagerState<'_>,
    title: impl Into<String>,
    message: impl Into<String>,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.open_error_popup(title, message);
}

pub(crate) fn apply_op_commit_result(
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

pub(crate) fn enter_settings_auth_kind(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.auth.enter_selected_kind();
}

pub(crate) fn move_editor_tab(state: &mut ManagerState<'_>, delta: isize, focus_tab_bar: bool) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let plan = editor_tab_move_plan(editor.active_tab, delta, focus_tab_bar);
    editor.apply_tab_move_plan(plan);
}

pub(crate) fn move_editor_field_selection(
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
        editor.tab_scroll.offset_y(),
        term.height,
        footer_h,
    );
    editor.apply_field_selection_plan(plan);
}
