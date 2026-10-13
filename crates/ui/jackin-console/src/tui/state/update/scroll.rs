// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor and settings scroll plans.

use crate::tui::screens::editor::update::{
    editor_tab_horizontal_scroll_plan, editor_workspace_mounts_horizontal_scroll_plan,
};
use crate::tui::screens::settings::update::settings_horizontal_scroll_plan;

use super::super::{ManagerStage, ManagerState};

pub(crate) fn scroll_editor_tab_horizontal(
    state: &mut ManagerState<'_>,
    delta: i16,
    term_width: u16,
    content_width: usize,
) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let plan = editor_tab_horizontal_scroll_plan(
        editor.tab_scroll.offset_x(),
        delta,
        term_width,
        content_width,
    );
    editor.apply_tab_horizontal_scroll_plan(plan);
}

pub(crate) fn scroll_editor_workspace_mounts_horizontal(
    state: &mut ManagerState<'_>,
    delta: i16,
    term_width: u16,
    content_width: usize,
) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    let plan = editor_workspace_mounts_horizontal_scroll_plan(
        editor.workspace_mounts_scroll.offset_x(),
        delta,
        term_width,
        content_width,
    );
    editor.apply_workspace_mounts_horizontal_scroll_plan(plan);
}

pub(crate) fn scroll_settings_global_mounts_horizontal(
    state: &mut ManagerState<'_>,
    delta: i16,
    term_width: u16,
    content_width: usize,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let scroll_x = settings_horizontal_scroll_plan(
        settings.mounts.scroll.offset_x(),
        delta,
        term_width,
        content_width,
    );
    settings.mounts.apply_horizontal_scroll(scroll_x);
}

pub(crate) fn scroll_settings_trust_horizontal(
    state: &mut ManagerState<'_>,
    delta: i16,
    term_width: u16,
    content_width: usize,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let scroll_x = settings_horizontal_scroll_plan(
        settings.trust.scroll.offset_x(),
        delta,
        term_width,
        content_width,
    );
    settings.trust.apply_horizontal_scroll(scroll_x);
}
