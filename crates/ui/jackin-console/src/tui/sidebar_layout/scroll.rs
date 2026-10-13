// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Sidebar scroll-area helpers.

use super::SidebarScrollArea;
use ratatui::layout::Rect;

use termrock::scroll::ScrollAxes;

pub fn clamp_scroll_area(area: SidebarScrollArea, scroll: &mut termrock::widgets::ScrollAreaState) {
    scroll.set_content_size(
        u16::try_from(area.content_width).unwrap_or(u16::MAX),
        u16::try_from(area.content_height).unwrap_or(u16::MAX),
    );
    scroll.set_viewport(
        u16::try_from(scroll_viewport_width(area.area)).unwrap_or(u16::MAX),
        u16::try_from(scroll_viewport_height(area.area)).unwrap_or(u16::MAX),
    );
    scroll.clamp();
}

#[must_use]
pub fn scroll_area_scrollable(area: SidebarScrollArea) -> bool {
    scroll_area_axes(area).any()
}

#[must_use]
pub fn scroll_area_axes(area: SidebarScrollArea) -> ScrollAxes {
    ScrollAxes {
        horizontal: is_scrollable(area.content_width, scroll_viewport_width(area.area)),
        vertical: is_scrollable(area.content_height, scroll_viewport_height(area.area)),
    }
}

pub(crate) fn mount_data_row_count(
    same_path_rows: impl IntoIterator<Item = bool>,
) -> Option<usize> {
    let mut saw_row = false;
    let mut lines = 0;
    for same_path in same_path_rows {
        saw_row = true;
        lines += if same_path { 1 } else { 2 };
    }
    saw_row.then_some(lines)
}

pub(crate) fn scroll_viewport_width(area: Rect) -> usize {
    termrock::scroll::viewport_width(area)
}

pub(crate) fn scroll_viewport_height(area: Rect) -> usize {
    termrock::scroll::viewport_height(area)
}

pub(crate) fn is_scrollable(content: usize, viewport: usize) -> bool {
    termrock::scroll::is_scrollable(content, viewport)
}
