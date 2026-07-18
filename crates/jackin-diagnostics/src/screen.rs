// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Compatibility seam for legacy metric emission pending its final removal.

/// Screen state no longer lives in a span. New UI instrumentation uses
/// `jackin_telemetry::ui::ScreenVisitTracker`.
#[must_use]
pub const fn current_screen_name() -> Option<&'static str> {
    None
}
