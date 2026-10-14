// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::layout::Rect;
use termrock::layout::bottom_rows;

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
