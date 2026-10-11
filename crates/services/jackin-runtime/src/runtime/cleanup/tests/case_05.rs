// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::instance::{InstanceIndex, InstanceStatus};

#[tokio::test]
async fn prune_instances_prunes_purged_tombstone_with_no_state_directory() {
    // Purged tombstones are index-only entries — the state dir is already gone.
    // purge_container_filesystem must tolerate NotFound so the tombstone is
    // removed from the index without error.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container = "jk-k7p9m2xq-agentsmith";
    // Register in the index but do NOT create the state directory.
    let manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
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
    });
    let mut manifest = manifest;
    manifest.mark_status(InstanceStatus::Purged);
    InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();

    let docker = FakeDockerClient::default(); // inspect returns NotFound → allow purge
    let mut runner = FakeRunner::default();
    prune_instances(&paths, &docker, &mut runner).await.unwrap();

    let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
    assert!(
        index
            .instances
            .iter()
            .all(|e| e.container_base != container),
        "tombstone should be cleared from the index"
    );
}

#[tokio::test]
async fn prune_dir_returns_err_with_path_context_on_failure() {
    // Create a file at the path so remove_dir_all fails (ENOTDIR on the
    // path's parent, or similar — exact error is platform-dependent but
    // it will not be NotFound).
    let temp = tempdir().unwrap();
    let blocker = temp.path().join("blocker");
    std::fs::write(&blocker, b"").unwrap();
    let target = blocker.join("child"); // child of a file — cannot exist

    let err = prune_dir(&target, "Test Label", "removing test label", "test label").unwrap_err();

    let msg = err.to_string();
    assert!(msg.contains("failed to remove test label"), "got: {msg}");
    assert!(msg.contains("child"), "got: {msg}");
}

#[tokio::test]
async fn prune_all_instances_removes_data_dir_entirely() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    std::fs::write(paths.data_dir.join("jk-abc123-thearchitect.lock"), b"").unwrap();
    std::fs::write(paths.data_dir.join("caffeinate.lock"), b"").unwrap();
    std::fs::write(paths.data_dir.join("caffeinate.pid"), b"99999").unwrap();
    std::fs::create_dir_all(paths.data_dir.join("the-architect.locks")).unwrap();
    std::fs::write(
        paths
            .data_dir
            .join("the-architect.locks")
            .join("default.repo.lock"),
        b"",
    )
    .unwrap();

    let docker = FakeDockerClient::default(); // exile_all: list_containers returns empty
    let mut runner = FakeRunner::default();
    prune_all_instances(&paths, &docker, &mut runner)
        .await
        .unwrap();

    assert!(
        !paths.data_dir.exists(),
        "data_dir should be completely removed"
    );
}

#[tokio::test]
async fn prune_all_instances_removes_data_dir_when_index_empty() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    std::fs::write(paths.data_dir.join("jk-stale.lock"), b"").unwrap();

    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();
    prune_all_instances(&paths, &docker, &mut runner)
        .await
        .unwrap();

    assert!(!paths.data_dir.exists(), "data_dir removed");
}

#[tokio::test]
async fn prune_jackin_home_removes_home() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(paths.jackin_home.join("leftover")).unwrap();

    prune_jackin_home(&paths).unwrap();

    assert!(!paths.jackin_home.exists(), "jackin_home should be removed");
}

#[tokio::test]
async fn prune_jackin_home_is_ok_when_absent() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    // jackin_home never created — must not panic
    prune_jackin_home(&paths).unwrap();
}

#[tokio::test]
async fn purge_container_state_refuses_path_escape() {
    // A container name escaping the data dir must be refused — never
    // recursively deleted — even though `data_dir.join(name)` resolves.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    let outside = paths.data_dir.parent().unwrap().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let canary = outside.join("canary.txt");
    std::fs::write(&canary, "canary").unwrap();

    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::NotFound])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();
    let error = purge_container_state(&paths, "../outside", &docker, &mut runner)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("escapes")
            || error.to_string().contains("refusing")
            || error.to_string().contains("parent traversal"),
        "got: {error}"
    );
    assert_eq!(std::fs::read_to_string(&canary).unwrap(), "canary");
}

#[tokio::test]
async fn prune_dir_refuses_symlink() {
    // A symlink where the pruned directory should be is refused loudly;
    // both the link and its target survive.
    let temp = tempdir().unwrap();
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let canary = outside.join("canary.txt");
    std::fs::write(&canary, "canary").unwrap();
    let link = temp.path().join("cache");
    std::os::unix::fs::symlink(&outside, &link).unwrap();

    let error = prune_dir(&link, "Cache", "removing cache", "cache").unwrap_err();
    assert!(format!("{error:#}").contains("symlink"), "got: {error:#}");
    assert_eq!(std::fs::read_to_string(&canary).unwrap(), "canary");
    assert!(
        std::fs::symlink_metadata(&link).is_ok_and(|meta| meta.file_type().is_symlink()),
        "refused symlink must be left for the operator"
    );
}

#[tokio::test]
async fn prune_boundaries_reject_coordination_namespace_overlap_before_mutation() {
    let temp = tempdir().unwrap();
    let mut paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.home_dir).unwrap();
    let sentinel = paths.home_dir.join("retain");
    std::fs::write(&sentinel, b"retained").unwrap();
    paths.data_dir = paths.home_dir.clone();
    paths.jackin_home = paths.home_dir.clone();
    paths.roles_dir = paths.home_dir.clone();
    paths.cache_dir = paths.home_dir.clone();
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();
    assert!(
        prune_all_instances(&paths, &docker, &mut runner)
            .await
            .unwrap_err()
            .to_string()
            .contains("coordination")
    );
    assert!(
        prune_jackin_home(&paths)
            .unwrap_err()
            .to_string()
            .contains("coordination")
    );
    assert!(
        prune_roles(&paths)
            .unwrap_err()
            .to_string()
            .contains("coordination")
    );
    assert!(
        prune_cache(&paths)
            .unwrap_err()
            .to_string()
            .contains("coordination")
    );
    assert_eq!(std::fs::read(sentinel).unwrap(), b"retained");
}
