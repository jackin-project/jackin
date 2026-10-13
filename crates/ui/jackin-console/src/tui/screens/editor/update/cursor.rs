// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor cursor and row-bound plans.

use super::{EditorFieldSelectionPlan, auth_row_is_focusable, editor_max_row_for_tab};

use super::super::model::{AuthRow, EditorTab, SecretsRow};

#[must_use]
pub fn editor_field_selection_plan(
    active_row: usize,
    delta: isize,
    max_row: usize,
    skipped_rows: &[usize],
    current_scroll_y: u16,
    term_height: u16,
    footer_h: u16,
) -> EditorFieldSelectionPlan {
    let candidate =
        crate::tui::focus::collection_move_index(active_row, max_row.saturating_add(1), delta);
    let next = if delta.is_negative() {
        step_cursor_up(skipped_rows, candidate)
    } else {
        step_cursor_down(skipped_rows, candidate, max_row)
    };
    EditorFieldSelectionPlan {
        active_row: next,
        tab_scroll_y: crate::tui::focus::cursor_scroll_for_panel(
            next,
            current_scroll_y,
            term_height,
            footer_h,
        ),
    }
}

#[must_use]
pub fn step_cursor_down(skipped_rows: &[usize], candidate: usize, max_row: usize) -> usize {
    let mut idx = candidate;
    while idx <= max_row {
        if skipped_rows.contains(&idx) {
            idx += 1;
        } else {
            return idx;
        }
    }
    candidate
}

#[must_use]
pub fn step_cursor_up(skipped_rows: &[usize], candidate: usize) -> usize {
    let mut idx = candidate;
    loop {
        if skipped_rows.contains(&idx) {
            if idx == 0 {
                return 0;
            }
            idx -= 1;
        } else {
            return idx;
        }
    }
}

#[must_use]
pub fn secrets_skipped_rows(rows: &[SecretsRow]) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter_map(|(idx, row)| matches!(row, SecretsRow::SectionSpacer).then_some(idx))
        .collect()
}

#[must_use]
pub fn editor_secrets_selection_bounds(rows: &[SecretsRow]) -> (usize, Vec<usize>) {
    (rows.len().saturating_sub(1), secrets_skipped_rows(rows))
}

#[must_use]
pub fn auth_skipped_rows<K>(rows: &[AuthRow<K>]) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter_map(|(idx, row)| (!auth_row_is_focusable(row)).then_some(idx))
        .collect()
}

#[must_use]
pub fn editor_selection_bounds<K>(
    tab: EditorTab,
    mount_count: usize,
    role_count: usize,
    secrets_rows: &[SecretsRow],
    auth_rows: &[AuthRow<K>],
) -> (usize, Vec<usize>) {
    match tab {
        EditorTab::Secrets => editor_secrets_selection_bounds(secrets_rows),
        EditorTab::Auth => (
            editor_max_row_for_tab(tab, mount_count, role_count, 0, auth_rows.len()),
            auth_skipped_rows(auth_rows),
        ),
        EditorTab::General | EditorTab::Mounts | EditorTab::Roles => (
            editor_max_row_for_tab(tab, mount_count, role_count, 0, 0),
            Vec::new(),
        ),
    }
}
