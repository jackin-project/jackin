// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Product-local dialog slot geometry for console modals.
//!
//! TermRock migration 0018 removed the fixed five-slot facade in favor of
//! explicit `bottom_rows` composition. Several jackin❯ dialogs still need the
//! historical five-row body (leading spacer, content, spacer, actions,
//! trailing spacer); keep that product composition here rather than
//! reintroducing a TermRock compatibility shim.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Split `inner` into the historical five-slot dialog body used by console
/// confirm / git-prompt surfaces.
#[must_use]
pub fn dialog_inner_chunks(inner: Rect, content_rows: Option<u16>) -> [Rect; 5] {
    let content = content_rows.map_or(Constraint::Min(1), Constraint::Length);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // leading spacer
            content,               // content
            Constraint::Length(1), // spacer
            Constraint::Length(1), // action row
            Constraint::Length(1), // trailing spacer
        ])
        .split(inner);
    [chunks[0], chunks[1], chunks[2], chunks[3], chunks[4]]
}
