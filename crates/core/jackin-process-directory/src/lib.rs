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
mod temporary_parent;

#[cfg(unix)]
mod running_image;

#[cfg(unix)]
pub use unix::current_dir;

/// Pinned descriptor-relative inspection for private installation trees.
///
/// This capability checks the supplied directory and descendants. It does not
/// select or authenticate the external pathname used to obtain the root.
#[cfg(unix)]
pub use secure_install::{CheckedInstallFile, CheckedInstallRoot, InstallFileKind};

/// Pinned descriptor-relative inspection of a selected source tree.
///
/// These capabilities check current filesystem objects and retain the
/// selected root's parent/name and relative directory edges for rechecking.
/// They do not authenticate source, establish a complete build-input closure,
/// or attest a build or running image.
#[cfg(unix)]
pub use secure_source::{CheckedSourceFile, CheckedSourceRoot};

/// Validate a physical temporary-directory parent against foreign rename authority.
///
/// Root-owned sticky directories with exact mode `01777` are accepted. Other
/// path components must be owned by root or the effective user and must not
/// be writable by group or other. Native ACL inspection applies to each
/// directory descriptor.
#[cfg(unix)]
pub use temporary_parent::validate_temporary_parent;

/// Filesystem-object identity for the image mapped as the current process.
///
/// This capability identifies a regular filesystem object. It does not
/// authenticate executable contents, provenance, permissions, or receipts.
#[cfg(unix)]
pub use running_image::RunningImage;

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
