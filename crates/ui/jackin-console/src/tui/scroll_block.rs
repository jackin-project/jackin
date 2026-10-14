// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::{Frame, layout::Rect, text::Line};
use termrock::{
    scroll::DialogScroll,
    style::DesignSystem,
    widgets::{PanelChrome, ScrollAreaState, ScrollOutcome, Viewport},
};

pub fn console_scroll_area_state() -> ScrollAreaState {
    ScrollAreaState::new().axes(true, true).wheel_steps(1, 1)
}

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

pub fn scroll_area_set_x(state: &mut ScrollAreaState, x: u16) {
    state.set_content_size(u16::MAX, u16::MAX);
    state.set_viewport(1, 1);
    state.set_offset_x(x);
}

pub fn scroll_area_set_y(state: &mut ScrollAreaState, y: u16) {
    state.set_content_size(u16::MAX, u16::MAX);
    state.set_viewport(1, 1);
    state.set_offset_y_quiet(y);
}

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
        .padded_content();
    if let Some(title) = title {
        viewport = viewport.title(title);
    }
    frame.render_stateful_widget(viewport, area, &mut scroll);
}
