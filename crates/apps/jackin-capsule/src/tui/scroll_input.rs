// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use termrock::input::{KeyCode, KeyEvent, KeyModifiers, MouseEventKind};
use termrock::scroll::{DialogScroll, ScrollAxes};

#[must_use]
pub fn apply_raw_dialog_scroll_key(
    scroll: &mut DialogScroll,
    key: &[u8],
    axes: ScrollAxes,
) -> bool {
    let code = match key {
        b"\x1b[A" | b"k" | b"K" if axes.vertical => KeyCode::Up,
        b"\x1b[B" | b"j" | b"J" if axes.vertical => KeyCode::Down,
        b"\x1b[D" | b"h" | b"H" if axes.horizontal => KeyCode::Left,
        b"\x1b[C" | b"l" | b"L" if axes.horizontal => KeyCode::Right,
        _ => return false,
    };
    scroll.handle_key_for_axes(
        KeyEvent::new(code, KeyModifiers::NONE),
        usize::MAX / 4,
        1,
        usize::MAX / 4,
        1,
        axes,
    )
}

#[must_use]
pub fn apply_sgr_wheel_button(scroll: &mut DialogScroll, button: u8, axes: ScrollAxes) -> bool {
    let forward = (button & 1) != 0;
    let horizontal = (button & 2) != 0 || (button & 4) != 0;
    let kind = match (horizontal, forward) {
        (true, true) => MouseEventKind::ScrollRight,
        (true, false) => MouseEventKind::ScrollLeft,
        (false, true) => MouseEventKind::ScrollDown,
        (false, false) => MouseEventKind::ScrollUp,
    };
    let modifiers = if (button & 4) != 0 {
        KeyModifiers::SHIFT
    } else {
        KeyModifiers::NONE
    };
    scroll.handle_mouse(kind, modifiers, axes)
}

#[cfg(test)]
mod tests;
