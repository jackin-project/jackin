// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Debug area layout helpers.

use ratatui::layout::Rect;

/// Split `area` into a main region and an optional 1-row debug bar at the
/// bottom.
#[must_use]
pub fn split_debug_area(area: Rect, debug_mode: bool) -> (Rect, Option<Rect>) {
    if !debug_mode || area.height < 3 {
        return (area, None);
    }
    // Reserve 2 rows: 1 blank spacer + 1 chip row.  The spacer separates the
    // hint bar from the debug chip (Defect 39 requirement: body → spacer →
    // hints → spacer → status/chip row).
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

/// Return the 1-row rect within a `split_debug_area` bar where the chip
/// is actually rendered.  The top row of the 2-row bar is the blank spacer;
/// the chip lives in the bottom row.
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
    // Skip only the high-frequency `Moved` (hover) flood. Clicks, drags, AND
    // scroll/wheel events must be logged — scroll events are exactly what a
    // "wheel does nothing" bug report needs, and filtering them out (as an
    // earlier version did) sent triage chasing a phantom "no wheel events".
    !matches!(mouse.kind, crossterm::event::MouseEventKind::Moved)
}
