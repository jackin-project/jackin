use jackin::agent::Agent;
use jackin::config::AppConfig;
use jackin::docker::{CommandRunner, RunOptions};
use jackin::docker_client::{
    ContainerRow, ContainerSpec, ContainerState, DockerApi, NetworkRow, RemoveImageOutcome,
};
use jackin::isolation::MountIsolation;
use jackin::paths::JackinPaths;
use jackin::runtime::{LoadOptions, load_role};
use jackin::selector::RoleSelector;
use jackin::workspace::{MountConfig, ResolvedWorkspace};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use tempfile::tempdir;

// Minimal DockerApi stub for integration tests: all GC/inspect calls return
// empty results so load_role proceeds as if no containers exist.
struct NoOpDocker;

impl DockerApi for NoOpDocker {
    async fn inspect_container_state(&self, _name: &str) -> ContainerState {
        ContainerState::NotFound
    }
    async fn remove_container(&self, _name: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn list_containers(
        &self,
        _label_filters: &[&str],
        _all: bool,
    ) -> anyhow::Result<Vec<ContainerRow>> {
        Ok(vec![])
    }
    async fn create_container(&self, _name: &str, _spec: ContainerSpec) -> anyhow::Result<()> {
        Ok(())
    }
    async fn start_container(&self, _name: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn remove_volume(&self, _name: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn create_network(
        &self,
        _name: &str,
        _labels: HashMap<String, String>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn remove_network(&self, _name: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn list_networks(&self, _label_filters: &[&str]) -> anyhow::Result<Vec<NetworkRow>> {
        Ok(vec![])
    }
    async fn list_image_tags(&self, _reference_filter: &str) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    async fn remove_image(&self, _name: &str) -> anyhow::Result<RemoveImageOutcome> {
        Ok(RemoveImageOutcome::NotFound)
    }
    async fn inspect_image_label(
        &self,
        _image: &str,
        _label: &str,
    ) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    async fn pull_image(&self, _image: &str, _debug: bool) -> anyhow::Result<()> {
        Ok(())
    }
    async fn exec_capture(&self, _container: &str, _cmd: &[&str]) -> anyhow::Result<String> {
        Ok(String::new())
    }
}

#[derive(Default)]
struct FakeRunner {
    recorded: Vec<String>,
    capture_queue: VecDeque<String>,
}

impl FakeRunner {
    fn for_load_agent(outputs: impl IntoIterator<Item = String>) -> Self {
        // Preamble: 4 identity lookups (git config user.name, user.email, id -u, id -g).
        // GC now uses DockerApi, not CommandRunner, so it no longer counts.
        let mut capture_queue = VecDeque::new();
        for _ in 0..4 {
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
    async fn run(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&Path>,
        _opts: &RunOptions,
    ) -> anyhow::Result<()> {
        self.recorded.push(format!("{program} {}", args.join(" ")));
        Ok(())
    }

    async fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.recorded.push(format!("{program} {}", args.join(" ")));
        Ok(self.capture_queue.pop_front().unwrap_or_default())
    }

    async fn capture_secret(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        // Delegates to `capture` and consumes one queue slot. Always provision
        // one entry per expected `capture_secret` call (e.g. `gh auth token`
        // in `resolve_github_token`) and document it above the `for_load_agent`
        // call — same discipline as for `capture` calls.
        self.capture(program, args, cwd).await
    }
}

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
        jackin::derived_image::create_derived_build_context(&repo_dir, &validated, None).unwrap();
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
        .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
        .expect("role docker run should run");
    assert!(
        !run_cmd.contains("JACKIN_AGENT"),
        "JACKIN_AGENT must not be in docker run; got: {run_cmd}"
    );
    assert!(run_cmd.contains("-e JACKIN_ROLE=agent-smith"), "{run_cmd}");
    assert!(
        run_cmd.contains("-e OPENAI_API_KEY=test-openai-key"),
        "{run_cmd}"
    );
    assert!(!run_cmd.contains("JACKIN_CODEX_MODEL"), "{run_cmd}");
    // JACKIN_AGENT and model flag are forwarded to the tmux session, not the docker run CMD.
    let session_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("tmux new-session") && call.contains("entrypoint.sh"))
        .expect("tmux primary session should start");
    assert!(session_cmd.contains("JACKIN_AGENT=codex"), "{session_cmd}");
    assert!(session_cmd.contains(" -m gpt-5"), "{session_cmd}");
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
        .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
        .expect("role docker run should run");
    assert!(
        !run_cmd.contains("JACKIN_AGENT"),
        "JACKIN_AGENT must not be in docker run; got: {run_cmd}"
    );
}
