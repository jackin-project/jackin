// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `editor_*_lines_for_state` render adapters.

use super::editor_tab_content_focused;
use ratatui::{layout::Rect, text::Line};

use super::super::WorkspaceEditorState;

pub(crate) fn editor_general_lines_for_state<
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
) -> Vec<Line<'static>> {
    super::super::general_tab::general_state_lines(state, editor_tab_content_focused(state))
}

pub(crate) fn editor_mount_lines_for_state<
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
) -> Vec<Line<'static>> {
    let show_cursor = !state.tab_bar_focused()
        && state.workspace_mounts_scroll_focused()
        && state.modal.is_none();
    super::super::mounts_tab::mount_state_lines(state, show_cursor)
}

pub(crate) fn editor_role_lines_for_state<
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
    config: &jackin_config::AppConfig,
) -> Vec<Line<'static>> {
    super::super::roles_tab::role_state_lines(
        state,
        config.roles.keys(),
        editor_tab_content_focused(state),
    )
}

pub(crate) fn editor_secret_lines_for_state<
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
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
) -> Vec<Line<'static>> {
    super::super::secrets_tab::secret_state_lines(
        state,
        editor_tab_content_focused(state),
        area.width,
        |role| config.roles.contains_key(role),
    )
}

pub(crate) fn editor_auth_lines_for_state<
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
    config: &jackin_config::AppConfig,
) -> Vec<Line<'static>> {
    super::super::auth_tab::auth_state_lines(state, config, editor_tab_content_focused(state))
}
