// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use termrock::runtime::FrameTick;
use termrock::style::MotionPolicy;
use termrock::widgets::SpinnerState;

#[must_use]
pub fn console_spinner_frame(tick: u8) -> &'static str {
    let spinner = SpinnerState::new();
    let elapsed = Duration::from_millis(u64::from(tick) * spinner.frame_period_ms());
    spinner.frame_glyph(
        FrameTick::manual(termrock::runtime::Instant::now(), elapsed, Duration::ZERO),
        MotionPolicy::Full,
    )
}
