// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn render_exit_preserves_universe_marker_when_instances_remain() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();
    super::universe::mark_start(&paths, super::universe::StartKind::FreshConstruct).await;
    let marker = crate::runtime::coordination::universe_dir(&paths)
        .unwrap()
        .join("universe-since");
    let docker = jackin_test_support::FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![
            jackin_docker::docker_client::ContainerRow {
                name: "jk-still-running".to_owned(),
                id: "container-id".to_owned(),
                labels: HashMap::new(),
            },
        ]])),
        ..Default::default()
    };
    render_exit(&paths, &docker).await;

    assert!(
        marker.exists(),
        "leaving one of multiple instances keeps the universe open"
    );
}

#[tokio::test]
async fn render_exit_preserves_universe_marker_when_running_list_fails() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();
    super::universe::mark_start(&paths, super::universe::StartKind::FreshConstruct).await;
    let marker = crate::runtime::coordination::universe_dir(&paths)
        .unwrap()
        .join("universe-since");
    let docker = jackin_test_support::FakeDockerClient {
        fail_with: vec![("docker ps".to_owned(), "daemon down".to_owned())],
        ..Default::default()
    };
    render_exit(&paths, &docker).await;

    assert!(
        marker.exists(),
        "unknown Docker state must not close the universe"
    );
}

#[tokio::test]
async fn load_agent_injects_global_operator_env_literal() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();

    // Seed a config.toml with a global operator env map.
    std::fs::write(
        &paths.config_file,
        format!(
            "{SINGLETON_CLAUDE_TOML}{}",
            r#"[env]
OPERATOR_SMOKE = "smoke-literal"
ON_DEMAND_LITERAL = { value = "on-demand-literal-secret", on_demand = true }

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#
        ),
    )
    .unwrap();

    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
    ]);
    let observed_env = observe_host_env_file(&mut runner, &paths);

    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    assert!(
        run_cmd.contains("--env-file") && !run_cmd.contains("smoke-literal"),
        "operator env must use the host env file; got: {run_cmd}"
    );
    let observed = observed_env.lock().unwrap().clone().unwrap();
    assert!(
        observed
            .contents
            .lines()
            .any(|line| line == "OPERATOR_SMOKE=smoke-literal")
    );
    assert!(!observed.path.exists());
    assert_host_env_file_outside_mounts(run_cmd, &observed.path);

    let container_name = launched_role_container_name(&runner);
    let socket_dir = paths.jackin_home.join("sockets").join(container_name);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&socket_dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }
    let capsule_config =
        std::fs::read_to_string(socket_dir.join(jackin_protocol::CAPSULE_CONFIG_FILENAME)).unwrap();
    assert!(!capsule_config.contains("on-demand-literal-secret"));
    assert!(capsule_config.contains("name = \"ON_DEMAND_LITERAL\""));
    assert!(capsule_config.contains("source = \"literal\""));
}

#[tokio::test]
async fn load_agent_keeps_zai_secret_out_of_capsule_config() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();

    std::fs::write(
        &paths.config_file,
        r#"[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
    )
    .unwrap();

    let mut config = AppConfig::load_or_init(&paths).unwrap();
    config.accounts.insert(
        "coding".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Coding".into(),
            provider: jackin_config::AiProvider::Zai,
            credential: jackin_config::AccountCredential::ApiKey {
                value: "super-secret-zai-key".into(),
                base_url: None,
                model: Some("glm-5.3".into()),
            },
        },
    );
    config
        .account_bindings
        .insert(jackin_core::Agent::Claude, "coding".into());
    config.agent_configurations.insert(
        "claude-main".into(),
        jackin_config::AgentConfiguration {
            agent: jackin_core::Agent::Claude,
            account: "coding".into(),
            model: None,
            base_url: None,
            display_label: None,
            invoked_via_wrapper: None,
        },
    );
    config.default_launch = Some(vec!["claude-main".into()]);
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
    ]);
    let observed_env = observe_host_env_file(&mut runner, &paths);

    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
    let opts = LoadOptions {
        ..LoadOptions::default()
    };
    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .unwrap();

    let container_name = launched_role_container_name(&runner);
    let capsule_config_path = paths
        .jackin_home
        .join("sockets")
        .join(&container_name)
        .join(jackin_protocol::CAPSULE_CONFIG_FILENAME);
    let capsule_config = std::fs::read_to_string(capsule_config_path).unwrap();
    assert!(
        !capsule_config.contains("super-secret-zai-key"),
        "CapsuleConfig must not persist resolved provider secrets: {capsule_config}"
    );
    assert!(
        !capsule_config.contains("zai_key"),
        "CapsuleConfig must not contain a provider secret field: {capsule_config}"
    );

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    assert!(!run_cmd.contains("super-secret-zai-key"));
    let observed = observed_env.lock().unwrap().clone().unwrap();
    assert!(!observed.contents.contains("super-secret-zai-key"));
    #[cfg(unix)]
    assert_eq!(observed.mode, 0o600);
    let credentials_path = paths.data_dir.join(&container_name).join(format!(
        "credentials/{}",
        jackin_protocol::account_credentials_filename("claude-main")
    ));
    let credentials: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&credentials_path).unwrap()).unwrap();
    assert_eq!(credentials["schema_version"], 1);
    assert_eq!(
        credentials["credential"]["env"]["ANTHROPIC_AUTH_TOKEN"],
        "super-secret-zai-key"
    );
    assert_eq!(credentials["instance"], "claude-main");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            std::fs::metadata(&credentials_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    assert_host_env_file_outside_mounts(run_cmd, &observed.path);
    assert!(!observed.path.exists());
}
