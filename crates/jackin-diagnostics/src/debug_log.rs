//! Adapter wiring `jackin-diagnostics`' debug-log helpers to the
//! `jackin-core::debug_log::DebugLogSink` port trait.

use jackin_core::debug_log::DebugLogSink;

use crate::logging::{emit_debug_line, is_debug_mode};

struct DiagnosticsDebugLog;

impl DebugLogSink for DiagnosticsDebugLog {
    fn is_active(&self) -> bool {
        is_debug_mode()
    }
    fn log(&self, category: &str, line: &str) {
        emit_debug_line(category, line);
    }
}

pub fn install_debug_log_sink() {
    jackin_core::debug_log::set_global_sink(Box::new(DiagnosticsDebugLog));
}

#[cfg(test)]
mod tests;
