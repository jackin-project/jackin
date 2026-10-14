// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::layout::Rect;

#[must_use]
pub fn split_debug_area(area: Rect, debug_mode: bool) -> (Rect, Option<Rect>) {
    if !debug_mode || area.height < 3 {
        return (area, None);
    }

    let main = Rect {
        height: area.height - 2,
        ..area
    };
    let bar = Rect {
        y: area.y + area.height - 2,
        height: 2,
        ..area
    };
    (main, Some(bar))
}

#[must_use]
pub fn debug_chip_row(bar: Rect) -> Rect {
    if bar.height < 2 {
        return bar;
    }
    Rect {
        y: bar.y + bar.height - 1,
        height: 1,
        ..bar
    }
}

#[must_use]
pub fn debug_invocation_id_label(invocation_id: Option<&str>) -> String {
    invocation_id
        .filter(|invocation_id| !invocation_id.is_empty())
        .unwrap_or_default()
        .to_owned()
}

#[must_use]
pub const fn should_debug_log_mouse(mouse: crossterm::event::MouseEvent) -> bool {
    !matches!(mouse.kind, crossterm::event::MouseEventKind::Moved)
}
