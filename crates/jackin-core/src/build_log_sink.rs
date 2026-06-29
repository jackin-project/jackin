//! Build-log line sink port (D2 in codebase-health-enforcement).
//!
//! Defined in the domain layer so infrastructure adapters (`jackin-docker`)
//! can call `push_line` without depending on the presentation layer.
//! `jackin-launch` provides the concrete adapter; `jackin-runtime` injects it.

/// Receives docker-build output lines for live display.
///
/// Architecture invariant: all callers of this trait must belong to
/// `jackin-docker` or lower layers only. The implementation lives in
/// `jackin-launch`.
pub trait BuildLogSink: Send + Sync + std::fmt::Debug {
    fn push_line(&self, line: &str);
}
