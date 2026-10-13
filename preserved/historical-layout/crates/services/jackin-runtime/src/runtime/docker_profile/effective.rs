// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `EffectiveGrants` resolution and folding.

use super::{
    DindGrant, DockerGrants, DockerSecurityProfile, GB, NetworkGrant, normalize_cap,
    parse_memory_bytes,
};

/// Fully resolved grants for a launch — every dimension has a concrete value,
/// produced by merging the profile's defaults with explicit overrides.
#[derive(Debug, Clone)]
pub struct EffectiveGrants {
    pub network: NetworkGrant,
    /// Merged list of allowed hosts for the `allowlist` network tier.
    pub allowed_hosts: Vec<String>,
    pub dind: DindGrant,
    /// Configured container username. Only `"root"` is load-bearing — it is
    /// compared against `sudo` for the mutually-exclusive check and feeds the
    /// network-enforcement label; the default `"agent"` is an inert sentinel.
    /// The actual `--user` flag is the fixed root capsule-supervisor identity,
    /// not this field. This field remains role grant metadata.
    pub user: String,
    pub sudo: bool,
    pub system_writes: bool,
    /// Parsed hard memory limit in bytes. `None` = no limit.
    pub memory_bytes: Option<u64>,
    /// Parsed soft memory limit in bytes. `None` = no soft limit.
    pub memory_reservation_bytes: Option<u64>,
    pub cpus: Option<f64>,
    pub pids: Option<i64>,
    pub nofile: Option<u64>,
    /// Additional capabilities beyond the profile's base set.
    pub capabilities_add: Vec<String>,
    /// Whether `--security-opt no-new-privileges` is applied to the container.
    /// `true` for `hardened` and `locked`; resolved to `true` for `standard`
    /// (WP-SUDO: sudo off by default, so `no_new_privileges` on) unless an explicit
    /// `sudo = true` grant is active. `false` for `compat` (sudo always on).
    pub no_new_privileges: bool,
}

/// Per-profile base grants. Explicit [`DockerGrants`] are layered on top via
/// [`apply_grants`].
pub fn profile_base_grants(profile: DockerSecurityProfile) -> EffectiveGrants {
    match profile {
        DockerSecurityProfile::Locked => EffectiveGrants {
            network: NetworkGrant::Allowlist,
            allowed_hosts: Vec::new(),
            dind: DindGrant::None,
            user: "agent".to_owned(),
            sudo: false,
            system_writes: false,
            memory_bytes: Some(4 * GB),
            memory_reservation_bytes: Some(3 * GB),
            cpus: Some(2.0),
            pids: Some(512),
            nofile: Some(2048),
            capabilities_add: Vec::new(),
            no_new_privileges: true,
        },
        DockerSecurityProfile::Hardened => EffectiveGrants {
            network: NetworkGrant::Allowlist,
            allowed_hosts: Vec::new(),
            dind: DindGrant::None,
            user: "agent".to_owned(),
            sudo: false,
            system_writes: false,
            memory_bytes: Some(16 * GB),
            memory_reservation_bytes: Some(12 * GB),
            cpus: Some(4.0),
            pids: Some(2048),
            nofile: Some(8192),
            capabilities_add: Vec::new(),
            no_new_privileges: true,
        },
        DockerSecurityProfile::Standard => EffectiveGrants {
            network: NetworkGrant::Open,
            allowed_hosts: Vec::new(),
            // WP4: DinD off by default outside `compat` (Decision 12).
            // Enable via explicit `dind = "rootless"` or `dind = "privileged"` grant.
            dind: DindGrant::None,
            user: "agent".to_owned(),
            // WP-SUDO: sudo is off by default outside `compat` (Decision 11).
            // Enable via explicit `sudo = true` grant.
            sudo: false,
            system_writes: true,
            memory_bytes: Some(16 * GB),
            memory_reservation_bytes: Some(12 * GB),
            cpus: Some(4.0),
            pids: Some(2048),
            nofile: Some(8192),
            capabilities_add: Vec::new(),
            // Resolved to `true` by apply_implicit_grants when sudo is false.
            no_new_privileges: false,
        },
        DockerSecurityProfile::Compat => EffectiveGrants {
            network: NetworkGrant::Open,
            allowed_hosts: Vec::new(),
            dind: DindGrant::Privileged,
            user: "agent".to_owned(),
            sudo: true,
            system_writes: true,
            memory_bytes: None,
            memory_reservation_bytes: None,
            cpus: None,
            pids: None,
            nofile: None,
            capabilities_add: Vec::new(),
            no_new_privileges: false,
        },
    }
}

/// Apply explicit grants on top of profile defaults. Each dimension takes the
/// more capable of the profile default and the explicit override.
///
/// Grants must already have been validated by [`validate_grants`].
/// Raise `slot` to `candidate` when it's larger (or unset). Grants only ever
/// widen a resource ceiling, never lower it.
pub(crate) fn raise_to_max<T: PartialOrd>(slot: &mut Option<T>, candidate: T) {
    match slot {
        Some(existing) if *existing >= candidate => {}
        _ => *slot = Some(candidate),
    }
}

pub fn apply_grants(mut base: EffectiveGrants, grants: &DockerGrants) -> EffectiveGrants {
    if let Some(network) = grants.network
        && network > base.network
    {
        base.network = network;
    }
    if !grants.allowed_hosts.is_empty() {
        base.allowed_hosts
            .extend(grants.allowed_hosts.iter().cloned());
        base.allowed_hosts.sort_unstable();
        base.allowed_hosts.dedup();
    }
    if let Some(dind) = grants.dind
        && dind > base.dind
    {
        base.dind = dind;
    }
    if let Some(ref user) = grants.user {
        base.user.clone_from(user);
    }
    if let Some(sudo) = grants.sudo {
        base.sudo = base.sudo || sudo;
    }
    if let Some(sw) = grants.system_writes {
        base.system_writes = base.system_writes || sw;
    }
    if let Some(ref mem) = grants.memory
        && let Some(bytes) = parse_memory_bytes(mem)
    {
        raise_to_max(&mut base.memory_bytes, bytes);
    }
    if let Some(ref res) = grants.memory_reservation
        && let Some(bytes) = parse_memory_bytes(res)
    {
        raise_to_max(&mut base.memory_reservation_bytes, bytes);
    }
    if let Some(cpus) = grants.cpus {
        raise_to_max(&mut base.cpus, cpus);
    }
    if let Some(pids) = grants.pids {
        raise_to_max(&mut base.pids, pids);
    }
    if let Some(nofile) = grants.nofile {
        raise_to_max(&mut base.nofile, nofile);
    }
    for cap in &grants.capabilities_add {
        let normalized = normalize_cap(cap);
        if !base.capabilities_add.contains(&normalized) {
            base.capabilities_add.push(normalized);
        }
    }
    base
    // No implicit cap injection here: apply_grants() is a pure layering function.
    // Injecting caps based on the merged network value would fire on every source
    // layer, producing duplicates. apply_implicit_grants() fires once post-merge.
}

/// Layer a role manifest's docker grants onto resolved effective grants.
///
/// [`apply_grants`] raises dind/network/hosts/caps (never lowers); then the role
/// may pin dind back to `None` — the **only** down-force in the grant system,
/// which `apply_grants` cannot express. A role that forbids `DinD` must override an
/// otherwise more-capable profile/config/workspace tier.
pub fn fold_role_grants(effective: EffectiveGrants, role: &DockerGrants) -> EffectiveGrants {
    let mut folded = apply_grants(effective, role);
    if role.dind == Some(DindGrant::None) {
        folded.dind = DindGrant::None;
    }
    folded
}
