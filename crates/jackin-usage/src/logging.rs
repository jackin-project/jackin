// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Telemetry-level state and panic handling for the in-container multiplexer.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static PANIC_HOOK_INSTALLED: OnceLock<()> = OnceLock::new();
static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);

/// `true` when the effective capsule telemetry level is debug or trace.
/// Captured once at `init()` time so per-line emit
/// paths can branch on it cheaply. Verbose governed DEBUG events callers compile
/// the format args lazily and skip the file write when `false` —
/// production runs stay quiet, `--debug` runs get the firehose.
pub fn debug_enabled() -> bool {
    DEBUG_ENABLED.load(Ordering::Relaxed)
}

pub fn init() {
    // One shared resolver owns JACKIN_TELEMETRY_LEVEL and config precedence.
    let level = jackin_diagnostics::telemetry_level(false);
    let debug = matches!(
        level,
        jackin_diagnostics::TelemetryLevel::Debug | jackin_diagnostics::TelemetryLevel::Trace
    );
    DEBUG_ENABLED.store(debug, Ordering::Relaxed);

    let () = PANIC_HOOK_INSTALLED.get_or_init(|| {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            jackin_diagnostics::emit_panic_crash(info, "capsule panic");
            crate::telemetry::shutdown();
            default_hook(info);
        }));
    });
}
