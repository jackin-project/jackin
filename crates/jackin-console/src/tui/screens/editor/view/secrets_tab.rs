// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Secrets tab lines, geometry, and width helpers extracted from the view
//! coordinator. Items re-exported from parent to preserve `super::` call
//! sites in tests and frame.

use ratatui::text::Line;

use crate::tui::components::editor_rows::{
    SecretEnvLineFrame, SecretLineRow, SecretValueDisplay, secret_env_lines,
};
use crate::tui::components::env_value::secret_display;
use crate::tui::screens::editor::model::{FieldFocus, SecretsRow, SecretsScopeTag};

use super::WorkspaceEditorState;

#[must_use]
#[allow(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) fn secret_lines<'a>(
    rows: &[SecretsRow],
    cursor: usize,
    show_cursor: bool,
    area_width: u16,
    value_for: impl Fn(&SecretsScopeTag, &str) -> Option<SecretValueDisplay<'a>>,
    is_unmasked: impl Fn(&SecretsScopeTag, &str) -> bool,
    role_in_registry: impl Fn(&str) -> bool,
    role_var_count: impl Fn(&str) -> usize,
) -> Vec<Line<'static>> {
    let display_rows: Vec<SecretLineRow<SecretsScopeTag>> = rows
        .iter()
        .map(|row| match row {
            SecretsRow::WorkspaceKeyRow(key) => SecretLineRow::Key {
                scope: SecretsScopeTag::Workspace,
                key: key.clone(),
            },
            SecretsRow::WorkspaceAddSentinel => SecretLineRow::WorkspaceAddSentinel,
            SecretsRow::RoleHeader { role, expanded } => SecretLineRow::RoleHeader {
                role: role.clone(),
                expanded: *expanded,
            },
            SecretsRow::RoleKeyRow { role, key } => SecretLineRow::Key {
                scope: SecretsScopeTag::Role(role.clone()),
                key: key.clone(),
            },
            SecretsRow::RoleAddSentinel(role) => SecretLineRow::RoleAddSentinel(role.clone()),
            SecretsRow::SectionSpacer => SecretLineRow::SectionSpacer,
        })
        .collect();
    secret_env_lines(
        &display_rows,
        SecretEnvLineFrame {
            cursor,
            show_cursor,
            area_width,
        },
        |scope, key| value_for(scope, key).or(Some(SecretValueDisplay::Plain(""))),
        is_unmasked,
        role_in_registry,
        role_var_count,
    )
}

#[must_use]
pub(crate) fn secret_state_lines<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    show_cursor: bool,
    area_width: u16,
    role_in_registry: impl Fn(&str) -> bool,
) -> Vec<Line<'static>> {
    let FieldFocus::Row(cursor) = state.active_field;
    let rows = state.secrets_flat_rows();
    secret_lines(
        &rows,
        cursor,
        show_cursor,
        area_width,
        |scope, key| match scope {
            SecretsScopeTag::Workspace => state.pending.env.get(key).map(secret_display),
            SecretsScopeTag::Role(role) => state
                .pending
                .roles
                .get(role)
                .and_then(|role_override| role_override.env.get(key))
                .map(secret_display),
        },
        |scope, key| {
            state
                .unmasked_rows
                .contains(&(scope.clone(), key.to_owned()))
        },
        role_in_registry,
        |role| {
            state
                .pending
                .roles
                .get(role)
                .map_or(0, |role| role.env.len())
        },
    )
}

#[must_use]
pub(crate) fn secret_state_geometry<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    area_width: u16,
    role_in_registry: impl Fn(&str) -> bool,
) -> super::EditorTabContentGeometry {
    let rows = state.secrets_flat_rows();
    let content_width = rows
        .iter()
        .map(|row| {
            editor_secret_line_width(
                row,
                area_width,
                |scope, key| match scope {
                    SecretsScopeTag::Workspace => state.pending.env.get(key).map(secret_display),
                    SecretsScopeTag::Role(role) => state
                        .pending
                        .roles
                        .get(role)
                        .and_then(|role_override| role_override.env.get(key))
                        .map(secret_display),
                },
                |scope, key| {
                    state
                        .unmasked_rows
                        .contains(&(scope.clone(), key.to_owned()))
                },
                |role| role_in_registry(role),
                |role| {
                    state
                        .pending
                        .roles
                        .get(role)
                        .map_or(0, |role| role.env.len())
                },
            )
        })
        .max()
        .unwrap_or(0);
    super::EditorTabContentGeometry {
        content_width,
        content_height: rows.len(),
    }
}

#[must_use]
pub(crate) fn editor_secret_line_width<'a>(
    row: &SecretsRow,
    area_width: u16,
    value_for: impl Fn(&SecretsScopeTag, &str) -> Option<SecretValueDisplay<'a>>,
    is_unmasked: impl Fn(&SecretsScopeTag, &str) -> bool,
    role_in_registry: impl Fn(&str) -> bool,
    role_var_count: impl Fn(&str) -> usize,
) -> usize {
    const LABEL_WIDTH: usize = 22;
    match row {
        SecretsRow::WorkspaceKeyRow(key) => {
            let scope = SecretsScopeTag::Workspace;
            let value = value_for(&scope, key).unwrap_or(SecretValueDisplay::Plain(""));
            secret_key_line_width(
                key,
                value,
                !is_unmasked(&scope, key),
                area_width,
                LABEL_WIDTH,
            )
        }
        SecretsRow::WorkspaceAddSentinel => super::padded_width("  + Add environment variable"),
        SecretsRow::RoleHeader { role, .. } => {
            let mut width = super::text_width(&format!(
                "       \u{25bc} Role: {role}  ({} vars)",
                role_var_count(role)
            ));
            if !role_in_registry(role) {
                width += super::text_width("  (not in registry)");
            }
            super::padded_width_cols(width, 7)
        }
        SecretsRow::RoleKeyRow { role, key } => {
            let scope = SecretsScopeTag::Role(role.clone());
            let value = value_for(&scope, key).unwrap_or(SecretValueDisplay::Plain(""));
            secret_key_line_width(
                key,
                value,
                !is_unmasked(&scope, key),
                area_width,
                LABEL_WIDTH,
            )
        }
        SecretsRow::RoleAddSentinel(role) => {
            super::padded_width(&format!("       + Add {role} environment variable"))
        }
        SecretsRow::SectionSpacer => 0,
    }
}

#[allow(
    unreachable_pub,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) fn secret_key_line_width(
    key: &str,
    value: SecretValueDisplay<'_>,
    masked: bool,
    area_width: u16,
    label_width: usize,
) -> usize {
    const OP_MARKER: &str = "[op] ";
    const NO_MARKER: &str = "     ";
    const MASK: &str =
        "\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}\u{25cf}";
    const OP_REF_REPICK_PLACEHOLDER: &str = "<unparseable path \u{2014} re-pick>";

    let op_breadcrumb = match value {
        SecretValueDisplay::OpRefPath(path) => {
            crate::tui::op_breadcrumb::parse_path_breadcrumb(path)
        }
        SecretValueDisplay::Plain(_) => None,
    };
    let marker = if op_breadcrumb.is_some() {
        OP_MARKER
    } else {
        NO_MARKER
    };
    let prefix_width = super::text_width("  ")
        + super::text_width(marker)
        + super::text_width(&format!("{key:label_width$}"))
        + 2;
    let value_width = if let Some(parts) = op_breadcrumb.as_ref() {
        crate::tui::op_breadcrumb::breadcrumb_display_width(parts)
    } else if masked {
        super::text_width(MASK)
    } else {
        let plain_str = match value {
            SecretValueDisplay::Plain(value) => value,
            SecretValueDisplay::OpRefPath(_) => OP_REF_REPICK_PLACEHOLDER,
        };
        let budget = (area_width as usize)
            .saturating_sub(label_width)
            .saturating_sub(8)
            .max(1);
        plain_str.chars().count().min(budget)
    };
    super::padded_width_cols(prefix_width + value_width, 2)
}
