use jackin::agent::Agent;
use jackin::config::AppConfig;
use jackin::docker::{CommandRunner, RunOptions};
use jackin::isolation::MountIsolation;
use jackin::paths::JackinPaths;
use jackin::runtime::{LoadOptions, load_role};
use jackin::selector::RoleSelector;
use jackin::workspace::{MountConfig, ResolvedWorkspace};
use std::collections::VecDeque;
use std::path::Path;
use tempfile::tempdir;

#[derive(Default)]
struct FakeRunner {
    recorded: Vec<String>,
    capture_queue: VecDeque<String>,
}

impl FakeRunner {
    fn for_load_agent(outputs: impl IntoIterator<Item = String>) -> Self {
        let mut capture_queue = VecDeque::new();
        for _ in 0..6 {
            capture_queue.push_back(String::new());
        }
        capture_queue.extend(outputs);
        Self {
            recorded: Vec::new(),
            capture_queue,
        }
    }
}

impl CommandRunner for FakeRunner {
    fn run(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&Path>,
        _opts: &RunOptions,
    ) -> anyhow::Result<()> {
        self.recorded.push(format!("{program} {}", args.join(" ")));
        Ok(())
    }

    fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.recorded.push(format!("{program} {}", args.join(" ")));
        Ok(self.capture_queue.pop_front().unwrap_or_default())
    }
}

#[test]
fn codex_launch_invokes_docker_run_with_codex_agent() {
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

    let repo_dir = paths.roles_dir.join("agent-smith");
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"dockerfile = "Dockerfile"
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
        jackin::derived_image::create_derived_build_context(&repo_dir, &validated, None).unwrap();
    let dockerfile = std::fs::read_to_string(&build.dockerfile_path).unwrap();
    assert!(dockerfile.contains("claude.ai/install.sh"));
    assert!(dockerfile.contains("openai/codex/releases"));

    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
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
    let mut runner = FakeRunner::for_load_agent([String::new()]);

    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &mut runner,
        &LoadOptions::default(),
    )
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
        .find(|call| call.contains("docker run -d -it"))
        .expect("role docker run should run");
    assert!(run_cmd.contains("-e JACKIN_AGENT=codex"), "{run_cmd}");
    assert!(run_cmd.contains("-e JACKIN_ROLE=agent-smith"), "{run_cmd}");
    assert!(
        run_cmd.contains("-e OPENAI_API_KEY=test-openai-key"),
        "{run_cmd}"
    );
    assert!(run_cmd.contains("/jackin/codex/config.toml"), "{run_cmd}");
    // Codex container must not receive any Claude-side mounts, and the
    // legacy ~/.claude / ~/.jackin paths must not surface.
    assert!(!run_cmd.contains("/jackin/claude/"), "{run_cmd}");
    assert!(!run_cmd.contains("/home/agent/.claude"), "{run_cmd}");
    assert!(!run_cmd.contains("/home/agent/.jackin"), "{run_cmd}");
    assert!(
        paths
            .data_dir
            .join("jackin-agent-smith")
            .join("codex")
            .join("config.toml")
            .is_file()
    );
}

#[test]
fn codex_launch_cli_agent_override_wins_over_workspace() {
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

    let repo_dir = paths.roles_dir.join("agent-smith");
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"dockerfile = "Dockerfile"
agents = ["claude", "codex"]

[claude]
plugins = []

[codex]
"#,
    )
    .unwrap();

    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
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
    let mut runner = FakeRunner::for_load_agent([String::new()]);
    let opts = LoadOptions {
        agent: Some(Agent::Codex),
        ..LoadOptions::default()
    };

    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &mut runner,
        &opts,
    )
    .unwrap();

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d -it"))
        .expect("role docker run should run");
    assert!(run_cmd.contains("-e JACKIN_AGENT=codex"), "{run_cmd}");
}
