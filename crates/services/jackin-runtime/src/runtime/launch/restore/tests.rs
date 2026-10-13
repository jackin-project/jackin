// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `restore`.
use super::*;
use crate::instance::{DockerResources, NewInstanceManifest};
use jackin_core::MountIsolation;
use jackin_core::{CleanupStatus, IsolationRecord};
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
        agent_runtime: jackin_core::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk_agent-smith",
        docker: DockerResources {
            role_container: container.to_owned(),
            dind_container: Some(format!("{container}-dind")),
            network: format!("{container}-net"),
            certs_volume: Some(format!("{container}-dind-certs")),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    })
}

fn record_with(container: &str, worktree_path: &str, status: CleanupStatus) -> IsolationRecord {
    IsolationRecord {
        workspace_name: Some(jackin_core::WorkspaceName::parse("ws").unwrap()),
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

    launch_candidate_for_manifest(&paths, &manifest_for(container), |_| "label".to_owned())
        .unwrap()
        .is_dirty
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
        launch_candidate_for_manifest(&paths, &manifest_for(container), |_| "label".to_owned())
            .unwrap();
    assert!(
        !candidate.is_dirty,
        "no isolation records → clean candidate"
    );
    assert!(candidate.inspect.is_empty());
    assert_eq!(candidate.label, "label");
}

#[test]
fn corrupt_isolation_cannot_become_clean_restore_candidate() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container = "jk-a1b2c3d4-agentsmith";
    let state = paths.data_dir.join(container).join(".jackin");
    std::fs::create_dir_all(&state).unwrap();
    let records = state.join("isolation.json");
    std::fs::write(&records, b"{").unwrap();
    let error =
        launch_candidate_for_manifest(&paths, &manifest_for(container), |_| "label".to_owned())
            .unwrap_err();
    assert!(format!("{error:#}").contains("cannot establish restore isolation state"));
    assert_eq!(std::fs::read(&records).unwrap(), b"{");
}

#[test]
fn corrupt_related_isolation_aborts_before_restore_dialog_or_status_mutation() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = manifest_for("jk-a1b2c3d4-agentsmith");
    let state = paths.data_dir.join(&manifest.container_base);
    manifest.write(&state).unwrap();
    let original = std::fs::read(state.join(".jackin/instance.json")).unwrap();
    std::fs::write(state.join(".jackin/isolation.json"), b"{").unwrap();
    let related = RelatedRestoreCandidate {
        manifest,
        docker_state: ContainerState::NotFound,
    };
    let error = present_restore_choice(None, &paths, "ws", "other-role", Vec::new(), &[related])
        .unwrap_err();
    assert!(format!("{error:#}").contains("cannot establish restore isolation state"));
    assert_eq!(
        std::fs::read(state.join(".jackin/instance.json")).unwrap(),
        original
    );
}
