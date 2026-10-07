// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Resource and capability docker flags.

use super::{
    DockerSecurityProfile, EffectiveGrants, MINIMUM_CAPABILITIES, drops_all_caps, normalize_cap,
};

/// Emit resource limit Docker CLI flags from resolved grants.
/// Returns an owned `Vec<String>` of alternating flag/value pairs ready to
/// extend a `Vec<&str>` `run_args` via `.iter().map(String::as_str)`.
pub fn resource_flags(grants: &EffectiveGrants) -> Vec<String> {
    let mut flags = Vec::new();
    if let Some(bytes) = grants.memory_bytes {
        flags.push("--memory".to_owned());
        flags.push(bytes.to_string());
    }
    if let Some(bytes) = grants.memory_reservation_bytes {
        flags.push("--memory-reservation".to_owned());
        flags.push(bytes.to_string());
    }
    if let Some(cpus) = grants.cpus {
        flags.push("--cpus".to_owned());
        flags.push(cpus.to_string());
    }
    if let Some(pids) = grants.pids {
        flags.push("--pids-limit".to_owned());
        flags.push(pids.to_string());
    }
    if let Some(nofile) = grants.nofile {
        flags.push("--ulimit".to_owned());
        flags.push(format!("nofile={nofile}:{nofile}"));
    }
    flags
}

/// Emit capability flags for the profile's base cap set.
///
/// Only meaningful when `grants.dind == DindGrant::None` — with `DinD` active,
/// capability drops are circumventable via `docker run --privileged` against
/// the sidecar. Returns empty when the profile uses Docker's default cap set
/// (`standard`/`compat`) to avoid redundant flags.
pub fn capability_flags(profile: DockerSecurityProfile, extra_caps: &[String]) -> Vec<String> {
    let drops_all = drops_all_caps(profile);
    if !drops_all && extra_caps.is_empty() {
        return Vec::new();
    }
    let mut flags = Vec::new();
    if drops_all {
        flags.push("--cap-drop=ALL".to_owned());
        for cap in MINIMUM_CAPABILITIES {
            flags.push("--cap-add".to_owned());
            flags.push(cap.to_string());
        }
    }
    for cap in extra_caps {
        let normalized = normalize_cap(cap);
        flags.push("--cap-add".to_owned());
        flags.push(normalized);
    }
    flags
}

/// Emit `--read-only` and `--tmpfs` flags for profiles that use a read-only
/// root filesystem.
///
/// The tmpfs preset covers paths that tooling writes to at runtime but that
/// are NOT already bind-mounted (`/jackin/run`, `/jackin/state`, and agent
/// home credential dirs are bind mounts and already writable regardless of
/// `--read-only`).
/// Tmpfs paths required for ALL read-only root profiles (hardened and locked).
/// These are the minimum paths needed for any agent session to start — shell
/// session state and the POSIX `/tmp` requirement.
pub(crate) const TMPFS_PATHS_MINIMAL: &[&str] = &[
    "/tmp",
    "/run",
    "/var/run",
    // Shell history and session state — must be writable for the shell to start.
    "/home/agent/.zsh_sessions",
    "/home/agent/.zsh_history",
    "/home/agent/.bash_history",
];

/// Additional tmpfs paths needed by `hardened` profile (roles that do package
/// management at build time but not at runtime). Under `locked`, these are
/// omitted because `apt install` is explicitly unsupported.
pub(crate) const TMPFS_PATHS_HARDENED_EXTRA: &[&str] = &[
    "/var/tmp",
    "/var/cache",
    "/var/log",
    "/var/lib/apt/lists",
    "/var/cache/apt/archives",
    "/var/lib/dpkg",
    "/home/agent/.cache",
];
