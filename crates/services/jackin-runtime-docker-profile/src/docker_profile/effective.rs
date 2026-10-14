// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    DindGrant, DockerGrants, DockerSecurityProfile, GB, NetworkGrant, normalize_cap,
    parse_memory_bytes,
};

#[derive(Debug, Clone)]
pub struct EffectiveGrants {
    pub network: NetworkGrant,

    pub allowed_hosts: Vec<String>,
    pub dind: DindGrant,

    pub user: String,
    pub sudo: bool,
    pub system_writes: bool,

    pub memory_bytes: Option<u64>,

    pub memory_reservation_bytes: Option<u64>,
    pub cpus: Option<f64>,
    pub pids: Option<i64>,
    pub nofile: Option<u64>,

    pub capabilities_add: Vec<String>,

    pub no_new_privileges: bool,
}

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

            dind: DindGrant::None,
            user: "agent".to_owned(),

            sudo: false,
            system_writes: true,
            memory_bytes: Some(16 * GB),
            memory_reservation_bytes: Some(12 * GB),
            cpus: Some(4.0),
            pids: Some(2048),
            nofile: Some(8192),
            capabilities_add: Vec::new(),

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
}

pub fn fold_role_grants(effective: EffectiveGrants, role: &DockerGrants) -> EffectiveGrants {
    let mut folded = apply_grants(effective, role);
    if role.dind == Some(DindGrant::None) {
        folded.dind = DindGrant::None;
    }
    folded
}
