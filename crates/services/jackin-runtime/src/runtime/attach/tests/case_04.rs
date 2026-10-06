// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
        agent_runtime: jackin_core::Agent::Claude,
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
        agent_runtime: jackin_core::Agent::Codex,
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
    manifest.docker_identity = Some(crate::instance::DockerIdentity {
        role_container_id: "role-container-id".into(),
        dind_container_id: Some("dind-container-id".into()),
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
            container_id_by_name: std::cell::RefCell::new(HashMap::from([
                (container_name.to_owned(), "role-container-id".to_owned()),
                (
                    format!("{container_name}-dind"),
                    "dind-container-id".to_owned(),
                ),
            ])),
            inspect_state_by_name: std::cell::RefCell::new(HashMap::from([
                (container_name.to_owned(), ContainerState::Running),
                (
                    format!("{container_name}-dind"),
                    ContainerState::Stopped {
                        exit_code: 137,
                        oom_killed: false,
                    },
                ),
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

    let sessions = inspect_agent_sessions(
        &docker,
        &test_container_handle("jk-agent-smith"),
        &ContainerState::Running,
    )
    .await;

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

    let sessions = inspect_agent_sessions(
        &docker,
        &test_container_handle("jk-agent-smith"),
        &ContainerState::Running,
    )
    .await;

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

    let sessions = inspect_agent_sessions(
        &docker,
        &test_container_handle("jk-agent-smith"),
        &ContainerState::Running,
    )
    .await;

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

    let sessions = inspect_agent_sessions(
        &docker,
        &test_container_handle("jk-agent-smith"),
        &ContainerState::Running,
    )
    .await;

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
        &test_container_handle("jk-agent-smith"),
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
        agent_runtime: jackin_core::Agent::Claude,
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

    let err = wait_for_dind(
        &test_container_handle("jk-agent-smith-dind"),
        "jk-agent-smith-dind-certs",
        &docker,
    )
    .await
    .unwrap_err();

    assert!(err.to_string().contains("timed out"), "got: {err}");
}
