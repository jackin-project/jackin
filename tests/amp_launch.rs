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
fn amp_launch_invokes_docker_run_with_amp_agent() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[env]
AMP_API_KEY = "test-amp-key"

[amp]
auth_forward = "api_key"

[roles.the-architect]
git = "https://github.com/jackin-project/jackin-the-architect.git"
trusted = true
"#,
    )
    .unwrap();

    let selector = RoleSelector::new(None, "the-architect");
    let repo_dir = jackin::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"dockerfile = "Dockerfile"
agents = ["amp"]

[amp]
"#,
    )
    .unwrap();

    let validated = jackin::repo::validate_role_repo(&repo_dir).unwrap();
    let build =
        jackin::derived_image::create_derived_build_context(&repo_dir, &validated, None).unwrap();
    let dockerfile = std::fs::read_to_string(&build.dockerfile_path).unwrap();
    assert!(dockerfile.contains("ampcode.com/install.sh"));
    assert!(dockerfile.contains("RUN amp --version"));

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
        default_agent: Some(Agent::Amp),
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

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d -it"))
        .expect("role docker run should run");
    assert!(run_cmd.contains("-e JACKIN_AGENT=amp"), "{run_cmd}");
    assert!(
        run_cmd.contains("-e JACKIN_ROLE=the-architect"),
        "{run_cmd}"
    );
    assert!(run_cmd.contains("-e AMP_API_KEY=test-amp-key"), "{run_cmd}");
    assert!(!run_cmd.contains("/jackin/claude/"), "{run_cmd}");
    assert!(!run_cmd.contains("/jackin/codex/"), "{run_cmd}");
    assert!(!run_cmd.contains("/jackin/amp/secrets.json"), "{run_cmd}");
}

#[test]
fn amp_launch_under_sync_mounts_secrets_json_in_docker_run() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    // Stage host ~/.local/share/amp/secrets.json under the test's fake
    // home (paths.home_dir, which load_role consults for host-side
    // auth state) so the Sync arm of provision_amp_auth produces a
    // non-None mounted path.
    let amp_dir = paths.home_dir.join(".local/share/amp");
    std::fs::create_dir_all(&amp_dir).unwrap();
    std::fs::write(
        amp_dir.join("secrets.json"),
        "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}",
    )
    .unwrap();

    std::fs::write(
        &paths.config_file,
        r#"[amp]
auth_forward = "sync"

[roles.the-architect]
git = "https://github.com/jackin-project/jackin-the-architect.git"
trusted = true
"#,
    )
    .unwrap();

    let selector = RoleSelector::new(None, "the-architect");
    let repo_dir = jackin::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"dockerfile = "Dockerfile"
agents = ["amp"]

[amp]
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
        default_agent: Some(Agent::Amp),
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

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d -it"))
        .expect("role docker run should run");
    assert!(
        run_cmd.contains(":/jackin/amp/secrets.json"),
        "Sync mode must mount secrets.json into the container: {run_cmd}"
    );
    // No AMP_API_KEY in env config → no -e flag.
    assert!(
        !run_cmd.contains("-e AMP_API_KEY="),
        "Sync mode without AMP_API_KEY must not inject the var: {run_cmd}"
    );
}
