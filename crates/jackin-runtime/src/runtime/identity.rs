// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capture host git user.name/email for in-container git defaults, and expose
//! the fixed root-supervisor identity used by the capsule boundary.
//!
//! All reads are best-effort: missing git config or id failures produce empty
//! strings rather than hard errors.

use jackin_core::CommandRunner;

/// `--user` value for Docker/container lifecycle commands. PID 1 must be
/// root: it is the only process allowed to read staged credentials and it
/// launches each PTY through the per-instance UID + Landlock wrapper.
pub(crate) const CAPSULE_SUPERVISOR_USER: &str = "0:0";

/// The host operator's effective UID, used to build runtime
/// `libnss-extrausers` entries. `None` on non-unix hosts.
#[cfg(unix)]
pub(crate) fn host_uid() -> Option<u32> {
    Some(nix::unistd::geteuid().as_raw())
}

#[cfg(not(unix))]
pub(crate) fn host_uid() -> Option<u32> {
    None
}

pub(super) struct GitIdentity {
    pub(super) user_name: String,
    pub(super) user_email: String,
}

#[cfg(test)]
impl GitIdentity {
    /// Test fixture constructor for `LaunchCore` boundary harnesses.
    pub(crate) fn for_tests(user_name: &str, user_email: &str) -> Self {
        Self {
            user_name: user_name.to_owned(),
            user_email: user_email.to_owned(),
        }
    }
}

pub(super) async fn try_capture(
    runner: &mut impl CommandRunner,
    program: &str,
    args: &[&str],
) -> Option<String> {
    runner
        .capture(program, args, None)
        .await
        .ok()
        .filter(|s| !s.is_empty())
}

pub(super) async fn load_git_identity(runner: &mut impl CommandRunner) -> GitIdentity {
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Identity,
        "git_identity",
        None,
    );
    let output = try_capture(
        runner,
        "git",
        &["config", "--get-regexp", "^user\\.(name|email)$"],
    )
    .await
    .unwrap_or_default();
    let identity = parse_git_identity_config(&output);
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Identity,
        "git_identity",
        Some(
            match (
                identity.user_name.is_empty(),
                identity.user_email.is_empty(),
            ) {
                (false, false) => "present",
                (true, true) => "missing",
                (true, false) => "missing_name",
                (false, true) => "missing_email",
            },
        ),
    );

    identity
}

fn parse_git_identity_config(output: &str) -> GitIdentity {
    let mut identity = GitIdentity {
        user_name: String::new(),
        user_email: String::new(),
    };
    for line in output.lines() {
        let Some((key, value)) = line.split_once(' ') else {
            continue;
        };
        match key {
            "user.name" => identity.user_name = value.trim().to_owned(),
            "user.email" => identity.user_email = value.trim().to_owned(),
            _ => {}
        }
    }
    identity
}

#[cfg(test)]
mod tests;
