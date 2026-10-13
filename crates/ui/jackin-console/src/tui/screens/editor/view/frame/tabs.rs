// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor per-tab content renderers.

use super::{
    editor_auth_lines_for_state, editor_general_lines_for_state, editor_mount_lines_for_state,
    editor_role_lines_for_state, editor_secret_lines_for_state,
};
use ratatui::{Frame, layout::Rect};

use super::super::WorkspaceEditorState;

pub(crate) fn render_general_tab<
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
) {
    let rows = editor_general_lines_for_state(state);
    let focused = editor_tab_content_focused(state);
    crate::tui::scroll_block::render_scrollable_block_at(
        frame,
        area,
        rows,
        state.tab_scroll.offset_x(),
        state.tab_scroll.offset_y(),
        focused,
        None,
    );
}

pub(crate) fn render_mounts_tab<
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
) {
    let lines = editor_mount_lines_for_state(state);
    crate::tui::scroll_block::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.workspace_mounts_scroll.offset_x(),
        state.tab_scroll.offset_y(),
        state.workspace_mounts_scroll_focused() && state.modal.is_none(),
        None,
    );
}

pub(crate) fn render_roles_tab<
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
) {
    let lines = editor_role_lines_for_state(state, config);
    let focused = editor_tab_content_focused(state);
    crate::tui::scroll_block::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.tab_scroll.offset_x(),
        state.tab_scroll.offset_y(),
        focused,
        None,
    );
}

pub(crate) fn render_secrets_tab<
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
) {
    let lines = editor_secret_lines_for_state(area, state, config);
    let focused = editor_tab_content_focused(state);
    crate::tui::scroll_block::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.tab_scroll.offset_x(),
        state.tab_scroll.offset_y(),
        focused,
        None,
    );
}

pub(crate) fn render_auth_tab<
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
) {
    let lines = editor_auth_lines_for_state(state, config);
    let focused = editor_tab_content_focused(state);
    crate::tui::scroll_block::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.tab_scroll.offset_x(),
        state.tab_scroll.offset_y(),
        focused,
        None,
    );
}

pub(crate) fn editor_tab_content_focused<
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
) -> bool {
    !state.tab_bar_focused() && state.tab_content_scroll_focused() && state.modal.is_none()
}
