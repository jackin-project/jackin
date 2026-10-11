// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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

    let err = wait_for_dind(
        &test_container_handle("jk-agent-smith-dind"),
        "jk-agent-smith-dind-certs",
        &docker,
    )
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
        provision_account_admission(&paths, "jk-agent-smith");
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
        let sessions =
            inspect_agent_sessions(&docker, &test_container_handle("jk-agent-smith"), &state).await;
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

    wait_for_dind(
        &test_container_handle("jk-agent-smith-dind"),
        "jk-agent-smith-dind-certs",
        &docker,
    )
    .await
    .unwrap();
}

#[test]
fn git_policy_env_pairs_encodes_only_enabled_toggles() {
    use jackin_core::{JACKIN_GIT_COAUTHOR_TRAILER_ENV_NAME, JACKIN_GIT_DCO_ENV_NAME};

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

#[tokio::test]
async fn revoked_account_blocks_focused_attach_agent_and_shell_before_exec() {
    use jackin_config::{AccountConfig, AccountCredential, AiProvider, AppConfig, WorkspaceConfig};
    for (route, disable) in ["focus", "agent", "shell"]
        .into_iter()
        .flat_map(|route| [false, true].map(|disable| (route, disable)))
    {
        let (_tmp, paths) = test_paths();
        let name = "jk-account-policy";
        let mut config = AppConfig::default();
        config.accounts.insert(
            "work".into(),
            AccountConfig {
                enabled: true,
                name: "Work".into(),
                provider: AiProvider::Anthropic,
                credential: AccountCredential::ApiKey {
                    value: "fixture-key".into(),
                    base_url: None,
                    model: None,
                },
            },
        );
        config.workspaces.insert(
            "project".into(),
            WorkspaceConfig {
                accounts: vec!["work".into()],
                workdir: "/workspace".into(),
                mounts: vec![jackin_config::MountConfig {
                    src: paths.home_dir.display().to_string(),
                    dst: "/workspace".into(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                }],
                ..WorkspaceConfig::default()
            },
        );
        let admitted = [crate::instance::AdmittedInstance::new(
            "work@claude",
            jackin_core::Agent::Claude,
            "work",
        )];
        write_admission_fixture(&paths, name, &config, Some("project"), &admitted);
        require_current_account_admission(&paths, name).unwrap();
        if disable {
            config.accounts.get_mut("work").unwrap().enabled = false;
        } else {
            config
                .workspaces
                .get_mut("project")
                .unwrap()
                .accounts
                .clear();
        }
        std::fs::write(
            paths.config_dir.join("config.toml"),
            toml::to_string(&config).unwrap(),
        )
        .unwrap();
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Running])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();
        let result = match route {
            "focus" => hardline_agent_with_focus(&paths, name, Some(7), &docker, &mut runner).await,
            "agent" => {
                spawn_agent_session(
                    &paths,
                    name,
                    None,
                    jackin_core::Agent::Claude,
                    &[],
                    false,
                    false,
                    &docker,
                    &mut runner,
                )
                .await
            }
            _ => spawn_shell_session(&paths, name, &docker, &mut runner).await,
        };
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("account policy changed")
        );
        assert!(
            runner.recorded.is_empty(),
            "{route} ran commands after account revocation"
        );
        assert!(
            !docker
                .recorded
                .borrow()
                .iter()
                .any(|call| call.contains("exec") || call.contains("start")),
            "{route} contacted capsule after account revocation"
        );
    }
}

#[test]
fn missing_policy_and_changed_binding_deny_reconnect() {
    use jackin_config::{AccountConfig, AccountCredential, AiProvider, AppConfig};
    let (_tmp, paths) = test_paths();
    let name = "jk-account-policy";
    let mut config = AppConfig::default();
    for id in ["personal", "work"] {
        config.accounts.insert(
            id.into(),
            AccountConfig {
                enabled: true,
                name: id.into(),
                provider: AiProvider::Anthropic,
                credential: AccountCredential::ApiKey {
                    value: format!("{id}-key").into(),
                    base_url: None,
                    model: None,
                },
            },
        );
    }
    config
        .account_bindings
        .insert(jackin_core::Agent::Claude, "personal".into());
    let admitted = [crate::instance::AdmittedInstance::new(
        "personal@claude",
        jackin_core::Agent::Claude,
        "personal",
    )];
    write_admission_fixture(&paths, name, &config, None, &admitted);
    require_current_account_admission(&paths, name).unwrap();
    config
        .account_bindings
        .insert(jackin_core::Agent::Claude, "work".into());
    std::fs::write(
        paths.config_dir.join("config.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();
    require_current_account_admission(&paths, name).unwrap_err();
    write_admission_fixture(&paths, name, &config, None, &[]);
    std::fs::remove_file(paths.data_dir.join(name).join("account-admission.sha256")).unwrap();
    require_current_account_admission(&paths, name).unwrap_err();
}

#[tokio::test]
async fn agent_session_rejects_account_overrides_before_container_access() {
    let (_tmp, paths) = test_paths();
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();
    for name in [
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_BASE_URL",
        "OPENAI_BASE_URL",
        "KIMI_API_KEY",
        "CODEX_HOME",
        "HOME",
        "OPENCODE_CONFIG_CONTENT",
    ] {
        let error = spawn_agent_session(
            &paths,
            "jk-agent-smith",
            None,
            jackin_core::Agent::Claude,
            &[(name.into(), "must-not-leak".into())],
            false,
            false,
            &docker,
            &mut runner,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("overrides are not allowed"));
        assert!(!error.to_string().contains("must-not-leak"));
    }
    assert!(docker.recorded.borrow().is_empty());
    assert!(runner.recorded.is_empty());
}

#[tokio::test]
async fn apple_backend_reconnect_rejects_unverified_account_admission() {
    use super::backend::ContainerBackend as _;
    let (_tmp, paths) = test_paths();
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    std::fs::File::create(paths.config_file.with_file_name("config.lock")).unwrap();
    let mut runner = FakeRunner::default();
    let error = backend::AppleContainerBackend::production()
        .reconnect(&paths, "jk-unverified", Some(9), &mut runner)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("cannot verify"));
    assert!(runner.recorded.is_empty());
}

#[tokio::test]
async fn start_stopped_container_errors_clearly_when_network_missing() {
    let (_tmp, paths) = test_paths();
    let container_name = "jk-agent-smith";
    provision_account_admission(&paths, container_name);
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            },
            ContainerState::NotFound, // for dind check
        ])),
        inspect_network_queue: std::cell::RefCell::new(VecDeque::from([None])),
        fail_with: vec![("start_container".to_owned(), "network not found".to_owned())],
        ..Default::default()
    };
    let mut runner = FakeRunner::default();

    let err = start_or_reconnect_capsule_client(&paths, container_name, &docker, &mut runner)
        .await
        .unwrap_err();

    assert!(
        err.to_string()
            .contains("cannot be started because its Docker network")
    );
    assert!(
        err.to_string()
            .contains("run `jackin load` to recreate the instance")
    );
}
