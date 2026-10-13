// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn hardline_new_session_forwards_coauthor_trailer_env_when_enabled() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    provision_agent_admission(&paths, container_name, jackin_core::Agent::Claude);
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
        jackin_core::Agent::Claude,
        &[],
        true,
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
            .any(|call| call.contains("-e=JACKIN_GIT_COAUTHOR_TRAILER=1")),
        "coauthor trailer env must be present when enabled; recorded: {:?}",
        runner.recorded
    );
    let call = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker exec"))
        .expect("expected docker exec call");
    let env_pos = call
        .find("-e=JACKIN_GIT_COAUTHOR_TRAILER=1")
        .expect("coauthor env flag must be present");
    let container_pos = call
        .find(container_name)
        .expect("container name must be present");
    assert!(
        env_pos < container_pos,
        "docker exec options must precede container name; got: {call}"
    );
}

#[tokio::test]
async fn hardline_new_session_forwards_dco_env_when_enabled() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    provision_agent_admission(&paths, container_name, jackin_core::Agent::Claude);
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
        jackin_core::Agent::Claude,
        &[],
        false,
        true,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();

    assert!(
        runner
            .recorded
            .iter()
            .any(|call| call.contains("-e=JACKIN_GIT_DCO=1")),
        "DCO env must be present when enabled; recorded: {:?}",
        runner.recorded
    );
    assert!(
        !runner
            .recorded
            .iter()
            .any(|call| call.contains("JACKIN_GIT_COAUTHOR_TRAILER")),
        "coauthor trailer env must be absent when disabled; recorded: {:?}",
        runner.recorded
    );
}

#[tokio::test]
async fn new_session_rejects_empty_v3_admission_before_capsule_spawn() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-empty-admission";
    provision_account_admission(&paths, container_name);
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Running])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    let error = spawn_agent_session(
        &paths,
        container_name,
        None,
        jackin_core::Agent::Claude,
        &[],
        false,
        false,
        &docker,
        &mut runner,
    )
    .await
    .expect_err("an explicit empty v3 admission set must reject --new");

    assert!(
        error.to_string().contains("not admitted"),
        "unexpected admission error: {error:#}"
    );
    assert!(
        runner.recorded.is_empty(),
        "rejected --new must not invoke a capsule/session runner: {:?}",
        runner.recorded
    );
    assert_eq!(
        docker.recorded.borrow().as_slice(),
        &[format!("docker inspect {container_name}")],
        "rejected --new may inspect lifecycle state but must not start/exec a session"
    );
}

#[tokio::test]
async fn new_session_routes_exact_duplicate_agent_instance_and_rejects_unknown_id() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-duplicate-agent";
    let _config = provision_duplicate_agent_admission(&paths, container_name);
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
        Some("claude-personal"),
        jackin_core::Agent::Claude,
        &[],
        false,
        false,
        &docker,
        &mut runner,
    )
    .await
    .unwrap();
    assert!(
        runner.recorded.iter().any(|call| {
            call.contains("jackin-capsule new claude-personal")
                && !call.contains("jackin-capsule new claude-work")
        }),
        "exact live instance ID must reach capsule: {:?}",
        runner.recorded
    );

    let (_tmp, paths) = test_paths();
    let container_name = "jk-duplicate-agent-missing";
    let _config = provision_duplicate_agent_admission(&paths, container_name);
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Running])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();
    let error = spawn_agent_session(
        &paths,
        container_name,
        Some("claude-removed"),
        jackin_core::Agent::Claude,
        &[],
        false,
        false,
        &docker,
        &mut runner,
    )
    .await
    .expect_err("unknown live instance ID must be rejected before exec");
    assert!(error.to_string().contains("not admitted"), "{error:#}");
    assert!(
        !runner
            .recorded
            .iter()
            .any(|call| call.contains("docker exec")),
        "unknown target must not execute capsule: {:?}",
        runner.recorded
    );
}

#[tokio::test]
async fn hardline_new_session_requires_running_container() {
    let (_tmp, paths) = test_paths();
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 137,
            oom_killed: false,
        }])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    let err = spawn_agent_session(
        &paths,
        "jk-agent-smith",
        None,
        jackin_core::Agent::Claude,
        &[],
        false,
        false,
        &docker,
        &mut runner,
    )
    .await
    .unwrap_err();

    assert!(err.to_string().contains("is stopped"));
    assert!(
        !runner
            .recorded
            .iter()
            .any(|call| call.starts_with("docker exec"))
    );
}

#[tokio::test]
async fn spawn_shell_session_execs_jackin_capsule_new_in_running_container() {
    let (_tmp, paths) = test_paths();
    provision_account_admission(&paths, "jk-agent-smith");
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Running])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    spawn_shell_session(&paths, "jk-agent-smith", &docker, &mut runner)
        .await
        .unwrap();

    assert!(
        runner.recorded.iter().any(|c| {
            c.contains("docker exec")
                && c.contains("jk-agent-smith")
                && c.contains("jackin-capsule")
                && c.contains("new")
        }),
        "expected docker exec with jackin-capsule new; got: {:?}",
        runner.recorded
    );
}

#[tokio::test]
async fn spawn_shell_session_does_not_set_tmux_env() {
    let (_tmp, paths) = test_paths();
    provision_account_admission(&paths, "jk-agent-smith");
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Running])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    spawn_shell_session(&paths, "jk-agent-smith", &docker, &mut runner)
        .await
        .unwrap();

    assert!(
        !runner.recorded.iter().any(|c| c.contains("TMUX=")),
        "TMUX= must not be set in jackin-capsule shell sessions"
    );
}

#[tokio::test]
async fn spawn_shell_session_errors_on_stopped_container() {
    let (_tmp, paths) = test_paths();
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 137,
            oom_killed: false,
        }])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    let err = spawn_shell_session(&paths, "jk-agent-smith", &docker, &mut runner)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("is stopped"));
    assert!(
        !runner.recorded.iter().any(|c| c.contains("docker exec")),
        "exec must not fire against a stopped container"
    );
}

#[tokio::test]
async fn spawn_shell_session_errors_on_not_found() {
    let (_tmp, paths) = test_paths();
    let docker = FakeDockerClient::default(); // empty inspect → NotFound
    let mut runner = FakeRunner::default();

    let err = spawn_shell_session(&paths, "jk-agent-smith", &docker, &mut runner)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("not found"));
    assert!(!runner.recorded.iter().any(|c| c.contains("docker exec")));
}

#[tokio::test]
async fn hardline_errors_when_container_not_found() {
    let (_tmp, paths) = test_paths();
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();

    let err = hardline_agent(&paths, "jk-agent-smith", &docker, &mut runner)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("not found"));
    assert!(
        !runner
            .recorded
            .iter()
            .any(|c| c.contains("docker start") || c.contains("jackin-capsule new"))
    );
}

#[tokio::test]
async fn hardline_errors_when_docker_inspect_is_unavailable() {
    let (_tmp, paths) = test_paths();
    let docker = FakeDockerClient {
        fail_with: vec![(
            "docker inspect jk-agent-smith".to_owned(),
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_owned(),
        )],
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    let err = hardline_agent(&paths, "jk-agent-smith", &docker, &mut runner)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("Docker is unavailable"));
    assert!(
        !runner
            .recorded
            .iter()
            .any(|c| c.contains("docker start") || c.contains("jackin-capsule new"))
    );
}
