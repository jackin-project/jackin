// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_core::ContainerHandle;

#[tokio::test]
async fn restored_start_activates_only_its_entry_before_foreground() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-restore-entry";
    provision_account_admission(&paths, container_name);
    let claim_docker = FakeDockerClient::default();
    let claim = universe::claim_entry(&paths, &claim_docker).await;
    let other_claim = universe::claim_entry(&paths, &claim_docker).await;
    assert_eq!(pending_entry_count(&paths), 2);
    let container = ContainerHandle::new(container_name, container_name).unwrap();
    let docker = FakeDockerClient {
        inspect_state_by_name: std::cell::RefCell::new(HashMap::from([(
            container_name.into(),
            ContainerState::Created,
        )])),
        ..Default::default()
    };
    let pending_dir = crate::runtime::coordination::universe_dir(&paths)
        .unwrap()
        .join("universe-pending");
    let mut runner = FakeRunner {
        side_effects: vec![(
            "jackin-capsule".into(),
            Box::new(move || {
                assert_eq!(
                    std::fs::read_dir(pending_dir).unwrap().count(),
                    1,
                    "entry must activate before foreground starts; another lease stays pending"
                );
            }),
        )],
        ..Default::default()
    };
    let admission = launch::AccountConfigRevision::acquire(&paths).unwrap();

    start_or_reconnect_capsule_client_with_handle_with_lease(
        &paths,
        container_name,
        &admission,
        &docker,
        &mut runner,
        Some(&container),
        Some(&claim),
    )
    .await
    .expect("restore should activate the entry and foreground attach");
    assert!(
        runner.side_effects.is_empty(),
        "foreground observation must run"
    );
    assert!(
        docker
            .bound_operations
            .borrow()
            .iter()
            .any(|op| op.starts_with("start:"))
    );
    assert_eq!(
        pending_entry_count(&paths),
        1,
        "other launch remains pending"
    );
    assert!(
        crate::runtime::coordination::universe_dir(&paths)
            .unwrap()
            .join("universe-since")
            .exists()
    );
    drop(claim);
    assert_eq!(pending_entry_count(&paths), 1);
    drop(other_claim);
    assert_eq!(pending_entry_count(&paths), 0);
}

#[tokio::test]
async fn restored_start_failure_keeps_entry_pending_until_owner_drops() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-restore-entry-start-failure";
    provision_account_admission(&paths, container_name);
    let claim = universe::claim_entry(&paths, &FakeDockerClient::default()).await;
    let container = ContainerHandle::new(container_name, container_name).unwrap();
    let docker = FakeDockerClient {
        inspect_state_by_name: std::cell::RefCell::new(HashMap::from([(
            container_name.into(),
            ContainerState::Created,
        )])),
        inspect_network_queue: std::cell::RefCell::new(VecDeque::from([Some(
            jackin_docker::docker_client::NetworkRow {
                name: format!("{container_name}-net"),
                labels: HashMap::default(),
            },
        )])),
        fail_with: vec![("start_container".into(), "role start failed".into())],
        ..Default::default()
    };
    let mut runner = FakeRunner::default();
    let admission = launch::AccountConfigRevision::acquire(&paths).unwrap();

    let error = start_or_reconnect_capsule_client_with_handle_with_lease(
        &paths,
        container_name,
        &admission,
        &docker,
        &mut runner,
        Some(&container),
        Some(&claim),
    )
    .await
    .expect_err("failed role start must preserve pending lease");

    assert!(
        format!("{error:#}").contains("role start failed"),
        "{error:#}"
    );
    assert_eq!(pending_entry_count(&paths), 1);
    assert!(
        !runner
            .recorded
            .iter()
            .any(|call| call.contains("jackin-capsule"))
    );
    drop(claim);
    assert_eq!(pending_entry_count(&paths), 0);
}
