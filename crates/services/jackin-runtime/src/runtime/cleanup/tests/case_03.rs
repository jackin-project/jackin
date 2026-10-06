// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::runtime::naming::{LABEL_KIND_DIND, LABEL_ROLE_KEY};

#[tokio::test]
async fn gc_removes_only_the_listed_sidecar_when_role_name_is_replaced() {
    let mut labels = HashMap::new();
    labels.insert(LABEL_ROLE_KEY.to_owned(), "jk-agent-smith".to_owned());
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([
            vec![ContainerRow {
                name: "jk-agent-smith-dind".to_owned(),
                id: "old-dind-id".to_owned(),
                labels,
            }],
            vec![], // The role was absent when the sidecar was classified orphaned.
        ])),
        list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
        container_id_by_name: std::cell::RefCell::new(HashMap::from([(
            "jk-agent-smith".to_owned(),
            "replacement-role-id".to_owned(),
        )])),
        inspect_state_by_name: std::cell::RefCell::new(HashMap::from([(
            "jk-agent-smith".to_owned(),
            ContainerState::Running,
        )])),
        ..Default::default()
    };

    gc_orphaned_resources(&gc_test_paths(), &docker).await;

    assert_eq!(
        docker.bound_operations.borrow().as_slice(),
        ["remove:old-dind-id"]
    );
    assert!(
        !docker
            .bound_operations
            .borrow()
            .contains(&"remove:replacement-role-id".to_owned()),
        "GC must never remove a same-name role replacement"
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
                id: "container-id".to_owned(),
                labels: labels.clone(),
            }],
            // list_role_names (running): role IS running — skip GC
            vec![ContainerRow {
                name: "jk-agent-smith".to_owned(),
                id: "container-id".to_owned(),
                labels: HashMap::default(),
            }],
        ])),
        list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // gc_orphaned_networks: no networks
        ..Default::default()
    };

    gc_orphaned_resources(&gc_test_paths(), &docker).await;

    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rm -f jk-agent-smith-dind"))
    );
}

#[tokio::test]
async fn gc_skips_dind_when_agent_is_stopped() {
    let mut labels = HashMap::new();
    labels.insert(LABEL_ROLE_KEY.to_owned(), "jk-agent-smith".to_owned());
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([
            // collect_labeled_dind: DinD sidecar present
            vec![ContainerRow {
                name: "jk-agent-smith-dind".to_owned(),
                id: "container-id".to_owned(),
                labels: labels.clone(),
            }],
            // list_role_names (including stopped): role container exists (stopped)
            vec![ContainerRow {
                name: "jk-agent-smith".to_owned(),
                id: "container-id".to_owned(),
                labels: HashMap::default(),
            }],
        ])),
        list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // gc_orphaned_networks: no networks
        ..Default::default()
    };

    gc_orphaned_resources(&gc_test_paths(), &docker).await;

    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rm -f jk-agent-smith-dind"))
    );
}

#[tokio::test]
async fn gc_keeps_state_owned_prewarm_dind_resources() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::launch::write_prewarmed_dind_state(
        &paths,
        &crate::runtime::launch::DindSidecarPrewarm {
            dind: "jk-prewarm-dind-dind".to_owned(),
            dind_id: "prewarm-dind-id".to_owned(),
            network: "jk-prewarm-dind-net".to_owned(),
            certs_volume: "jk-prewarm-dind-certs".to_owned(),
            ready_ms: 1,
            kept: true,
        },
    )
    .unwrap();
    let mut labels = HashMap::new();
    labels.insert("jackin.kind".to_owned(), "prewarm-dind".to_owned());
    labels.insert("jackin.prewarm".to_owned(), "true".to_owned());
    labels.insert(LABEL_ROLE_KEY.to_owned(), "jk-prewarm-dind".to_owned());
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([
            vec![],
            vec![ContainerRow {
                name: "jk-prewarm-dind-dind".to_owned(),
                id: "container-id".to_owned(),
                labels,
            }],
        ])),
        list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
        ..Default::default()
    };

    gc_orphaned_resources(&paths, &docker).await;

    let recorded = docker.recorded.borrow();
    assert!(
        recorded
            .iter()
            .any(|call| call == "docker ps -a --filter jackin.kind=prewarm-dind"),
        "GC must scan prewarm sidecars after role GC: {recorded:?}"
    );
    assert!(
        !recorded.iter().any(|call| call.contains("jk-prewarm-dind")),
        "state-owned prewarm sidecars are reserved for adoption: {recorded:?}"
    );
}

#[tokio::test]
async fn gc_keeps_state_less_prewarm_dind_resources_without_identity() {
    let mut labels = HashMap::new();
    labels.insert("jackin.kind".to_owned(), "prewarm-dind".to_owned());
    labels.insert("jackin.prewarm".to_owned(), "true".to_owned());
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([
            vec![],
            vec![ContainerRow {
                name: "jk-prewarm-dind-dind".to_owned(),
                id: "container-id".to_owned(),
                labels,
            }],
        ])),
        list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
        ..Default::default()
    };

    gc_orphaned_resources(&gc_test_paths(), &docker).await;

    let recorded = docker.recorded.borrow();
    assert!(
        !recorded.iter().any(|call| call.contains("docker rm")),
        "state-less prewarm DinD cleanup must fail closed: {recorded:?}"
    );
    assert!(
        !recorded
            .iter()
            .any(|call| call.contains("docker volume rm")),
        "state-less prewarm cert cleanup must fail closed: {recorded:?}"
    );
    assert!(
        !recorded
            .iter()
            .any(|call| call.contains("docker network rm")),
        "state-less prewarm network cleanup must fail closed: {recorded:?}"
    );
}

#[tokio::test]
async fn gc_does_nothing_when_no_orphans() {
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // collect_labeled_dind: no DinD
        list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // gc_orphaned_networks: no networks
        ..Default::default()
    };

    gc_orphaned_resources(&gc_test_paths(), &docker).await;

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

    gc_orphaned_resources(&gc_test_paths(), &docker).await;

    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker network rm jk-agent-smith-net"))
    );
}

#[tokio::test]
async fn gc_preserves_network_when_role_container_is_stopped() {
    let mut net_labels = HashMap::new();
    net_labels.insert(LABEL_ROLE_KEY.to_owned(), "jk-agent-smith".to_owned());
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([
            vec![], // collect_labeled_dind: no DinD sidecars
            // list_role_names (including stopped): role container exists (stopped)
            vec![ContainerRow {
                name: "jk-agent-smith".to_owned(),
                id: "container-id".to_owned(),
                labels: HashMap::default(),
            }],
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

    gc_orphaned_resources(&gc_test_paths(), &docker).await;

    assert!(
        !docker
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
                    id: "container-id".to_owned(),
                    labels: labels_smith,
                },
                ContainerRow {
                    name: "jk-neo-dind".to_owned(),
                    id: "container-id".to_owned(),
                    labels: labels_neo,
                },
            ],
            // list_role_names (running): no running roles
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

    gc_orphaned_resources(&gc_test_paths(), &docker).await; // must not panic
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

    gc_orphaned_resources(&gc_test_paths(), &docker).await; // must not panic
}
