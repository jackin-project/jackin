// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Console adapter around TermRock [`Viewport`] for bordered scrollable panels.
//!
//! Migration 0018 removed free-function `render_scrollable_block*` helpers in
//! favor of the canonical stateful widget. This thin adapter preserves the
//! call shape used across workspace/settings/editor tabs.
//!
//! `focused` means **interaction ownership** (green border via
//! [`PanelEmphasis::Focused`]). Callers that implement the passive-scroll
//! focusability rule must clear their focus state when content fits, before
//! calling this helper.
//!
//! Visual contracts for [`Viewport`] itself are owned by TermRock tests; jackin❯
//! product tests assert screen-level composition (one focus owner, product
//! wording) rather than TermRock role RGB mapping.

use ratatui::{Frame, layout::Rect, text::Line};
use termrock::{
    Theme,
    scroll::DialogScroll,
    widgets::{PanelEmphasis, Viewport},
};

/// Render a bordered scrollable block using TermRock `Viewport`.
pub fn render_scrollable_block_at(
    frame: &mut Frame<'_>,
    area: Rect,
    lines: Vec<Line<'_>>,
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
    title: Option<&str>,
) {
    let theme = Theme::default();
    let mut scroll = DialogScroll::default();
    scroll.scroll_x = scroll_x;
    scroll.scroll_y = scroll_y;
    let emphasis = if focused {
        PanelEmphasis::Focused
    } else {
        PanelEmphasis::Normal
    };
    let mut viewport = Viewport::new(&lines, &theme).emphasis(emphasis);
    if let Some(title) = title {
        viewport = viewport.title(title);
    }
    frame.render_stateful_widget(viewport, area, &mut scroll);
}
