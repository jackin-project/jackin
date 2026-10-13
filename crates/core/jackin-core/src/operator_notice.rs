// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Operator-notice port trait.
//!
//! Domain crates (L0) emit short operator-visible messages
//! (validation warnings, recoverable non-fatal notes) without
//! depending on the L2 diagnostics layer. The trait is defined
//! here so the type system can enforce the no-direct-edge
//! invariant; the actual impl lives in `jackin-diagnostics` and
//! is registered at process init time via [`set_global_sink`].
//!
//! The default state is a no-op sink (anything that runs before
//! the entry layer registers the real impl still compiles and
//! passes, just with no log output — observable behaviour
//! matches what the binary's tests need today, and live
//! invocations register the real sink via
//! `jackin_diagnostics::install_operator_notice_sink` before
//! the CLI starts processing the first command).
//!
//! Architecture Invariant: depends only on `std::sync` and
//! `std::fmt`. No `jackin-*` deps.

use std::sync::{OnceLock, RwLock};

/// Receives short operator-visible notices (warnings, recoverable notes).
pub trait OperatorNoticeSink: Send + Sync + 'static {
    /// Emit one notice line tagged with a short `kind` (e.g. `"warn"`).
    fn notice(&self, kind: &str, line: &str);
}

static GLOBAL_SINK: OnceLock<RwLock<Box<dyn OperatorNoticeSink>>> = OnceLock::new();

fn global() -> &'static RwLock<Box<dyn OperatorNoticeSink>> {
    GLOBAL_SINK.get_or_init(|| RwLock::new(Box::new(NoopSink)))
}

/// Install the process-wide operator-notice sink (typically at startup).
pub fn set_global_sink(sink: Box<dyn OperatorNoticeSink>) {
    let cell = global();
    if let Ok(mut guard) = cell.write() {
        *guard = sink;
    }
}

/// Emit one operator-facing notice line through the global sink.
pub fn emit_compact_line(kind: &str, line: &str) {
    if let Ok(guard) = global().read() {
        (**guard).notice(kind, line);
    }
}

struct NoopSink;
impl OperatorNoticeSink for NoopSink {
    fn notice(&self, _kind: &str, _line: &str) {}
}
