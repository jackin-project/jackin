// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! General tab lines.

use super::super::model::SettingsGeneralState;

use ratatui::text::Line;

#[must_use]
pub fn general_lines(
    selected_row: usize,
    pending_coauthor_trailer: bool,
    pending_dco: bool,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    use crate::tui::screens::form_model::{FieldRow, FormSection};
    FormSection::new(
        vec![
            FieldRow::new(
                "Co-author trailer",
                if pending_coauthor_trailer {
                    "enabled"
                } else {
                    "disabled"
                },
            ),
            FieldRow::new(
                "DCO sign-off",
                if pending_dco { "enabled" } else { "disabled" },
            ),
        ],
        selected_row,
        show_cursor,
        26,
    )
    .lines()
}

#[must_use]
pub fn general_state_lines(state: &SettingsGeneralState, show_cursor: bool) -> Vec<Line<'static>> {
    general_lines(
        state.selected,
        state.pending_coauthor_trailer,
        state.pending_dco,
        show_cursor,
    )
}
