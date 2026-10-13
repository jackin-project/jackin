use std::collections::{HashMap, HashSet};

use super::{SnapshotTransport, apply_snapshot_result, should_snapshot_instance};

fn instance(
    container_base: &str,
    status: jackin_runtime::instance::InstanceStatus,
) -> jackin_runtime::instance::InstanceIndexEntry {
    jackin_runtime::instance::InstanceIndexEntry {
        instance_id: container_base.to_owned(),
        container_base: container_base.to_owned(),
        workspace_name: Some("default".to_owned()),
        workspace_label: "default".to_owned(),
        workdir: "/workspace".to_owned(),
        role_key: "agent-smith".to_owned(),
        agent_runtime: "claude".to_owned(),
        status,
        updated_at: "2026-07-04T00:00:00Z".to_owned(),
    }
}

#[test]
fn snapshot_filter_uses_running_container_set() {
    let running = HashSet::from(["jk-running".to_owned()]);

    assert!(should_snapshot_instance(
        &instance(
            "jk-running",
            jackin_runtime::instance::InstanceStatus::Active
        ),
        Some(&running),
    ));
    assert!(!should_snapshot_instance(
        &instance(
            "jk-stopped",
            jackin_runtime::instance::InstanceStatus::Active
        ),
        Some(&running),
    ));
    assert!(!should_snapshot_instance(
        &instance(
            "jk-clean",
            jackin_runtime::instance::InstanceStatus::CleanExited
        ),
        Some(&running),
    ));
}

#[test]
fn snapshot_filter_preserves_legacy_behavior_without_docker_ps() {
    assert!(should_snapshot_instance(
        &instance(
            "jk-unknown",
            jackin_runtime::instance::InstanceStatus::Running
        ),
        None,
    ));
}

#[test]
fn snapshot_result_records_snapshot_and_transport() {
    let mut snapshots = HashMap::new();
    let mut exec_fallback_seen = false;
    let snapshot = jackin_runtime::runtime::snapshot::InstanceSnapshot {
        tabs: Vec::new(),
        active_tab: 0,
    };

    assert!(!apply_snapshot_result(
        "jk-running".to_owned(),
        Ok((Some(snapshot), SnapshotTransport::DirectSocket)),
        &mut snapshots,
        &mut exec_fallback_seen,
    ));
    assert!(!exec_fallback_seen);
    assert!(snapshots.contains_key("jk-running"));
}

#[test]
fn snapshot_result_records_fallback_without_snapshot() {
    let mut snapshots = HashMap::new();
    let mut exec_fallback_seen = false;

    assert!(!apply_snapshot_result(
        "jk-running".to_owned(),
        Ok((None, SnapshotTransport::DockerExecFallback)),
        &mut snapshots,
        &mut exec_fallback_seen,
    ));
    assert!(exec_fallback_seen);
    assert!(snapshots.is_empty());
}

#[test]
fn snapshot_result_marks_fetch_failure_for_recovery() {
    let mut snapshots = HashMap::new();
    let mut exec_fallback_seen = false;

    assert!(apply_snapshot_result(
        "jk-failed".to_owned(),
        Err(anyhow::anyhow!("snapshot unavailable")),
        &mut snapshots,
        &mut exec_fallback_seen,
    ));
    assert!(!exec_fallback_seen);
    assert!(snapshots.is_empty());
}
