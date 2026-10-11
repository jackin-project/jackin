// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Cgroup/apparmor probes and validation.

use super::DockerSecurityProfile;

/// WP2: whether the role's Docker network must be created `internal`.
///
/// `locked` runs on a Docker-internal network so traffic cannot leave the
/// bridge even before the in-container iptables allowlist is installed — a
/// second, daemon-level egress boundary independent of `firewall-apply`. Every
/// other profile uses an ordinary (routable) network.
pub const fn role_network_internal(profile: DockerSecurityProfile) -> bool {
    matches!(profile, DockerSecurityProfile::Locked)
}

// ── Host probes (WP3 observability) ─────────────────────────────────────────

/// Detect the cgroup version on the host that will run containers.
///
/// Returns `"v2"`, `"v1"`, or `"hybrid"`.  The check is synchronous and reads
/// from `/sys/fs/cgroup/` — on Linux this file is always readable; on macOS the
/// Docker engine runs in a Linux VM so the host process checks the VM's cgroup
/// namespace through the Docker socket instead (callers should treat an unknown
/// result as `"v2"` on macOS since Docker Desktop always runs cgroup v2).
pub fn probe_cgroup_version() -> &'static str {
    // cgroup v2 has a unified hierarchy — `cgroup.controllers` exists at root.
    if std::path::Path::new("/sys/fs/cgroup/cgroup.controllers").exists() {
        // Hybrid: also has legacy `/sys/fs/cgroup/memory` mounts.
        if std::path::Path::new("/sys/fs/cgroup/memory").exists() {
            return "hybrid";
        }
        return "v2";
    }
    if std::path::Path::new("/sys/fs/cgroup").exists() {
        return "v1";
    }
    // Not Linux (macOS host with Docker Desktop/OrbStack) — inner VM is v2.
    "v2"
}

/// Parse `AppArmor` availability and layer from `docker info --format '{{.SecurityOptions}}'`.
///
/// Returns `(available, layer)` where `layer` is `"host"` or `"backend-vm"`.
/// `"backend-vm"` is reported when the Docker engine runs in a VM (Docker
/// Desktop / `OrbStack` on macOS) because `AppArmor` in the VM does not protect
/// the host's filesystem — it is a weaker boundary than host-native `AppArmor`.
pub fn parse_apparmor_from_docker_info(security_options: &str) -> (bool, &'static str) {
    let available = security_options.contains("apparmor");
    // On a macOS host (Docker Desktop / OrbStack) the engine runs in a Linux VM,
    // so AppArmor protects the VM but not the host. `/usr/bin/sw_vers` is a
    // macOS-only binary, so its presence flags the backend-VM layer.
    let layer = if std::path::Path::new("/usr/bin/sw_vers").exists() {
        "backend-vm"
    } else {
        "host"
    };
    (available, layer)
}

/// Validate cgroup version against profile requirements. `Err` = unsupported
/// (fail-closed); `Ok(Some(warning))` = supported but degraded, for the caller
/// to surface; `Ok(None)` = fully supported.
///
/// Decision 14: `hardened`/`locked` require cgroup v2; fail-closed on v1.
/// `standard` degrades `memory_reservation` on v1 (warn only). Pure policy — the
/// caller owns telemetry emission.
pub fn validate_cgroup_for_profile(
    profile: DockerSecurityProfile,
    cgroup_version: &str,
) -> Result<Option<&'static str>, String> {
    if cgroup_version == "v1" {
        match profile {
            DockerSecurityProfile::Locked | DockerSecurityProfile::Hardened => {
                return Err(format!(
                    "Docker profile `{profile}` requires cgroup v2 for resource enforcement \
                     (memory limits, pids, cpus), but this host runs cgroup v1. \
                     Upgrade to a cgroup v2 host or use `--docker-profile standard`."
                ));
            }
            DockerSecurityProfile::Standard => {
                return Ok(Some(
                    "cgroup v1 host: memory_reservation will not be enforced under `standard` profile (requires v2)",
                ));
            }
            DockerSecurityProfile::Compat => {}
        }
    }
    Ok(None)
}
