// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::docker_profile::{DindGrant, profile_base_grants};

fn locked_grants() -> EffectiveGrants {
    profile_base_grants(crate::docker_profile::DockerSecurityProfile::Locked)
}

#[test]
fn workflow_grant_only_lowers_and_denies() {
    let base = locked_grants();
    let grant = WorkflowGrant {
        cpu_max: Some(64.0),
        memory_max_bytes: Some(u64::MAX),
        pids_max: Some(1_000_000),
        deny_caps: vec!["NET_ADMIN".to_owned()],
        ..Default::default()
    };
    let applied = apply_workflow_grant(base.clone(), &grant).expect("cap-only grant applies");
    assert_eq!(applied.cpus, base.cpus);
    assert_eq!(applied.memory_bytes, base.memory_bytes);
    assert_eq!(applied.pids, base.pids);
    assert_eq!(applied.dind, DindGrant::None);
    assert!(!applied.sudo);
    assert!(applied.no_new_privileges);
    assert!(
        !applied.capabilities_add.contains(&"NET_ADMIN".to_owned()),
        "denied caps are removed, never added"
    );
}

#[test]
fn workflow_grant_lowers_a_loose_base() {
    let mut base = locked_grants();
    base.cpus = Some(8.0);
    base.pids = Some(4096);
    let grant = WorkflowGrant {
        cpu_max: Some(2.0),
        pids_max: Some(512),
        ..Default::default()
    };
    let applied = apply_workflow_grant(base, &grant).expect("lowering applies");
    assert_eq!(applied.cpus, Some(2.0));
    assert_eq!(applied.pids, Some(512));
}

#[test]
fn workflow_grant_denies_role_privileges() {
    let mut base = locked_grants();
    base.dind = DindGrant::Privileged;
    base.sudo = true;
    base.no_new_privileges = false;
    base.capabilities_add.push("SYS_PTRACE".to_owned());
    let grant = WorkflowGrant {
        deny_caps: vec!["SYS_PTRACE".to_owned()],
        ..Default::default()
    };
    let applied = apply_workflow_grant(base, &grant).expect("deny applies");
    assert_eq!(applied.dind, DindGrant::None);
    assert!(!applied.sudo);
    assert!(applied.no_new_privileges);
    assert!(applied.capabilities_add.is_empty());
}

#[test]
fn workflow_grant_rejects_hard_disk_quota() {
    let base = locked_grants();
    let grant = WorkflowGrant {
        disk_quota_bytes: Some(10_000_000_000),
        ..Default::default()
    };
    assert!(matches!(
        apply_workflow_grant(base, &grant),
        Err(WorkflowGrantError::HardDiskQuotaRequested { .. })
    ));
    assert!(matches!(
        admit_disk_policy(Some(1)),
        Err(WorkflowGrantError::HardDiskQuotaRequested { .. })
    ));
    assert_eq!(admit_disk_policy(None).unwrap(), DiskPolicy::MonitorOnly);
}

#[test]
fn workflow_grant_rejects_unenforceable_network_rule() {
    let err = admit_network_enforcement(false, true, NetworkEnforcementLevel::Hostname)
        .expect_err("stronger rule than the profile enforces is rejected");
    assert!(matches!(
        err,
        WorkflowGrantError::UnsupportedNetworkEnforcement { .. }
    ));
    assert_eq!(
        admit_network_enforcement(true, true, NetworkEnforcementLevel::Hostname).unwrap(),
        NetworkEnforcementLevel::IpPort
    );
    assert_eq!(
        admit_network_enforcement(false, false, NetworkEnforcementLevel::Hostname).unwrap(),
        NetworkEnforcementLevel::Hostname
    );
}

fn matching_inspection(grants: &EffectiveGrants) -> ContainerFullInspection {
    ContainerFullInspection {
        user: grants.user.clone(),
        no_new_privileges: grants.no_new_privileges,
        memory_bytes: grants.memory_bytes,
        nano_cpus: grants.cpus.map(|c| (c * 1_000_000_000.0).round() as i64),
        pids_limit: grants.pids,
        network_mode: "none".to_owned(),
        ..Default::default()
    }
}

#[test]
fn grant_vs_actual_match_passes() {
    let grants = apply_workflow_grant(locked_grants(), &WorkflowGrant::default()).unwrap();
    // Locked base carries NET_ADMIN/NET_RAW implicit caps; the comparison
    // only checks workflow-denied caps, so a clean readback matches.
    let actual = matching_inspection(&grants);
    let mismatches = compare_grant_vs_actual(&grants, &WorkflowGrant::default(), &actual);
    assert!(
        mismatches.is_empty(),
        "unexpected mismatches: {mismatches:?}"
    );
}

#[test]
fn grant_vs_actual_mismatch_stops_launch() {
    let grants = apply_workflow_grant(locked_grants(), &WorkflowGrant::default()).unwrap();
    let mut actual = matching_inspection(&grants);
    actual.privileged = true;
    actual.memory_bytes = Some(1);
    let mismatches = compare_grant_vs_actual(&grants, &WorkflowGrant::default(), &actual);
    assert!(mismatches.iter().any(|m| m.field == "privileged"));
    assert!(mismatches.iter().any(|m| m.field == "memory_bytes"));

    let mut actual = matching_inspection(&grants);
    actual.user = "root".to_owned();
    let mismatches = compare_grant_vs_actual(&grants, &WorkflowGrant::default(), &actual);
    assert!(mismatches.iter().any(|m| m.field == "user"));

    let workflow = WorkflowGrant {
        deny_caps: vec!["NET_ADMIN".to_owned()],
        ..Default::default()
    };
    let mut actual = matching_inspection(&grants);
    actual.cap_add.push("NET_ADMIN".to_owned());
    let mismatches = compare_grant_vs_actual(&grants, &workflow, &actual);
    assert!(mismatches.iter().any(|m| m.field == "cap_add"));
}

#[test]
fn grant_vs_actual_allows_stricter_actual() {
    let mut base = locked_grants();
    base.no_new_privileges = false;
    let grants = apply_workflow_grant(base, &WorkflowGrant::default()).unwrap();
    assert!(grants.no_new_privileges);
    let actual = matching_inspection(&grants);
    let mismatches = compare_grant_vs_actual(&grants, &WorkflowGrant::default(), &actual);
    assert!(
        mismatches.is_empty(),
        "unexpected mismatches: {mismatches:?}"
    );
}
