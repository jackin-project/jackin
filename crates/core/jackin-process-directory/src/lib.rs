// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[cfg(unix)]
mod unix;

#[cfg(target_os = "macos")]
mod macos_wait;

#[cfg(unix)]
mod secure_install;

#[cfg(unix)]
pub use unix::current_dir;

/// Pinned descriptor-relative inspection for private installation trees.
///
/// This capability checks the supplied directory and descendants. It does not
/// select or authenticate the external pathname used to obtain the root.
#[cfg(unix)]
pub use secure_install::{CheckedInstallFile, CheckedInstallRoot, InstallFileKind};

/// On macOS, inspect the current process's fixed SIGCHLD action, expose
/// non-consuming direct-child selectors, and provide a diagnostic-only
/// caught-action guard for an exclusively owned fresh worker.
///
/// Callers that share child ownership with a reaper must hold their common
/// ownership coordinate across a child peek, the ownership lookup, and the
/// selected-PID wait. The child selectors do not reap or signal a child.
#[cfg(target_os = "macos")]
pub use macos_wait::{
    CaughtSigchldDiagnosticGuard, SigchldActionSnapshot, SigchldDisposition,
    TokioSigchldEligibility, current_sigchld_action, peek_exited_child, peek_specific_exited_child,
    tokio_sigchld_eligibility,
};

#[cfg(test)]
mod tests;
