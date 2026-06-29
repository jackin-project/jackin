#![expect(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "integration test command capture and fixture parsing should fail immediately with source location"
)]

mod common;

use common::{FakeRunner, NoOpDocker, install_agent_binary_stubs, install_capsule_binary_stub};
use jackin::agent::Agent;
use jackin::config::AppConfig;
use jackin::isolation::MountIsolation;
use jackin::paths::JackinPaths;
use jackin::runtime::{LoadOptions, load_role};
use jackin::selector::RoleSelector;
use jackin::workspace::{MountConfig, ResolvedWorkspace};
use std::path::Path;
use tempfile::tempdir;

fn recorded_docker_build(runner: &FakeRunner) -> &str {
    runner
        .recorded
        .iter()
        .find(|call| call.contains("docker build ") || call.contains("docker buildx build "))
        .map(String::as_str)
        .expect("docker build should run")
}

fn recorded_role_run(runner: &FakeRunner) -> &str {
    runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run") && call.contains("jackin.kind=role"))
        .map(String::as_str)
        .expect("role docker run should run")
}

fn recorded_capsule_exec(runner: &FakeRunner) -> &str {
    runner
        .recorded
        .iter()
        .find(|call| call.contains("docker exec") && call.contains("jackin-capsule"))
        .map(String::as_str)
        .expect("jackin-capsule exec session should start")
}

fn recorded_role_container_name(run_cmd: &str) -> &str {
    run_cmd
        .split_once(" --name ")
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .expect("role docker run should include --name")
}

fn capsule_config_for_run(paths: &JackinPaths, run_cmd: &str) -> jackin_protocol::CapsuleConfig {
    let capsule_config_path = paths
        .jackin_home
        .join("sockets")
        .join(recorded_role_container_name(run_cmd))
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

#[tokio::test]
async fn codex_launch_invokes_docker_run_with_codex_agent() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    install_capsule_binary_stub(&paths);
    install_agent_binary_stubs(&paths);
    std::fs::write(
        &paths.config_file,
        r#"[env]
OPENAI_API_KEY = "test-openai-key"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
    )
    .unwrap();

    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = jackin::repo::CachedRepo::new(&paths, &selector).repo_dir;
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
    let validated = jackin::repo::validate_role_repo(&repo_dir).unwrap();
    let build =
        jackin_image::derived_image::create_derived_build_context(&repo_dir, &validated, None, None)
            .unwrap();
    let dockerfile = std::fs::read_to_string(&build.dockerfile_path).unwrap();
    assert_cached_agent_install_blocks(&dockerfile);

    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let workspace = codex_workspace(&repo_dir);
    // Capture queue (role-specific, after 4-slot preamble):
    //   [0] capture_secret: gh auth token → empty (no gh session in test)
    let mut runner = FakeRunner::for_load_agent([String::new()]);
    let docker = NoOpDocker;

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

    let run_cmd = recorded_role_run(&runner);
    assert!(
        !run_cmd.contains("JACKIN_AGENT="),
        "JACKIN_AGENT must not be a container env var; got: {run_cmd}"
    );
    assert!(
        run_cmd.ends_with(" codex"),
        "initial agent must be passed as container argv; got: {run_cmd}"
    );
    assert!(
        !run_cmd.contains("JACKIN_AGENT_MODEL_OVERRIDES"),
        "{run_cmd}"
    );
    assert!(!run_cmd.contains("-e JACKIN_ROLE="), "{run_cmd}");
    assert!(!run_cmd.contains("-e JACKIN_WORKDIR="), "{run_cmd}");
    assert!(
        !run_cmd.contains(":/home/agent/.local/bin/codex:ro"),
        "codex binary is baked into the image and must not be bind-mounted at run time; got: {run_cmd}"
    );
    assert!(
        run_cmd.contains("-e OPENAI_API_KEY=test-openai-key"),
        "{run_cmd}"
    );
    assert!(!run_cmd.contains("JACKIN_CODEX_MODEL"), "{run_cmd}");
    // Model overrides are handed to Capsule PID 1 and applied when it spawns
    // each PTY. The foreground exec still only attaches to the Capsule client.
    let session_cmd = recorded_capsule_exec(&runner);
    assert!(session_cmd.contains("jackin-capsule"), "{session_cmd}");
    assert!(!run_cmd.contains("/jackin/codex/config.toml"), "{run_cmd}");
    let capsule_config = capsule_config_for_run(&paths, run_cmd);
    assert_eq!(capsule_config.role, "agent-smith");
    assert_eq!(capsule_config.workdir, "/workspace");
    assert_eq!(capsule_config.agents, vec!["claude", "codex"]);
    assert_eq!(capsule_config.models.get("codex").unwrap(), "gpt-5");
    assert!(!capsule_config.models.contains_key("claude"));
    // Multi-agent role (`agents = ["claude", "codex"]`) provisions
    // every supported agent's home state so `hardline --new --agent
    // claude` can switch agents without re-authentication. Both
    // agents' mount blocks must appear; the selected agent is Codex.
    assert!(run_cmd.contains("/home/agent/.claude"), "{run_cmd}");
    assert!(run_cmd.contains("/home/agent/.codex"), "{run_cmd}");
    assert!(!run_cmd.contains("/home/agent/.jackin"), "{run_cmd}");
    let codex_config = std::fs::read_to_string(
        paths
            .data_dir
            .join(recorded_role_container_name(run_cmd))
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
        r#"[env]
OPENAI_API_KEY = "test-openai-key"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
    )
    .unwrap();

    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = jackin::repo::CachedRepo::new(&paths, &selector).repo_dir;
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
    };
    // Capture queue (role-specific, after 4-slot preamble):
    //   [0] capture_secret: gh auth token → empty (no gh session in test)
    let mut runner = FakeRunner::for_load_agent([String::new()]);
    let docker = NoOpDocker;
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

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run") && call.contains("jackin.kind=role"))
        .expect("role docker run should run");
    assert!(
        !run_cmd.contains("JACKIN_AGENT="),
        "JACKIN_AGENT must not be a container env var; got: {run_cmd}"
    );
    assert!(
        run_cmd.ends_with(" codex"),
        "initial agent must be passed as container argv; got: {run_cmd}"
    );
}
