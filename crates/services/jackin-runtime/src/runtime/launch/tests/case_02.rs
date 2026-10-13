// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn capsule_config_serializes_manifest_models() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
agents = ["claude", "codex", "amp", "kimi", "opencode"]

[claude]
model = "sonnet"

[codex]
model = "gpt-5"

[amp]

[kimi]
model = "kimi-k2"

[opencode]
model = "zai/glm"
"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();

    let manifest = jackin_manifest::load_role_manifest(temp.path()).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");
    let instance = |config_id: &str, agent: jackin_core::Agent| jackin_config::ResolvedInstance {
        config_id: config_id.into(),
        agent,
        account_id: "test".into(),
        model: None,
        base_url: None,
        xdg_roots: None,
        label: config_id.into(),
        synthesized: true,
    };
    let instances = [
        instance("test@claude", jackin_core::Agent::Claude),
        instance("test@codex", jackin_core::Agent::Codex),
        instance("test@amp", jackin_core::Agent::Amp),
        instance("test@kimi", jackin_core::Agent::Kimi),
        instance("test@opencode", jackin_core::Agent::Opencode),
    ];
    let config = capsule_config(
        &selector,
        "/workspace",
        &manifest,
        "ask",
        Vec::new(),
        Vec::new(),
        Vec::new(),
        &instances,
    );
    let auth_modes =
        super::capsule_setup::capsule_auth_modes(&jackin_config::AppConfig::default(), &[])
            .unwrap();

    assert_eq!(config.role, "chainargos/the-architect");
    assert_eq!(config.workdir, "/workspace");
    assert_eq!(
        config.instances,
        vec![
            "test@claude",
            "test@codex",
            "test@amp",
            "test@kimi",
            "test@opencode"
        ]
    );
    assert_eq!(config.models.get("test@claude").unwrap(), "sonnet");
    assert_eq!(config.models.get("test@codex").unwrap(), "gpt-5");
    assert_eq!(config.models.get("test@kimi").unwrap(), "kimi-k2");
    assert_eq!(config.models.get("test@opencode").unwrap(), "zai/glm");
    assert!(!config.models.contains_key("test@amp"));
    assert!(auth_modes.is_empty());
}

#[test]
fn selected_account_model_overrides_native_role_model_only_when_admitted() {
    use jackin_core::Agent;
    let mut config = AppConfig::default();
    config.accounts.insert(
        "coding".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Coding".into(),
            provider: jackin_config::AiProvider::Moonshot,
            credential: jackin_config::AccountCredential::ApiKey {
                value: "fixture-key".into(),
                base_url: None,
                model: Some("k3-256k".into()),
            },
        },
    );
    config
        .account_bindings
        .insert(Agent::Codex, "coding".into());
    config
        .workspaces
        .insert("isolated".into(), jackin_config::WorkspaceConfig::default());
    let baseline = jackin_protocol::CapsuleConfig {
        models: std::collections::BTreeMap::from([("codex".into(), "native-model".into())]),
        ..Default::default()
    };
    let mut launch = baseline.clone();
    let admitted = [jackin_config::ResolvedInstance {
        config_id: "coding-codex".into(),
        agent: Agent::Codex,
        account_id: "coding".into(),
        model: Some("k3-256k".into()),
        base_url: None,
        xdg_roots: None,
        label: "coding-codex".into(),
        synthesized: false,
    }];
    super::capsule_setup::apply_account_models(&mut launch, &config, &admitted).unwrap();
    assert_eq!(launch.models["coding-codex"], "k3-256k");
    let mut isolated = baseline;
    let unadmitted = [jackin_config::ResolvedInstance {
        config_id: "coding-codex".into(),
        agent: Agent::Codex,
        account_id: "coding".into(),
        model: None,
        base_url: None,
        xdg_roots: None,
        label: "coding-codex".into(),
        synthesized: false,
    }];
    super::capsule_setup::apply_account_models(&mut isolated, &config, &unadmitted).unwrap();
    assert_eq!(isolated.models["codex"], "native-model");
}

#[tokio::test]
async fn diagnose_premature_exit_returns_none_when_container_running() {
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Running])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();
    let result = diagnose_premature_exit(
        &docker,
        &mut runner,
        "jk-the-architect",
        ExitPhase::PreAttach,
    )
    .await;
    assert!(
        result.is_none(),
        "running container must not be diagnosed as a failure"
    );
}

#[tokio::test]
async fn diagnose_premature_exit_includes_logs_when_container_already_stopped() {
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 127,
            oom_killed: false,
        }])),

        ..Default::default()
    };
    let mut runner = FakeRunner::with_combined_queue([
        "/jackin/runtime/entrypoint.sh: line 85: exec: codex: not found".to_owned(),
    ]);
    let err = diagnose_premature_exit(
        &docker,
        &mut runner,
        "jk-the-architect",
        ExitPhase::PreAttach,
    )
    .await
    .expect("stopped container must produce a diagnostic error");
    let msg = err.to_string();
    assert!(
        msg.contains("exit 127"),
        "exit code missing from msg: {msg}"
    );
    assert!(
        msg.contains("codex: not found"),
        "logs missing from msg: {msg}"
    );
    assert!(
        runner
            .recorded
            .iter()
            .any(|c| c.contains("docker logs --tail 40 jk-the-architect")),
        "must shell out to `docker logs` to capture the entrypoint output"
    );
}

#[tokio::test]
async fn diagnose_premature_exit_flags_oom_kill_distinct_from_normal_exit() {
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 137,
            oom_killed: true,
        }])),

        ..Default::default()
    };
    let mut runner = FakeRunner::with_combined_queue([String::new()]);
    let err = diagnose_premature_exit(&docker, &mut runner, "jackin-x", ExitPhase::PreAttach)
        .await
        .expect("OOM-killed container is a premature exit");
    let msg = err.to_string();
    assert!(msg.contains("OOM killed"), "expected OOM marker in: {msg}");
    assert!(
        msg.contains("no log output"),
        "empty logs branch missing: {msg}"
    );
}

#[tokio::test]
async fn diagnose_premature_exit_passes_through_when_inspect_returns_notfound() {
    use jackin_test_support::FakeDockerClient;
    let docker = FakeDockerClient::default(); // empty queue → NotFound
    let mut runner = FakeRunner::default();
    assert!(
        diagnose_premature_exit(&docker, &mut runner, "jackin-x", ExitPhase::PreAttach)
            .await
            .is_none(),
        "NotFound must not abort launch before exec attempt"
    );
}

#[tokio::test]
async fn diagnose_premature_exit_swallows_post_attach_clean_exit() {
    // Operator typed `/exit` in the agent → multiplexer drained
    // the last live session → container shut itself down with
    // exit 0. The container-lifecycle policy treats this as the
    // happy path; the host CLI must not surface it as an error.
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 0,
            oom_killed: false,
        }])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();
    let result = diagnose_premature_exit(
        &docker,
        &mut runner,
        "jk-the-architect",
        ExitPhase::PostAttach,
    )
    .await;
    assert!(
        result.is_none(),
        "post-attach exit 0 is the lifecycle-policy clean-shutdown path, not an error"
    );
    assert!(
        runner.recorded.is_empty(),
        "no `docker logs` fetch when the post-attach exit is clean"
    );
}

#[tokio::test]
async fn diagnose_premature_exit_surfaces_post_attach_nonzero_exit() {
    // Post-attach exit with a non-zero code still indicates a
    // problem inside the multiplexer / agent — operator wants the
    // logs surfaced even though the container is gone now.
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 137,
            oom_killed: false,
        }])),
        ..Default::default()
    };
    let mut runner = FakeRunner::with_combined_queue(["panic: VT screen overflow".to_owned()]);
    let err = diagnose_premature_exit(
        &docker,
        &mut runner,
        "jk-the-architect",
        ExitPhase::PostAttach,
    )
    .await
    .expect("post-attach non-zero exit must produce a diagnostic error");
    let msg = err.to_string();
    assert!(
        msg.contains("exited during session"),
        "phase label missing in: {msg}"
    );
    assert!(msg.contains("exit 137"), "exit code missing in: {msg}");
    assert!(
        msg.contains("panic: VT screen overflow"),
        "logs missing in: {msg}"
    );
}

#[tokio::test]
async fn diagnose_premature_exit_surfaces_pre_attach_exit_zero() {
    // Pre-attach exit 0 is still suspicious — PID 1 exited
    // without doing anything, most likely a bad image or missing
    // entrypoint. Operator wants the heads-up even though the
    // exit code looks clean.
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 0,
            oom_killed: false,
        }])),
        ..Default::default()
    };
    let mut runner = FakeRunner::with_combined_queue([String::new()]);
    let err = diagnose_premature_exit(
        &docker,
        &mut runner,
        "jk-the-architect",
        ExitPhase::PreAttach,
    )
    .await
    .expect("pre-attach exit 0 must still flag a missing Capsule");
    let msg = err.to_string();
    assert!(
        msg.contains("exited before attach"),
        "phase label missing in: {msg}"
    );
    assert!(msg.contains("exit 0"), "exit code missing in: {msg}");
}

#[tokio::test]
async fn diagnose_premature_exit_reports_empty_docker_logs() {
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;

    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 1,
            oom_killed: false,
        }])),
        ..Default::default()
    };
    let mut runner = FakeRunner::with_combined_queue([String::new()]);
    let err = diagnose_premature_exit(
        &docker,
        &mut runner,
        "jk-the-architect",
        ExitPhase::PreAttach,
    )
    .await
    .expect("pre-attach exit 1 must produce a diagnostic error");
    let msg = err.to_string();
    assert!(
        msg.contains("no log output"),
        "empty-log detail missing: {msg}"
    );
}
