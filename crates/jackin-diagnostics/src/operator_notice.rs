//! Adapter that wires `jackin-diagnostics`' `emit_compact_line` to the
//! `jackin-core::operator_notice::OperatorNoticeSink` port trait. Lets
//! domain crates (L0) emit compact operator-visible lines without
//! taking a direct L0 → L2 dep on `jackin-diagnostics`.

use jackin_core::operator_notice::OperatorNoticeSink;

use crate::logging::emit_compact_line;

struct DiagnosticsOperatorNotice;

impl OperatorNoticeSink for DiagnosticsOperatorNotice {
    fn notice(&self, kind: &str, line: &str) {
        emit_compact_line(kind, line);
    }
}

/// Register `jackin-diagnostics`' `emit_compact_line` as the global
/// `OperatorNoticeSink` impl. Idempotent — safe to call multiple times
/// (the most recent registration wins, matching the "latest sink" model
/// used for sub-process re-spawns in tests). Returns the number of
/// times the sink has been registered so callers can debug double-init.
pub fn install_operator_notice_sink() {
    jackin_core::operator_notice::set_global_sink(Box::new(DiagnosticsOperatorNotice));
}

#[cfg(test)]
mod tests;
