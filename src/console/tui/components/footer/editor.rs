//! Footer hint items for the workspace editor screen.

use std::cmp::Ordering;

use jackin_tui::HintSpan;

use crate::config::AppConfig;
use crate::console::tui::components::footer::modal::modal_footer_items;
use crate::console::tui::state::{
    AuthRow, EditorState, EditorStateExt, EditorTab, FieldFocus, Modal, SecretsRow,
    auth_flat_rows, secrets_flat_rows,
};
use crate::operator_env::EnvValue;
use jackin_console::tui::components::footer_hints::{
    EditorContextFooterMode, content_footer_items, editor_contextual_row_footer_items,
    editor_save_footer_label, tab_bar_footer_items,
};

pub(crate) fn editor_footer_items(
    state: &EditorState<'_>,
    config: &AppConfig,
    op_available: bool,
) -> Vec<HintSpan<'static>> {
    if let Some(modal) = &state.modal {
        return modal_footer_items(
            modal,
            matches!(modal, Modal::AuthForm { .. })
                && crate::console::tui::input::auth::auth_form_can_generate_token(state),
        );
    }
    if state.tab_bar_focused {
        return tab_bar_footer_items(
            editor_save_footer_label(),
            state.active_tab != EditorTab::General,
            state.is_dirty().then(|| state.change_count()),
        );
    }
    let row_items = contextual_row_items(state, config, op_available);
    content_footer_items(
        editor_save_footer_label(),
        row_items,
        state.is_dirty().then(|| state.change_count()),
    )
}

#[allow(clippy::too_many_lines)]
pub(crate) fn contextual_row_items(
    state: &EditorState<'_>,
    config: &AppConfig,
    op_available: bool,
) -> Vec<HintSpan<'static>> {
    editor_contextual_row_footer_items(editor_context_footer_mode(state, config), op_available)
}

fn editor_context_footer_mode(
    state: &EditorState<'_>,
    config: &AppConfig,
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
                Ordering::Less => EditorContextFooterMode::MountRow {
                    has_github_url: state
                        .pending
                        .mounts
                        .get(cursor)
                        .and_then(|m| state.mount_info_cache.github_web_url(&m.src))
                        .is_some(),
                },
                Ordering::Equal => EditorContextFooterMode::MountAddRow,
                Ordering::Greater => EditorContextFooterMode::Empty,
            }
        }
        EditorTab::Roles => EditorContextFooterMode::RoleRow {
            is_existing_role: cursor < config.roles.len(),
        },
        EditorTab::Secrets => {
            let rows = secrets_flat_rows(state);
            let focused_value_is_op_ref = match rows.get(cursor) {
                Some(SecretsRow::WorkspaceKeyRow(key)) => state
                    .pending
                    .env
                    .get(key)
                    .is_some_and(|v| matches!(v, EnvValue::OpRef(_))),
                Some(SecretsRow::RoleKeyRow { role, key }) => state
                    .pending
                    .roles
                    .get(role)
                    .and_then(|ov| ov.env.get(key))
                    .is_some_and(|v| matches!(v, EnvValue::OpRef(_))),
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
            let flat = auth_flat_rows(state, config);
            match flat.get(cursor) {
                Some(AuthRow::AuthKindRow { .. }) => {
                    EditorContextFooterMode::AuthManage
                }
                Some(AuthRow::WorkspaceMode { .. } | AuthRow::RoleMode { .. }) => {
                    EditorContextFooterMode::AuthEditMode
                }
                Some(AuthRow::RoleHeader { .. }) => {
                    EditorContextFooterMode::AuthRoleHeader
                }
                Some(AuthRow::AddSentinel { .. }) => EditorContextFooterMode::AuthAddOverride,
                Some(AuthRow::WorkspaceSource { .. } | AuthRow::RoleSource { .. }) => {
                    EditorContextFooterMode::AuthEditSource
                }
                Some(AuthRow::Spacer) | None => EditorContextFooterMode::Empty,
            }
        }
    }
}
