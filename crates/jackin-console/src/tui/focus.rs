// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Focus management helpers: track which TUI component owns input focus and
//! compute cursor movement within a scrollable list.
//!
//! Not responsible for: rendering focus indicators or routing key events.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountScrollFocus {
    Workspace,
    Global,
    RoleGlobal,
    Roles,
}

#[must_use]
pub fn moved_selection(selected: usize, row_count: usize, delta: isize) -> usize {
    let last = row_count.saturating_sub(1);
    if delta.is_negative() {
        selected.saturating_sub(delta.unsigned_abs())
    } else {
        selected.saturating_add(delta as usize).min(last)
    }
}

#[must_use]
pub fn selected_index(selected: usize, row_count: usize) -> usize {
    selected.min(row_count.saturating_sub(1))
}

#[must_use]
pub fn follow_cursor_y(
    cursor: usize,
    content_height: usize,
    viewport_h: usize,
    stored_scroll_y: u16,
) -> u16 {
    jackin_tui::components::scrollable_panel::cursor_follow_offset(
        cursor,
        content_height,
        viewport_h,
        stored_scroll_y,
    )
}

#[must_use]
pub fn cursor_scroll_for_panel(
    cursor: usize,
    scroll_y: u16,
    term_height: u16,
    footer_h: u16,
) -> u16 {
    // header(3) + tab-strip(2) + block-borders(2) + the renderer's dynamic footer.
    let chrome = 7u16.saturating_add(footer_h);
    let viewport_h = (term_height.saturating_sub(chrome) as usize).max(1);
    // content_height - viewport_h = u16::MAX exactly: max_offset returns u16::MAX without
    // tripping its debug_assert, while the upper clamp on cursor rows stays unreachable.
    let content_height = usize::from(u16::MAX).saturating_add(viewport_h);
    follow_cursor_y(cursor, content_height, viewport_h, scroll_y)
}
