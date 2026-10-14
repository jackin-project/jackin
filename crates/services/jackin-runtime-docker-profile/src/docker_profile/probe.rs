// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::DockerSecurityProfile;

pub const fn role_network_internal(profile: DockerSecurityProfile) -> bool {
    matches!(profile, DockerSecurityProfile::Locked)
}

pub fn probe_cgroup_version() -> &'static str {
    if std::path::Path::new("/sys/fs/cgroup/cgroup.controllers").exists() {
        if std::path::Path::new("/sys/fs/cgroup/memory").exists() {
            return "hybrid";
        }
        return "v2";
    }
    if std::path::Path::new("/sys/fs/cgroup").exists() {
        return "v1";
    }

    "v2"
}

pub fn parse_apparmor_from_docker_info(security_options: &str) -> (bool, &'static str) {
    let available = security_options.contains("apparmor");

    let layer = if std::path::Path::new("/usr/bin/sw_vers").exists() {
        "backend-vm"
    } else {
        "host"
    };
    (available, layer)
}

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
