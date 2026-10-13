// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared spinner for console modal loading panels.

use std::time::Duration;

use termrock::runtime::FrameTick;
use termrock::style::MotionPolicy;
use termrock::widgets::SpinnerState;

/// Braille spinner glyph for a loading-panel tick counter.
///
/// The counter advances once per loop tick (the op-picker's `OpLoadState`
/// owns it); elapsed time is the counter scaled by the state's frame period
/// so `spinner_step` resolves frame `tick % frames.len()` — identical
/// cadence and glyphs to the retired hand-rolled frame table, which matched
/// upstream `SPINNER_BRAILLE_FRAMES` exactly.
#[must_use]
pub fn console_spinner_frame(tick: u8) -> &'static str {
    let spinner = SpinnerState::new();
    let elapsed = Duration::from_millis(u64::from(tick) * spinner.frame_period_ms());
    spinner.frame_glyph(
        FrameTick::manual(termrock::runtime::Instant::now(), elapsed, Duration::ZERO),
        MotionPolicy::Full,
    )
}
