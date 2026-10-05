// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `cleanup`.
use super::super::naming::matching_family;
use super::*;
use crate::instance::{DockerResources, InstanceManifest};
use crate::runtime::launch::LoadCleanup;
use jackin_core::RoleSelector;
use jackin_core::{DockerApi, JackinPaths};
use jackin_docker::docker_client::{ContainerRow, ContainerState, NetworkRow};
use jackin_test_support::{FakeDockerClient, FakeRunner};
use std::collections::{HashMap, VecDeque};
use tempfile::tempdir;

fn gc_test_paths() -> JackinPaths {
    let temp = tempdir().unwrap();
    JackinPaths::for_tests(temp.path())
}

fn network_id_for(container: &str) -> jackin_core::NetworkId {
    let hash = container.bytes().fold(1u128, |hash, byte| {
        hash.wrapping_mul(257).wrapping_add(u128::from(byte))
    });
    jackin_core::NetworkId::parse(&format!("{hash:064x}")).unwrap()
}

fn admit_fixture_network(docker: &FakeDockerClient, container: &str, name: &str) {
    let id = network_id_for(container);
    docker
        .network_id_by_name
        .borrow_mut()
        .insert(name.to_owned(), id.clone());
    docker
        .inspect_network_queue
        .borrow_mut()
        .push_back(Some(NetworkRow {
            id,
            name: name.to_owned(),
            labels: HashMap::from([(LABEL_ROLE_KEY.to_owned(), container.to_owned())]),
        }));
}

fn fixture_daemon() -> jackin_core::DaemonServerId {
    jackin_core::DaemonServerId::parse("jackin-test-daemon").unwrap()
}

fn fixture_lifetime(paths: &JackinPaths, owner: &str) -> crate::instance::SharedDockerLifetime {
    crate::instance::SharedDockerLifetime::load(paths, &fixture_daemon(), owner)
        .unwrap()
        .unwrap()
}

fn fixture_lifetime_path(
    paths: &JackinPaths,
    lifetime: &crate::instance::SharedDockerLifetime,
) -> std::path::PathBuf {
    std::fs::read_dir(paths.jackin_home.join("shared-docker-lifetimes"))
        .unwrap()
        .map(|entry| {
            entry
                .unwrap()
                .path()
                .join(format!("{}.json", lifetime.generation()))
        })
        .find(|path| path.exists())
        .expect("saved generation record must exist")
}

fn admit_fixture_shared(
    docker: &FakeDockerClient,
    lifetime: &crate::instance::SharedDockerLifetime,
) {
    let labels = HashMap::from([
        (
            "jackin.shared-generation".to_owned(),
            lifetime.generation().to_owned(),
        ),
        (
            "jackin.shared-owner".to_owned(),
            lifetime.namespace_owner().to_owned(),
        ),
        ("jackin.managed".to_owned(), "true".to_owned()),
        (LABEL_ROLE_KEY.to_owned(), lifetime.owner().to_owned()),
    ]);
    if let (Some(name), Some(id)) = (lifetime.network_name(), lifetime.network_id()) {
        docker
            .network_id_by_name
            .borrow_mut()
            .insert(name.to_owned(), id.clone());
        docker
            .inspect_network_queue
            .borrow_mut()
            .push_back(Some(NetworkRow {
                id: id.clone(),
                name: name.to_owned(),
                labels: labels.clone(),
            }));
    }
    if let Some(name) = lifetime.certs_volume_name() {
        docker.volumes_by_name.borrow_mut().insert(
            name.to_owned(),
            jackin_core::VolumeRow {
                name: name.to_owned(),
                labels,
                driver: "local".to_owned(),
            },
        );
    }
}

fn write_owned_cleanup_manifest(
    paths: &JackinPaths,
    role: &str,
    dind: &str,
) -> crate::instance::SharedDockerLifetime {
    write_cleanup_manifest_for_role(paths, role, dind, "agent-smith")
}

fn write_cleanup_manifest_for_role(
    paths: &JackinPaths,
    role: &str,
    dind: &str,
    role_key: &str,
) -> crate::instance::SharedDockerLifetime {
    let mut lifetime =
        crate::instance::SharedDockerLifetime::fresh(&fixture_daemon(), role, true, true).unwrap();
    lifetime.save_pending(paths).unwrap();
    lifetime.capture_network(network_id_for(role)).unwrap();
    lifetime.capture_certs_volume().unwrap();
    lifetime.save(paths).unwrap();
    let mut manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
        container_base: role,
        workspace_name: None,
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key,
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk_agent-smith",
        docker: DockerResources {
            role_container: role.to_owned(),
            dind_container: Some(dind.to_owned()),
            network: lifetime.network_name().unwrap().to_owned(),
            certs_volume: lifetime.certs_volume_name().map(str::to_owned),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });
    manifest.docker_identity = Some(crate::instance::DockerIdentity {
        role_container_id: role.to_owned(),
        dind_container_id: Some(dind.to_owned()),
        network_id: Some(network_id_for(role)),
    });
    manifest.write(&paths.data_dir.join(role)).unwrap();
    lifetime
}

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
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut lifetime =
        crate::instance::SharedDockerLifetime::fresh(&fixture_daemon(), role, true, true).unwrap();
    lifetime.save_pending(&paths).unwrap();
    lifetime.capture_network(network_id_for(role)).unwrap();
    lifetime.capture_certs_volume().unwrap();
    let custody =
        crate::runtime::launch::SharedDockerCreationCustody::adopt(&paths, lifetime.clone())
            .unwrap();
    admit_fixture_shared(&docker, &lifetime);
    docker.volumes_by_name.borrow_mut().insert(
        lifetime.certs_volume_name().unwrap().to_owned(),
        jackin_core::VolumeRow {
            name: lifetime.certs_volume_name().unwrap().to_owned(),
            driver: "local".to_owned(),
            labels: HashMap::from([
                (
                    "jackin.shared-generation".to_owned(),
                    lifetime.generation().to_owned(),
                ),
                (
                    "jackin.shared-owner".to_owned(),
                    lifetime.namespace_owner().to_owned(),
                ),
                ("jackin.managed".to_owned(), "true".to_owned()),
            ]),
        },
    );
    let cleanup = LoadCleanup::new(
        &paths,
        role.to_owned(),
        dind.to_owned(),
        lifetime.certs_volume_name().unwrap().to_owned(),
    )
    .unwrap();
    cleanup.set_dind_handle(dind_handle);
    cleanup.set_network_id(network_id_for(role));
    cleanup.bind_shared_custody(custody);

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
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(None, "agent-smith");
    let names = vec![
        "jk-k7p9m2xq-agentsmith".to_owned(),
        "jk-a1b2c3d4-myproject-agentsmith".to_owned(),
        "jk-w9x8y7z6-chainargos-thearchitect".to_owned(),
    ];

    for name in &names[..2] {
        write_cleanup_manifest_for_role(&paths, name, &format!("{name}-dind"), "agent-smith");
    }
    write_cleanup_manifest_for_role(
        &paths,
        &names[2],
        &format!("{}-dind", names[2]),
        "chainargos/the-architect",
    );
    let matched = matching_family(&paths, &selector, &names).unwrap();

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
    write_owned_cleanup_manifest(&paths, primary, &format!("{primary}-dind"));
    let manifest = InstanceManifest::read_optional(&paths.data_dir.join(primary))
        .unwrap()
        .unwrap();
    manifest.write(&paths.data_dir.join(primary)).unwrap();
    InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
    write_owned_cleanup_manifest(&paths, second, &format!("{second}-dind"));
    let second_manifest = InstanceManifest::read_optional(&paths.data_dir.join(second))
        .unwrap()
        .unwrap();
    second_manifest.write(&paths.data_dir.join(second)).unwrap();
    InstanceIndex::update_manifest(&paths.data_dir, &second_manifest).unwrap();
    let unrelated = "jk-w9x8y7z6-chainargos-thearchitect";
    write_cleanup_manifest_for_role(
        &paths,
        unrelated,
        &format!("{unrelated}-dind"),
        "chainargos/the-architect",
    );
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
    write_owned_cleanup_manifest(&paths, container, &format!("{container}-dind"));
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
    write_owned_cleanup_manifest(&paths, container, &format!("{container}-dind"));
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
        write_owned_cleanup_manifest(&paths, container, &format!("{container}-dind"));
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

    admit_fixture_shared(&docker, &fixture_lifetime(&paths, "jk-agent-smith"));

    eject_role(&paths, "jk-agent-smith", &docker).await.unwrap();

    assert_eq!(
        docker
            .recorded
            .borrow()
            .iter()
            .filter(|operation| !operation.starts_with("docker network inspect")
                && !operation.starts_with("docker volume inspect")
                && !operation.starts_with("docker info"))
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            "docker inspect jk-agent-smith",
            "docker inspect jk-agent-smith-dind",
            "docker rm -f jk-agent-smith",
            "docker rm -f jk-agent-smith-dind",
            format!(
                "docker volume rm {}",
                fixture_lifetime(&paths, "jk-agent-smith")
                    .certs_volume_name()
                    .unwrap()
            )
            .as_str(),
            format!(
                "docker network rm {}",
                fixture_lifetime(&paths, "jk-agent-smith")
                    .network_name()
                    .unwrap()
            )
            .as_str(),
        ]
    );
    assert!(docker.bound_operations.borrow().contains(&format!(
        "remove_network:{}",
        network_id_for("jk-agent-smith")
    )));
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
    let lifetime = write_owned_cleanup_manifest(&paths, container, "jk-prewarm-dind-dind");
    admit_fixture_shared(&docker, &lifetime);

    eject_role(&paths, container, &docker).await.unwrap();

    assert_eq!(
        docker
            .recorded
            .borrow()
            .iter()
            .filter(|operation| !operation.starts_with("docker network inspect")
                && !operation.starts_with("docker volume inspect")
                && !operation.starts_with("docker info"))
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            "docker inspect jk-agent-smith",
            "docker inspect jk-prewarm-dind-dind",
            "docker rm -f jk-agent-smith",
            "docker rm -f jk-prewarm-dind-dind",
            format!(
                "docker volume rm {}",
                fixture_lifetime(&paths, "jk-agent-smith")
                    .certs_volume_name()
                    .unwrap()
            )
            .as_str(),
            format!(
                "docker network rm {}",
                fixture_lifetime(&paths, "jk-agent-smith")
                    .network_name()
                    .unwrap()
            )
            .as_str(),
        ]
    );
    assert!(docker.bound_operations.borrow().contains(&format!(
        "remove_network:{}",
        network_id_for("jk-agent-smith")
    )));
}

#[tokio::test]
async fn eject_agent_ignores_missing_runtime_resources() {
    let docker = FakeDockerClient::default();
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    write_owned_cleanup_manifest(&paths, "jk-agent-smith", "jk-agent-smith-dind");

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
        admit_fixture_shared(&docker, &fixture_lifetime(&paths, role));
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
    assert!(docker.recorded.borrow().iter().any(|c| {
        c.contains(
            format!(
                "docker volume rm {}",
                fixture_lifetime(&paths, "jk-k7p9m2xq-agentsmith")
                    .certs_volume_name()
                    .unwrap()
            )
            .as_str(),
        )
    }));
    assert!(docker.recorded.borrow().iter().any(|c| {
        c.contains(
            format!(
                "docker network rm {}",
                fixture_lifetime(&paths, "jk-k7p9m2xq-agentsmith")
                    .network_name()
                    .unwrap()
            )
            .as_str(),
        )
    }));
    assert!(docker.bound_operations.borrow().contains(&format!(
        "remove_network:{}",
        network_id_for("jk-k7p9m2xq-agentsmith")
    )));
    assert!(docker.bound_operations.borrow().contains(&format!(
        "remove_network:{}",
        network_id_for("jk-a1b2c3d4-myworkspace-agentsmith")
    )));
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
        admit_fixture_shared(&docker, &fixture_lifetime(&paths, role));
    }
    exile_all(&paths, &docker).await.unwrap();

    assert_eq!(
        docker
            .recorded
            .borrow()
            .iter()
            .filter(|operation| !operation.starts_with("docker network inspect")
                && !operation.starts_with("docker volume inspect")
                && !operation.starts_with("docker info"))
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            "docker ps -a --filter jackin.kind=role",
            "docker inspect jk-k7p9m2xq-agentsmith",
            "docker inspect jk-k7p9m2xq-agentsmith-dind",
            "docker inspect jk-a1b2c3d4-myworkspace-agentsmith",
            "docker inspect jk-a1b2c3d4-myworkspace-agentsmith-dind",
            "docker rm -f jk-k7p9m2xq-agentsmith",
            "docker rm -f jk-k7p9m2xq-agentsmith-dind",
            format!(
                "docker volume rm {}",
                fixture_lifetime(&paths, "jk-k7p9m2xq-agentsmith")
                    .certs_volume_name()
                    .unwrap()
            )
            .as_str(),
            format!(
                "docker network rm {}",
                fixture_lifetime(&paths, "jk-k7p9m2xq-agentsmith")
                    .network_name()
                    .unwrap()
            )
            .as_str(),
            "docker rm -f jk-a1b2c3d4-myworkspace-agentsmith",
            "docker rm -f jk-a1b2c3d4-myworkspace-agentsmith-dind",
            format!(
                "docker volume rm {}",
                fixture_lifetime(&paths, "jk-a1b2c3d4-myworkspace-agentsmith")
                    .certs_volume_name()
                    .unwrap()
            )
            .as_str(),
            format!(
                "docker network rm {}",
                fixture_lifetime(&paths, "jk-a1b2c3d4-myworkspace-agentsmith")
                    .network_name()
                    .unwrap()
            )
            .as_str(),
        ]
    );
    assert!(docker.bound_operations.borrow().contains(&format!(
        "remove_network:{}",
        network_id_for("jk-k7p9m2xq-agentsmith")
    )));
    assert!(docker.bound_operations.borrow().contains(&format!(
        "remove_network:{}",
        network_id_for("jk-a1b2c3d4-myworkspace-agentsmith")
    )));
}

#[tokio::test]
async fn exile_all_admits_every_identity_before_first_removal() {
    for unavailable_by_id in [false, true] {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let first = "jk-aaaaaaaa-agentsmith";
        let later = "jk-bbbbbbbb-agentsmith";
        let docker = FakeDockerClient::default();
        for container in [first, later] {
            let dind = format!("{container}-dind");
            write_owned_cleanup_manifest(&paths, container, &dind);
            docker.inspect_state_by_name.borrow_mut().extend([
                (container.to_owned(), ContainerState::Running),
                (dind, ContainerState::Running),
            ]);
        }
        let later_manifest = paths.data_dir.join(later).join(".jackin/instance.json");
        if unavailable_by_id {
            docker.inspect_by_id_queue.borrow_mut().extend([
                ContainerState::Running,
                ContainerState::Running,
                ContainerState::InspectUnavailable("later immutable ID unavailable".to_owned()),
            ]);
        } else {
            let mut manifest = InstanceManifest::read_optional(&paths.data_dir.join(later))
                .unwrap()
                .unwrap();
            manifest.docker_identity = None;
            manifest.write(&paths.data_dir.join(later)).unwrap();
        }
        let manifest_bytes = std::fs::read(&later_manifest).unwrap();

        exile_all(&paths, &docker).await.unwrap_err();

        assert_eq!(std::fs::read(later_manifest).unwrap(), manifest_bytes);
        assert!(paths.data_dir.join(first).exists());
        assert!(
            !docker
                .bound_operations
                .borrow()
                .iter()
                .any(|operation| operation.starts_with("remove:"))
        );
        assert!(!docker.recorded.borrow().iter().any(
            |operation| operation.starts_with("docker rm")
                || operation.starts_with("docker network rm")
                || operation.starts_with("docker volume rm")
        ));
        assert!(
            docker
                .bound_operations
                .borrow()
                .iter()
                .any(|operation| operation.starts_with("inspect:")),
            "earlier valid identity must be admitted before later rejection"
        );
    }
}

#[tokio::test]
async fn exile_all_refuses_later_symlinked_socket_before_any_docker_mutation() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let first = "jk-aaaaaaaa-agentsmith";
    let later = "jk-bbbbbbbb-agentsmith";
    let docker = FakeDockerClient::default();
    for container in [first, later] {
        let sidecar = format!("{container}-dind");
        write_owned_cleanup_manifest(&paths, container, &sidecar);
        docker.inspect_state_by_name.borrow_mut().extend([
            (container.to_owned(), ContainerState::Running),
            (sidecar, ContainerState::Running),
        ]);
    }
    let outside = temp.path().join("outside-sockets");
    std::fs::create_dir_all(&outside).unwrap();
    let canary = outside.join("retain");
    std::fs::write(&canary, b"outside socket bytes").unwrap();
    let sockets = paths.jackin_home.join("sockets");
    std::fs::create_dir_all(&sockets).unwrap();
    let socket_link = sockets.join(later);
    std::os::unix::fs::symlink(&outside, &socket_link).unwrap();
    let first_manifest = paths.data_dir.join(first).join(".jackin/instance.json");
    let later_manifest = paths.data_dir.join(later).join(".jackin/instance.json");
    let first_bytes = std::fs::read(&first_manifest).unwrap();
    let later_bytes = std::fs::read(&later_manifest).unwrap();

    exile_all(&paths, &docker).await.unwrap_err();

    assert_eq!(std::fs::read(first_manifest).unwrap(), first_bytes);
    assert_eq!(std::fs::read(later_manifest).unwrap(), later_bytes);
    assert_eq!(std::fs::read(canary).unwrap(), b"outside socket bytes");
    assert!(
        std::fs::symlink_metadata(socket_link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        !docker
            .bound_operations
            .borrow()
            .iter()
            .any(|operation| operation.starts_with("remove:")
                || operation.starts_with("remove_network:"))
    );
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|operation| operation.starts_with("docker rm")
                || operation.starts_with("docker network rm")
                || operation.starts_with("docker volume rm"))
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
                id: "jk-agent-smith-dind".to_owned(),
                labels: labels.clone(),
            }],
            // list_role_names (running): no running role containers
            vec![],
        ])),
        list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // gc_orphaned_networks: no networks
        ..Default::default()
    };

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    write_owned_cleanup_manifest(&paths, "jk-agent-smith", "jk-agent-smith-dind");
    admit_fixture_shared(&docker, &fixture_lifetime(&paths, "jk-agent-smith"));
    docker
        .inspect_state_by_name
        .borrow_mut()
        .insert("jk-agent-smith-dind".to_owned(), ContainerState::Running);

    gc_orphaned_resources(&paths, &docker).await;

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
            .any(|c| c == "docker inspect jk-agent-smith")
    );
    assert!(docker.recorded.borrow().iter().any(|c| {
        c.contains(
            format!(
                "docker volume rm {}",
                fixture_lifetime(&paths, "jk-agent-smith")
                    .certs_volume_name()
                    .unwrap()
            )
            .as_str(),
        )
    }));
    assert!(docker.recorded.borrow().iter().any(|c| {
        c.contains(
            format!(
                "docker network rm {}",
                fixture_lifetime(&paths, "jk-agent-smith")
                    .network_name()
                    .unwrap()
            )
            .as_str(),
        )
    }));
    assert!(docker.bound_operations.borrow().contains(&format!(
        "remove_network:{}",
        network_id_for("jk-agent-smith")
    )));
}

#[tokio::test]
async fn gc_retains_unowned_sidecar_when_role_name_is_replaced() {
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

    assert!(docker.bound_operations.borrow().is_empty());
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
    let lifetime = write_owned_cleanup_manifest(&paths, "jk-prewarm-dind", "jk-prewarm-dind-dind");
    crate::runtime::launch::write_prewarmed_dind_state(
        &paths,
        &crate::runtime::launch::DindSidecarPrewarm {
            dind: "jk-prewarm-dind-dind".to_owned(),
            dind_id: "prewarm-dind-id".to_owned(),
            network: lifetime.network_name().unwrap().to_owned(),
            network_id: lifetime.network_id().unwrap().clone(),
            lifetime_owner: lifetime.owner().to_owned(),
            certs_volume: lifetime.certs_volume_name().unwrap().to_owned(),
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
                id: network_id_for("jk-agent-smith"),
                name: "jk-agent-smith-net".to_owned(),
                labels: net_labels,
            }],
        ])),
        ..Default::default()
    };

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    write_owned_cleanup_manifest(&paths, "jk-agent-smith", "jk-agent-smith-dind");
    admit_fixture_shared(&docker, &fixture_lifetime(&paths, "jk-agent-smith"));

    let network = docker
        .inspect_network_queue
        .borrow()
        .front()
        .unwrap()
        .as_ref()
        .unwrap()
        .clone();
    *docker.list_networks_queue.borrow_mut() = VecDeque::from([vec![network]]);
    gc_orphaned_resources(&paths, &docker).await;

    assert!(docker.recorded.borrow().iter().any(|c| {
        c.contains(
            format!(
                "docker network rm {}",
                fixture_lifetime(&paths, "jk-agent-smith")
                    .network_name()
                    .unwrap()
            )
            .as_str(),
        )
    }));
    assert!(docker.bound_operations.borrow().contains(&format!(
        "remove_network:{}",
        network_id_for("jk-agent-smith")
    )));
}

#[tokio::test]
async fn gc_retains_network_without_persisted_ownership_identity() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container = "jk-agent-smith";
    let id = network_id_for(container);
    let docker = FakeDockerClient {
        list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![NetworkRow {
            id: id.clone(),
            name: "jk-agent-smith-net".to_owned(),
            labels: HashMap::from([(LABEL_ROLE_KEY.to_owned(), container.to_owned())]),
        }]])),
        ..Default::default()
    };
    admit_fixture_network(&docker, container, "jk-agent-smith-net");

    gc_orphaned_networks(&paths, &docker, None).await;

    assert!(
        !docker
            .bound_operations
            .borrow()
            .iter()
            .any(|operation| operation.starts_with("remove_network:"))
    );
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|operation| operation.starts_with("docker network rm"))
    );
    assert_eq!(
        docker.network_id_by_name.borrow().get("jk-agent-smith-net"),
        Some(&id)
    );
}

#[tokio::test]
async fn load_cleanup_rejects_unadmitted_shared_custody_before_any_effect() {
    for fault in [
        "pending-network",
        "pending-volume",
        "durable-mismatch",
        "corrupt-ledger",
        "foreign-volume-labels",
        "missing-dind-handle",
        "missing-shared-custody",
        "changed-daemon",
    ] {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let owner = "jk-agent-smith";
        let docker = FakeDockerClient::default();
        let mut lifetime =
            crate::instance::SharedDockerLifetime::fresh(&fixture_daemon(), owner, true, true)
                .unwrap();
        lifetime.save_pending(&paths).unwrap();
        if fault != "pending-network" {
            lifetime.capture_network(network_id_for(owner)).unwrap();
        }
        if fault != "pending-volume" && fault != "pending-network" {
            lifetime.capture_certs_volume().unwrap();
        }
        let custody =
            crate::runtime::launch::SharedDockerCreationCustody::adopt(&paths, lifetime.clone())
                .unwrap();
        let ledger = fixture_lifetime_path(&paths, &lifetime);
        if fault == "durable-mismatch" {
            let mut durable = serde_json::to_value(&lifetime).unwrap();
            durable["certs_volume"] = serde_json::json!({"state": "pending", "name": lifetime.certs_volume_name().unwrap()});
            std::fs::write(&ledger, serde_json::to_vec(&durable).unwrap()).unwrap();
        } else if fault == "corrupt-ledger" {
            std::fs::write(&ledger, b"{corrupt durable lifetime").unwrap();
        }
        let ledger_bytes = std::fs::read(&ledger).unwrap();
        let socket = paths.jackin_home.join("sockets").join(owner);
        std::fs::create_dir_all(&socket).unwrap();
        let socket_config = socket.join("agent.toml");
        std::fs::write(&socket_config, b"retain rollback socket").unwrap();
        if lifetime.network_id().is_some() {
            admit_fixture_shared(&docker, &lifetime);
        }
        let mut labels = HashMap::from([
            (
                "jackin.shared-generation".to_owned(),
                lifetime.generation().to_owned(),
            ),
            (
                "jackin.shared-owner".to_owned(),
                lifetime.namespace_owner().to_owned(),
            ),
            ("jackin.managed".to_owned(), "true".to_owned()),
        ]);
        if fault == "foreign-volume-labels" {
            labels.insert(
                "jackin.shared-generation".to_owned(),
                "foreign-generation".to_owned(),
            );
        }
        let volume = lifetime.certs_volume_name().unwrap();
        docker.volumes_by_name.borrow_mut().insert(
            volume.to_owned(),
            jackin_core::VolumeRow {
                name: volume.to_owned(),
                labels,
                driver: "local".to_owned(),
            },
        );
        let cleanup = LoadCleanup::new(
            &paths,
            owner.to_owned(),
            "jk-agent-smith-dind".to_owned(),
            volume.to_owned(),
        )
        .unwrap();
        cleanup.set_role_handle(jackin_core::ContainerHandle::new(owner, "captured-role").unwrap());
        if fault == "missing-dind-handle" {
            cleanup.set_dind_required(true);
        } else {
            cleanup.set_dind_handle(
                jackin_core::ContainerHandle::new("jk-agent-smith-dind", "captured-dind").unwrap(),
            );
        }
        if let Some(network) = lifetime.network_id() {
            cleanup.set_network_id(network.clone());
        }
        if fault != "missing-shared-custody" {
            cleanup.bind_shared_custody(custody);
        }

        if fault == "changed-daemon" {
            docker.set_daemon_server_id(
                jackin_core::DaemonServerId::parse("different-daemon").unwrap(),
            );
        }
        cleanup.run(&docker).await;

        assert_eq!(std::fs::read(&ledger).unwrap(), ledger_bytes, "{fault}");
        assert_eq!(
            std::fs::read(socket_config).unwrap(),
            b"retain rollback socket",
            "{fault}"
        );
        assert!(
            !docker
                .bound_operations
                .borrow()
                .iter()
                .any(|operation| operation.starts_with("remove:")
                    || operation.starts_with("remove_network:")),
            "{fault}"
        );
        assert!(
            !docker
                .recorded
                .borrow()
                .iter()
                .any(|operation| operation.starts_with("docker rm")
                    || operation.starts_with("docker network rm")
                    || operation.starts_with("docker volume rm")),
            "{fault}"
        );
    }
}

#[tokio::test]
async fn load_cleanup_refuses_exact_socket_symlink_before_any_docker_effect() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let owner = "jk-agent-smith";
    let lifetime = write_owned_cleanup_manifest(&paths, owner, "jk-agent-smith-dind");
    let custody =
        crate::runtime::launch::SharedDockerCreationCustody::adopt(&paths, lifetime.clone())
            .unwrap();
    let ledger = fixture_lifetime_path(&paths, &lifetime);
    let ledger_bytes = std::fs::read(&ledger).unwrap();
    let outside = temp.path().join("outside-rollback-socket");
    std::fs::create_dir_all(&outside).unwrap();
    let config = outside.join("agent.toml");
    std::fs::write(&config, b"retain outside socket config").unwrap();
    let sockets = paths.jackin_home.join("sockets");
    std::fs::create_dir_all(&sockets).unwrap();
    let socket = sockets.join(owner);
    std::os::unix::fs::symlink(&outside, &socket).unwrap();
    let docker = FakeDockerClient::default();
    admit_fixture_shared(&docker, &lifetime);
    let cleanup = LoadCleanup::new(
        &paths,
        owner.to_owned(),
        "jk-agent-smith-dind".to_owned(),
        lifetime.certs_volume_name().unwrap().to_owned(),
    )
    .unwrap();
    cleanup.set_role_handle(jackin_core::ContainerHandle::new(owner, "captured-role").unwrap());
    cleanup.set_dind_handle(
        jackin_core::ContainerHandle::new("jk-agent-smith-dind", "captured-dind").unwrap(),
    );
    cleanup.set_network_id(lifetime.network_id().unwrap().clone());
    cleanup.bind_shared_custody(custody);

    cleanup.run(&docker).await;

    assert_eq!(std::fs::read(ledger).unwrap(), ledger_bytes);
    assert_eq!(
        std::fs::read(config).unwrap(),
        b"retain outside socket config"
    );
    assert!(
        std::fs::symlink_metadata(socket)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        !docker
            .bound_operations
            .borrow()
            .iter()
            .any(|operation| operation.starts_with("remove:")
                || operation.starts_with("remove_network:"))
    );
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|operation| operation.starts_with("docker rm")
                || operation.starts_with("docker network rm")
                || operation.starts_with("docker volume rm"))
    );
}

#[tokio::test]
async fn load_cleanup_retains_socket_created_after_absence_admission() {
    thread_local! {
        static APPEARING_SOCKET: std::cell::RefCell<Option<std::path::PathBuf>> = const { std::cell::RefCell::new(None) };
    }
    fn create_socket_during_removal(operation: &str) {
        if operation.starts_with("docker rm -f jk-agent-smith") {
            APPEARING_SOCKET.with(|slot| {
                if let Some(path) = slot.borrow_mut().take() {
                    std::fs::create_dir_all(&path).unwrap();
                    std::fs::write(path.join("agent.toml"), b"new socket after admission").unwrap();
                }
            });
        }
    }
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let owner = "jk-agent-smith";
    let lifetime = write_owned_cleanup_manifest(&paths, owner, "jk-agent-smith-dind");
    let custody =
        crate::runtime::launch::SharedDockerCreationCustody::adopt(&paths, lifetime.clone())
            .unwrap();
    let socket = paths.jackin_home.join("sockets").join(owner);
    assert!(!socket.exists());
    APPEARING_SOCKET.with(|slot| *slot.borrow_mut() = Some(socket.clone()));
    let docker = FakeDockerClient {
        operation_hook: Some(create_socket_during_removal),
        ..Default::default()
    };
    admit_fixture_shared(&docker, &lifetime);
    let cleanup = LoadCleanup::new(
        &paths,
        owner.to_owned(),
        "jk-agent-smith-dind".to_owned(),
        lifetime.certs_volume_name().unwrap().to_owned(),
    )
    .unwrap();
    cleanup.set_role_handle(jackin_core::ContainerHandle::new(owner, "captured-role").unwrap());
    cleanup.set_dind_handle(
        jackin_core::ContainerHandle::new("jk-agent-smith-dind", "captured-dind").unwrap(),
    );
    cleanup.set_network_id(lifetime.network_id().unwrap().clone());
    cleanup.bind_shared_custody(custody);

    cleanup.run(&docker).await;

    assert_eq!(
        std::fs::read(socket.join("agent.toml")).unwrap(),
        b"new socket after admission"
    );
    assert!(
        docker
            .bound_operations
            .borrow()
            .contains(&"remove:captured-role".to_owned()),
        "hook must follow admitted Docker effects"
    );
    APPEARING_SOCKET
        .with(|slot| assert!(slot.borrow().is_none(), "socket creation hook must fire"));
}

#[tokio::test]
async fn load_cleanup_rolls_back_captured_network_without_container_handles() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container = "jk-agent-smith";
    let docker = FakeDockerClient::default();
    let id = network_id_for(container);
    docker
        .network_id_by_name
        .borrow_mut()
        .insert("jk-agent-smith-net".to_owned(), id.clone());
    let cleanup = LoadCleanup::new(
        &paths,
        container.to_owned(),
        "jk-agent-smith-dind".to_owned(),
        "jk-agent-smith-dind-certs".to_owned(),
    )
    .unwrap();
    cleanup.set_network_id(id.clone());

    cleanup.run(&docker).await;

    assert!(
        docker
            .bound_operations
            .borrow()
            .contains(&format!("remove_network:{id}"))
    );
    assert!(
        !docker
            .bound_operations
            .borrow()
            .iter()
            .any(|operation| operation.starts_with("remove:"))
    );
    assert!(
        !docker
            .network_id_by_name
            .borrow()
            .contains_key("jk-agent-smith-net")
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
                id: network_id_for("jk-agent-smith"),
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
                    id: "jk-agent-smith-dind".to_owned(),
                    labels: labels_smith,
                },
                ContainerRow {
                    name: "jk-neo-dind".to_owned(),
                    id: "jk-neo-dind".to_owned(),
                    labels: labels_neo,
                },
            ],
            // list_role_names (running): no running roles
            vec![],
        ])),
        list_networks_queue: std::cell::RefCell::new(VecDeque::from([vec![]])), // gc_orphaned_networks: no networks
        ..Default::default()
    };

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    write_owned_cleanup_manifest(&paths, "jk-agent-smith", "jk-agent-smith-dind");
    admit_fixture_shared(&docker, &fixture_lifetime(&paths, "jk-agent-smith"));
    docker
        .inspect_state_by_name
        .borrow_mut()
        .insert("jk-agent-smith-dind".to_owned(), ContainerState::Running);
    write_owned_cleanup_manifest(&paths, "jk-neo", "jk-neo-dind");
    admit_fixture_shared(&docker, &fixture_lifetime(&paths, "jk-neo"));
    docker
        .inspect_state_by_name
        .borrow_mut()
        .insert("jk-neo-dind".to_owned(), ContainerState::Running);

    gc_orphaned_resources(&paths, &docker).await;

    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rm -f jk-agent-smith-dind"))
    );
    assert!(docker.recorded.borrow().iter().any(|c| {
        c.contains(
            format!(
                "docker volume rm {}",
                fixture_lifetime(&paths, "jk-agent-smith")
                    .certs_volume_name()
                    .unwrap()
            )
            .as_str(),
        )
    }));
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rm -f jk-neo-dind"))
    );
    assert!(docker.recorded.borrow().iter().any(|c| {
        c.contains(
            format!(
                "docker volume rm {}",
                fixture_lifetime(&paths, "jk-neo")
                    .certs_volume_name()
                    .unwrap()
            )
            .as_str(),
        )
    }));
    assert!(docker.recorded.borrow().iter().any(|c| {
        c.contains(
            format!(
                "docker network rm {}",
                fixture_lifetime(&paths, "jk-neo").network_name().unwrap()
            )
            .as_str(),
        )
    }));
    assert!(docker.bound_operations.borrow().contains(&format!(
        "remove_network:{}",
        network_id_for("jk-agent-smith")
    )));
    assert!(
        docker
            .bound_operations
            .borrow()
            .contains(&format!("remove_network:{}", network_id_for("jk-neo")))
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
            id: network_id_for("jk-agent-smith"),
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
    write_owned_cleanup_manifest(paths, container, &format!("{container}-dind"));
    let mut manifest = InstanceManifest::read_optional(&paths.data_dir.join(container))
        .unwrap()
        .unwrap();
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
    let index_path = paths.data_dir.join("instances.json");
    let index_bytes = std::fs::read(&index_path).unwrap();
    let manifest_path = paths.data_dir.join(container).join(".jackin/instance.json");
    let manifest_bytes = std::fs::read(&manifest_path).unwrap();

    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Running])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();
    prune_instances(&paths, &docker, &mut runner)
        .await
        .unwrap_err();

    assert!(paths.data_dir.join(container).exists());
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
    assert!(
        index
            .instances
            .iter()
            .any(|e| e.container_base == container)
    );
    assert_eq!(std::fs::read(index_path).unwrap(), index_bytes);
    assert!(runner.recorded.is_empty());
    assert!(
        !docker
            .bound_operations
            .borrow()
            .iter()
            .any(|operation| operation.starts_with("remove:")
                || operation.starts_with("remove_network:"))
    );
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|operation| operation.starts_with("docker rm")
                || operation.starts_with("docker network rm")
                || operation.starts_with("docker volume rm"))
    );
    assert_eq!(std::fs::read(manifest_path).unwrap(), manifest_bytes);
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
async fn normal_prune_preflight_preserves_payload_index_and_stale_active_manifest() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let stale_active = "jk-aaaaaaaa-agentsmith";
    let earlier = "jk-bbbbbbbb-agentsmith";
    let corrupt = "jk-cccccccc-agentsmith";
    make_instance_at(&paths, stale_active, InstanceStatus::Active);
    make_instance_at(&paths, earlier, InstanceStatus::CleanExited);
    make_instance_at(&paths, corrupt, InstanceStatus::FailedSetup);
    let earlier_state = paths.data_dir.join(earlier);
    let clone =
        crate::isolation::materialize::clone_path_for(&earlier_state, "/workspace", earlier);
    std::fs::create_dir_all(&clone).unwrap();
    let payload = clone.join("payload");
    std::fs::write(&payload, b"retain earlier terminal clone").unwrap();
    crate::isolation::state::write_records(
        &earlier_state,
        &[jackin_core::IsolationRecord {
            workspace_name: None,
            mount_dst: "/workspace".to_owned(),
            original_src: temp.path().join("source").display().to_string(),
            isolation: jackin_core::MountIsolation::Clone,
            worktree_path: clone.display().to_string(),
            scratch_branch: String::new(),
            base_commit: String::new(),
            selector_key: "agent-smith".to_owned(),
            container_name: earlier.to_owned(),
            cleanup_status: jackin_core::CleanupStatus::Active,
        }],
    )
    .unwrap();
    let corrupt_records = paths.data_dir.join(corrupt).join(".jackin/isolation.json");
    std::fs::write(&corrupt_records, b"{later corrupt terminal records").unwrap();
    let snapshots: Vec<_> = [
        paths.data_dir.join("instances.json"),
        paths
            .data_dir
            .join(stale_active)
            .join(".jackin/instance.json"),
        earlier_state.join(".jackin/instance.json"),
        earlier_state.join(".jackin/isolation.json"),
        paths.data_dir.join(corrupt).join(".jackin/instance.json"),
        corrupt_records,
        payload,
    ]
    .into_iter()
    .map(|path| {
        let bytes = std::fs::read(&path).unwrap();
        (path, bytes)
    })
    .collect();
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();

    prune_instances(&paths, &docker, &mut runner)
        .await
        .unwrap_err();

    for (path, bytes) in snapshots {
        assert_eq!(
            std::fs::read(&path).unwrap(),
            bytes,
            "changed {}",
            path.display()
        );
    }
    let manifest = InstanceManifest::read_optional(&paths.data_dir.join(stale_active))
        .unwrap()
        .unwrap();
    assert_eq!(
        manifest.status,
        InstanceStatus::Active,
        "failed admission must not reconcile stale Active state"
    );
    assert!(runner.recorded.is_empty());
    assert!(
        !docker
            .bound_operations
            .borrow()
            .iter()
            .any(|operation| operation.starts_with("remove:")
                || operation.starts_with("remove_network:"))
    );
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|operation| operation.starts_with("docker rm")
                || operation.starts_with("docker network rm")
                || operation.starts_with("docker volume rm"))
    );
}

#[tokio::test]
async fn prune_instances_preserves_held_and_idle_coordination_inodes() {
    use fs4::FileExt;
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    let held = super::super::coordination::open_lock(&paths, "name-jk-held").unwrap();
    let idle = super::super::coordination::open_lock(&paths, "name-jk-idle").unwrap();
    FileExt::try_lock(&held).unwrap();
    FileExt::try_lock(&idle).unwrap();
    FileExt::unlock(&idle).unwrap();
    let idle_path = super::super::coordination::root(&paths)
        .unwrap()
        .join("name-jk-idle.lock");
    let held_path = super::super::coordination::root(&paths)
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

#[tokio::test]
async fn prune_instances_retains_unowned_purged_tombstone_without_manifest() {
    // An index tombstone alone supplies no persisted deletion authority.
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

    let index_path = paths.data_dir.join("instances.json");
    let index_bytes = std::fs::read(&index_path).unwrap();

    let docker = FakeDockerClient::default(); // inspect returns NotFound → allow purge
    let mut runner = FakeRunner::default();
    prune_instances(&paths, &docker, &mut runner)
        .await
        .unwrap_err();

    let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
    assert!(
        index
            .instances
            .iter()
            .any(|e| e.container_base == container),
        "unknown tombstone custody must remain in the index"
    );
    assert_eq!(std::fs::read(index_path).unwrap(), index_bytes);
    assert!(runner.recorded.is_empty());
    assert!(
        !docker
            .bound_operations
            .borrow()
            .iter()
            .any(|operation| operation.starts_with("remove:")
                || operation.starts_with("remove_network:"))
    );
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|operation| operation.starts_with("docker rm")
                || operation.starts_with("docker network rm")
                || operation.starts_with("docker volume rm"))
    );
    assert!(docker.recorded.borrow().is_empty());
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
async fn prune_all_instances_retains_unowned_lock_directory() {
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
        .unwrap_err();

    assert!(paths.data_dir.exists());
    assert_eq!(
        std::fs::read(paths.data_dir.join("jk-abc123-thearchitect.lock")).unwrap(),
        b""
    );
    assert_eq!(
        std::fs::read(paths.data_dir.join("caffeinate.pid")).unwrap(),
        b"99999"
    );
    assert_eq!(
        std::fs::read(paths.data_dir.join("the-architect.locks/default.repo.lock")).unwrap(),
        b""
    );
    assert!(docker.bound_operations.borrow().is_empty());
    assert!(docker.recorded.borrow().is_empty());
}

#[tokio::test]
async fn prune_all_instances_retains_unowned_file_when_index_empty() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    std::fs::write(paths.data_dir.join("jk-stale.lock"), b"").unwrap();

    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();
    prune_all_instances(&paths, &docker, &mut runner)
        .await
        .unwrap_err();

    assert_eq!(
        std::fs::read(paths.data_dir.join("jk-stale.lock")).unwrap(),
        b""
    );
    assert!(docker.bound_operations.borrow().is_empty());
    assert!(docker.recorded.borrow().is_empty());
}

#[tokio::test]
async fn class_purge_uses_exact_persisted_role_namespace() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selected = "jk-aaaaaaaa-agentsmith";
    let unrelated = "jk-bbbbbbbb-agentsmith";
    write_cleanup_manifest_for_role(
        &paths,
        selected,
        &format!("{selected}-dind"),
        "alpha/agent-smith",
    );
    write_cleanup_manifest_for_role(
        &paths,
        unrelated,
        &format!("{unrelated}-dind"),
        "beta/agent-smith",
    );
    let retained_manifest = paths.data_dir.join(unrelated).join(".jackin/instance.json");
    let retained_bytes = std::fs::read(&retained_manifest).unwrap();
    let slug_collision = "jk-cccccccc-agentsmith";
    write_cleanup_manifest_for_role(
        &paths,
        slug_collision,
        &format!("{slug_collision}-dind"),
        "alpha/agentsmith",
    );
    let collision_manifest = paths
        .data_dir
        .join(slug_collision)
        .join(".jackin/instance.json");
    let collision_bytes = std::fs::read(&collision_manifest).unwrap();
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();

    purge_class_data(
        &paths,
        &RoleSelector::new(Some("alpha"), "agent-smith"),
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert!(!paths.data_dir.join(selected).exists());
    assert_eq!(std::fs::read(retained_manifest).unwrap(), retained_bytes);
    assert_eq!(std::fs::read(collision_manifest).unwrap(), collision_bytes);
    assert!(docker.bound_operations.borrow().is_empty());
}

#[tokio::test]
async fn local_and_class_purge_retain_live_shared_resources_without_containers() {
    for class_purge in [false, true] {
        for live_network in [false, true] {
            let temp = tempdir().unwrap();
            let paths = JackinPaths::for_tests(temp.path());
            let container = "jk-aaaaaaaa-agentsmith";
            let lifetime =
                write_owned_cleanup_manifest(&paths, container, &format!("{container}-dind"));
            let state = paths.data_dir.join(container);
            let clone =
                crate::isolation::materialize::clone_path_for(&state, "/workspace", container);
            std::fs::create_dir_all(&clone).unwrap();
            let payload = clone.join("payload");
            std::fs::write(&payload, b"retain live shared custody").unwrap();
            crate::isolation::state::write_records(
                &state,
                &[jackin_core::IsolationRecord {
                    workspace_name: None,
                    mount_dst: "/workspace".to_owned(),
                    original_src: temp.path().join("source").display().to_string(),
                    isolation: jackin_core::MountIsolation::Clone,
                    worktree_path: clone.display().to_string(),
                    scratch_branch: String::new(),
                    base_commit: String::new(),
                    selector_key: "agent-smith".to_owned(),
                    container_name: container.to_owned(),
                    cleanup_status: jackin_core::CleanupStatus::Active,
                }],
            )
            .unwrap();
            let socket = paths.jackin_home.join("sockets").join(container);
            std::fs::create_dir_all(&socket).unwrap();
            std::fs::write(socket.join("agent.toml"), b"retain socket configuration").unwrap();
            let snapshots: Vec<_> = [
                state.join(".jackin/instance.json"),
                state.join(".jackin/isolation.json"),
                payload,
                socket.join("agent.toml"),
                fixture_lifetime_path(&paths, &lifetime),
            ]
            .into_iter()
            .map(|path| {
                let bytes = std::fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect();
            let docker = FakeDockerClient::default();
            admit_fixture_shared(&docker, &lifetime);
            if live_network {
                docker.volumes_by_name.borrow_mut().clear();
            } else {
                docker.inspect_network_queue.borrow_mut().clear();
                docker.network_id_by_name.borrow_mut().clear();
            }
            let mut runner = FakeRunner::default();

            if class_purge {
                purge_class_data(
                    &paths,
                    &RoleSelector::new(None, "agent-smith"),
                    &docker,
                    &mut runner,
                )
                .await
                .unwrap_err();
            } else {
                purge_container_state(&paths, container, &docker, &mut runner)
                    .await
                    .unwrap_err();
            }

            for (path, bytes) in snapshots {
                assert_eq!(
                    std::fs::read(&path).unwrap(),
                    bytes,
                    "changed {}",
                    path.display()
                );
            }
            assert!(runner.recorded.is_empty());
            assert!(
                !docker
                    .bound_operations
                    .borrow()
                    .iter()
                    .any(|operation| operation.starts_with("remove:")
                        || operation.starts_with("remove_network:"))
            );
            assert!(!docker.recorded.borrow().iter().any(|operation| {
                operation.starts_with("docker rm")
                    || operation.starts_with("docker network rm")
                    || operation.starts_with("docker volume rm")
            }));
        }
    }
}

#[tokio::test]
async fn class_purge_admits_every_selected_target_before_filesystem_effects() {
    for unavailable_backend in [false, true] {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let first = "jk-aaaaaaaa-agentsmith";
        let later = "jk-bbbbbbbb-agentsmith";
        for container in [first, later] {
            write_owned_cleanup_manifest(&paths, container, &format!("{container}-dind"));
            let socket = paths.jackin_home.join("sockets").join(container);
            std::fs::create_dir_all(&socket).unwrap();
            std::fs::write(socket.join("agent.toml"), container.as_bytes()).unwrap();
        }
        let first_state = paths.data_dir.join(first);
        let clone =
            crate::isolation::materialize::clone_path_for(&first_state, "/workspace", first);
        std::fs::create_dir_all(&clone).unwrap();
        let payload = clone.join("payload");
        std::fs::write(&payload, b"recoverable earlier clone").unwrap();
        crate::isolation::state::write_records(
            &first_state,
            &[jackin_core::IsolationRecord {
                workspace_name: None,
                mount_dst: "/workspace".to_owned(),
                original_src: temp.path().join("source").display().to_string(),
                isolation: jackin_core::MountIsolation::Clone,
                worktree_path: clone.display().to_string(),
                scratch_branch: String::new(),
                base_commit: String::new(),
                selector_key: "agent-smith".to_owned(),
                container_name: first.to_owned(),
                cleanup_status: jackin_core::CleanupStatus::Active,
            }],
        )
        .unwrap();
        let later_records = paths.data_dir.join(later).join(".jackin/isolation.json");
        if unavailable_backend {
            crate::isolation::state::write_records(&paths.data_dir.join(later), &[]).unwrap();
        } else {
            std::fs::write(&later_records, b"{corrupt later matching-role records").unwrap();
        }
        let snapshots: Vec<_> = [first, later]
            .into_iter()
            .flat_map(|container| {
                [
                    paths.data_dir.join(container).join(".jackin/instance.json"),
                    paths
                        .data_dir
                        .join(container)
                        .join(".jackin/isolation.json"),
                    paths
                        .jackin_home
                        .join("sockets")
                        .join(container)
                        .join("agent.toml"),
                ]
            })
            .map(|path| {
                let bytes = std::fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect();
        let docker = FakeDockerClient::default();
        if unavailable_backend {
            docker.inspect_state_by_name.borrow_mut().insert(
                later.to_owned(),
                ContainerState::InspectUnavailable("later backend unavailable".to_owned()),
            );
        }
        let mut runner = FakeRunner::default();

        purge_class_data(
            &paths,
            &RoleSelector::new(None, "agent-smith"),
            &docker,
            &mut runner,
        )
        .await
        .unwrap_err();

        assert_eq!(
            std::fs::read(payload).unwrap(),
            b"recoverable earlier clone"
        );
        for (path, bytes) in snapshots {
            assert_eq!(
                std::fs::read(&path).unwrap(),
                bytes,
                "changed {}",
                path.display()
            );
        }
        assert!(runner.recorded.is_empty());
        assert!(
            !docker
                .bound_operations
                .borrow()
                .iter()
                .any(|operation| operation.starts_with("remove:")
                    || operation.starts_with("remove_network:"))
        );
        assert!(!docker.recorded.borrow().iter().any(
            |operation| operation.starts_with("docker rm")
                || operation.starts_with("docker network rm")
                || operation.starts_with("docker volume rm")
        ));
    }
}

#[tokio::test]
async fn class_purge_cleans_isolation_for_truncated_role_name() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let selector = RoleSelector::new(
        Some("alpha"),
        "a-role-name-long-enough-to-exceed-the-container-dns-budget-completely",
    );
    let container = crate::instance::naming::container_name_with_id(None, &selector, "aaaaaaaa");
    assert!(
        !container.ends_with(&crate::instance::naming::compact_component(
            &selector.name,
            "role",
        ))
    );
    write_cleanup_manifest_for_role(
        &paths,
        &container,
        &format!("{container}-dind"),
        &selector.key(),
    );
    let state = paths.data_dir.join(&container);
    let clone = crate::isolation::materialize::clone_path_for(&state, "/workspace", &container);
    std::fs::create_dir_all(&clone).unwrap();
    std::fs::write(clone.join("payload"), b"owned clone").unwrap();
    crate::isolation::state::write_records(
        &state,
        &[jackin_core::IsolationRecord {
            workspace_name: None,
            mount_dst: "/workspace".to_owned(),
            original_src: temp.path().join("source").display().to_string(),
            isolation: jackin_core::MountIsolation::Clone,
            worktree_path: clone.display().to_string(),
            scratch_branch: String::new(),
            base_commit: String::new(),
            selector_key: selector.key(),
            container_name: container.clone(),
            cleanup_status: jackin_core::CleanupStatus::Active,
        }],
    )
    .unwrap();
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();

    purge_class_data(&paths, &selector, &docker, &mut runner)
        .await
        .unwrap();

    assert!(!state.exists());
    assert!(!clone.exists());
    assert!(docker.bound_operations.borrow().is_empty());
}

#[tokio::test]
async fn bulk_prune_refuses_unindexed_corrupt_manifest_before_docker_mutation() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = paths
        .data_dir
        .join("jk-aaaaaaaa-agentsmith/.jackin/instance.json");
    std::fs::create_dir_all(manifest.parent().unwrap()).unwrap();
    std::fs::write(&manifest, b"{corrupt manifest").unwrap();
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();

    prune_all_instances(&paths, &docker, &mut runner)
        .await
        .unwrap_err();

    assert_eq!(std::fs::read(manifest).unwrap(), b"{corrupt manifest");
    assert!(docker.recorded.borrow().is_empty());
    assert!(docker.bound_operations.borrow().is_empty());
    assert!(runner.recorded.is_empty());
}

#[tokio::test]
async fn bulk_prune_refuses_unindexed_corrupt_records_before_docker_mutation() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container = "jk-aaaaaaaa-agentsmith";
    write_owned_cleanup_manifest(&paths, container, &format!("{container}-dind"));
    let manifest = paths.data_dir.join(container).join(".jackin/instance.json");
    let manifest_bytes = std::fs::read(&manifest).unwrap();
    let records = paths
        .data_dir
        .join(container)
        .join(".jackin/isolation.json");
    std::fs::write(&records, b"{corrupt isolation records").unwrap();
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();

    prune_all_instances(&paths, &docker, &mut runner)
        .await
        .unwrap_err();

    assert_eq!(std::fs::read(manifest).unwrap(), manifest_bytes);
    assert_eq!(
        std::fs::read(records).unwrap(),
        b"{corrupt isolation records"
    );
    assert!(docker.recorded.borrow().is_empty());
    assert!(docker.bound_operations.borrow().is_empty());
    assert!(runner.recorded.is_empty());
}

#[tokio::test]
async fn bulk_prune_preflight_preserves_migratable_v1_before_corrupt_sibling() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let historical = "jk-aaaaaaaa-agentsmith";
    let corrupt = "jk-bbbbbbbb-agentsmith";
    for container in [historical, corrupt] {
        write_owned_cleanup_manifest(&paths, container, &format!("{container}-dind"));
    }
    let historical_records = paths
        .data_dir
        .join(historical)
        .join(".jackin/isolation.json");
    let bytes = serde_json::to_vec(&serde_json::json!({
        "version": 1,
        "records": [{
            "workspace": "workspace",
            "mount_dst": "/workspace",
            "original_src": temp.path().join("source").display().to_string(),
            "isolation": "clone",
            "worktree_path": crate::isolation::materialize::clone_path_for(&paths.data_dir.join(historical), "/workspace", historical).display().to_string(),
            "scratch_branch": "",
            "base_commit": "",
            "selector_key": "agent-smith",
            "container_name": historical,
            "cleanup_status": "active"
        }]
    })).unwrap();
    std::fs::write(&historical_records, &bytes).unwrap();
    let corrupt_records = paths.data_dir.join(corrupt).join(".jackin/isolation.json");
    std::fs::write(&corrupt_records, b"{corrupt sibling").unwrap();
    let historical_manifest = paths
        .data_dir
        .join(historical)
        .join(".jackin/instance.json");
    let manifest_bytes = std::fs::read(&historical_manifest).unwrap();
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();

    prune_all_instances(&paths, &docker, &mut runner)
        .await
        .unwrap_err();

    assert_eq!(std::fs::read(historical_records).unwrap(), bytes);
    assert_eq!(std::fs::read(historical_manifest).unwrap(), manifest_bytes);
    assert_eq!(std::fs::read(corrupt_records).unwrap(), b"{corrupt sibling");
    assert!(docker.recorded.borrow().is_empty());
    assert!(docker.bound_operations.borrow().is_empty());
    assert!(runner.recorded.is_empty());
}

#[tokio::test]
async fn bulk_prune_retains_failed_custody_and_removes_released_index_sibling() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let released = "jk-aaaaaaaa-agentsmith";
    let retained = "jk-bbbbbbbb-agentsmith";
    for container in [released, retained] {
        write_owned_cleanup_manifest(&paths, container, &format!("{container}-dind"));
        let manifest = InstanceManifest::read_optional(&paths.data_dir.join(container))
            .unwrap()
            .unwrap();
        InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
    }
    let manifest = paths.data_dir.join(retained).join(".jackin/instance.json");
    let manifest_bytes = std::fs::read(&manifest).unwrap();
    let retained_state = paths.data_dir.join(retained);
    let clone =
        crate::isolation::materialize::clone_path_for(&retained_state, "/workspace", retained);
    std::fs::create_dir_all(&clone).unwrap();
    crate::isolation::state::write_records(
        &retained_state,
        &[jackin_core::IsolationRecord {
            workspace_name: None,
            mount_dst: "/workspace".to_owned(),
            original_src: temp.path().join("source").display().to_string(),
            isolation: jackin_core::MountIsolation::Clone,
            worktree_path: clone.display().to_string(),
            scratch_branch: String::new(),
            base_commit: String::new(),
            selector_key: "agent-smith".to_owned(),
            container_name: retained.to_owned(),
            cleanup_status: jackin_core::CleanupStatus::Active,
        }],
    )
    .unwrap();
    let records = retained_state.join(".jackin/isolation.json");
    let record_bytes = std::fs::read(&records).unwrap();
    let payload = clone.join("payload");
    std::fs::write(&payload, b"retain recoverable data").unwrap();
    let docker = FakeDockerClient::default();
    docker.inspect_queue.borrow_mut().extend([
        ContainerState::NotFound, // released role: global admission
        ContainerState::NotFound, // released sidecar: global admission
        ContainerState::NotFound, // retained role: global admission
        ContainerState::NotFound, // retained sidecar: global admission
        ContainerState::NotFound, // released role: filesystem purge
        ContainerState::NotFound, // released sidecar: filesystem purge
        ContainerState::InspectUnavailable("daemon unavailable after admission".to_owned()),
    ]);
    let mut runner = FakeRunner::default();

    let error = prune_all_instances(&paths, &docker, &mut runner)
        .await
        .unwrap_err();

    assert!(format!("{error:#}").contains(retained), "{error:#}");
    assert_eq!(std::fs::read(manifest).unwrap(), manifest_bytes);
    assert_eq!(std::fs::read(payload).unwrap(), b"retain recoverable data");
    assert_eq!(std::fs::read(records).unwrap(), record_bytes);
    assert!(!paths.data_dir.join(released).exists());
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
    assert!(
        !index
            .instances
            .iter()
            .any(|entry| entry.container_base == released)
    );
    assert!(
        index
            .instances
            .iter()
            .any(|entry| entry.container_base == retained)
    );
    assert!(
        !docker
            .bound_operations
            .borrow()
            .iter()
            .any(|operation| operation.starts_with("remove:")
                || operation.starts_with("remove_network:"))
    );
}

#[tokio::test]
async fn aggregate_cleanup_census_rejects_unadmitted_active_lifetimes_before_effects() {
    for bulk_prune in [false, true] {
        for fault in [
            "orphan",
            "prewarm",
            "pending-network",
            "pending-volume",
            "foreign-daemon",
            "backend_mismatch",
        ] {
            let temp = tempdir().unwrap();
            let paths = JackinPaths::for_tests(temp.path());
            let earlier = "jk-aaaaaaaa-agentsmith";
            let first = write_owned_cleanup_manifest(&paths, earlier, &format!("{earlier}-dind"));
            let earlier_ledger = fixture_lifetime_path(&paths, &first);
            let payload = paths.data_dir.join(earlier).join("payload");
            std::fs::write(&payload, b"retain earlier admitted user state").unwrap();
            let owner = if fault == "prewarm" {
                "jk-prewarm-dind"
            } else {
                "jk-bbbbbbbb-agentsmith"
            };
            let daemon = if fault == "foreign-daemon" {
                jackin_core::DaemonServerId::parse("foreign-test-daemon").unwrap()
            } else {
                fixture_daemon()
            };
            let shared_enabled = fault != "backend_mismatch";
            let mut other = crate::instance::SharedDockerLifetime::fresh(
                &daemon,
                owner,
                shared_enabled,
                shared_enabled,
            )
            .unwrap();
            other.save_pending(&paths).unwrap();
            if shared_enabled && fault != "pending-network" {
                other.capture_network(network_id_for(owner)).unwrap();
            }
            if shared_enabled && fault != "pending-network" && fault != "pending-volume" {
                other.capture_certs_volume().unwrap();
            }
            other.save(&paths).unwrap();
            if matches!(
                fault,
                "pending-network" | "pending-volume" | "backend_mismatch"
            ) {
                let mut manifest = InstanceManifest::read_optional(&paths.data_dir.join(earlier))
                    .unwrap()
                    .unwrap();
                manifest.container_base = owner.to_owned();
                manifest.docker = DockerResources {
                    role_container: owner.to_owned(),
                    dind_container: Some(format!("{owner}-dind")),
                    network: other.network_name().unwrap_or_default().to_owned(),
                    certs_volume: other.certs_volume_name().map(str::to_owned),
                };
                manifest.docker_identity = Some(crate::instance::DockerIdentity {
                    role_container_id: owner.to_owned(),
                    dind_container_id: Some(format!("{owner}-dind")),
                    network_id: other.network_id().cloned(),
                });
                if manifest.backend.is_some() {
                    manifest.backend = Some(crate::instance::BackendResources::Docker(
                        manifest.docker.clone(),
                    ));
                }
                if fault == "backend_mismatch" {
                    manifest.docker_identity = None;
                    manifest.docker.dind_container = None;
                    manifest.backend = Some(crate::instance::BackendResources::AppleContainer(
                        crate::instance::AppleContainerResources {
                            container_name: owner.to_owned(),
                            role_image_ref: "example.invalid/role:fixture".to_owned(),
                            inner_docker_enabled: false,
                        },
                    ));
                }
                manifest.write(&paths.data_dir.join(owner)).unwrap();
            }
            let other_ledger = fixture_lifetime_path(&paths, &other);
            let mut snapshots: Vec<_> = [
                paths.data_dir.join(earlier).join(".jackin/instance.json"),
                payload,
                earlier_ledger,
                other_ledger,
            ]
            .into_iter()
            .map(|path| {
                let bytes = std::fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect();
            if matches!(
                fault,
                "pending-network" | "pending-volume" | "backend_mismatch"
            ) {
                let path = paths.data_dir.join(owner).join(".jackin/instance.json");
                let bytes = std::fs::read(&path).unwrap();
                snapshots.push((path, bytes));
            }
            let docker = FakeDockerClient::default();
            docker.inspect_state_by_name.borrow_mut().extend([
                (earlier.to_owned(), ContainerState::Running),
                (format!("{earlier}-dind"), ContainerState::Running),
            ]);
            admit_fixture_shared(&docker, &first);
            let mut runner = FakeRunner::default();

            if bulk_prune {
                prune_all_instances(&paths, &docker, &mut runner)
                    .await
                    .unwrap_err();
            } else {
                exile_all(&paths, &docker).await.unwrap_err();
            }

            for (path, bytes) in snapshots {
                assert_eq!(
                    std::fs::read(&path).unwrap(),
                    bytes,
                    "{fault}: changed {}",
                    path.display()
                );
            }
            assert!(runner.recorded.is_empty(), "{fault}");
            assert!(
                !docker
                    .bound_operations
                    .borrow()
                    .iter()
                    .any(|operation| operation.starts_with("remove:")
                        || operation.starts_with("remove_network:")),
                "{fault}"
            );
            assert!(
                !docker
                    .recorded
                    .borrow()
                    .iter()
                    .any(|operation| operation.starts_with("docker rm")
                        || operation.starts_with("docker network rm")
                        || operation.starts_with("docker volume rm")),
                "{fault}"
            );
        }
    }
}

#[tokio::test]
async fn home_prune_rejects_active_corrupt_or_symlinked_lifetime_without_instance_data() {
    for fault in ["active", "corrupt", "symlink"] {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let lifetime = crate::instance::SharedDockerLifetime::fresh(
            &fixture_daemon(),
            "jk-orphan",
            false,
            false,
        )
        .unwrap();
        lifetime.save_pending(&paths).unwrap();
        let ledger = fixture_lifetime_path(&paths, &lifetime);
        let outside = temp.path().join("outside-ledger");
        if fault == "corrupt" {
            std::fs::write(&ledger, b"{corrupt census ledger").unwrap();
        } else if fault == "symlink" {
            std::fs::write(&outside, std::fs::read(&ledger).unwrap()).unwrap();
            std::fs::remove_file(&ledger).unwrap();
            std::os::unix::fs::symlink(&outside, &ledger).unwrap();
        }
        let bytes = std::fs::read(&ledger).unwrap();
        assert!(!paths.data_dir.exists());

        prune_jackin_home(&paths).unwrap_err();

        assert_eq!(std::fs::read(&ledger).unwrap(), bytes, "{fault}");
        assert!(!paths.data_dir.exists(), "{fault}");
        if fault == "symlink" {
            assert!(
                std::fs::symlink_metadata(ledger)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            assert_eq!(std::fs::read(outside).unwrap(), bytes);
        }
    }
}

#[tokio::test]
async fn home_prune_retains_index_only_active_custody_without_manifest_or_lifetime() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container = "jk-aaaaaaaa-agentsmith";
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
    manifest.mark_status(InstanceStatus::Active);
    InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
    let index = paths.data_dir.join("instances.json");
    let index_bytes = std::fs::read(&index).unwrap();
    std::fs::create_dir_all(&paths.jackin_home).unwrap();
    let marker = paths.jackin_home.join("retain-home-marker");
    std::fs::write(&marker, b"retain index-only home custody").unwrap();
    assert!(!paths.data_dir.join(container).exists());
    assert!(!paths.jackin_home.join("shared-docker-lifetimes").exists());

    prune_jackin_home(&paths).unwrap_err();

    assert_eq!(std::fs::read(index).unwrap(), index_bytes);
    assert_eq!(
        std::fs::read(marker).unwrap(),
        b"retain index-only home custody"
    );
    assert!(!paths.data_dir.join(container).exists());
}

#[tokio::test]
async fn home_prune_removes_validated_retired_only_lifetime_store() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let lifetime =
        crate::instance::SharedDockerLifetime::fresh(&fixture_daemon(), "jk-retired", false, false)
            .unwrap();
    lifetime.save_pending(&paths).unwrap();
    lifetime.retire(&paths).unwrap();
    assert!(!paths.data_dir.exists());

    prune_jackin_home(&paths).unwrap();

    assert!(!paths.jackin_home.exists());
}

// ── prune_jackin_home ────────────────────────────────────────────────────

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

// ── owned-validated-path removal ─────────────────────────────────────────

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
        error.to_string().contains("escapes") || error.to_string().contains("refusing"),
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
