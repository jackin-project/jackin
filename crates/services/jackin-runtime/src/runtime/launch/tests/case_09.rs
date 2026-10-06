// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn load_agent_runs_attached_without_runtime_plugins_mount() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        "false 0 false".to_owned(),
        "false 0 false".to_owned(),
    ]);

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
plugins = ["code-review@claude-plugins-official"]
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
    let docker = fake_docker_for_clean_attached_exit();
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

    assert!(
        runner
            .recorded
            .iter()
            .any(|call| call.contains("docker build ")
                && call.contains("--output type=docker,name=jk_agent-smith"))
    );
    assert!(
        runner
            .run_recorded
            .iter()
            .any(|call| call.contains("docker build "))
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| { call.contains("docker inspect jk-") && call.contains("agentsmith") })
    );
    assert!(
        runner
            .recorded
            .iter()
            .any(|call| call.contains("docker run -d --name jk-") && call.contains("agentsmith"))
    );
    assert!(
        !runner
            .recorded
            .iter()
            .any(|call| call.contains("/jackin/claude/plugins.json:ro"))
    );
    assert!(
        !runner
            .recorded
            .iter()
            .any(|call| call.contains("claude plugin install"))
    );
}

#[tokio::test]
async fn load_agent_launches_codex_from_workspace_agent() {
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
            provider: jackin_config::AiProvider::OpenAi,
            credential: jackin_config::AccountCredential::ApiKey {
                value: "test-openai-key".into(),
                base_url: None,
                model: None,
            },
        },
    );
    config
        .account_bindings
        .insert(jackin_core::Agent::Codex, "coding".into());
    config.agent_configurations.insert(
        "codex-main".into(),
        jackin_config::AgentConfiguration {
            agent: jackin_core::Agent::Codex,
            account: "coding".into(),
            model: None,
            base_url: None,
            display_label: None,
            invoked_via_wrapper: None,
        },
    );
    config.default_launch = Some(vec!["codex-main".into()]);
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([String::new()]);

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
agents = ["claude", "codex"]

[claude]
plugins = ["code-review@claude-plugins-official"]

[codex]
model = "gpt-5"
"#,
    )
    .unwrap();

    let mut workspace = repo_workspace(&repo_dir);
    workspace.default_agent = Some(jackin_core::Agent::Codex);
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

    let build_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker build ") && call.contains("DerivedDockerfile"))
        .unwrap();
    // No published_image and no --rebuild → workspace mode without --pull
    assert!(!build_cmd.contains("--pull"));
    // The derived image is agent-independent and installs every supported
    // agent. This role supports Claude (a cache-bust install), so the build
    // consumes JACKIN_CACHE_BUST regardless of which agent was selected — the
    // cache-bust axis is keyed on the supported set, not the launched agent.
    assert!(
        build_cmd.contains("--build-arg JACKIN_CACHE_BUST="),
        "supported set includes a cache-bust agent (claude); got: {build_cmd}"
    );
    assert!(
        !build_cmd.contains("--label jackin.recipe.cache.bust=unused"),
        "supported set with claude must record an active cache bust; got: {build_cmd}"
    );

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    assert!(
        !run_cmd.contains("JACKIN_AGENT="),
        "JACKIN_AGENT must not be a container env var"
    );
    assert!(
        run_cmd.ends_with(" codex-main"),
        "initial instance must be passed as container argv"
    );
    assert!(!run_cmd.contains("/jackin/codex/config.toml"));
    // Multi-agent role `agents = ["claude", "codex"]` provisions and mounts
    // credentials only for the actively selected agent (Codex).
    assert!(!run_cmd.contains("/home/agent/.claude"));
    assert!(run_cmd.contains("/home/agent/.codex"));
    let container_name = launched_role_container_name(&runner);
    let credentials: serde_json::Value = serde_json::from_slice(
        &std::fs::read(paths.data_dir.join(&container_name).join(format!(
            "credentials/{}",
            jackin_protocol::account_credentials_filename("codex-main")
        )))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(credentials["schema_version"], 1);
    assert_eq!(
        credentials["credential"]["env"]["OPENAI_API_KEY"],
        "test-openai-key"
    );
    assert_eq!(credentials["instance"], "codex-main");
    assert!(!run_cmd.contains("test-openai-key"));
    let codex_config = std::fs::read_to_string(
        paths
            .data_dir
            .join(container_name)
            .join("home/.codex/config.toml"),
    )
    .unwrap();
    assert!(codex_config.contains("[projects.\"/workspace\"]"));
    assert!(codex_config.contains("trust_level = \"trusted\""));
}

#[tokio::test]
async fn load_agent_succeeds_when_sibling_agent_has_multiple_accounts() {
    use jackin_core::Agent;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"default_launch = ["claude-selected"]

[roles.multi-agent-role]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true

[accounts.codex-1]
name = "Codex 1"
provider = "openai"
[accounts.codex-1.credential]
type = "api_key"
value = "key-1"

[accounts.codex-2]
name = "Codex 2"
provider = "openai"
[accounts.codex-2.credential]
type = "api_key"
value = "key-2"

[accounts.claude-main]
name = "Claude Main"
provider = "anthropic"
[accounts.claude-main.credential]
type = "api_key"
value = "claude-key"

[agent_configurations.claude-selected]
agent = "claude"
account = "claude-main"

[workspaces.my-workspace]
workdir = "/workspace"
accounts = ["codex-1", "codex-2", "claude-main"]
[workspaces.my-workspace.account_bindings]
claude = "claude-main"

[[workspaces.my-workspace.mounts]]
src = "/tmp"
dst = "/workspace"
"#,
    )
    .unwrap();
    let mut config = AppConfig::load_or_init(&paths).unwrap();

    let selector = RoleSelector::new(None, "multi-agent-role");
    let mut runner = FakeRunner::for_load_agent([String::new()]);

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
agents = ["claude", "codex"]

[claude]
plugins = []

[codex]
model = "gpt-5"
"#,
    )
    .unwrap();

    let mut workspace = repo_workspace(&repo_dir);
    workspace.name = "my-workspace".into();
    workspace.default_agent = Some(Agent::Claude);
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

    // Only the selected agent (Claude) is provisioned and mounted into the container.
    assert!(run_cmd.contains("/home/agent/.claude"));
    assert!(!run_cmd.contains("/home/agent/.codex"));

    let container_name = launched_role_container_name(&runner);
    let credentials: serde_json::Value = serde_json::from_slice(
        &std::fs::read(paths.data_dir.join(&container_name).join(format!(
            "credentials/{}",
            jackin_protocol::account_credentials_filename("claude-selected")
        )))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(credentials["schema_version"], 1);
    assert_eq!(
        credentials["credential"]["env"]["ANTHROPIC_API_KEY"],
        "claude-key"
    );
    assert_eq!(credentials["instance"], "claude-selected");

    let capsule_config_path = paths
        .jackin_home
        .join("sockets")
        .join(&container_name)
        .join(jackin_protocol::CAPSULE_CONFIG_FILENAME);
    let capsule_config: jackin_protocol::CapsuleConfig =
        toml::from_str(&std::fs::read_to_string(capsule_config_path).unwrap()).unwrap();
    assert_eq!(capsule_config.instances, vec!["claude-selected"]);
    assert_eq!(
        capsule_config.auth_modes.get("claude-selected").unwrap(),
        "api_key"
    );
}
