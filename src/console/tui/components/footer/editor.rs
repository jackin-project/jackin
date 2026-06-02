//! Footer hint items for the workspace editor screen.

use std::cmp::Ordering;

use jackin_tui::HintSpan;

use crate::config::AppConfig;
use crate::console::tui::components::footer::modal::modal_footer_items;
use crate::console::tui::state::{
    AuthRow, EditorState, EditorTab, FieldFocus, Modal, SecretsRow,
    auth_flat_rows, secrets_flat_rows,
};
use crate::operator_env::EnvValue;
use jackin_console::tui::components::footer_hints::{
    AuthRowFooterMode, add_row_footer_items, auth_row_footer_items, content_footer_items,
    editor_general_row_footer_items,
    editor_role_row_footer_items, secret_add_row_footer_items, secret_op_ref_row_footer_items,
    secret_plain_row_footer_items, secret_role_header_footer_items, tab_bar_footer_items,
    workspace_mount_row_footer_items,
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
            "save workspace",
            state.active_tab != EditorTab::General,
            state.is_dirty().then(|| state.change_count()),
        );
    }
    let row_items = contextual_row_items(state, config, op_available);
    content_footer_items(
        "save workspace",
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
    let FieldFocus::Row(cursor) = state.active_field;
    match state.active_tab {
        EditorTab::General => {
            editor_general_row_footer_items(cursor, !state.pending.mounts.is_empty())
        }
        EditorTab::Mounts => {
            let mount_count = state.pending.mounts.len();
            match cursor.cmp(&mount_count) {
                Ordering::Less => workspace_mount_row_footer_items(
                    state
                        .pending
                        .mounts
                        .get(cursor)
                        .and_then(|m| state.mount_info_cache.github_web_url(&m.src))
                        .is_some(),
                ),
                Ordering::Equal => add_row_footer_items("add"),
                Ordering::Greater => Vec::new(),
            }
        }
        EditorTab::Roles => {
            editor_role_row_footer_items(cursor < config.roles.len())
        }
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
                    secret_op_ref_row_footer_items(op_available)
                }
                Some(SecretsRow::WorkspaceKeyRow(_) | SecretsRow::RoleKeyRow { .. }) => {
                    secret_plain_row_footer_items(op_available)
                }
                Some(SecretsRow::RoleHeader { .. }) => secret_role_header_footer_items(),
                Some(SecretsRow::WorkspaceAddSentinel | SecretsRow::RoleAddSentinel(_)) => {
                    secret_add_row_footer_items(op_available)
                }
                Some(SecretsRow::SectionSpacer) | None => vec![],
            }
        }
        EditorTab::Auth => {
            let flat = auth_flat_rows(state, config);
            match flat.get(cursor) {
                Some(AuthRow::AuthKindRow { .. }) => {
                    auth_row_footer_items(AuthRowFooterMode::ManageAuth)
                }
                Some(AuthRow::WorkspaceMode { .. } | AuthRow::RoleMode { .. }) => {
                    auth_row_footer_items(AuthRowFooterMode::EditMode)
                }
                Some(AuthRow::RoleHeader { .. }) => {
                    auth_row_footer_items(AuthRowFooterMode::RoleHeader)
                }
                Some(AuthRow::AddSentinel { .. }) => add_row_footer_items("add override"),
                Some(AuthRow::WorkspaceSource { .. } | AuthRow::RoleSource { .. }) => {
                    auth_row_footer_items(AuthRowFooterMode::EditSource)
                }
                Some(AuthRow::Spacer) | None => auth_row_footer_items(AuthRowFooterMode::Empty),
            }
        }
    }
}
