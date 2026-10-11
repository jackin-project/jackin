// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Environments tab lines.

use super::super::model::SettingsEnvRow;
use super::super::model::SettingsEnvScope;
use super::super::model::SettingsEnvState;

use ratatui::text::Line;
use std::collections::BTreeMap;

use crate::tui::components::editor_rows::{
    SecretEnvLineFrame, SecretLineRow, SecretValueDisplay, secret_env_lines,
};

#[must_use]
pub fn env_lines<'a>(
    rows: &[SettingsEnvRow],
    selected_row: usize,
    show_cursor: bool,
    area_width: u16,
    value_for: impl Fn(&SettingsEnvScope, &str) -> Option<SecretValueDisplay<'a>>,
    is_unmasked: impl Fn(&SettingsEnvScope, &str) -> bool,
    role_var_count: impl Fn(&str) -> usize,
) -> Vec<Line<'static>> {
    let display_rows: Vec<SecretLineRow<SettingsEnvScope>> = rows
        .iter()
        .map(|row| match row {
            SettingsEnvRow::Key { scope, key } => SecretLineRow::Key {
                scope: scope.clone(),
                key: key.clone(),
            },
            SettingsEnvRow::GlobalAddSentinel => SecretLineRow::WorkspaceAddSentinel,
            SettingsEnvRow::RoleHeader { role, expanded } => SecretLineRow::RoleHeader {
                role: role.clone(),
                expanded: *expanded,
            },
            SettingsEnvRow::RoleAddSentinel(role) => SecretLineRow::RoleAddSentinel(role.clone()),
            SettingsEnvRow::SectionSpacer => SecretLineRow::SectionSpacer,
        })
        .collect();
    secret_env_lines(
        &display_rows,
        SecretEnvLineFrame {
            cursor: selected_row,
            show_cursor,
            area_width,
        },
        value_for,
        is_unmasked,
        |_| true,
        role_var_count,
    )
}

#[must_use]
pub fn env_state_lines<Modal>(
    state: &SettingsEnvState<jackin_core::EnvValue, Modal>,
    show_cursor: bool,
    area_width: u16,
) -> Vec<Line<'static>> {
    let rows = crate::tui::screens::settings::update::settings_env_flat_rows(
        &state.pending,
        &state.expanded,
    );
    env_lines(
        &rows,
        state.selected,
        show_cursor,
        area_width,
        |scope, key| {
            state
                .pending_value(scope, key)
                .map(crate::tui::components::env_value::secret_display)
        },
        |scope, key| state.is_unmasked(scope, key),
        |role| state.pending.roles.get(role).map_or(0, BTreeMap::len),
    )
}
