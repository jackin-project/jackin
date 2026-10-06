// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn persisted_cleanup_refuses_same_name_role_or_dind_replacements() {
    for replacement_is_role in [true, false] {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let role = "jk-agent-smith";
        let dind = "jk-agent-smith-dind";
        write_owned_cleanup_manifest(&paths, role, dind);
        let docker = FakeDockerClient::default();
        docker.inspect_state_by_name.borrow_mut().extend([
            (role.to_owned(), ContainerState::Running),
            (dind.to_owned(), ContainerState::Running),
        ]);
        docker.container_id_by_name.borrow_mut().insert(
            if replacement_is_role { role } else { dind }.to_owned(),
            "replacement-id".to_owned(),
        );
        let error = eject_role(&paths, role, &docker).await.unwrap_err();
        assert!(
            error.to_string().contains("ownership identity mismatch"),
            "{error}"
        );
        assert!(docker.bound_operations.borrow().is_empty());
        assert!(!docker.recorded.borrow().iter().any(|op| op.starts_with("docker network rm") || op.starts_with("docker volume rm")));
    }
}

#[tokio::test]
async fn persisted_cleanup_refuses_corrupt_manifest_without_mutation() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let role = "jk-agent-smith";
    std::fs::create_dir_all(paths.data_dir.join(role).join(".jackin")).unwrap();
    std::fs::write(paths.data_dir.join(role).join(".jackin/instance.json"), "{").unwrap();
    let docker = FakeDockerClient::default();
    let error = eject_role(&paths, role, &docker).await.unwrap_err();
    assert!(error.to_string().contains("parsing"), "{error}");
    assert!(docker.bound_operations.borrow().is_empty());
}

#[tokio::test]
async fn purge_refuses_corrupt_ownership_state_before_filesystem_removal() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let role = "jk-agent-smith";
    let state = paths.data_dir.join(role);
    std::fs::create_dir_all(state.join(".jackin")).unwrap();
    std::fs::write(state.join(".jackin/instance.json"), "{").unwrap();
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();
    let error = purge_container_state(&paths, role, &docker, &mut runner)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("parsing"), "{error}");
    assert!(state.join(".jackin/instance.json").exists());
    assert!(docker.recorded.borrow().is_empty());
    assert!(runner.recorded.is_empty());
}

#[tokio::test]
async fn lifecycle_operations_keep_original_id_after_same_name_replacement() {
    let name = "jk-agent-smith";
    let docker = FakeDockerClient::default();
    docker
        .container_id_by_name
        .borrow_mut()
        .insert(name.to_owned(), "old-container-id".to_owned());
    docker
        .inspect_state_by_name
        .borrow_mut()
        .insert(name.to_owned(), ContainerState::Running);

    let inspection = docker.inspect_container_by_name(name).await;
    let handle = inspection.handle.expect("running container has an ID");

    // A replacement can claim the mutable name after lookup. Every lifecycle
    // operation below must still target the originally inspected daemon ID.
    docker
        .container_id_by_name
        .borrow_mut()
        .insert(name.to_owned(), "replacement-container-id".to_owned());
    docker.start_container_by_id(&handle).await.unwrap();
    docker.exec_capture_by_id(&handle, &["true"]).await.unwrap();
    docker.remove_container_by_id(&handle).await.unwrap();

    assert_eq!(
        docker.bound_operations.borrow().as_slice(),
        [
            "start:old-container-id",
            "exec:old-container-id",
            "remove:old-container-id",
        ]
    );
}

#[tokio::test]
async fn cleanup_keeps_captured_role_and_dind_ids_after_same_name_replacement() {
    let role = "jk-agent-smith";
    let dind = "jk-agent-smith-dind";
    let docker = FakeDockerClient::default();
    docker.container_id_by_name.borrow_mut().extend([
        (role.to_owned(), "old-role-id".to_owned()),
        (dind.to_owned(), "old-dind-id".to_owned()),
    ]);
    docker.inspect_state_by_name.borrow_mut().extend([
        (role.to_owned(), ContainerState::Running),
        (dind.to_owned(), ContainerState::Running),
    ]);
    let role_handle = docker
        .inspect_container_by_name(role)
        .await
        .handle
        .expect("role container has an ID");
    let dind_handle = docker
        .inspect_container_by_name(dind)
        .await
        .handle
        .expect("DinD container has an ID");
    let cleanup = LoadCleanup::new(
        role.to_owned(),
        dind.to_owned(),
        "jk-agent-smith-dind-certs".to_owned(),
        "jk-agent-smith-net".to_owned(),
        std::env::temp_dir().join("jackin-cleanup-replacement-test"),
    );
    cleanup.set_dind_handle(dind_handle);

    docker.container_id_by_name.borrow_mut().extend([
        (role.to_owned(), "replacement-role-id".to_owned()),
        (dind.to_owned(), "replacement-dind-id".to_owned()),
    ]);
    cleanup.run_with_role_handle(&docker, &role_handle).await;

    let bound = docker.bound_operations.borrow();
    assert!(
        bound.contains(&"remove:old-role-id".to_owned()),
        "{bound:?}"
    );
    assert!(
        bound.contains(&"remove:old-dind-id".to_owned()),
        "{bound:?}"
    );
    assert!(
        !bound.contains(&"remove:replacement-role-id".to_owned()),
        "{bound:?}"
    );
    assert!(
        !bound.contains(&"remove:replacement-dind-id".to_owned()),
        "{bound:?}"
    );
}

#[tokio::test]
async fn eject_keeps_captured_role_and_dind_ids_after_same_name_replacement() {
    let role = "jk-agent-smith";
    let dind = "jk-agent-smith-dind";
    let docker = FakeDockerClient::default();
    docker.container_id_by_name.borrow_mut().extend([
        (role.to_owned(), "old-role-id".to_owned()),
        (dind.to_owned(), "old-dind-id".to_owned()),
    ]);
    docker.inspect_state_by_name.borrow_mut().extend([
        (role.to_owned(), ContainerState::Running),
        (dind.to_owned(), ContainerState::Running),
    ]);
    let role_handle = docker
        .inspect_container_by_name(role)
        .await
        .handle
        .expect("role container has an ID");
    let dind_handle = docker
        .inspect_container_by_name(dind)
        .await
        .handle
        .expect("DinD container has an ID");
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    docker.container_id_by_name.borrow_mut().extend([
        (role.to_owned(), "replacement-role-id".to_owned()),
        (dind.to_owned(), "replacement-dind-id".to_owned()),
    ]);
    eject_docker_role_with_handles(&paths, role, &docker, &role_handle, Some(&dind_handle))
        .await
        .unwrap();

    let bound = docker.bound_operations.borrow();
    assert!(
        bound.contains(&"remove:old-role-id".to_owned()),
        "{bound:?}"
    );
    assert!(
        bound.contains(&"remove:old-dind-id".to_owned()),
        "{bound:?}"
    );
    assert!(
        !bound.contains(&"remove:replacement-role-id".to_owned()),
        "{bound:?}"
    );
    assert!(
        !bound.contains(&"remove:replacement-dind-id".to_owned()),
        "{bound:?}"
    );
}

#[tokio::test]
async fn eject_refuses_destructive_cleanup_without_dind_identity() {
    let role = "jk-agent-smith";
    let docker = FakeDockerClient {
        inspect_state_by_name: std::cell::RefCell::new(HashMap::from([(
            role.to_owned(),
            ContainerState::Running,
        )])),
        ..Default::default()
    };
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let role_handle = docker
        .inspect_container_by_name(role)
        .await
        .handle
        .expect("role container has an ID");

    let error = eject_docker_role_with_handles(&paths, role, &docker, &role_handle, None)
        .await
        .unwrap_err();

    assert!(
        error.to_string().contains("identity unavailable"),
        "{error}"
    );
    assert!(
        docker.bound_operations.borrow().is_empty(),
        "no container may be removed without both identities"
    );
}

#[tokio::test]
async fn eject_all_targets_only_requested_class_family() {
    let selector = RoleSelector::new(None, "agent-smith");
    let names = vec![
        "jk-k7p9m2xq-agentsmith".to_owned(),
        "jk-a1b2c3d4-myproject-agentsmith".to_owned(),
        "jk-w9x8y7z6-chainargos-thearchitect".to_owned(),
    ];

    let matched = matching_family(&selector, &names);

    assert_eq!(
        matched,
        vec!["jk-k7p9m2xq-agentsmith", "jk-a1b2c3d4-myproject-agentsmith",]
    );
}

#[tokio::test]
async fn purge_all_removes_matching_state_directories() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let primary = "jk-k7p9m2xq-agentsmith";
    let second = "jk-a1b2c3d4-workspace-agentsmith";
    let manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
        container_base: primary,
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk_agent-smith",
        docker: DockerResources {
            role_container: primary.into(),
            dind_container: Some(format!("{primary}-dind")),
            network: format!("{primary}-net"),
            certs_volume: Some(format!("{primary}-dind-certs")),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });
    manifest.write(&paths.data_dir.join(primary)).unwrap();
    InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
    let second_manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
        container_base: second,
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk_agent-smith",
        docker: DockerResources {
            role_container: second.into(),
            dind_container: Some(format!("{second}-dind")),
            network: format!("{second}-net"),
            certs_volume: Some(format!("{second}-dind-certs")),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });
    second_manifest.write(&paths.data_dir.join(second)).unwrap();
    InstanceIndex::update_manifest(&paths.data_dir, &second_manifest).unwrap();
    let unrelated = "jk-w9x8y7z6-chainargos-thearchitect";
    std::fs::create_dir_all(paths.data_dir.join(unrelated)).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");

    // FakeDockerClient with NotFound for all containers (safe to purge)
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::NotFound, // primary role container
            ContainerState::NotFound, // primary dind
            ContainerState::NotFound, // second role container
            ContainerState::NotFound, // second dind
        ])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();
    purge_class_data(&paths, &selector, &docker, &mut runner)
        .await
        .unwrap();

    assert!(!paths.data_dir.join(primary).exists());
    assert!(!paths.data_dir.join(second).exists());
    assert!(paths.data_dir.join(unrelated).exists());
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
    assert_eq!(
        index
            .instances
            .iter()
            .filter(|entry| entry.status == InstanceStatus::Purged)
            .count(),
        2
    );
}

#[tokio::test]
async fn purge_container_state_refuses_when_role_container_exists() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container = "jk-agent-smith";
    std::fs::create_dir_all(paths.data_dir.join(container)).unwrap();
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 0,
            oom_killed: false,
        }])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    let err = purge_container_state(&paths, container, &docker, &mut runner)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("still exists but is stopped"),
        "got: {err}"
    );
    assert!(err.to_string().contains("jackin eject"), "got: {err}");
    assert!(paths.data_dir.join(container).exists());
}
