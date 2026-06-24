//! Tests for `restore`.
use super::*;
use crate::instance::{DockerResources, NewInstanceManifest};
use jackin_core::isolation::MountIsolation;
use jackin_core::isolation_record::{CleanupStatus, IsolationRecord};
use tempfile::tempdir;

fn manifest_for(container: &str) -> InstanceManifest {
    InstanceManifest::new(NewInstanceManifest {
        container_base: container,
        workspace_name: Some("ws"),
        workspace_label: "ws",
        workdir: "/ws",
        host_workdir_fingerprint: "sha256:test",
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::agent::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk_agent-smith",
        docker: DockerResources {
            role_container: container.to_owned(),
            dind_container: format!("{container}-dind"),
            network: format!("{container}-net"),
            certs_volume: format!("{container}-dind-certs"),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    })
}

fn record_with(container: &str, worktree_path: &str, status: CleanupStatus) -> IsolationRecord {
    IsolationRecord {
        workspace: "ws".to_owned(),
        mount_dst: "/ws".to_owned(),
        original_src: "/host/ws".to_owned(),
        isolation: MountIsolation::Worktree,
        worktree_path: worktree_path.to_owned(),
        scratch_branch: "jackin/scratch".to_owned(),
        base_commit: "0".repeat(40),
        selector_key: "agent-smith".to_owned(),
        container_name: container.to_owned(),
        cleanup_status: status,
    }
}

fn is_dirty_for(status: CleanupStatus) -> bool {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container = "jk-k7p9m2xq-agentsmith";
    let state_dir = paths.data_dir.join(container);
    std::fs::create_dir_all(&state_dir).unwrap();
    // worktree_path points at a non-git dir; worktree_inspect degrades to an
    // empty file list, which is fine — this asserts the is_dirty derivation.
    let wt = temp.path().join("wt");
    std::fs::create_dir_all(&wt).unwrap();
    crate::isolation::state::write_records(
        &state_dir,
        &[record_with(container, wt.to_str().unwrap(), status)],
    )
    .unwrap();

    launch_candidate_for_manifest(&paths, &manifest_for(container), "label".to_owned()).is_dirty
}

#[test]
fn launch_candidate_is_dirty_for_preserved_records() {
    assert!(
        is_dirty_for(CleanupStatus::PreservedDirty),
        "PreservedDirty → dirty candidate (requires delete confirmation)"
    );
    assert!(
        is_dirty_for(CleanupStatus::PreservedUnpushed),
        "PreservedUnpushed → dirty candidate"
    );
    assert!(
        !is_dirty_for(CleanupStatus::Active),
        "Active record → not dirty"
    );
}

#[test]
fn launch_candidate_is_not_dirty_with_no_records() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container = "jk-a1b2c3d4-agentsmith";
    std::fs::create_dir_all(paths.data_dir.join(container)).unwrap();

    let candidate =
        launch_candidate_for_manifest(&paths, &manifest_for(container), "label".to_owned());
    assert!(
        !candidate.is_dirty,
        "no isolation records → clean candidate"
    );
    assert!(candidate.inspect.is_empty());
    assert_eq!(candidate.label, "label");
}
