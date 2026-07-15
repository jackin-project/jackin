// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Build-log line sink port (D2 in the completed codebase-health track).
//!
//! Defined in the domain layer so infrastructure adapters (`jackin-docker`)
//! can call `push_line` without depending on the presentation layer.
//! `jackin-launch-tui` provides the concrete adapter; `jackin-runtime` injects it.

/// Receives docker-build output lines for live display.
///
/// Architecture invariant: all callers of this trait must belong to
/// `jackin-docker` or lower layers only. The implementation lives in
/// `jackin-launch-tui`.
pub trait BuildLogSink: Send + Sync + std::fmt::Debug {
    /// Append one build-log line for live display.
    fn push_line(&self, line: &str);
}
