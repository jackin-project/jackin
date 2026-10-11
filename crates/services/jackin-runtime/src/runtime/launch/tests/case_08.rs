// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn restore_role_source_override_uses_manifest_source_without_mutating_config() {
    let selector = RoleSelector::new(None, "agent-smith");
    let mut config = AppConfig::default();
    config.roles.insert(
        "agent-smith".to_owned(),
        jackin_config::RoleSource {
            git: "https://example.invalid/current.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );

    let (source, is_new, restore_override) = resolve_launch_role_source(
        &mut config,
        &selector,
        Some("https://example.invalid/recorded.git"),
    )
    .unwrap();

    assert_eq!(source.git, "https://example.invalid/recorded.git");
    assert!(source.trusted);
    assert!(!is_new);
    assert!(restore_override);
    assert_eq!(
        config.roles.get("agent-smith").unwrap().git,
        "https://example.invalid/current.git"
    );
}

#[tokio::test]
async fn load_namespaced_agent_registers_source_and_trusts_on_accept() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"default_launch = ["claude-main"]

[accounts.test]
name = "Test"
provider = "anthropic"
[accounts.test.credential]
type = "api_key"
value = "test-key"

[agent_configurations.claude-main]
agent = "claude"
account = "test"
"#,
    )
    .unwrap();
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");
    let mut runner =
        FakeRunner::for_load_agent(["false 0 false".to_owned(), "false 0 false".to_owned()]);

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
model = "sonnet"
plugins = ["code-review@claude-plugins-official"]
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
    load_role_with(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &compat_dind_load_options(),
        auto_trust,
        |_, _, _| Ok(()),
    )
    .await
    .unwrap();

    // Source was auto-registered and persisted with trust
    let persisted = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(persisted.contains("chainargos/the-architect"));
    assert!(persisted.contains("trusted = true"));
    assert!(
        runner
            .recorded
            .iter()
            .any(|call| call.contains("git -C") || call.contains("git clone"))
    );
    assert!(runner.recorded.iter().any(|call| {
        call.contains("docker build ")
            && call.contains("--output type=docker,name=jk_chainargos_the-architect")
    }));
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| { call.contains("docker inspect jk-") && call.contains("thearchitect") })
    );
    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| {
            call.contains("docker run -d --name jk-")
                && call.contains("thearchitect")
                && call.contains("jackin.kind=role")
        })
        .unwrap();
    let container_name = launched_role_container_name(&runner);
    assert!(crate::instance::naming::is_dns_label(&container_name));
    assert!(!container_name.contains("__"));
    assert!(!container_name.contains("clone"));
    assert!(!run_cmd.contains("JACKIN_CODEX_MODEL"));
    assert!(!run_cmd.contains("JACKIN_AGENT_MODEL_OVERRIDES"));
    assert!(!run_cmd.contains("-e JACKIN_ROLE="));
    let capsule_config_path = paths
        .jackin_home
        .join("sockets")
        .join(&container_name)
        .join(jackin_protocol::CAPSULE_CONFIG_FILENAME);
    let capsule_config: jackin_protocol::CapsuleConfig =
        toml::from_str(&std::fs::read_to_string(capsule_config_path).unwrap()).unwrap();
    assert_eq!(capsule_config.role, "chainargos/the-architect");
    assert_eq!(capsule_config.workdir, workspace.workdir);
    assert_eq!(capsule_config.instances, vec!["claude-main"]);
    assert_eq!(capsule_config.models.get("claude-main").unwrap(), "sonnet");
    assert!(
        !runner
            .recorded
            .iter()
            .any(|call| call.contains("claude plugin install"))
    );

    let (dind, dind_spec) = launched_dind_container(&docker);
    assert!(crate::instance::naming::is_dns_label(&dind));
    assert!(!dind.contains("__"));
    assert!(
        dind_spec
            .env
            .contains(&format!("DOCKER_TLS_SAN=DNS:{dind}")),
        "DinD SAN must include the DNS-safe DinD name with a DNS: prefix"
    );
}

#[tokio::test]
async fn role_container_never_mounts_host_docker_socket() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"default_launch = ["claude-main"]

[accounts.test]
name = "Test"
provider = "anthropic"
[accounts.test.credential]
type = "api_key"
value = "test-key"

[agent_configurations.claude-main]
agent = "claude"
account = "test"
"#,
    )
    .unwrap();
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");
    let mut runner =
        FakeRunner::for_load_agent(["false 0 false".to_owned(), "false 0 false".to_owned()]);

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
model = "sonnet"
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
    load_role_with(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &LoadOptions::default(),
        auto_trust,
        |_, _, _| Ok(()),
    )
    .await
    .unwrap();

    let role_run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d --name jk-") && call.contains("jackin.kind=role"))
        .expect("expected role docker run command");
    assert!(
        !role_run_cmd.contains("docker.sock"),
        "role container must never bind-mount the host Docker socket; run cmd was: {role_run_cmd}"
    );
    // Belt and suspenders: no container the fake daemon created (the DinD
    // sidecar included) binds the host Docker socket.
    for (name, spec) in docker.created_containers.borrow().iter() {
        assert!(
            !spec.binds.iter().any(|b| b.contains("docker.sock")),
            "container {name} must not bind-mount docker.sock"
        );
    }
}

#[tokio::test]
async fn load_namespaced_agent_aborts_when_trust_declined() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(Some("evil-org"), "backdoor");
    let mut runner = FakeRunner::for_load_agent([String::new(), String::new()]);

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
    let error = load_role_with(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &LoadOptions::default(),
        deny_trust,
        |_, _, _| Ok(()),
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("not trusted"));

    // Source was NOT persisted when trust was declined
    let persisted = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!persisted.contains("evil-org/backdoor"));

    // No Docker build or run commands were issued
    assert!(
        !runner
            .recorded
            .iter()
            .any(|call| call.contains("docker build") || call.contains("docker run"))
    );
}

#[tokio::test]
async fn load_agent_injects_configured_mounts() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let selector = RoleSelector::new(Some("chainargos"), "agent-brown");
    let mut runner =
        FakeRunner::for_load_agent(["false 0 false".to_owned(), "false 0 false".to_owned()]);

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

    let mount_src = temp.path().join("test-mount");
    std::fs::create_dir_all(&mount_src).unwrap();
    std::fs::create_dir_all(&paths.config_dir).unwrap();

    let config_content = format!(
        "{SINGLETON_CLAUDE_TOML}[roles.\"chainargos/agent-brown\"]\ngit = \"git@github.com:chainargos/jackin-agent-brown.git\"\ntrusted = true\n"
    );
    std::fs::write(&paths.config_file, config_content).unwrap();
    let mut config = AppConfig::load_or_init(&paths).unwrap();

    let workspace = jackin_config::ResolvedWorkspace {
        name: String::new(),
        label: "/workspace".to_owned(),
        workdir: "/workspace".to_owned(),
        mounts: vec![
            jackin_config::MountConfig {
                src: repo_dir.display().to_string(),
                dst: "/workspace".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
            jackin_config::MountConfig {
                src: mount_src.display().to_string(),
                dst: "/test-data".to_owned(),
                readonly: true,
                isolation: MountIsolation::Shared,
            },
        ],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
        mount_heal: jackin_config::MountHealReport::default(),
    };

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
    assert!(run_cmd.contains(&format!("{}:/test-data:ro", mount_src.display())));
}
