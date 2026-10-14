// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    DockerSecurityProfile, EffectiveGrants, MINIMUM_CAPABILITIES, drops_all_caps, normalize_cap,
};

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

pub(crate) const TMPFS_PATHS_MINIMAL: &[&str] = &[
    "/tmp",
    "/run",
    "/var/run",
    "/home/agent/.zsh_sessions",
    "/home/agent/.zsh_history",
    "/home/agent/.bash_history",
];

pub(crate) const TMPFS_PATHS_HARDENED_EXTRA: &[&str] = &[
    "/var/tmp",
    "/var/cache",
    "/var/log",
    "/var/lib/apt/lists",
    "/var/cache/apt/archives",
    "/var/lib/dpkg",
    "/home/agent/.cache",
];
