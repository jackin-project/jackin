// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Status-pipe constants and parsers shared between the runtime and the
//! isolation subsystem.
//!
//! Owns the canonical status-pipe command template the container-side
//! `jackin-capsule status` emits and the `Sessions: <N>` header parser that
//! callers (runtime attach + isolation finalize) both need. Pure logic — no IO
//! — so it lives in `jackin-core` and both crates reach it without depending on
//! each other.

/// Shell command the capsule-side daemon runs to report its session state on
/// the status socket. The unary `test -S` precedes the runtime invocation so
/// the daemon is gated on the socket being present — without the guard a
/// missing socket surfaces as `jackin-capsule status` failing loudly rather
/// than silently producing an empty status body.
pub const JACKIN_STATUS_CMD: &str =
    "test -S /jackin/run/jackin.sock && /jackin/runtime/jackin-capsule status";

/// Parse the `Sessions: <N>` header from `jackin-capsule status` output.
/// Returns `None` if no parsable header line is present — daemon unreachable,
/// torn write, or post-format-drift.
pub fn parse_session_count(output: &str) -> Option<usize> {
    output.lines().find_map(|line| {
        line.trim()
            .strip_prefix("Sessions:")
            .and_then(|value| value.trim().parse().ok())
    })
}

#[cfg(test)]
mod tests;
