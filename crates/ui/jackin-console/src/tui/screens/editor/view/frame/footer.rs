// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor contextual footer items and scroll axes.

use ratatui::layout::Rect;
use termrock::widgets::HintSpan;

use crate::tui::components::footer_hints::{
    EditorContextFooterMode, editor_contextual_row_footer_items,
};
use crate::tui::screens::editor::model::{AuthRow, EditorTab, FieldFocus, SecretsRow};

use super::super::WorkspaceEditorState;

pub(crate) fn editor_contextual_footer_items<
    Modal,
    SaveFlow,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        jackin_core::EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
    op_available: bool,
    body_area: Rect,
) -> Vec<HintSpan<'static>> {
    editor_contextual_row_footer_items(
        editor_context_footer_mode(state, config, body_area),
        op_available,
    )
}

pub(crate) fn editor_context_footer_mode<
    Modal,
    SaveFlow,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        jackin_core::EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    config: &jackin_config::AppConfig,
    body_area: Rect,
) -> EditorContextFooterMode {
    let FieldFocus::Row(cursor) = state.active_field;
    match state.active_tab {
        EditorTab::General => EditorContextFooterMode::General {
            row: cursor,
            has_mounts: !state.pending.mounts.is_empty(),
        },
        EditorTab::Mounts => {
            let mount_count = state.pending.mounts.len();
            match cursor.cmp(&mount_count) {
                std::cmp::Ordering::Less => EditorContextFooterMode::MountRow {
                    has_github_url: state
                        .pending
                        .mounts
                        .get(cursor)
                        .and_then(|m| state.mount_info_cache.github_web_url(&m.src))
                        .is_some(),
                    scroll_axes: workspace_mount_scroll_axes(state, body_area),
                },
                std::cmp::Ordering::Equal => EditorContextFooterMode::MountAddRow,
                std::cmp::Ordering::Greater => EditorContextFooterMode::Empty,
            }
        }
        EditorTab::Roles => EditorContextFooterMode::RoleRow {
            is_existing_role: cursor < config.roles.len(),
        },
        EditorTab::Secrets => {
            let rows = state.secrets_flat_rows();
            let focused_value_is_op_ref = match rows.get(cursor) {
                Some(SecretsRow::WorkspaceKeyRow(key)) => state
                    .pending
                    .env
                    .get(key)
                    .is_some_and(|v| matches!(v, jackin_core::EnvValue::OpRef(_))),
                Some(SecretsRow::RoleKeyRow { role, key }) => state
                    .pending
                    .roles
                    .get(role)
                    .and_then(|ov| ov.env.get(key))
                    .is_some_and(|v| matches!(v, jackin_core::EnvValue::OpRef(_))),
                _ => false,
            };
            match rows.get(cursor) {
                Some(SecretsRow::WorkspaceKeyRow(_) | SecretsRow::RoleKeyRow { .. })
                    if focused_value_is_op_ref =>
                {
                    EditorContextFooterMode::SecretOpRefRow
                }
                Some(SecretsRow::WorkspaceKeyRow(_) | SecretsRow::RoleKeyRow { .. }) => {
                    EditorContextFooterMode::SecretPlainRow
                }
                Some(SecretsRow::RoleHeader { .. }) => EditorContextFooterMode::SecretRoleHeader,
                Some(SecretsRow::WorkspaceAddSentinel | SecretsRow::RoleAddSentinel(_)) => {
                    EditorContextFooterMode::SecretAddRow
                }
                Some(SecretsRow::SectionSpacer) | None => EditorContextFooterMode::Empty,
            }
        }
        EditorTab::Auth => {
            let flat = state.auth_flat_rows(config);
            match flat.get(cursor) {
                Some(AuthRow::Account { .. } | AuthRow::Binding { .. }) => {
                    EditorContextFooterMode::Accounts
                }
                Some(AuthRow::WorkspaceMode { .. } | AuthRow::RoleMode { .. }) => {
                    EditorContextFooterMode::AuthEditMode
                }
                None => EditorContextFooterMode::Empty,
            }
        }
    }
}

pub(crate) fn workspace_mount_scroll_axes<
    Modal,
    SaveFlow,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        jackin_core::EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    body_area: Rect,
) -> termrock::scroll::ScrollAxes {
    let content_width = crate::tui::mount_display::workspace_config_mounts_content_width_with_cache(
        &state.pending.mounts,
        &state.mount_info_cache,
    );
    crate::tui::list_geometry::horizontal_scroll_axes(
        !state.pending.mounts.is_empty(),
        content_width,
        body_area,
    )
}
