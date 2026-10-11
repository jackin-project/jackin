// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::runtime::naming::LABEL_ROLE_KEY;

#[tokio::test]
async fn purge_container_state_refuses_when_dind_sidecar_exists() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container = "jk-agent-smith";
    std::fs::create_dir_all(paths.data_dir.join(container)).unwrap();
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::NotFound, // role container not found
            ContainerState::Running,  // dind running
        ])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    let err = purge_container_state(&paths, container, &docker, &mut runner)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("DinD sidecar"), "got: {err}");
    assert!(
        err.to_string().contains("still exists and is running"),
        "got: {err}"
    );
    assert!(paths.data_dir.join(container).exists());
}

#[tokio::test]
async fn purge_container_state_refuses_for_active_non_running_states() {
    use jackin_docker::docker_client::ContainerState;
    let cases: &[(ContainerState, &str)] = &[
        (ContainerState::Paused, "and is paused"),
        (ContainerState::Restarting, "and is restarting"),
        (ContainerState::Created, "and is being created"),
        (ContainerState::Removing, "and is being removed"),
        (ContainerState::Dead, "but is dead"),
    ];
    let container = "jk-agent-smith";
    for (state, expected_phrase) in cases {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        std::fs::create_dir_all(paths.data_dir.join(container)).unwrap();
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(VecDeque::from([state.clone()])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();
        let err = purge_container_state(&paths, container, &docker, &mut runner)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains(expected_phrase),
            "state={state:?}: got: {err}"
        );
    }
}

#[tokio::test]
async fn eject_agent_removes_container_dind_and_network() {
    let docker = FakeDockerClient {
        inspect_state_by_name: std::cell::RefCell::new(HashMap::from([
            ("jk-agent-smith".to_owned(), ContainerState::Running),
            ("jk-agent-smith-dind".to_owned(), ContainerState::Running),
        ])),
        ..Default::default()
    };
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    write_owned_cleanup_manifest(&paths, "jk-agent-smith", "jk-agent-smith-dind");

    eject_role(&paths, "jk-agent-smith", &docker).await.unwrap();

    assert_eq!(
        docker.recorded.borrow().clone(),
        vec![
            "docker inspect jk-agent-smith",
            "docker inspect jk-agent-smith-dind",
            "docker rm -f jk-agent-smith",
            "docker rm -f jk-agent-smith-dind",
            "docker volume rm jk-agent-smith-dind-certs",
            "docker network rm jk-agent-smith-net",
        ]
    );
}

#[tokio::test]
async fn eject_agent_removes_manifest_recorded_sidecar_resources() {
    let docker = FakeDockerClient {
        inspect_state_by_name: std::cell::RefCell::new(HashMap::from([
            ("jk-agent-smith".to_owned(), ContainerState::Running),
            ("jk-prewarm-dind-dind".to_owned(), ContainerState::Running),
        ])),
        ..Default::default()
    };
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container = "jk-agent-smith";
    let mut manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
        container_base: container,
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
            role_container: container.to_owned(),
            dind_container: Some("jk-prewarm-dind-dind".to_owned()),
            network: "jk-prewarm-dind-net".to_owned(),
            certs_volume: Some("jk-prewarm-dind-certs".to_owned()),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });
    manifest.docker_identity = Some(crate::instance::DockerIdentity {
        role_container_id: container.to_owned(),
        dind_container_id: Some("jk-prewarm-dind-dind".to_owned()),
    });
    manifest.write(&paths.data_dir.join(container)).unwrap();

    eject_role(&paths, container, &docker).await.unwrap();

    assert_eq!(
        docker.recorded.borrow().clone(),
        vec![
            "docker inspect jk-agent-smith",
            "docker inspect jk-prewarm-dind-dind",
            "docker rm -f jk-agent-smith",
            "docker rm -f jk-prewarm-dind-dind",
            "docker volume rm jk-prewarm-dind-certs",
            "docker network rm jk-prewarm-dind-net",
        ]
    );
}

#[tokio::test]
async fn eject_agent_ignores_missing_runtime_resources() {
    let docker = FakeDockerClient::default();
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    let error = eject_role(&paths, "jk-agent-smith", &docker)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("is not present"), "{error}");

    assert_eq!(
        docker.recorded.borrow().clone(),
        vec![
            "docker inspect jk-agent-smith",
            "docker inspect jk-agent-smith-dind"
        ]
    );
}

#[tokio::test]
async fn eject_role_phase1_failure_prevents_phase2_calls() {
    // When remove_container fails, remove_volume and remove_network must not be called.
    let docker = FakeDockerClient {
        inspect_state_by_name: std::cell::RefCell::new(HashMap::from([
            ("jk-agent-smith".to_owned(), ContainerState::Running),
            ("jk-agent-smith-dind".to_owned(), ContainerState::Running),
        ])),
        fail_with: vec![(
            "docker rm -f jk-agent-smith".to_owned(),
            "Error response from daemon: permission denied".to_owned(),
        )],
        ..Default::default()
    };

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    write_owned_cleanup_manifest(&paths, "jk-agent-smith", "jk-agent-smith-dind");
    let err = eject_role(&paths, "jk-agent-smith", &docker)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("permission denied"), "got: {err}");
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker volume rm")),
        "volume rm must not be called after phase-1 failure; recorded: {:?}",
        docker.recorded.borrow()
    );
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker network rm")),
        "network rm must not be called after phase-1 failure; recorded: {:?}",
        docker.recorded.borrow()
    );
}

#[tokio::test]
async fn exile_all_ejects_all_managed_agents() {
    let docker = FakeDockerClient {
        inspect_state_by_name: std::cell::RefCell::new(HashMap::from([
            ("jk-k7p9m2xq-agentsmith".to_owned(), ContainerState::Running),
            (
                "jk-k7p9m2xq-agentsmith-dind".to_owned(),
                ContainerState::Running,
            ),
            (
                "jk-a1b2c3d4-myworkspace-agentsmith".to_owned(),
                ContainerState::Running,
            ),
            (
                "jk-a1b2c3d4-myworkspace-agentsmith-dind".to_owned(),
                ContainerState::Running,
            ),
        ])),
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![
            ContainerRow {
                name: "jk-k7p9m2xq-agentsmith".to_owned(),
                id: "container-id".to_owned(),
                labels: HashMap::default(),
            },
            ContainerRow {
                name: "jk-a1b2c3d4-myworkspace-agentsmith".to_owned(),
                id: "container-id".to_owned(),
                labels: HashMap::default(),
            },
        ]])),
        ..Default::default()
    };

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    for role in [
        "jk-k7p9m2xq-agentsmith",
        "jk-a1b2c3d4-myworkspace-agentsmith",
    ] {
        write_owned_cleanup_manifest(&paths, role, &format!("{role}-dind"));
    }
    exile_all(&paths, &docker).await.unwrap();

    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rm -f jk-k7p9m2xq-agentsmith"))
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rm -f jk-a1b2c3d4-myworkspace-agentsmith"))
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker volume rm jk-k7p9m2xq-agentsmith-dind-certs"))
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker network rm jk-k7p9m2xq-agentsmith-net"))
    );
}

#[tokio::test]
async fn exile_all_continues_when_some_runtime_resources_are_missing() {
    let docker = FakeDockerClient {
        inspect_state_by_name: std::cell::RefCell::new(HashMap::from([
            ("jk-k7p9m2xq-agentsmith".to_owned(), ContainerState::Running),
            (
                "jk-k7p9m2xq-agentsmith-dind".to_owned(),
                ContainerState::Running,
            ),
            (
                "jk-a1b2c3d4-myworkspace-agentsmith".to_owned(),
                ContainerState::Running,
            ),
            (
                "jk-a1b2c3d4-myworkspace-agentsmith-dind".to_owned(),
                ContainerState::Running,
            ),
        ])),
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![
            ContainerRow {
                name: "jk-k7p9m2xq-agentsmith".to_owned(),
                id: "container-id".to_owned(),
                labels: HashMap::default(),
            },
            ContainerRow {
                name: "jk-a1b2c3d4-myworkspace-agentsmith".to_owned(),
                id: "container-id".to_owned(),
                labels: HashMap::default(),
            },
        ]])),
        ..Default::default()
    };

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    for role in [
        "jk-k7p9m2xq-agentsmith",
        "jk-a1b2c3d4-myworkspace-agentsmith",
    ] {
        write_owned_cleanup_manifest(&paths, role, &format!("{role}-dind"));
    }
    exile_all(&paths, &docker).await.unwrap();

    assert_eq!(
        docker.recorded.borrow().clone(),
        vec![
            "docker ps -a --filter jackin.kind=role",
            "docker inspect jk-k7p9m2xq-agentsmith",
            "docker inspect jk-k7p9m2xq-agentsmith-dind",
            "docker rm -f jk-k7p9m2xq-agentsmith",
            "docker rm -f jk-k7p9m2xq-agentsmith-dind",
            "docker volume rm jk-k7p9m2xq-agentsmith-dind-certs",
            "docker network rm jk-k7p9m2xq-agentsmith-net",
            "docker inspect jk-a1b2c3d4-myworkspace-agentsmith",
            "docker inspect jk-a1b2c3d4-myworkspace-agentsmith-dind",
            "docker rm -f jk-a1b2c3d4-myworkspace-agentsmith",
            "docker rm -f jk-a1b2c3d4-myworkspace-agentsmith-dind",
            "docker volume rm jk-a1b2c3d4-myworkspace-agentsmith-dind-certs",
            "docker network rm jk-a1b2c3d4-myworkspace-agentsmith-net",
        ]
    );
}

#[tokio::test]
async fn gc_removes_orphaned_dind_and_network() {
    let mut labels = HashMap::new();
    labels.insert(LABEL_ROLE_KEY.to_owned(), "jk-agent-smith".to_owned());
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([
            // collect_labeled_dind: DinD sidecar with jackin.role label
            vec![ContainerRow {
                name: "jk-agent-smith-dind".to_owned(),
                id: "container-id".to_owned(),
                labels: labels.clone(),
            }],
            // list_role_names (running): no running role containers
            vec![],
        ])),
        list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // gc_orphaned_networks: no networks
        ..Default::default()
    };

    gc_orphaned_resources(&gc_test_paths(), &docker).await;

    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rm -f jk-agent-smith-dind"))
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rm -f jk-agent-smith"))
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker volume rm jk-agent-smith-dind-certs"))
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker network rm jk-agent-smith-net"))
    );
}
