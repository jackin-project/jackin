// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Capability drops and `dind` grant helpers.

use super::{DindGrant, DockerSecurityProfile, EffectiveGrants, NetworkGrant};

/// Returns `true` when the profile uses `--cap-drop=ALL` + minimum cap set.
/// Centralises the Hardened/Locked check so callers don't re-spell it.
pub const fn drops_all_caps(profile: DockerSecurityProfile) -> bool {
    matches!(
        profile,
        DockerSecurityProfile::Hardened | DockerSecurityProfile::Locked
    )
}

/// Returns `true` when the effective grants enable any `DinD` tier.
pub fn dind_enabled(grants: &EffectiveGrants) -> bool {
    grants.dind != DindGrant::None
}

/// Returns `true` when the container gets no Docker network at all (`--network
/// none`): the `none` tier with no `DinD` sidecar needing the bridge.
pub fn network_disabled(grants: &EffectiveGrants) -> bool {
    grants.network == NetworkGrant::None && !dind_enabled(grants)
}

/// Returns `true` when the effective `DinD` tier is `Privileged`.
pub fn dind_privileged(grants: &EffectiveGrants) -> bool {
    grants.dind == DindGrant::Privileged
}

/// Major-pinned Docker-in-Docker images used by the sidecar.
pub const DIND_PRIVILEGED_IMAGE: &str = "docker:29-dind";
pub const DIND_ROOTLESS_IMAGE: &str = "docker:29-dind-rootless";

/// WP4 Part B: the sidecar image and `--privileged` flag for a `DinD` tier.
///
/// `rootless` runs the rootless `DinD` image in a user namespace with no
/// `--privileged`; `privileged` runs the standard `DinD` image with
/// `--privileged`. `none` never starts a sidecar — it maps to the privileged
/// pair only as an unreachable default (the caller gates on `dind_enabled`).
pub const fn dind_image_and_privileged(grant: DindGrant) -> (&'static str, bool) {
    match grant {
        DindGrant::Rootless => (DIND_ROOTLESS_IMAGE, false),
        DindGrant::Privileged | DindGrant::None => (DIND_PRIVILEGED_IMAGE, true),
    }
}

/// WP4 Part B: rootless `DinD` requires cgroup v2.
///
/// Fails closed on a cgroup-v1 host rather than silently falling back to a
/// privileged sidecar (which would defeat the operator's choice). Other tiers
/// impose no cgroup requirement here (the profile-level cgroup gate is separate,
/// see [`validate_cgroup_for_profile`]).
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
