// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{DindGrant, EffectiveGrants, normalize_cap};
use jackin_core::ContainerFullInspection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkEnforcementLevel {
    IpPort,
    Hostname,
}

impl std::fmt::Display for NetworkEnforcementLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IpPort => write!(f, "ip-and-port"),
            Self::Hostname => write!(f, "hostname"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskPolicy {
    MonitorOnly,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorkflowGrantError {
    HardDiskQuotaRequested {
        bytes: u64,
    },
    UnsupportedNetworkEnforcement {
        requested: String,
        enforceable: NetworkEnforcementLevel,
    },
    HostNamespaceRequested {
        field: &'static str,
    },
}

impl std::fmt::Display for WorkflowGrantError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HardDiskQuotaRequested { bytes } => write!(
                f,
                "workflow grant requests a hard disk quota of {bytes} bytes: \
                 rejected; the MVP profile provides disk admission checks, a \
                 disk-use monitor, and bounded artifact sizes, not a hard quota"
            ),
            Self::UnsupportedNetworkEnforcement {
                requested,
                enforceable,
            } => write!(
                f,
                "workflow grant requests network enforcement {requested:?}, \
                 stronger than this profile can enforce ({enforceable}); \
                 refusing rather than silently downgrading"
            ),
            Self::HostNamespaceRequested { field } => write!(
                f,
                "workflow grant requests host namespace mode via {field}: \
                 the workflow path never permits host namespaces"
            ),
        }
    }
}

impl std::error::Error for WorkflowGrantError {}

#[derive(Debug, Clone, Default)]
pub struct WorkflowGrant {
    pub cpu_max: Option<f64>,
    pub memory_max_bytes: Option<u64>,
    pub memory_reservation_max_bytes: Option<u64>,
    pub pids_max: Option<i64>,
    pub nofile_max: Option<u64>,
    pub deny_caps: Vec<String>,
    pub deadline_secs: Option<u64>,
    pub network_enforcement: Option<NetworkEnforcementLevel>,
    pub disk_quota_bytes: Option<u64>,
    pub require_ip_port_enforcement: bool,
}

fn lower_max<T: PartialOrd>(slot: &mut Option<T>, cap: Option<T>) {
    if let Some(cap) = cap {
        match slot {
            Some(existing) if *existing <= cap => {}
            _ => *slot = Some(cap),
        }
    }
}

pub fn apply_workflow_grant(
    mut base: EffectiveGrants,
    grant: &WorkflowGrant,
) -> Result<EffectiveGrants, WorkflowGrantError> {
    if let Some(bytes) = grant.disk_quota_bytes {
        return Err(WorkflowGrantError::HardDiskQuotaRequested { bytes });
    }
    lower_max(&mut base.cpus, grant.cpu_max);
    lower_max(&mut base.memory_bytes, grant.memory_max_bytes);
    lower_max(
        &mut base.memory_reservation_bytes,
        grant.memory_reservation_max_bytes,
    );
    lower_max(&mut base.pids, grant.pids_max);
    lower_max(&mut base.nofile, grant.nofile_max);

    base.dind = DindGrant::None;
    base.sudo = false;
    base.no_new_privileges = true;
    if !grant.deny_caps.is_empty() {
        base.capabilities_add.retain(|cap| {
            !grant
                .deny_caps
                .iter()
                .any(|denied| normalize_cap(denied) == *cap)
        });
    }
    Ok(base)
}

pub fn admit_network_enforcement(
    ip_port_enforceable: bool,
    require_ip_port: bool,
    fallback: NetworkEnforcementLevel,
) -> Result<NetworkEnforcementLevel, WorkflowGrantError> {
    if require_ip_port && !ip_port_enforceable {
        return Err(WorkflowGrantError::UnsupportedNetworkEnforcement {
            requested: "ip-and-port".to_owned(),
            enforceable: fallback,
        });
    }
    Ok(if ip_port_enforceable {
        NetworkEnforcementLevel::IpPort
    } else {
        fallback
    })
}

pub fn admit_disk_policy(quota_bytes: Option<u64>) -> Result<DiskPolicy, WorkflowGrantError> {
    match quota_bytes {
        Some(bytes) => Err(WorkflowGrantError::HardDiskQuotaRequested { bytes }),
        None => Ok(DiskPolicy::MonitorOnly),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantMismatch {
    pub field: &'static str,
    pub expected: String,
    pub actual: String,
}

impl std::fmt::Display for GrantMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "grant mismatch on {}: expected {}, inspected {}",
            self.field, self.expected, self.actual
        )
    }
}

fn mismatch(field: &'static str, expected: String, actual: String) -> GrantMismatch {
    GrantMismatch {
        field,
        expected,
        actual,
    }
}

pub fn compare_grant_vs_actual(
    grant: &EffectiveGrants,
    workflow: &WorkflowGrant,
    actual: &ContainerFullInspection,
) -> Vec<GrantMismatch> {
    let mut out = Vec::new();
    if actual.privileged {
        out.push(mismatch(
            "privileged",
            "false".to_owned(),
            "true".to_owned(),
        ));
    }
    if grant.dind != DindGrant::None {
        out.push(mismatch(
            "dind",
            DindGrant::None.to_string(),
            grant.dind.to_string(),
        ));
    }
    if grant.sudo {
        out.push(mismatch("sudo", "false".to_owned(), "true".to_owned()));
    }
    if grant.no_new_privileges && !actual.no_new_privileges {
        out.push(mismatch(
            "no_new_privileges",
            "true".to_owned(),
            "false".to_owned(),
        ));
    }
    if actual.user != grant.user {
        out.push(mismatch("user", grant.user.clone(), actual.user.clone()));
    }
    match (grant.memory_bytes, actual.memory_bytes) {
        (Some(want), Some(got)) if want != got => {
            out.push(mismatch("memory_bytes", want.to_string(), got.to_string()));
        }
        (Some(want), None) => out.push(mismatch(
            "memory_bytes",
            want.to_string(),
            "unlimited".to_owned(),
        )),
        (None, Some(got)) => out.push(mismatch(
            "memory_bytes",
            "unlimited".to_owned(),
            got.to_string(),
        )),
        _ => {}
    }
    match (grant.cpus, actual.nano_cpus) {
        (Some(want), Some(got)) => {
            let want_nano = (want * 1_000_000_000.0).round() as i64;
            if want_nano != got {
                out.push(mismatch(
                    "cpus",
                    want.to_string(),
                    format!("nano_cpus={got}"),
                ));
            }
        }
        (Some(want), None) => out.push(mismatch("cpus", want.to_string(), "unlimited".to_owned())),
        (None, Some(got)) => out.push(mismatch(
            "cpus",
            "unlimited".to_owned(),
            format!("nano_cpus={got}"),
        )),
        _ => {}
    }
    match (grant.pids, actual.pids_limit) {
        (Some(want), Some(got)) if want != got => {
            out.push(mismatch("pids", want.to_string(), got.to_string()));
        }
        (Some(want), None) => out.push(mismatch("pids", want.to_string(), "unlimited".to_owned())),
        (None, Some(got)) => out.push(mismatch("pids", "unlimited".to_owned(), got.to_string())),
        _ => {}
    }
    for denied in &workflow.deny_caps {
        let normalized = normalize_cap(denied);
        if actual.cap_add.contains(&normalized) {
            out.push(mismatch(
                "cap_add",
                format!("without {normalized}"),
                format!("has {normalized}"),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests;
