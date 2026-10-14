// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{DindGrant, DockerSecurityProfile, EffectiveGrants, NetworkGrant};

pub const fn drops_all_caps(profile: DockerSecurityProfile) -> bool {
    matches!(
        profile,
        DockerSecurityProfile::Hardened | DockerSecurityProfile::Locked
    )
}

pub fn dind_enabled(grants: &EffectiveGrants) -> bool {
    grants.dind != DindGrant::None
}

pub fn network_disabled(grants: &EffectiveGrants) -> bool {
    grants.network == NetworkGrant::None && !dind_enabled(grants)
}

pub fn dind_privileged(grants: &EffectiveGrants) -> bool {
    grants.dind == DindGrant::Privileged
}

pub const DIND_PRIVILEGED_IMAGE: &str = "docker:29-dind";
pub const DIND_ROOTLESS_IMAGE: &str = "docker:29-dind-rootless";

pub const fn dind_image_and_privileged(grant: DindGrant) -> (&'static str, bool) {
    match grant {
        DindGrant::Rootless => (DIND_ROOTLESS_IMAGE, false),
        DindGrant::Privileged | DindGrant::None => (DIND_PRIVILEGED_IMAGE, true),
    }
}

pub fn validate_dind_grant_for_cgroup(
    grant: DindGrant,
    cgroup_version: &str,
) -> Result<(), String> {
    if grant == DindGrant::Rootless && cgroup_version == "v1" {
        return Err(
            "rootless DinD requires cgroup v2 for user-namespace isolation; this host is cgroup v1. \
             Use `dind = \"privileged\"` or run on a cgroup v2 host — jackin❯ will not silently fall \
             back to a privileged sidecar."
                .to_owned(),
        );
    }
    Ok(())
}
