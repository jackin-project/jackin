// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Console adapter around `TermRock` [`Viewport`] for bordered scrollable panels.
//!
//! Render-path carve-out (C1): the upstream `ScrollArea` *paint* widget is
//! deliberately NOT adopted — it cannot produce byte-identical pixels
//! against the current Viewport + fade + explicit-scrollbar paint, which
//! the parity gate forbids. The adoption is `ScrollAreaState` (the scroll
//! model) driving this existing render path.
//!
//! Migration 0018 removed free-function `render_scrollable_block*` helpers in
//! favor of the canonical stateful widget. This thin adapter preserves the
//! call shape used across workspace/settings/editor tabs.
//!
//! `focused` means **interaction ownership** (green border via
//! [`PanelChrome::Focused`]). Callers that implement the passive-scroll
//! focusability rule must clear their focus state when content fits, before
//! calling this helper.
//!
//! Visual contracts for [`Viewport`] itself are owned by `TermRock` tests; jackin❯
//! product tests assert screen-level composition (one focus owner, product
//! wording) rather than `TermRock` role RGB mapping.

use ratatui::{Frame, layout::Rect, text::Line};
use termrock::{
    scroll::DialogScroll,
    style::DesignSystem,
    widgets::{PanelChrome, ScrollAreaState, ScrollOutcome, Viewport},
};

/// Console scroll-block state constructor (mouse parity matrix rows 2, 12):
/// upstream wheel-step defaults are 3/4 but the console feel is 1 line/col
/// per tick on both axes, and every console block is dual-axis today.
pub fn console_scroll_area_state() -> ScrollAreaState {
    ScrollAreaState::new().axes(true, true).wheel_steps(1, 1)
}

/// The dims-plumbing idiom every wheel/keyboard/drag write uses: project the
/// caller-measured content + viewport dims, then apply the deltas. Clamp
/// parity with the retired raw-offset helpers is structural — `scroll_by`
/// rides the same upstream `apply_delta_u16` they did.
pub fn scroll_block_by(
    state: &mut ScrollAreaState,
    area: Rect,
    content_w: usize,
    content_h: usize,
    dy: isize,
    dx: isize,
) -> ScrollOutcome {
    state.set_content_size(
        u16::try_from(content_w).unwrap_or(u16::MAX),
        u16::try_from(content_h).unwrap_or(u16::MAX),
    );
    state.set_viewport(
        u16::try_from(termrock::scroll::viewport_width(area)).unwrap_or(u16::MAX),
        u16::try_from(termrock::scroll::viewport_height(area)).unwrap_or(u16::MAX),
    );
    state.scroll_by(dy, dx)
}

/// Absolute-offset write for paths whose arithmetic lives in pure scroll
/// plans (saturating, content-clamp deferred to render exactly as the raw
/// `u16` fields behaved): dims are pinned so the state's own clamp is a
/// no-op and the stored offset is exactly the plan's output. The pinned
/// viewport is 1, not 0 — upstream `max_offset(_, 0)` is 0, which would
/// clamp the offset to zero.
pub fn scroll_area_set_x(state: &mut ScrollAreaState, x: u16) {
    state.set_content_size(u16::MAX, u16::MAX);
    state.set_viewport(1, 1);
    state.set_offset_x(x);
}

/// Vertical twin of [`scroll_area_set_x`]. Uses the quiet setter: these
/// plan-driven writes are cursor-reveal class, not user-driven position
/// changes.
pub fn scroll_area_set_y(state: &mut ScrollAreaState, y: u16) {
    state.set_content_size(u16::MAX, u16::MAX);
    state.set_viewport(1, 1);
    state.set_offset_y_quiet(y);
}

/// Render a bordered scrollable block using `TermRock` `Viewport`.
pub fn render_scrollable_block_at(
    frame: &mut Frame<'_>,
    area: Rect,
    lines: Vec<Line<'_>>,
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
    title: Option<&str>,
) {
    let theme = DesignSystem::default();
    let mut scroll = DialogScroll::default();
    scroll.scroll_x = scroll_x;
    scroll.scroll_y = scroll_y;
    let emphasis = if focused {
        PanelChrome::Focused
    } else {
        PanelChrome::Normal
    };
    let mut viewport = Viewport::new(&lines, &theme)
        .emphasis(emphasis)
        // Bordered subpanels keep the Panel body column: content insets by
        // the density pad on X while rows stay flush with the border on Y.
        .padded_content();
    if let Some(title) = title {
        viewport = viewport.title(title);
    }
    frame.render_stateful_widget(viewport, area, &mut scroll);
}
