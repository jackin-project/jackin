#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
)]
mod common;

use common::{
    FakeDockerClient, FakeRunner, install_agent_binary_stubs, install_capsule_binary_stub,
    launched_role_container,
};
use jackin::workspace::{MountConfig, ResolvedWorkspace};
use jackin_config::AppConfig;
use jackin_core::Agent;
use jackin_core::ContainerSpec;
use jackin_core::JackinPaths;
use jackin_core::MountIsolation;
use jackin_core::RoleSelector;
use jackin_runtime::runtime::{LoadOptions, load_role};
use std::path::Path;
use tempfile::tempdir;

fn recorded_docker_build(runner: &FakeRunner) -> &str {
    runner
        .recorded
        .iter()
        .find(|call| call.contains("docker build ") || call.contains("buildx build "))
        .map(String::as_str)
        .expect("docker build should run")
}

fn capsule_config_for_container(
    paths: &JackinPaths,
    container_name: &str,
) -> jackin_protocol::CapsuleConfig {
    let capsule_config_path = paths
        .jackin_home
        .join("sockets")
        .join(container_name)
        .join(jackin_protocol::CAPSULE_CONFIG_FILENAME);
    toml::from_str(&std::fs::read_to_string(capsule_config_path).unwrap()).unwrap()
}

fn codex_workspace(repo_dir: &Path) -> ResolvedWorkspace {
    ResolvedWorkspace {
        name: String::new(),
        label: repo_dir.display().to_string(),
        workdir: "/workspace".to_owned(),
        mounts: vec![MountConfig {
            src: repo_dir.display().to_string(),
            dst: "/workspace".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        default_agent: Some(Agent::Codex),
        keep_awake_enabled: false,
        git_pull_on_entry: false,
        mount_heal: jackin_config::MountHealReport::default(),
    }
}

fn assert_cached_agent_install_blocks(dockerfile: &str) {
    // This direct build-context helper call does not pass agent install
    // recipes. The full launch path prepares and bakes supported agents.
    assert!(
        !dockerfile.contains("agent-binaries"),
        "direct build context must not stage agent binaries without install recipes; got: {dockerfile}"
    );
}

fn assert_codex_container_spec(spec: &ContainerSpec) {
    assert!(
        !spec
            .env
            .iter()
            .any(|entry| entry.starts_with("JACKIN_AGENT=")),
        "JACKIN_AGENT must not be a container env var; got: {:?}",
        spec.env
    );
    assert_eq!(spec.command, Some(vec!["codex-main".to_owned()]));
    assert!(
        !spec
            .env
            .iter()
            .any(|entry| entry.starts_with("JACKIN_AGENT_MODEL_OVERRIDES="))
    );
    assert!(
        !spec
            .env
            .iter()
            .any(|entry| entry.starts_with("JACKIN_ROLE="))
    );
    assert!(
        !spec
            .env
            .iter()
            .any(|entry| entry.starts_with("JACKIN_WORKDIR="))
    );
    assert!(
        !spec
            .binds
            .iter()
            .any(|bind| bind.ends_with(":/home/agent/.local/bin/codex:ro")),
        "codex binary is baked into the image and must not be bind-mounted at run time; got: {:?}",
        spec.binds
    );
    assert!(
        !spec
            .env
            .iter()
            .any(|entry| entry.contains("test-openai-key"))
    );
    assert!(
        !spec
            .env
            .iter()
            .any(|entry| entry.starts_with("JACKIN_CODEX_MODEL="))
    );
    assert!(
        !spec
            .binds
            .iter()
            .any(|bind| bind.contains("/jackin/codex/config.toml"))
    );
    assert!(
        !spec
            .binds
            .iter()
            .any(|bind| bind.contains("/home/agent/.claude"))
    );
    assert!(
        spec.binds
            .iter()
            .any(|bind| bind.contains("/home/agent/.codex"))
    );
    assert!(
        !spec
            .binds
            .iter()
            .any(|bind| bind.contains("/home/agent/.jackin"))
    );
}

#[tokio::test]
async fn codex_launch_creates_container_with_codex_agent() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    install_capsule_binary_stub(&paths);
    install_agent_binary_stubs(&paths);
    std::fs::write(
        &paths.config_file,
        r#"default_launch = ["codex-main"]

[accounts.openai-test]
name = "OpenAI test"
provider = "openai"
[accounts.openai-test.credential]
type = "api_key"
value = "test-openai-key"

[agent_configurations.codex-main]
agent = "codex"
account = "openai-test"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
    )
    .unwrap();

    let selector = RoleSelector::new(None, "agent-smith");
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
    let validated = jackin_manifest::repo::validate_role_repo(&repo_dir).unwrap();
    let build = jackin_image::derived_image::create_derived_build_context(
        &repo_dir, &validated, None, None,
    )
    .unwrap();
    let dockerfile = std::fs::read_to_string(&build.dockerfile_path).unwrap();
    assert_cached_agent_install_blocks(&dockerfile);

    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let workspace = codex_workspace(&repo_dir);
    // Capture queue (role-specific, after 4-slot preamble):
    //   [0] capture_secret: gh auth token → empty (no gh session in test)
    let mut runner = FakeRunner::for_load_agent([String::new()]);
    let docker = FakeDockerClient::default();

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

    let build_cmd = recorded_docker_build(&runner);
    // No published_image and no --rebuild → workspace mode; --pull is omitted
    assert!(!build_cmd.contains("--pull"), "{build_cmd}");

    let (container_name, spec) = launched_role_container(&docker);
    assert_codex_container_spec(&spec);
    let credentials_path = paths.data_dir.join(&container_name).join(format!(
        "credentials/{}",
        jackin_protocol::account_credentials_filename("codex-main")
    ));
    let credentials: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&credentials_path).unwrap()).unwrap();
    assert_eq!(credentials["schema_version"], 1);
    assert_eq!(
        credentials["credential"]["env"]["OPENAI_API_KEY"],
        "test-openai-key"
    );
    assert_eq!(credentials["instance"], "codex-main");
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
    // Model overrides are handed to Capsule PID 1 and applied when it spawns
    // each PTY. Classic attach is bound to the created daemon ID; when the
    // ambient shell has host-attach enabled, the client path is socket-based.
    if !jackin_runtime::runtime::host_attach::host_attach_enabled(&paths) {
        assert!(
            docker
                .bound_operations
                .borrow()
                .iter()
                .any(|operation| operation.starts_with("exec:")),
            "expected capsule exec bound to the created container ID: {:?}",
            docker.bound_operations.borrow()
        );
    }
    let capsule_config = capsule_config_for_container(&paths, &container_name);
    assert_eq!(capsule_config.role, "agent-smith");
    assert_eq!(capsule_config.workdir, "/workspace");
    assert_eq!(capsule_config.instances, vec!["codex-main"]);
    assert_eq!(capsule_config.agents.get("codex-main").unwrap(), "codex");
    assert_eq!(capsule_config.models.get("codex-main").unwrap(), "gpt-5");
    assert_eq!(capsule_config.models.len(), 1);
    // Multi-agent role (`agents = ["claude", "codex"]`) admits only the
    // configured launch instance (Codex); unadmitted manifest agents get
    // neither credentials nor capsule config entries.
    let codex_config = std::fs::read_to_string(
        paths
            .data_dir
            .join(&container_name)
            .join("home/.codex/config.toml"),
    )
    .unwrap();
    assert!(codex_config.contains("[projects.\"/workspace\"]"));
    assert!(codex_config.contains("trust_level = \"trusted\""));
}

#[tokio::test]
async fn codex_launch_cli_agent_override_wins_over_workspace() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    install_capsule_binary_stub(&paths);
    install_agent_binary_stubs(&paths);
    std::fs::write(
        &paths.config_file,
        r#"default_launch = ["codex-main"]

[accounts.openai-test]
name = "OpenAI test"
provider = "openai"
[accounts.openai-test.credential]
type = "api_key"
value = "test-openai-key"

[agent_configurations.codex-main]
agent = "codex"
account = "openai-test"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
    )
    .unwrap();

    let selector = RoleSelector::new(None, "agent-smith");
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
"#,
    )
    .unwrap();

    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let workspace = ResolvedWorkspace {
        name: String::new(),
        label: repo_dir.display().to_string(),
        workdir: "/workspace".to_owned(),
        mounts: vec![MountConfig {
            src: repo_dir.display().to_string(),
            dst: "/workspace".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        default_agent: Some(Agent::Claude),
        keep_awake_enabled: false,
        git_pull_on_entry: false,
        mount_heal: jackin_config::MountHealReport::default(),
    };
    // Capture queue (role-specific, after 4-slot preamble):
    //   [0] capture_secret: gh auth token → empty (no gh session in test)
    let mut runner = FakeRunner::for_load_agent([String::new()]);
    let docker = FakeDockerClient::default();
    let opts = LoadOptions {
        agent: Some(Agent::Codex),
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

    let (_, spec) = launched_role_container(&docker);
    assert_codex_container_spec(&spec);
}
