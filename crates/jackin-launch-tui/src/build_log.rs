//! `DiagnosticsBuildLogSink`: adapter from the `BuildLogSink` port to the
//! `jackin-diagnostics` process-global build-log buffer.
//!
//! `jackin-runtime` constructs this and injects it into `RunOptions` before
//! any docker-build invocation, so `jackin-docker`'s `ShellRunner` never
//! imports from presentation crates or `jackin-diagnostics` directly for teeing.

use jackin_core::BuildLogSink;

/// Wraps the process-global `jackin-diagnostics::build_log` buffer.
///
/// A zero-sized type; every `push_line` call forwards directly to the global.
#[derive(Debug)]
pub struct DiagnosticsBuildLogSink;

impl BuildLogSink for DiagnosticsBuildLogSink {
    fn push_line(&self, line: &str) {
        jackin_diagnostics::build_log::push_line(line);
    }
}
