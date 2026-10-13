// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::instance::InstanceManifest;
use jackin_core::ContainerHandle;

#[tokio::test]
async fn start_rejects_rotation_after_lifecycle_inspect_before_container_start() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-start-generation";
    provision_account_admission(&paths, container_name);
    let mut rotated = jackin_config::AppConfig::default();
    rotated
        .env
        .insert("ROTATED_DURING_START".into(), "new".into());
    let _rotation =
        schedule_config_rotation(&paths, format!("docker inspect {container_name}"), &rotated);
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Stopped {
                exit_code: 1,
                oom_killed: false,
            },
            ContainerState::Running,
        ])),
        operation_hook: Some(rotate_config_on_operation),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    let error = start_or_reconnect_capsule_client(&paths, container_name, &docker, &mut runner)
        .await
        .expect_err("container start must fail when config rotates after inspect");
    assert!(
        error
            .to_string()
            .contains("configuration changed during launch"),
        "unexpected rotation error: {error:#}"
    );
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call.starts_with("start_container:")),
        "container start ran after the generation rotated: {:?}",
        docker.recorded.borrow()
    );
}

#[tokio::test]
async fn existing_role_rejects_same_name_replacement_before_start_or_exec() {
    for running in [false, true] {
        let (_tmp, paths) = test_paths();
        let container_name = "jk-persisted-role-identity";
        provision_account_admission(&paths, container_name);
        let docker = FakeDockerClient {
            container_id_by_name: std::cell::RefCell::new(HashMap::from([(
                container_name.into(),
                "replacement-role-id".into(),
            )])),
            inspect_state_by_name: std::cell::RefCell::new(HashMap::from([(
                container_name.into(),
                if running {
                    ContainerState::Running
                } else {
                    ContainerState::Created
                },
            )])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();
        let error = if running {
            hardline_agent(&paths, container_name, &docker, &mut runner).await
        } else {
            start_or_reconnect_capsule_client(&paths, container_name, &docker, &mut runner).await
        }
        .expect_err("a name replacement must never become the recorded role");
        assert!(
            error.to_string().contains("ownership identity mismatch"),
            "{error:#}"
        );
        assert!(
            docker.bound_operations.borrow().is_empty(),
            "{:?}",
            docker.bound_operations.borrow()
        );
        assert!(runner.recorded.is_empty(), "{:?}", runner.recorded);
        assert_eq!(
            InstanceManifest::read(&paths.data_dir.join(container_name))
                .unwrap()
                .docker_identity
                .unwrap()
                .role_container_id,
            container_name,
            "inspection must not backfill replacement ownership"
        );
    }
}

#[tokio::test]
async fn live_historical_role_without_recorded_identity_denies_exec_and_backfill() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-historical-role-identity";
    provision_account_admission(&paths, container_name);
    let root = paths.data_dir.join(container_name);
    let mut manifest = InstanceManifest::read(&root).unwrap();
    manifest.docker_identity = None;
    manifest.write(&root).unwrap();
    let docker = FakeDockerClient {
        inspect_state_by_name: std::cell::RefCell::new(HashMap::from([(
            container_name.into(),
            ContainerState::Running,
        )])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();
    let error = spawn_shell_session(&paths, container_name, &docker, &mut runner)
        .await
        .expect_err("historical live roles need explicit ownership recovery");
    assert!(
        error
            .to_string()
            .contains("recover the original launch identity explicitly"),
        "{error:#}"
    );
    assert!(docker.bound_operations.borrow().is_empty());
    assert!(runner.recorded.is_empty());
    assert!(
        InstanceManifest::read(&root)
            .unwrap()
            .docker_identity
            .is_none()
    );
}

#[tokio::test]
async fn handle_aware_restore_refuses_same_name_replacement_before_start() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-restore-identity";
    provision_account_admission(&paths, container_name);
    let root = paths.data_dir.join(container_name);
    let mut manifest = InstanceManifest::read(&root).unwrap();
    manifest.docker_identity.as_mut().unwrap().role_container_id = "original-role-id".into();
    manifest.write(&root).unwrap();
    let original = ContainerHandle::new(container_name, "original-role-id").unwrap();
    let docker = FakeDockerClient {
        inspect_state_by_name: std::cell::RefCell::new(HashMap::from([(
            container_name.into(),
            ContainerState::Created,
        )])),
        container_id_by_name: std::cell::RefCell::new(HashMap::from([(
            container_name.to_owned(),
            "replacement-role-id".to_owned(),
        )])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    let error = start_or_hardline_agent_with_container_handle(
        &paths,
        container_name,
        &launch::AccountConfigRevision::acquire(&paths).unwrap(),
        &docker,
        &mut runner,
        true,
        &original,
        None,
    )
    .await
    .expect_err("a replaced name must not redirect restore to the replacement");

    assert!(
        error.to_string().contains("ownership identity mismatch"),
        "unexpected replacement error: {error:#}"
    );
    assert!(docker.bound_operations.borrow().is_empty());
    assert!(
        !docker
            .bound_operations
            .borrow()
            .iter()
            .any(|operation| operation.starts_with("start:")),
        "replacement must not receive a lifecycle operation: {:?}",
        docker.bound_operations.borrow()
    );
    assert!(
        runner
            .recorded
            .iter()
            .all(|call| !call.contains("jackin-capsule")),
        "replacement must not receive an attach exec: {:?}",
        runner.recorded
    );
}

#[tokio::test]
async fn attach_rejects_missing_lock_before_capsule_exec() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-missing-generation-lock";
    provision_account_admission(&paths, container_name);
    std::fs::remove_file(paths.config_file.with_file_name("config.lock")).unwrap();
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Running])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    let error = hardline_agent(&paths, container_name, &docker, &mut runner)
        .await
        .expect_err("missing config lock must deny attach");
    assert!(
        error.to_string().contains("required config lock"),
        "unexpected missing-lock error: {error:#}"
    );
    assert!(runner.recorded.is_empty());
}

#[tokio::test]
async fn hardline_clean_exit_ejects_runtime_resources() {
    let (_tmp, paths) = test_paths();
    provision_account_admission(&paths, "jk-agent-smith");
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Running,
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            },
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            },
            ContainerState::Running,
        ])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    hardline_agent(&paths, "jk-agent-smith", &docker, &mut runner)
        .await
        .unwrap();

    let recorded = docker.recorded.borrow();
    assert!(
        recorded
            .iter()
            .any(|op| op == "docker rm -f jk-agent-smith"),
        "clean exit should remove role container; recorded: {recorded:?}"
    );
    assert!(
        recorded
            .iter()
            .any(|op| op == "docker rm -f jk-agent-smith-dind"),
        "clean exit should remove DinD sidecar; recorded: {recorded:?}"
    );
    assert!(
        recorded
            .iter()
            .any(|op| op == "docker volume rm jk-agent-smith-dind-certs"),
        "clean exit should remove cert volume; recorded: {recorded:?}"
    );
    assert!(
        recorded
            .iter()
            .any(|op| op == "docker network rm jk-agent-smith-net"),
        "clean exit should remove role network; recorded: {recorded:?}"
    );
}

#[tokio::test]
async fn hardline_detach_with_live_sessions_preserves_runtime_resources() {
    let (_tmp, paths) = test_paths();
    provision_account_admission(&paths, "jk-agent-smith");
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Running,
            ContainerState::Running,
        ])),
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from([
            "Sessions: 1\n  [1] Claude (claude) state=working active=true".to_owned(),
        ])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    hardline_agent(&paths, "jk-agent-smith", &docker, &mut runner)
        .await
        .unwrap();

    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|op| op.starts_with("docker rm -f")),
        "detach with live sessions must not eject resources; recorded: {:?}",
        docker.recorded.borrow()
    );
}

#[tokio::test]
async fn hardline_new_session_execs_entrypoint_in_running_container() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    provision_agent_admission(&paths, container_name, jackin_core::Agent::Codex);
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Running,
            ContainerState::Running,
            ContainerState::Running,
        ])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    spawn_agent_session(
        &paths,
        container_name,
        None,
        jackin_core::Agent::Codex,
        &[("EDITOR".into(), "vim".into())],
        false,
        false,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert!(
        runner
            .recorded
            .iter()
            .any(|call| call.contains("EDITOR=vim"))
    );
    assert!(
        runner.recorded.iter().any(|call| {
            call.contains("docker exec")
                && !call.contains("JACKIN_AGENT=")
                && call.contains("--workdir /workspace")
                && call.contains("jk-k7p9m2xq-workspace-agentsmith")
                && call.contains("jackin-capsule")
                && call.contains("new")
                && call.contains("codex")
        }),
        "expected jackin-capsule new for codex; got: {:?}",
        runner.recorded
    );
}
