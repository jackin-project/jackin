#![expect(

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

    clippy::expect_used,
    reason = "integration test command capture should fail immediately when expected Docker commands are absent"
)]

mod common;

use common::{FakeRunner, NoOpDocker, install_agent_binary_stubs, install_capsule_binary_stub};

use jackin::workspace::{MountConfig, ResolvedWorkspace};
use jackin_config::AppConfig;
use jackin_core::Agent;
use jackin_core::MountIsolation;
use jackin_core::RoleSelector;
use jackin_core::paths::JackinPaths;
use jackin_runtime::runtime::{LoadOptions, load_role};
use tempfile::tempdir;

fn recorded_role_container_name(run_cmd: &str) -> &str {
    run_cmd
        .split_once(" --name ")
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .expect("role docker run should include --name")
}

fn assert_amp_not_staged_without_install_recipe(dockerfile: &str) {
    // This direct build-context helper call does not pass an agent install
    // recipe. The full launch path prepares and bakes supported agents.
    assert!(
        !dockerfile.contains("agent-binaries"),
        "direct build context must not stage an amp binary without an install recipe; got: {dockerfile}"
    );
}

#[tokio::test]
async fn amp_launch_invokes_docker_run_with_amp_agent() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    install_capsule_binary_stub(&paths);
    install_agent_binary_stubs(&paths);
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
agents = ["amp"]

[amp]
"#,
    )
    .unwrap();

    let validated = jackin_manifest::repo::validate_role_repo(&repo_dir).unwrap();
    let build = jackin_image::derived_image::create_derived_build_context(
        &repo_dir, &validated, None, None,
    )
    .unwrap();
    let dockerfile = std::fs::read_to_string(&build.dockerfile_path).unwrap();
    assert_amp_not_staged_without_install_recipe(&dockerfile);

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
        .find(|call| call.contains("docker run") && call.contains("jackin.kind=role"))
        .expect("role docker run should run");
    assert!(
        !run_cmd.contains("JACKIN_AGENT="),
        "JACKIN_AGENT must not be a container env var; got: {run_cmd}"
    );
    assert!(
        run_cmd.ends_with(" amp"),
        "initial agent must be passed as container argv; got: {run_cmd}"
    );
    assert!(
        !run_cmd.contains(":/home/agent/.amp/bin/amp:ro"),
        "amp binary is baked into the image and must not be bind-mounted at run time; got: {run_cmd}"
    );
    assert!(!run_cmd.contains("-e JACKIN_ROLE="), "{run_cmd}");
    assert!(run_cmd.contains("-e AMP_API_KEY=test-amp-key"), "{run_cmd}");
    assert!(!run_cmd.contains("/jackin/claude/"), "{run_cmd}");
    assert!(!run_cmd.contains("/jackin/codex/"), "{run_cmd}");
    assert!(!run_cmd.contains("/jackin/amp/secrets.json"), "{run_cmd}");
    let capsule_config_path = paths
        .jackin_home
        .join("sockets")
        .join(recorded_role_container_name(run_cmd))
        .join(jackin_protocol::CAPSULE_CONFIG_FILENAME);
    let capsule_config: jackin_protocol::CapsuleConfig =
        toml::from_str(&std::fs::read_to_string(capsule_config_path).unwrap()).unwrap();
    assert_eq!(capsule_config.role, "the-architect");
    assert_eq!(capsule_config.workdir, "/workspace");
    assert_eq!(capsule_config.agents, vec!["amp"]);
    assert!(capsule_config.models.is_empty());
}

#[tokio::test]
async fn amp_launch_under_sync_mounts_secrets_json_in_docker_run() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    install_capsule_binary_stub(&paths);
    install_agent_binary_stubs(&paths);

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
agents = ["amp"]

[amp]
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
        .find(|call| call.contains("docker run") && call.contains("jackin.kind=role"))
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
