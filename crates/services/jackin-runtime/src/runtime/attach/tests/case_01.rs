// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn attach_proxy_exec_args_use_stdio_not_tty() {
    assert_eq!(
        attach_proxy_exec_args(&test_container_handle("jk-agent-smith")),
        vec![
            "exec",
            "-i",
            "jk-agent-smith-id",
            JACKIN_CAPSULE_PATH,
            ATTACH_PROXY_SUBCOMMAND,
        ]
    );
}

#[test]
fn host_attach_transport_falls_back_when_socket_path_is_missing() {
    let (_tmp, paths) = short_test_paths();

    let plan = select_host_attach_transport(&paths, "jk-agent-smith");

    match plan {
        HostAttachTransportPlan::AttachProxy {
            socket_path,
            direct_error,
        } => {
            assert!(socket_path.ends_with("sockets/jk-agent-smith/jackin.sock"));
            assert_eq!(direct_error, None);
        }
        other @ HostAttachTransportPlan::DirectSocket { .. } => {
            panic!("expected attach-proxy fallback, got {other:?}")
        }
    }
}

#[test]
fn host_attach_transport_surfaces_over_sun_len_socket_path() {
    // Bug 10: a socket path at/over the sun_path limit can never connect
    // directly; instead of a swallowed generic connect error, the plan must fall
    // back to the proxy with an explicit, descriptive reason (not silent).
    let (_tmp, paths) = short_test_paths();
    let long_container = format!("jk-{}", "x".repeat(110));

    let plan = select_host_attach_transport(&paths, &long_container);

    match plan {
        HostAttachTransportPlan::AttachProxy { direct_error, .. } => {
            let reason = direct_error.expect("explicit over-limit reason");
            assert!(
                reason.contains("sun_path"),
                "reason must name the sun_path limit: {reason}"
            );
        }
        other @ HostAttachTransportPlan::DirectSocket { .. } => {
            panic!("an over-limit path must not use the direct socket, got {other:?}")
        }
    }
}

#[test]
fn host_attach_transport_uses_direct_socket_when_connect_succeeds() {
    let (_tmp, paths) = short_test_paths();
    let socket_path = ensure_socket_parent(&paths, "jk-agent-smith");
    let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
    let server = spawn_capsule_preface_ack(listener);

    let plan = select_host_attach_transport(&paths, "jk-agent-smith");

    assert_eq!(plan, HostAttachTransportPlan::DirectSocket { socket_path });
    server.join().unwrap();
}

#[test]
fn host_attach_transport_falls_back_when_socket_inode_refuses_connect() {
    let (_tmp, paths) = short_test_paths();
    let socket_path = ensure_socket_parent(&paths, "jk-agent-smith");
    std::fs::write(&socket_path, b"not a socket").unwrap();

    let plan = select_host_attach_transport(&paths, "jk-agent-smith");

    match plan {
        HostAttachTransportPlan::AttachProxy {
            socket_path: actual,
            direct_error,
        } => {
            assert_eq!(actual, socket_path);
            assert!(
                direct_error.is_some_and(|error| !error.is_empty()),
                "expected concrete direct-connect error"
            );
        }
        other @ HostAttachTransportPlan::DirectSocket { .. } => {
            panic!("expected attach-proxy fallback, got {other:?}")
        }
    }
}

#[test]
fn insert_run_as_user_places_flag_immediately_after_exec() {
    let user = Some("1001:20".to_owned());
    let mut args = vec!["exec", "-it", "ctr", "cmd"];
    insert_run_as_user(&mut args, user.as_deref());
    assert_eq!(args, vec!["exec", "--user", "1001:20", "-it", "ctr", "cmd"]);
}

#[test]
fn insert_run_as_user_is_noop_when_absent() {
    let user: Option<String> = None;
    let mut args = vec!["exec", "-it", "ctr"];
    insert_run_as_user(&mut args, user.as_deref());
    assert_eq!(args, vec!["exec", "-it", "ctr"]);
}

#[tokio::test]
async fn wait_for_capsule_daemon_uses_direct_socket_without_exec() {
    let (_tmp, paths) = short_test_paths();
    let socket_path = ensure_socket_parent(&paths, "jk-agent-smith");
    let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
    let server = spawn_capsule_preface_ack(listener);
    let docker = FakeDockerClient {
        fail_with: vec![("docker exec".to_owned(), "unexpected exec".to_owned())],
        ..Default::default()
    };

    wait_for_capsule_daemon_with_handle(&paths, &test_container_handle("jk-agent-smith"), &docker)
        .await
        .unwrap();

    server.join().unwrap();
    assert!(
        docker.recorded.borrow().is_empty(),
        "direct socket readiness must not spawn docker exec"
    );
}

#[tokio::test]
async fn hardline_attaches_when_container_is_running() {
    let (_tmp, paths) = test_paths();
    provision_account_admission(&paths, "jk-agent-smith");
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Running])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    hardline_agent(&paths, "jk-agent-smith", &docker, &mut runner)
        .await
        .unwrap();

    assert!(
        runner.recorded.iter().any(|c| {
            c.contains("docker exec")
                && c.contains("jk-agent-smith")
                && c.contains("jackin-capsule")
        }),
        "expected jackin-capsule exec in recorded commands; got: {:?}",
        runner.recorded
    );
}

#[tokio::test]
async fn attach_rejects_rotation_after_readiness_before_capsule_exec() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-attach-generation";
    provision_account_admission(&paths, container_name);
    let mut rotated = jackin_config::AppConfig::default();
    rotated
        .env
        .insert("ROTATED_DURING_ATTACH".into(), "new".into());
    let _rotation =
        schedule_config_rotation(&paths, format!("docker exec {container_name}"), &rotated);
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Running])),
        operation_hook: Some(rotate_config_on_operation),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    let result = hardline_agent(&paths, container_name, &docker, &mut runner).await;
    let error = match result {
        Ok(()) => panic!(
            "attach unexpectedly succeeded; docker={:?}, runner={:?}",
            docker.recorded.borrow(),
            runner.recorded
        ),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("configuration changed during launch"),
        "unexpected rotation error: {error:#}"
    );
    assert!(
        error.is::<ReconnectAdmissionFailure>(),
        "readiness rotation must remain a reconnect admission error: {error:#}"
    );
    assert!(
        !runner
            .recorded
            .iter()
            .any(|call| call.contains("jackin-capsule")),
        "capsule exec ran after the generation rotated: {:?}",
        runner.recorded
    );
}

#[tokio::test]
async fn start_removes_container_if_generation_rotates_during_start() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-start-generation-after-await";
    provision_account_admission(&paths, container_name);
    let mut rotated = jackin_config::AppConfig::default();
    rotated
        .env
        .insert("ROTATED_DURING_START_AWAIT".into(), "new".into());
    let _rotation = schedule_config_rotation(
        &paths,
        format!("start_container:{container_name}"),
        &rotated,
    );
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 1,
            oom_killed: false,
        }])),
        operation_hook: Some(rotate_config_on_operation),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    let error = start_or_reconnect_capsule_client(&paths, container_name, &docker, &mut runner)
        .await
        .expect_err("start must fail after a generation rotates during Docker start");
    assert!(
        error
            .to_string()
            .contains("configuration changed during launch"),
        "unexpected rotation error: {error:#}"
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call == &format!("docker rm -f {container_name}")),
        "stale started container must be force-removed: {:?}",
        docker.recorded.borrow()
    );
    assert!(
        !runner
            .recorded
            .iter()
            .any(|call| call.contains("jackin-capsule")),
        "reconnect must not run after stale container cleanup: {:?}",
        runner.recorded
    );
}

#[tokio::test]
async fn ambiguous_start_failure_cleans_container_if_generation_rotates_during_diagnosis() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-start-ambiguous-generation";
    provision_account_admission(&paths, container_name);
    let mut rotated = jackin_config::AppConfig::default();
    rotated
        .env
        .insert("ROTATED_DURING_START_DIAGNOSIS".into(), "new".into());
    let _rotation = schedule_config_rotation(
        &paths,
        format!("docker network inspect {container_name}-net"),
        &rotated,
    );
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Stopped {
                exit_code: 1,
                oom_killed: false,
            },
            ContainerState::NotFound,
        ])),
        inspect_network_queue: std::cell::RefCell::new(VecDeque::from([None])),
        fail_with: vec![("start_container".to_owned(), "daemon timeout".to_owned())],
        operation_hook: Some(rotate_config_on_operation),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    let error = start_or_reconnect_capsule_client(&paths, container_name, &docker, &mut runner)
        .await
        .expect_err("ambiguous start failure must reject a stale generation");
    assert!(
        error
            .to_string()
            .contains("configuration changed during launch"),
        "unexpected rotation error: {error:#}"
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call == &format!("docker rm -f {container_name}")),
        "stale container must be removed after ambiguous start diagnosis: {:?}",
        docker.recorded.borrow()
    );
}

#[tokio::test]
async fn reconnect_rejects_rotation_after_capsule_exec() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-reconnect-generation-after-await";
    provision_account_admission(&paths, container_name);
    let mut rotated = jackin_config::AppConfig::default();
    rotated
        .env
        .insert("ROTATED_DURING_RECONNECT_AWAIT".into(), "new".into());
    let path = paths.config_file.clone();
    let bytes = toml::to_string(&rotated).unwrap().into_bytes();
    let mut runner = FakeRunner::default();
    runner.side_effects.push((
        "docker exec".to_owned(),
        Box::new(move || std::fs::write(&path, &bytes).unwrap()),
    ));
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Running,
            ContainerState::Running,
        ])),
        ..Default::default()
    };

    let error = hardline_agent(&paths, container_name, &docker, &mut runner)
        .await
        .expect_err("reconnect must reject a generation rotation during capsule exec");
    assert!(
        error
            .to_string()
            .contains("configuration changed during launch"),
        "unexpected rotation error: {error:#}"
    );
    assert!(
        error.is::<ReconnectAdmissionFailure>(),
        "post-exec lease failure must remain a reconnect admission error: {error:#}"
    );
    assert!(
        runner
            .recorded
            .iter()
            .any(|call| call.contains("jackin-capsule")),
        "reconnect should reach the awaited capsule exec: {:?}",
        runner.recorded
    );
}
