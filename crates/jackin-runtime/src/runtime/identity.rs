//! Capture host git user.name/email for in-container git defaults, and the
//! host operator's UID/GID for the runtime `docker run --user` mapping.
//!
//! All reads are best-effort: missing git config or id failures produce empty
//! strings rather than hard errors.

use jackin_core::CommandRunner;

/// `--user` value that runs the in-container process as the host operator's
/// UID and primary GID (`<uid>:<gid>`).
///
/// The host operator is this jackin process's own effective UID — it creates
/// every bind-mount source under `~/.jackin`, so matching it makes host-owned
/// mounts transparently readable/writable inside the container. The derived
/// image also bakes this UID into `/home/agent` ownership. Returns `None` on
/// non-unix hosts, where no mapping applies.
///
/// The process also receives supplementary group 0 at `docker run` time. The
/// image's `/home/agent` tree is normalized to host-UID ownership and group 0
/// write at build time, so a host-identity process can use image-baked home
/// paths and perform owner-only syscalls such as chmod(2). Matching `agent`
/// passwd/group entries for the host UID/GID are supplied at runtime via
/// `libnss-extrausers` so
/// `getpwuid`/`$HOME` resolve correctly.
///
/// Security tradeoff (the agent is untrusted code): supplementary group 0 lets
/// the in-container process read/write image paths that are group-`root` and
/// group-readable/writable. This is acceptable only because the in-container
/// privilege boundary is the container itself, not the `agent` user — the
/// agent is already free to run arbitrary code as its own UID, and the
/// construct image ships no group-0-writable path that grants escalation *out*
/// of the container (no setuid-root binary is left group-0-writable, and the
/// docker socket is never mounted into a role container). The host side uses
/// the host UID/GID as primary identity, so files created in bind mounts match
/// the operator's normal access model.
#[cfg(unix)]
pub(crate) fn host_run_as_user() -> Option<String> {
    Some(format!(
        "{}:{}",
        nix::unistd::geteuid().as_raw(),
        nix::unistd::getegid().as_raw()
    ))
}

#[cfg(not(unix))]
pub(crate) fn host_run_as_user() -> Option<String> {
    None
}

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

/// The host operator's effective primary GID, used to run the container and to
/// build runtime `libnss-extrausers` entries. `None` on non-unix hosts.
#[cfg(unix)]
pub(crate) fn host_gid() -> Option<u32> {
    Some(nix::unistd::getegid().as_raw())
}

#[cfg(not(unix))]
pub(crate) fn host_gid() -> Option<u32> {
    None
}

pub(super) struct GitIdentity {
    pub(super) user_name: String,
    pub(super) user_email: String,
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
    jackin_diagnostics::active_timing_started("identity", "git_user_name", None);
    let user_name = try_capture(runner, "git", &["config", "user.name"])
        .await
        .unwrap_or_default();
    jackin_diagnostics::active_timing_done(
        "identity",
        "git_user_name",
        Some(if user_name.is_empty() {
            "missing"
        } else {
            "present"
        }),
    );

    jackin_diagnostics::active_timing_started("identity", "git_user_email", None);
    let user_email = try_capture(runner, "git", &["config", "user.email"])
        .await
        .unwrap_or_default();
    jackin_diagnostics::active_timing_done(
        "identity",
        "git_user_email",
        Some(if user_email.is_empty() {
            "missing"
        } else {
            "present"
        }),
    );

    GitIdentity {
        user_name,
        user_email,
    }
}
