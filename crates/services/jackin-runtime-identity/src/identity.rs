// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Best-effort host Git identity for in-container defaults, the host effective
//! UID, and the root identity used for Capsule supervisor commands.

use jackin_core::CommandRunner;

/// Root user/group value for Capsule supervisor lifecycle and control commands.
/// The supervisor loads protected staged account credentials. Production Linux
/// agent and shell PTYs use a wrapper that checks their configured identity,
/// switches to that non-root UID/GID, and installs Landlock before target exec.
pub const CAPSULE_SUPERVISOR_USER: &str = "0:0";

/// Return the host operator's effective UID on Unix, or `None` on other hosts.
#[cfg(unix)]
pub fn host_uid() -> Option<u32> {
    Some(nix::unistd::geteuid().as_raw())
}

/// Return the host operator's effective UID on Unix, or `None` on other hosts.
#[cfg(not(unix))]
pub fn host_uid() -> Option<u32> {
    None
}

/// Host Git name and email used as defaults; unavailable fields are empty.
#[derive(Debug)]
pub struct GitIdentity {
    pub user_name: String,
    pub user_email: String,
}

#[cfg(any(test, feature = "test-support"))]
impl GitIdentity {
    /// Construct a Git identity fixture without invoking the command runner.
    pub fn for_tests(user_name: &str, user_email: &str) -> Self {
        Self {
            user_name: user_name.to_owned(),
            user_email: user_email.to_owned(),
        }
    }
}

pub async fn try_capture(
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

/// Read host Git name/email as best-effort defaults. Command failure or absent
/// configuration leaves the unavailable fields empty.
pub async fn load_git_identity(runner: &mut impl CommandRunner) -> GitIdentity {
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
