// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared host-terminal mode escape helpers.

pub fn enable_mouse_capture<W: std::io::Write>(out: &mut W) -> std::io::Result<()> {
    // ?1000h press/release, ?1002h drag, ?1003h any-event motion, ?1006h SGR
    // coordinates. SGR-only matters for trackpads: enabling old urxvt ?1015
    // alongside SGR makes some terminals report scroll gestures as motion.
    out.write_all(b"\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h")?;
    out.flush()
}

pub fn disable_mouse_capture<W: std::io::Write>(out: &mut W) -> std::io::Result<()> {
    // Disable the exact modes we enable, plus ?1015l defensively in case an
    // older build left urxvt coordinates on.
    out.write_all(b"\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l")?;
    out.flush()
}

#[cfg(test)]
mod tests;
