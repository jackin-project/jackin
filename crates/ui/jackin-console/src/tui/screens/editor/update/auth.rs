// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor auth row plans.

use super::super::model::{AuthRow, EditorTab};
use crate::tui::screens::settings::model::AuthFormTarget;

#[must_use]
pub const fn auth_row_is_focusable<K>(row: &AuthRow<K>) -> bool {
    matches!(
        row,
        AuthRow::Account { .. }
            | AuthRow::Binding { .. }
            | AuthRow::WorkspaceMode { .. }
            | AuthRow::RoleMode { .. }
    )
}

#[must_use]
pub fn auth_focusable_index_at_visual_row<K>(rows: &[AuthRow<K>], row: usize) -> Option<usize> {
    rows.get(row)
        .filter(|auth_row| auth_row_is_focusable(auth_row))?;
    Some(row)
}

#[must_use]
pub fn editor_auth_row_index_at_position<K>(
    active_tab: EditorTab,
    modal_open: bool,
    area: ratatui::layout::Rect,
    col: u16,
    row: u16,
    scroll_y: u16,
    rows: &[AuthRow<K>],
) -> Option<usize> {
    if active_tab != EditorTab::Auth || modal_open {
        return None;
    }
    crate::tui::layout::bordered_content_hit_at_position(area, col, row, scroll_y, |visual_row| {
        auth_focusable_index_at_visual_row(rows, visual_row)
    })
}

#[must_use]
pub fn resolve_auth_form_target<K: Clone>(
    rows: &[AuthRow<K>],
    row: usize,
) -> Option<AuthFormTarget<K>> {
    match rows.get(row)? {
        AuthRow::WorkspaceMode { kind } => Some(AuthFormTarget::Workspace { kind: kind.clone() }),
        AuthRow::RoleMode { role, kind } => Some(AuthFormTarget::WorkspaceRole {
            role: role.clone(),
            kind: kind.clone(),
        }),
        _ => None,
    }
}
