// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[cfg(unix)]
mod unix;

#[cfg(target_os = "macos")]
mod macos_wait;

#[cfg(unix)]
mod secure_install;

#[cfg(unix)]
mod secure_source;

#[cfg(unix)]
pub use unix::current_dir;

#[cfg(unix)]
pub use secure_install::{CheckedInstallFile, CheckedInstallRoot, InstallFileKind};

#[cfg(unix)]
pub use secure_source::{CheckedSourceFile, CheckedSourceRoot};

#[cfg(target_os = "macos")]
pub use macos_wait::{
    CaughtSigchldDiagnosticGuard, SigchldActionSnapshot, SigchldDisposition,
    TokioSigchldEligibility, current_sigchld_action, peek_exited_child, peek_specific_exited_child,
    tokio_sigchld_eligibility,
};

#[cfg(test)]
mod tests;
