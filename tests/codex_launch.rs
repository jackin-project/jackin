mod common;

use common::{FakeRunner, NoOpDocker};
use jackin::agent::Agent;
use jackin::config::AppConfig;
use jackin::isolation::MountIsolation;
use jackin::paths::JackinPaths;
use jackin::runtime::{LoadOptions, load_role};
use jackin::selector::RoleSelector;
use jackin::workspace::{MountConfig, ResolvedWorkspace};
use tempfile::tempdir;

#[tokio::test]
async fn codex_launch_invokes_docker_run_with_codex_agent() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
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
        jackin::derived_image::create_derived_build_context(&repo_dir, &validated, None, None)
            .unwrap();
    let dockerfile = std::fs::read_to_string(&build.dockerfile_path).unwrap();
    assert!(dockerfile.contains("claude.ai/install.sh"));
    assert!(dockerfile.contains("openai/codex/releases"));

    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let workspace = ResolvedWorkspace {
        label: repo_dir.display().to_string(),
        workdir: "/workspace".to_string(),
        mounts: vec![MountConfig {
            src: repo_dir.display().to_string(),
            dst: "/workspace".to_string(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        default_agent: Some(Agent::Codex),
        keep_awake_enabled: false,
        git_pull_on_entry: false,
    };
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

    let build_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker build "))
        .expect("docker build should run");
    // No published_image and no --rebuild → workspace mode; --pull is omitted
    assert!(!build_cmd.contains("--pull"), "{build_cmd}");

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run") && call.contains("/run/jackin/"))
        .expect("role docker run should run");
    assert!(
        run_cmd.contains("JACKIN_AGENT=codex"),
        "JACKIN_AGENT=codex must be in docker run; got: {run_cmd}"
    );
    assert!(run_cmd.contains("-e JACKIN_ROLE=agent-smith"), "{run_cmd}");
    assert!(
        run_cmd.contains("-e OPENAI_API_KEY=test-openai-key"),
        "{run_cmd}"
    );
    assert!(!run_cmd.contains("JACKIN_CODEX_MODEL"), "{run_cmd}");
    // JACKIN_AGENT is forwarded in docker run; model flag goes to exec session.
    // The exec session (docker exec -it <container> jackin-container) carries the model flag
    // as an arg to jackin-container when passed through env or future protocol extension.
    // For now assert the exec command targets jackin-container.
    let session_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker exec") && call.contains("jackin-container"))
        .expect("jackin-container exec session should start");
    assert!(session_cmd.contains("jackin-container"), "{session_cmd}");
    assert!(!run_cmd.contains("/jackin/codex/config.toml"), "{run_cmd}");
    // Multi-agent role (`agents = ["claude", "codex"]`) provisions
    // every supported agent's home state so `hardline --new --agent
    // claude` can switch agents without re-authentication. Both
    // agents' mount blocks must appear; the selected agent is Codex.
    assert!(run_cmd.contains("/home/agent/.claude"), "{run_cmd}");
    assert!(run_cmd.contains("/home/agent/.codex"), "{run_cmd}");
    assert!(!run_cmd.contains("/home/agent/.jackin"), "{run_cmd}");
}

#[tokio::test]
async fn codex_launch_cli_agent_override_wins_over_workspace() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
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
        label: repo_dir.display().to_string(),
        workdir: "/workspace".to_string(),
        mounts: vec![MountConfig {
            src: repo_dir.display().to_string(),
            dst: "/workspace".to_string(),
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
        .find(|call| call.contains("docker run") && call.contains("/run/jackin/"))
        .expect("role docker run should run");
    assert!(
        run_cmd.contains("JACKIN_AGENT=codex"),
        "JACKIN_AGENT=codex must be in docker run; got: {run_cmd}"
    );
}
