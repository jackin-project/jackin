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
async fn amp_launch_invokes_docker_run_with_amp_agent() {
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
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
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

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
        .expect("role docker run should run");
    assert!(
        !run_cmd.contains("JACKIN_AGENT"),
        "JACKIN_AGENT must not be in docker run; got: {run_cmd}"
    );
    assert!(
        run_cmd.contains("-e JACKIN_ROLE=the-architect"),
        "{run_cmd}"
    );
    assert!(run_cmd.contains("-e AMP_API_KEY=test-amp-key"), "{run_cmd}");
    assert!(!run_cmd.contains("/jackin/claude/"), "{run_cmd}");
    assert!(!run_cmd.contains("/jackin/codex/"), "{run_cmd}");
    assert!(!run_cmd.contains("/jackin/amp/secrets.json"), "{run_cmd}");
}

#[tokio::test]
async fn amp_launch_under_sync_mounts_secrets_json_in_docker_run() {
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
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
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

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("supervisor.sh"))
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
