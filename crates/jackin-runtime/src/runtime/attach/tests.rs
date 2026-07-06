//! Tests for `attach`.
use std::collections::{HashMap, VecDeque};

use super::super::test_support::FakeRunner;
use super::*;
use crate::runtime::test_support::FakeDockerClient;
use tempfile::TempDir;

fn test_paths() -> (TempDir, JackinPaths) {
    let dir = TempDir::new().unwrap();
    let paths = JackinPaths::for_tests(dir.path());
    (dir, paths)
}

fn short_test_paths() -> (TempDir, JackinPaths) {
    let dir = tempfile::Builder::new()
        .prefix("jk-attach-")
        .tempdir_in("/tmp")
        .unwrap();
    let paths = JackinPaths::for_tests(dir.path());
    (dir, paths)
}

fn ensure_socket_parent(paths: &JackinPaths, container_name: &str) -> PathBuf {
    let socket_path = super::super::snapshot::socket_path(paths, container_name);
    std::fs::create_dir_all(socket_path.parent().unwrap()).unwrap();
    socket_path
}

#[test]
fn attach_proxy_exec_args_use_stdio_not_tty() {
    assert_eq!(
        attach_proxy_exec_args("jk-agent-smith"),
        vec![
            "exec",
            "-i",
            "jk-agent-smith",
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
    let _listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();

    let plan = select_host_attach_transport(&paths, "jk-agent-smith");

    assert_eq!(plan, HostAttachTransportPlan::DirectSocket { socket_path });
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
async fn wait_for_capsule_daemon_polls_socket_status_command() {
    let (_tmp, paths) = test_paths();
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let guard = run.activate();
    let docker = FakeDockerClient {
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from(["Sessions: 1\n".to_owned()])),
        ..Default::default()
    };

    wait_for_capsule_daemon(&paths, "jk-agent-smith", &docker)
        .await
        .unwrap();

    let recorded = docker.recorded.borrow();
    assert!(
        recorded
            .iter()
            .any(|call| call.contains(&format!("sh -c {JACKIN_STATUS_CMD}"))),
        "expected socket/status wait command; recorded: {recorded:?}"
    );
    drop(guard);
    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("\"kind\":\"timing_done\"")
            && diagnostics.contains("wait_capsule_socket")
            && diagnostics.contains("ready"),
        "expected wait_capsule_socket timing in diagnostics: {diagnostics}"
    );
}

#[tokio::test]
async fn wait_for_capsule_daemon_uses_direct_socket_without_exec() {
    let (_tmp, paths) = short_test_paths();
    let socket_path = ensure_socket_parent(&paths, "jk-agent-smith");
    let _listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
    let docker = FakeDockerClient {
        fail_with: vec![("docker exec".to_owned(), "unexpected exec".to_owned())],
        ..Default::default()
    };

    wait_for_capsule_daemon(&paths, "jk-agent-smith", &docker)
        .await
        .unwrap();

    assert!(
        docker.recorded.borrow().is_empty(),
        "direct socket readiness must not spawn docker exec"
    );
}

#[tokio::test]
async fn start_or_reconnect_uses_capsule_client_not_start_attach() {
    let (_tmp, paths) = test_paths();
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
    let guard = run.activate();
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 0,
            oom_killed: false,
        }])),
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from(["Sessions: 1\n".to_owned()])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    start_or_reconnect_capsule_client(&paths, "jk-agent-smith", &docker, &mut runner)
        .await
        .unwrap();

    let docker_recorded = docker.recorded.borrow();
    assert!(
        docker_recorded
            .iter()
            .any(|call| call == "start_container:jk-agent-smith"),
        "expected detached Docker API start; recorded: {docker_recorded:?}"
    );
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.contains(&format!("sh -c {JACKIN_STATUS_CMD}"))),
        "expected socket/status wait before client exec; recorded: {docker_recorded:?}"
    );
    assert!(
        runner.recorded.iter().any(|call| {
            call.contains("docker exec")
                && call.contains("-it")
                && call.contains("jk-agent-smith")
                && call.contains("/jackin/runtime/jackin-capsule")
        }),
        "expected capsule client exec; recorded: {:?}",
        runner.recorded
    );
    assert!(
        !runner
            .recorded
            .iter()
            .any(|call| call.contains("docker start -ai")),
        "restart path must not attach to PID 1; recorded: {:?}",
        runner.recorded
    );
    drop(guard);
    let diagnostics = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        diagnostics.contains("restore_inspect")
            && diagnostics.contains("restore_start_container")
            && diagnostics.contains("started"),
        "expected restore timing diagnostics: {diagnostics}"
    );
}

#[tokio::test]
async fn hardline_attaches_when_container_is_running() {
    let (_tmp, paths) = test_paths();
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
async fn hardline_clean_exit_ejects_runtime_resources() {
    let (_tmp, paths) = test_paths();
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Running,
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            },
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
    let manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
        container_base: container_name,
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace/project",
        host_workdir_fingerprint: "sha256:test",
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::agent::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk-agent-smith",
        docker: crate::instance::DockerResources {
            role_container: container_name.to_owned(),
            dind_container: Some(format!("{container_name}-dind")),
            network: format!("{container_name}-net"),
            certs_volume: Some(format!("{container_name}-dind-certs")),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });
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
        Some(&manifest),
        jackin_core::agent::Agent::Codex,
        None,
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
            call.contains("docker exec")
                && !call.contains("JACKIN_AGENT=")
                && call.contains("--workdir /workspace/project")
                && call.contains("jk-k7p9m2xq-workspace-agentsmith")
                && call.contains("jackin-capsule")
                && call.contains("new")
                && call.contains("codex")
        }),
        "expected jackin-capsule new for codex; got: {:?}",
        runner.recorded
    );
}

#[tokio::test]
async fn hardline_new_session_forwards_coauthor_trailer_env_when_enabled() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
        container_base: container_name,
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace/project",
        host_workdir_fingerprint: "sha256:test",
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::agent::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk-agent-smith",
        docker: crate::instance::DockerResources {
            role_container: container_name.to_owned(),
            dind_container: Some(format!("{container_name}-dind")),
            network: format!("{container_name}-net"),
            certs_volume: Some(format!("{container_name}-dind-certs")),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });
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
        Some(&manifest),
        jackin_core::agent::Agent::Claude,
        None,
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
    let manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
        container_base: container_name,
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace/project",
        host_workdir_fingerprint: "sha256:test",
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::agent::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk-agent-smith",
        docker: crate::instance::DockerResources {
            role_container: container_name.to_owned(),
            dind_container: Some(format!("{container_name}-dind")),
            network: format!("{container_name}-net"),
            certs_volume: Some(format!("{container_name}-dind-certs")),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });
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
        Some(&manifest),
        jackin_core::agent::Agent::Claude,
        None,
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
        jackin_core::agent::Agent::Claude,
        None,
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

#[tokio::test]
async fn hardline_marks_missing_manifest_restore_available() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let mut manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
        container_base: container_name,
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::agent::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk-agent-smith",
        docker: crate::instance::DockerResources {
            role_container: container_name.to_owned(),
            dind_container: Some(format!("{container_name}-dind")),
            network: format!("{container_name}-net"),
            certs_volume: Some(format!("{container_name}-dind-certs")),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });
    manifest.mark_status(InstanceStatus::Crashed);
    let state_dir = paths.data_dir.join(container_name);
    manifest.write(&state_dir).unwrap();
    InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
    let docker = FakeDockerClient::default(); // NotFound
    let mut runner = FakeRunner::default();

    let err = hardline_agent(&paths, container_name, &docker, &mut runner)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("state remains recoverable"));
    let manifest = InstanceManifest::read(&state_dir).unwrap();
    assert_eq!(manifest.status, InstanceStatus::RestoreAvailable);
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
    assert_eq!(index.instances[0].status, InstanceStatus::RestoreAvailable);
}

#[tokio::test]
async fn inspect_hardline_instance_reports_state_without_attaching() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let mut manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
        container_base: container_name,
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::agent::Agent::Codex,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: Some("feature/role"),
        image_tag: "jk-agent-smith",
        docker: crate::instance::DockerResources {
            role_container: container_name.to_owned(),
            dind_container: Some(format!("{container_name}-dind")),
            network: format!("{container_name}-net"),
            certs_volume: Some(format!("{container_name}-dind-certs")),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });
    manifest.mark_status(InstanceStatus::PreservedDirty);
    manifest.last_attach_outcome = Some("exit:137".to_owned());
    manifest
        .write(&paths.data_dir.join(container_name))
        .unwrap();
    // inspect: role container running, dind stopped
    // exec_capture: jackin-capsule status returns two sessions
    // inspect_network: network present
    let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(VecDeque::from([
                ContainerState::Running,
                ContainerState::Stopped {
                    exit_code: 137,
                    oom_killed: false,
                },
            ])),
            exec_capture_queue: std::cell::RefCell::new(VecDeque::from([
                "Sessions: 2\n  [1] jackin-claude-abc123 (claude) state=working active=true\n  [2] jackin-codex-abc (codex) state=idle active=false".to_owned(),
            ])),
            inspect_network_queue: std::cell::RefCell::new(VecDeque::from([
                Some(jackin_docker::docker_client::NetworkRow {
                    name: format!("{container_name}-net"),
                    labels: HashMap::default(),
                }),
            ])),
            ..Default::default()
        };
    let report = inspect_hardline_instance(&paths, container_name, &docker)
        .await
        .unwrap();

    assert!(report.contains("Instance ID: k7p9m2xq"), "{report}");
    assert!(report.contains("Workspace: workspace"), "{report}");
    assert!(report.contains("Role: agent-smith"), "{report}");
    assert!(report.contains("Agent: codex"), "{report}");
    assert!(report.contains("Status: preserved_dirty"), "{report}");
    assert!(report.contains("Last attach outcome: exit:137"), "{report}");
    assert!(
        report.contains("Agent sessions: jackin-claude-abc123; jackin-codex-abc"),
        "{report}"
    );
    assert!(report.contains("Role container: jk-k7p9m2xq-workspace-agentsmith (running)"));
    assert!(
        report.contains("DinD container: jk-k7p9m2xq-workspace-agentsmith-dind (stopped exit:137)")
    );
    assert!(report.contains("Docker network: jk-k7p9m2xq-workspace-agentsmith-net (present)"));
}

#[tokio::test]
async fn inspect_agent_sessions_lists_jackin_sessions() {
    let docker = FakeDockerClient {
            exec_capture_queue: std::cell::RefCell::new(VecDeque::from([
                "Sessions: 2\n  [1] Claude (claude) state=working active=true\n  [2] Codex (codex) state=idle active=false".to_owned(),
            ])),
            ..Default::default()
        };

    let sessions =
        inspect_agent_sessions(&docker, "jk-agent-smith", &ContainerState::Running).await;

    let AgentSessionInventory::Sessions(sessions) = sessions else {
        panic!("expected sessions");
    };
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0].name, "Claude");
    assert_eq!(sessions[1].name, "Codex");
}

#[tokio::test]
async fn inspect_agent_sessions_returns_empty_when_no_sessions_running() {
    let docker = FakeDockerClient {
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from(["Sessions: 0".to_owned()])),
        ..Default::default()
    };

    let sessions =
        inspect_agent_sessions(&docker, "jk-agent-smith", &ContainerState::Running).await;

    assert_eq!(sessions, AgentSessionInventory::Sessions(vec![]));
}

#[tokio::test]
async fn inspect_agent_sessions_returns_unavailable_on_missing_header() {
    // A daemon that crashed mid-call or a cosmetic change to the
    // status print must surface as Unavailable, not as "zero sessions".
    let docker = FakeDockerClient {
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from([String::new()])),
        ..Default::default()
    };

    let sessions =
        inspect_agent_sessions(&docker, "jk-agent-smith", &ContainerState::Running).await;

    assert!(
        matches!(sessions, AgentSessionInventory::Unavailable(_)),
        "expected Unavailable on missing header; got {sessions:?}"
    );
}

#[tokio::test]
async fn inspect_agent_sessions_returns_unavailable_on_count_mismatch() {
    let docker = FakeDockerClient {
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from([
            "Sessions: 5\n  [1] Claude (claude) state=working active=true".to_owned(),
        ])),
        ..Default::default()
    };

    let sessions =
        inspect_agent_sessions(&docker, "jk-agent-smith", &ContainerState::Running).await;

    assert!(
        matches!(sessions, AgentSessionInventory::Unavailable(_)),
        "expected Unavailable on count mismatch; got {sessions:?}"
    );
}

#[tokio::test]
async fn inspect_agent_sessions_skips_query_when_container_is_not_running() {
    let docker = FakeDockerClient::default();

    let sessions = inspect_agent_sessions(
        &docker,
        "jk-agent-smith",
        &ContainerState::Stopped {
            exit_code: 137,
            oom_killed: false,
        },
    )
    .await;

    assert_eq!(sessions, AgentSessionInventory::NotRunning);
    assert!(docker.recorded.borrow().is_empty());
}

#[tokio::test]
async fn inspect_hardline_instance_still_reports_manifest_when_docker_unavailable() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
        container_base: container_name,
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::agent::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk-agent-smith",
        docker: crate::instance::DockerResources {
            role_container: container_name.to_owned(),
            dind_container: Some(format!("{container_name}-dind")),
            network: format!("{container_name}-net"),
            certs_volume: Some(format!("{container_name}-dind-certs")),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });
    manifest
        .write(&paths.data_dir.join(container_name))
        .unwrap();
    let docker = FakeDockerClient {
        fail_with: vec![(
            "docker inspect jk-k7p9m2xq-workspace-agentsmith".to_owned(),
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_owned(),
        )],
        ..Default::default()
    };
    let report = inspect_hardline_instance(&paths, container_name, &docker)
        .await
        .unwrap();

    assert!(report.contains("Workspace: workspace"), "{report}");
    assert!(report.contains("Role container: jk-k7p9m2xq-workspace-agentsmith (unavailable:"));
}

#[tokio::test]
async fn hardline_errors_on_clean_exit() {
    let (_tmp, paths) = test_paths();
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 0,
            oom_killed: false,
        }])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    let err = hardline_agent(&paths, "jk-agent-smith", &docker, &mut runner)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("exited cleanly"));
    assert!(
        !runner
            .recorded
            .iter()
            .any(|c| c.contains("docker start") || c.contains("jackin-capsule new"))
    );
}

#[tokio::test]
async fn hardline_refuses_crashed_container() {
    let (_tmp, paths) = test_paths();
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 137,
            oom_killed: false,
        }])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    let err = hardline_agent(&paths, "jk-agent-smith", &docker, &mut runner)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("stopped") && err.to_string().contains("jackin load"),
        "expected error directing to jackin load; got: {err}"
    );
    assert!(
        !runner
            .recorded
            .iter()
            .any(|c| c.contains("docker start") || c.contains("tmux")),
        "hardline must not restart or attach stopped containers"
    );
}

#[tokio::test]
async fn hardline_refuses_oom_killed_container() {
    let (_tmp, paths) = test_paths();
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 0,
            oom_killed: true,
        }])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    let err = hardline_agent(&paths, "jk-agent-smith", &docker, &mut runner)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("OOM") && err.to_string().contains("jackin load"),
        "expected OOM error directing to jackin load; got: {err}"
    );
}

#[tokio::test]
async fn wait_for_dind_times_out_when_all_attempts_fail() {
    tokio::time::pause(); // make all sleeps instant
    let docker = FakeDockerClient {
        fail_with: vec![("docker exec".to_owned(), "connection refused".to_owned())],
        ..Default::default()
    };

    let err = wait_for_dind("jk-agent-smith-dind", "jk-agent-smith-dind-certs", &docker)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("timed out"), "got: {err}");
}

#[tokio::test]
async fn wait_for_dind_fails_when_cert_absent() {
    // First exec (docker info) succeeds; second exec (test -f) exits with code 1.
    let docker = FakeDockerClient {
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from([
            // docker info: success
            String::new(),
        ])),
        fail_with: vec![(
            "test -f /certs/client/ca.pem".to_owned(),
            "exec in jk-agent-smith-dind exited with code 1: ".to_owned(),
        )],
        ..Default::default()
    };

    let err = wait_for_dind("jk-agent-smith-dind", "jk-agent-smith-dind-certs", &docker)
        .await
        .unwrap_err();

    assert!(
        err.to_string()
            .contains("TLS client certificates not found"),
        "got: {err}"
    );
}

#[tokio::test]
async fn spawn_shell_session_succeeds_when_container_paused_or_restarting() {
    for state in [ContainerState::Paused, ContainerState::Restarting] {
        let (_tmp, paths) = test_paths();
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(VecDeque::from([state.clone()])),
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
            }),
            "state={state:?}: expected docker exec with jackin-capsule; got: {:?}",
            runner.recorded
        );
    }
}

#[tokio::test]
async fn hardline_agent_errors_on_inactive_states() {
    let cases: &[(ContainerState, &str)] = &[
        (ContainerState::Created, "created"),
        (ContainerState::Dead, "dead"),
        (ContainerState::Removing, "removing"),
    ];
    for (state, expected_phrase) in cases {
        let (_tmp, paths) = test_paths();
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(VecDeque::from([state.clone()])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();
        let err = hardline_agent(&paths, "jk-agent-smith", &docker, &mut runner)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains(expected_phrase),
            "state={state:?}: expected phrase {expected_phrase:?}; got: {err}"
        );
        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("tmux") || c.contains("docker start")),
            "state={state:?}: no exec or start must fire"
        );
    }
}

#[tokio::test]
async fn inspect_agent_sessions_returns_not_running_for_non_running_states() {
    for state in [ContainerState::Paused, ContainerState::Restarting] {
        let docker = FakeDockerClient::default();
        let sessions = inspect_agent_sessions(&docker, "jk-agent-smith", &state).await;
        assert_eq!(
            sessions,
            AgentSessionInventory::NotRunning,
            "state={state:?}"
        );
        assert!(
            docker.recorded.borrow().is_empty(),
            "state={state:?}: exec_capture must not be called"
        );
    }
}

#[tokio::test]
async fn wait_for_dind_succeeds_when_daemon_ready_immediately() {
    // docker info succeeds on first attempt; test -f /certs/client/ca.pem also succeeds.
    let docker = FakeDockerClient {
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from([
            String::new(), // docker info
            String::new(), // test -f /certs/client/ca.pem
        ])),
        ..Default::default()
    };

    wait_for_dind("jk-agent-smith-dind", "jk-agent-smith-dind-certs", &docker)
        .await
        .unwrap();
}

#[test]
fn git_policy_env_pairs_encodes_only_enabled_toggles() {
    use jackin_core::env_model::{JACKIN_GIT_COAUTHOR_TRAILER_ENV_NAME, JACKIN_GIT_DCO_ENV_NAME};

    assert!(git_policy_env_pairs(false, false).is_empty());
    assert_eq!(
        git_policy_env_pairs(true, false),
        vec![(JACKIN_GIT_COAUTHOR_TRAILER_ENV_NAME, "1")]
    );
    assert_eq!(
        git_policy_env_pairs(false, true),
        vec![(JACKIN_GIT_DCO_ENV_NAME, "1")]
    );
    assert_eq!(
        git_policy_env_pairs(true, true),
        vec![
            (JACKIN_GIT_COAUTHOR_TRAILER_ENV_NAME, "1"),
            (JACKIN_GIT_DCO_ENV_NAME, "1"),
        ]
    );
}
