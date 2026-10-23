// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[cfg(unix)]
mod unix;

#[cfg(target_os = "macos")]
mod macos_wait;

#[cfg(unix)]
pub use unix::current_dir;

/// Peek once at one exited direct child without consuming its wait status.
///
/// Callers that share child ownership with a reaper must hold their common
/// ownership coordinate across this call, the ownership lookup, and the
/// selected-PID wait. The function does not reap or signal the returned child.
#[cfg(target_os = "macos")]
pub use macos_wait::{peek_exited_child, peek_specific_exited_child};

#[cfg(test)]
mod tests;
