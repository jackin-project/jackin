// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Product dialog body composition via TermRock [`bottom_rows`] (migration 0018).
//!
//! TermRock no longer ships a fixed five-slot dialog facade. Console modals
//! that still want a leading spacer, scrollable body, mid spacer, action row,
//! and trailing spacer compose that shape explicitly here.

use ratatui::layout::Rect;
use termrock::layout::bottom_rows;

/// Split a dialog `inner` rect into content body and action row.
///
/// Layout (top → bottom):
/// - 1-row leading spacer (absorbed into body split)
/// - content (remaining body)
/// - 1-row mid spacer
/// - 1-row actions
/// - 1-row trailing spacer
#[must_use]
pub fn dialog_content_and_actions(inner: Rect) -> (Rect, Rect) {
    let (body, [_mid_spacer, actions, _trailing]) = bottom_rows(inner, [1, 1, 1]);
    let leading = body.height.min(1);
    let content = Rect::new(
        body.x,
        body.y.saturating_add(leading),
        body.width,
        body.height.saturating_sub(leading),
    );
    (content, actions)
}
