use std::collections::HashSet;

use super::should_snapshot_instance;

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
