//! Tests for `cleanup`.
use super::super::naming::matching_family;
use super::super::test_support::FakeRunner;
use super::*;
use crate::instance::{DockerResources, InstanceManifest};
use crate::runtime::test_support::FakeDockerClient;
use jackin_core::paths::JackinPaths;
use jackin_core::selector::RoleSelector;
use jackin_docker::docker_client::{ContainerRow, ContainerState, NetworkRow};
use std::collections::{HashMap, VecDeque};
use tempfile::tempdir;

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
        agent_runtime: jackin_core::agent::Agent::Claude,
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
        agent_runtime: jackin_core::agent::Agent::Claude,
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
    let docker = FakeDockerClient::default();
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    eject_role(&paths, "jk-agent-smith", &docker).await.unwrap();

    assert_eq!(
        docker.recorded.borrow().clone(),
        vec![
            "docker rm -f jk-agent-smith",
            "docker rm -f jk-agent-smith-dind",
            "docker volume rm jk-agent-smith-dind-certs",
            "docker network rm jk-agent-smith-net",
        ]
    );
}

#[tokio::test]
async fn eject_agent_removes_manifest_recorded_sidecar_resources() {
    let docker = FakeDockerClient::default();
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container = "jk-agent-smith";
    let manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
        container_base: container,
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::agent::Agent::Claude,
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
    manifest.write(&paths.data_dir.join(container)).unwrap();

    eject_role(&paths, container, &docker).await.unwrap();

    assert_eq!(
        docker.recorded.borrow().clone(),
        vec![
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

    eject_role(&paths, "jk-agent-smith", &docker).await.unwrap();

    assert_eq!(
        docker.recorded.borrow().clone(),
        vec![
            "docker rm -f jk-agent-smith",
            "docker rm -f jk-agent-smith-dind",
            "docker volume rm jk-agent-smith-dind-certs",
            "docker network rm jk-agent-smith-net",
        ]
    );
}

#[tokio::test]
async fn eject_role_phase1_failure_prevents_phase2_calls() {
    // When remove_container fails, remove_volume and remove_network must not be called.
    let docker = FakeDockerClient {
        fail_with: vec![(
            "docker rm -f jk-agent-smith".to_owned(),
            "Error response from daemon: permission denied".to_owned(),
        )],
        ..Default::default()
    };

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
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
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![
            ContainerRow {
                name: "jk-k7p9m2xq-agentsmith".to_owned(),
                labels: HashMap::default(),
            },
            ContainerRow {
                name: "jk-a1b2c3d4-myworkspace-agentsmith".to_owned(),
                labels: HashMap::default(),
            },
        ]])),
        ..Default::default()
    };

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
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
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![
            ContainerRow {
                name: "jk-k7p9m2xq-agentsmith".to_owned(),
                labels: HashMap::default(),
            },
            ContainerRow {
                name: "jk-a1b2c3d4-myworkspace-agentsmith".to_owned(),
                labels: HashMap::default(),
            },
        ]])),
        ..Default::default()
    };

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    exile_all(&paths, &docker).await.unwrap();

    assert_eq!(
        docker.recorded.borrow().clone(),
        vec![
            "docker ps -a --filter jackin.kind=role",
            "docker rm -f jk-k7p9m2xq-agentsmith",
            "docker rm -f jk-k7p9m2xq-agentsmith-dind",
            "docker volume rm jk-k7p9m2xq-agentsmith-dind-certs",
            "docker network rm jk-k7p9m2xq-agentsmith-net",
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
                labels: labels.clone(),
            }],
            // list_role_names (running): no running role containers
            vec![],
        ])),
        list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // gc_orphaned_networks: no networks
        ..Default::default()
    };

    gc_orphaned_resources(&docker).await;

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

#[tokio::test]
async fn gc_skips_dind_when_agent_is_running() {
    let mut labels = HashMap::new();
    labels.insert(LABEL_ROLE_KEY.to_owned(), "jk-agent-smith".to_owned());
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([
            // collect_labeled_dind: DinD sidecar present
            vec![ContainerRow {
                name: "jk-agent-smith-dind".to_owned(),
                labels: labels.clone(),
            }],
            // list_role_names (running): role IS running — skip GC
            vec![ContainerRow {
                name: "jk-agent-smith".to_owned(),
                labels: HashMap::default(),
            }],
        ])),
        list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // gc_orphaned_networks: no networks
        ..Default::default()
    };

    gc_orphaned_resources(&docker).await;

    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rm -f jk-agent-smith-dind"))
    );
}

#[tokio::test]
async fn gc_ignores_prewarm_dind_resources() {
    let mut labels = HashMap::new();
    labels.insert("jackin.kind".to_owned(), "prewarm-dind".to_owned());
    labels.insert("jackin.prewarm".to_owned(), "true".to_owned());
    labels.insert(LABEL_ROLE_KEY.to_owned(), "jk-prewarm-dind".to_owned());
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![ContainerRow {
            name: "jk-prewarm-dind-dind".to_owned(),
            labels,
        }]])),
        list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
        ..Default::default()
    };

    gc_orphaned_resources(&docker).await;

    let recorded = docker.recorded.borrow();
    assert!(
        recorded
            .iter()
            .any(|call| call == "docker ps -a --filter jackin.kind=dind"),
        "GC must keep scanning only role-owned dind sidecars: {recorded:?}"
    );
    assert!(
        !recorded.iter().any(|call| call.contains("jk-prewarm-dind")),
        "prewarm-owned sidecars are reserved for daemon/runtime adoption and must not be purged by role GC: {recorded:?}"
    );
}

#[tokio::test]
async fn gc_does_nothing_when_no_orphans() {
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // collect_labeled_dind: no DinD
        list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // gc_orphaned_networks: no networks
        ..Default::default()
    };

    gc_orphaned_resources(&docker).await;

    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rm"))
    );
}

#[tokio::test]
async fn gc_removes_orphaned_network_without_dind() {
    let mut net_labels = HashMap::new();
    net_labels.insert(LABEL_ROLE_KEY.to_owned(), "jk-agent-smith".to_owned());
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([
            vec![], // collect_labeled_dind: no DinD sidecars
            // list_role_names (running) for gc_orphaned_networks: role not running
            vec![],
        ])),
        list_networks_queue: std::cell::RefCell::new(VecDeque::from([
            // gc_orphaned_networks: has a network with jackin.role label
            vec![NetworkRow {
                name: "jk-agent-smith-net".to_owned(),
                labels: net_labels,
            }],
        ])),
        ..Default::default()
    };

    gc_orphaned_resources(&docker).await;

    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker network rm jk-agent-smith-net"))
    );
}

#[tokio::test]
async fn gc_cleans_multiple_orphans() {
    let mut labels_smith = HashMap::new();
    labels_smith.insert(LABEL_ROLE_KEY.to_owned(), "jk-agent-smith".to_owned());
    let mut labels_neo = HashMap::new();
    labels_neo.insert(LABEL_ROLE_KEY.to_owned(), "jk-neo".to_owned());
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([
            // collect_labeled_dind: two orphaned DinD sidecars
            vec![
                ContainerRow {
                    name: "jk-agent-smith-dind".to_owned(),
                    labels: labels_smith,
                },
                ContainerRow {
                    name: "jk-neo-dind".to_owned(),
                    labels: labels_neo,
                },
            ],
            // list_role_names (running): no running roles
            vec![],
        ])),
        list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // gc_orphaned_networks: no networks
        ..Default::default()
    };

    gc_orphaned_resources(&docker).await;

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
            .any(|c| c.contains("docker volume rm jk-agent-smith-dind-certs"))
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rm -f jk-neo-dind"))
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker volume rm jk-neo-dind-certs"))
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker network rm jk-neo-net"))
    );
}

#[tokio::test]
async fn gc_does_not_panic_when_collect_orphaned_dind_fails() {
    // Docker daemon unreachable — the DinD ps call fails. gc_orphaned_resources
    // must emit a warning and return without panicking.
    let docker = FakeDockerClient {
        fail_with: vec![(
            LABEL_KIND_DIND.to_owned(),
            "Error response from daemon: socket timeout".to_owned(),
        )],
        ..Default::default()
    };

    gc_orphaned_resources(&docker).await; // must not panic
}

#[tokio::test]
async fn gc_does_not_panic_when_network_ls_fails() {
    // DinD list succeeds (no orphans), but docker network ls fails.
    // gc_orphaned_networks must emit a warning and return without panicking.
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // no DinD sidecars
        fail_with: vec![(
            "docker network ls".to_owned(),
            "Error response from daemon: socket timeout".to_owned(),
        )],
        ..Default::default()
    };

    gc_orphaned_resources(&docker).await; // must not panic
}

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

    gc_orphaned_resources(&docker).await; // must not panic

    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker network rm"))
    );
}

// ── prune_dir ────────────────────────────────────────────────────────────

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

// ── prune_instances ──────────────────────────────────────────────────────

fn make_instance_at(paths: &JackinPaths, container: &str, status: InstanceStatus) {
    let mut manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
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
            dind_container: Some(format!("{container}-dind")),
            network: format!("{container}-net"),
            certs_volume: Some(format!("{container}-dind-certs")),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });
    manifest.mark_status(status);
    let state_dir = paths.data_dir.join(container);
    std::fs::create_dir_all(&state_dir).unwrap();
    manifest.write(&state_dir).unwrap();
    InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
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
        InstanceManifest::read_or_log(&paths.data_dir.join(container), "test").expect("manifest");
    assert_eq!(manifest.status, InstanceStatus::Crashed);
}

#[tokio::test]
async fn prune_instances_reaps_only_unheld_name_locks() {
    use fs4::FileExt;
    // D9: an orphaned `<name>.lock` (no live holder) is removed; one still held
    // by a live process is left untouched.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    let held = paths.data_dir.join("jk-held.lock");
    let orphan = paths.data_dir.join("jk-orphan.lock");
    std::fs::write(&held, b"").unwrap();
    std::fs::write(&orphan, b"").unwrap();
    // Hold an exclusive flock on `held` for the duration of the prune; flock
    // conflicts across separate open descriptions, so the reaper's try_lock fails.
    #[expect(
        clippy::disallowed_methods,
        reason = "test holds a real flock to verify the reaper skips a held lock"
    )]
    let held_file = std::fs::File::open(&held).unwrap();
    FileExt::try_lock(&held_file).unwrap();

    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();
    prune_instances(&paths, &docker, &mut runner).await.unwrap();

    assert!(held.exists(), "a held name-lock must not be reaped");
    assert!(!orphan.exists(), "an unheld name-lock must be reaped");
    FileExt::unlock(&held_file).unwrap();
}

// ── prune_images ─────────────────────────────────────────────────────────

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
        agent_runtime: jackin_core::agent::Agent::Claude,
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

// ── prune_jackin_home ────────────────────────────────────────────────────

#[tokio::test]
async fn prune_jackin_home_removes_home() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(paths.jackin_home.join("leftover")).unwrap();

    prune_jackin_home(&paths);

    assert!(!paths.jackin_home.exists(), "jackin_home should be removed");
}

#[tokio::test]
async fn prune_jackin_home_is_ok_when_absent() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    // jackin_home never created — must not panic
    prune_jackin_home(&paths);
}
