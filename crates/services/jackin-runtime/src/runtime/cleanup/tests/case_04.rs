// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn gc_does_not_panic_when_list_role_names_fails_in_orphaned_networks() {
    // Network ls succeeds (non-empty), but the docker ps to list running roles fails.
    // gc_orphaned_networks must emit a warning and return without calling network rm.
    let mut net_labels = HashMap::new();
    net_labels.insert(LABEL_ROLE_KEY.to_owned(), "jk-agent-smith".to_owned());
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([
            vec![], // collect_labeled_dind: no DinD
                    // list_role_names call inside gc_orphaned_networks will fail via fail_with
        ])),
        list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![NetworkRow {
            name: "jk-agent-smith-net".to_owned(),
            labels: net_labels,
        }]])),
        fail_with: vec![(
            LABEL_KIND_ROLE.to_owned(),
            "Error response from daemon: socket timeout".to_owned(),
        )],
        ..Default::default()
    };

    gc_orphaned_resources(&gc_test_paths(), &docker).await; // must not panic

    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker network rm"))
    );
}

#[tokio::test]
async fn prune_dir_removes_existing_directory() {
    let temp = tempdir().unwrap();
    let target = temp.path().join("cache");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("file.txt"), b"data").unwrap();

    prune_dir(&target, "Cache", "removing cache", "cache").unwrap();

    assert!(!target.exists());
}

#[tokio::test]
async fn prune_dir_is_ok_when_directory_absent() {
    let temp = tempdir().unwrap();
    let target = temp.path().join("cache");

    prune_dir(&target, "Cache", "removing cache", "cache").unwrap();
}

#[tokio::test]
async fn prune_instances_removes_terminal_statuses_only() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let prunable = "jk-k7p9m2xq-agentsmith";
    let kept = "jk-a1b2c3d4-agentsmith";
    make_instance_at(&paths, prunable, InstanceStatus::CleanExited);
    make_instance_at(&paths, kept, InstanceStatus::Crashed);

    let docker = FakeDockerClient::default(); // inspect returns NotFound → allow purge
    let mut runner = FakeRunner::default();
    prune_instances(&paths, &docker, &mut runner).await.unwrap();

    assert!(!paths.data_dir.join(prunable).exists());
    assert!(paths.data_dir.join(kept).exists());
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
    assert!(index.instances.iter().all(|e| e.container_base != prunable));
    assert!(index.instances.iter().any(|e| e.container_base == kept));
}

#[tokio::test]
async fn prune_instances_skips_when_docker_resources_present() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container = "jk-k7p9m2xq-agentsmith";
    make_instance_at(&paths, container, InstanceStatus::CleanExited);

    // inspect_queue returns Running → container still exists → skip purge.
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Running])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();
    prune_instances(&paths, &docker, &mut runner).await.unwrap();

    assert!(paths.data_dir.join(container).exists());
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
    assert!(
        index
            .instances
            .iter()
            .any(|e| e.container_base == container)
    );
}

#[tokio::test]
async fn prune_instances_is_ok_when_data_dir_absent() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();
    prune_instances(&paths, &docker, &mut runner).await.unwrap();
}

#[tokio::test]
async fn prune_instances_reconciles_stale_active_to_crashed() {
    // D9: an Active row whose container is gone (crash mid-session) must become
    // a Crashed restore candidate, not vanish and not stay falsely Active.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container = "jk-k7p9m2xq-agentsmith";
    make_instance_at(&paths, container, InstanceStatus::Active);

    let docker = FakeDockerClient::default(); // inspect → NotFound
    let mut runner = FakeRunner::default();
    prune_instances(&paths, &docker, &mut runner).await.unwrap();

    // Row survives (Crashed is not prunable) and is now Crashed in both surfaces.
    assert!(paths.data_dir.join(container).exists());
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
    let entry = index
        .instances
        .iter()
        .find(|e| e.container_base == container)
        .expect("row retained");
    assert_eq!(entry.status, InstanceStatus::Crashed);
    let manifest =
        InstanceManifest::read_optional_lossy(&paths.data_dir.join(container)).expect("manifest");
    assert_eq!(manifest.status, InstanceStatus::Crashed);
}

#[tokio::test]
async fn prune_instances_preserves_held_and_idle_coordination_inodes() {
    use fs4::FileExt;
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    let held = coordination::open_lock(&paths, "name-jk-held").unwrap();
    let idle = coordination::open_lock(&paths, "name-jk-idle").unwrap();
    FileExt::try_lock(&held).unwrap();
    FileExt::try_lock(&idle).unwrap();
    FileExt::unlock(&idle).unwrap();
    let idle_path = coordination::root(&paths)
        .unwrap()
        .join("name-jk-idle.lock");
    let held_path = coordination::root(&paths)
        .unwrap()
        .join("name-jk-held.lock");
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt as _;
        let held = held.metadata().unwrap();
        let idle = idle.metadata().unwrap();
        ((held.dev(), held.ino()), (idle.dev(), idle.ino()))
    };
    drop(idle);
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();
    prune_instances(&paths, &docker, &mut runner).await.unwrap();
    assert!(held_path.exists(), "held coordination inode must persist");
    assert!(idle_path.exists(), "idle coordination inode must persist");
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        let held = std::fs::metadata(held_path).unwrap();
        let idle = std::fs::metadata(idle_path).unwrap();
        assert_eq!(
            ((held.dev(), held.ino()), (idle.dev(), idle.ino())),
            identity
        );
    }
    FileExt::unlock(&held).unwrap();
}

#[tokio::test]
async fn prune_images_skips_images_in_use_by_role_containers() {
    // Image listed, but a role container has jackin.image label pointing to it.
    let mut image_labels = HashMap::new();
    image_labels.insert(LABEL_IMAGE_KEY.to_owned(), "jk_agent-smith".to_owned());
    let docker = FakeDockerClient {
        list_image_tags_queue: std::cell::RefCell::new(VecDeque::from([vec![
            "jk_agent-smith:latest".to_owned(),
        ]])),
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![ContainerRow {
            name: "jk-foo".to_owned(),
            id: "container-id".to_owned(),
            labels: image_labels,
        }]])),
        ..Default::default()
    };

    prune_images(&docker).await.unwrap();

    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rmi"))
    );
}

#[tokio::test]
async fn prune_images_counts_rmi_in_use_error_as_skipped_not_failed() {
    // Image passes the pre-filter (not in the in_use set from list_containers)
    // but remove_image returns InUse. prune_images must still return Ok.
    let docker = FakeDockerClient {
        list_image_tags_queue: std::cell::RefCell::new(VecDeque::from([vec![
            "jk_agent-smith:latest".to_owned(),
        ]])),
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // no containers in index
        remove_image_queue: std::cell::RefCell::new(VecDeque::from([RemoveImageOutcome::InUse])),
        ..Default::default()
    };

    prune_images(&docker).await.unwrap();

    // rmi was attempted (image was not in the pre-filter set)
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rmi jk_agent-smith:latest"))
    );
}

#[tokio::test]
async fn prune_images_removes_images_not_in_use() {
    let docker = FakeDockerClient {
        list_image_tags_queue: std::cell::RefCell::new(VecDeque::from([vec![
            "jk_agent-smith:latest".to_owned(),
        ]])),
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
        remove_image_queue: std::cell::RefCell::new(VecDeque::from([RemoveImageOutcome::Removed])),
        ..Default::default()
    };

    prune_images(&docker).await.unwrap();

    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rmi jk_agent-smith:latest"))
    );
}

#[tokio::test]
async fn prune_images_is_ok_when_no_images_found() {
    let docker = FakeDockerClient {
        list_image_tags_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
        ..Default::default()
    };

    prune_images(&docker).await.unwrap();

    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rmi"))
    );
}

#[tokio::test]
async fn prune_images_is_ok_when_rmi_fails_with_real_error() {
    // A real Docker error (not in-use, not missing) is printed to stderr
    // but prune_images still returns Ok — best-effort cleanup.
    let docker = FakeDockerClient {
        list_image_tags_queue: std::cell::RefCell::new(VecDeque::from([vec![
            "jk_agent-smith:latest".to_owned(),
        ]])),
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
        fail_with: vec![(
            "docker rmi jk_agent-smith:latest".to_owned(),
            "Error response from daemon: permission denied".to_owned(),
        )],
        ..Default::default()
    };

    prune_images(&docker).await.unwrap();

    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rmi jk_agent-smith:latest"))
    );
}

#[tokio::test]
async fn prune_images_mixed_removed_and_skipped() {
    // One image is in-use (pre-filtered via jackin.image label), one is removed.
    let mut image_labels = HashMap::new();
    image_labels.insert(LABEL_IMAGE_KEY.to_owned(), "jk_neo".to_owned()); // no :tag → jk_neo:latest
    let docker = FakeDockerClient {
        list_image_tags_queue: std::cell::RefCell::new(VecDeque::from([vec![
            "jk_agent-smith:latest".to_owned(),
            "jk_neo:latest".to_owned(),
        ]])),
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![ContainerRow {
            name: "jk-bar".to_owned(),
            id: "container-id".to_owned(),
            labels: image_labels,
        }]])),
        remove_image_queue: std::cell::RefCell::new(VecDeque::from([RemoveImageOutcome::Removed])),
        ..Default::default()
    };

    prune_images(&docker).await.unwrap();

    // Only jk_agent-smith:latest should have had rmi attempted.
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rmi jk_agent-smith:latest"))
    );
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rmi jk_neo:latest"))
    );
}

#[tokio::test]
async fn prune_images_skips_when_image_disappears_between_list_and_rmi() {
    // TOCTOU: image listed but already gone by rmi time — should be skipped, not failed.
    let docker = FakeDockerClient {
        list_image_tags_queue: std::cell::RefCell::new(VecDeque::from([vec![
            "jk_agent-smith:latest".to_owned(),
        ]])),
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
        remove_image_queue: std::cell::RefCell::new(VecDeque::from([RemoveImageOutcome::NotFound])),
        ..Default::default()
    };

    prune_images(&docker).await.unwrap();
}

#[tokio::test]
async fn prune_instances_removes_all_four_prunable_statuses() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let clean = "jk-a1b2c3d4-agentsmith";
    let superseded = "jk-b2c3d4e5-agentsmith";
    let failed = "jk-c3d4e5f6-agentsmith";
    let purged = "jk-d4e5f6a7-agentsmith";
    let crashed = "jk-e5f6a7b8-agentsmith";
    make_instance_at(&paths, clean, InstanceStatus::CleanExited);
    make_instance_at(&paths, superseded, InstanceStatus::Superseded);
    make_instance_at(&paths, failed, InstanceStatus::FailedSetup);
    make_instance_at(&paths, purged, InstanceStatus::Purged);
    make_instance_at(&paths, crashed, InstanceStatus::Crashed);

    let docker = FakeDockerClient::default(); // inspect returns NotFound → allow purge
    let mut runner = FakeRunner::default();
    prune_instances(&paths, &docker, &mut runner).await.unwrap();

    let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
    for name in [clean, superseded, failed, purged] {
        assert!(
            !paths.data_dir.join(name).exists(),
            "{name} should be pruned"
        );
        assert!(
            index.instances.iter().all(|e| e.container_base != name),
            "{name} should be absent from index"
        );
    }
    assert!(
        paths.data_dir.join(crashed).exists(),
        "crashed should be kept"
    );
}
