// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    DockerSecurityProfile, EffectiveGrants, TMPFS_PATHS_HARDENED_EXTRA, TMPFS_PATHS_MINIMAL,
};

pub fn tmpfs_paths(profile: DockerSecurityProfile) -> Vec<&'static str> {
    let extra: &[&str] = if matches!(profile, DockerSecurityProfile::Locked) {
        &[]
    } else {
        TMPFS_PATHS_HARDENED_EXTRA
    };
    TMPFS_PATHS_MINIMAL.iter().chain(extra).copied().collect()
}

pub fn readonly_home_env(grants: &EffectiveGrants) -> Vec<String> {
    if grants.system_writes {
        return Vec::new();
    }
    vec!["GIT_CONFIG_GLOBAL=/jackin/state/gitconfig".to_owned()]
}

pub fn readonly_root_flags(
    profile: DockerSecurityProfile,
    grants: &EffectiveGrants,
) -> Vec<String> {
    if grants.system_writes {
        return Vec::new();
    }
    let mut flags = vec!["--read-only".to_owned()];
    for path in tmpfs_paths(profile) {
        flags.push("--tmpfs".to_owned());
        flags.push(format!("{path}:rw,nosuid,nodev"));
    }
    if grants.sudo {
        flags.push("--tmpfs".to_owned());
        flags.push(jackin_core::container_paths::SUDOERS_TMPFS_SPEC.to_owned());
    }
    flags
}
