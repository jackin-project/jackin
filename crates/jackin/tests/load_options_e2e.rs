//! End-to-end smoke for the programmatic (`LoadOptions`) launch surface.
//!
//! Drives `jackin_runtime::runtime::load_role` directly — no CLI, no PTY, no
//! `script(1)` — with every launch decision pre-supplied: role selector, agent,
//! registered account ID, model, effort, env, pre-approved on-demand bindings,
//! mounts, and force. The launch must run to a started container without
//! opening a single dialog and hand back the instance identity, which the test
//! verifies through `jackin status --format json`.
//!
//! Uses an isolated config, a local role, and a synthetic account. A non-TTY
//! launch leaves the instance running; the test queries it through the real
//! CLI with the same isolated paths, then removes the container.

// Expects only apply when the e2e feature compiles the body; without it the
// crate is empty and unfulfilled-expect would fail `cargo clippy -p jackin`.
#![cfg_attr(
    feature = "e2e",
    expect(
        clippy::disallowed_methods,
        reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
    )
)]
#![cfg(feature = "e2e")]

use std::process::Command;

use jackin_config::{AppConfig, MountConfig, ResolvedWorkspace};
use jackin_core::{Agent, JackinPaths, MountIsolation, ReasoningEffort, RoleSelector};
use jackin_docker::ShellRunner;
use jackin_docker::docker_client::BollardDockerClient;
use jackin_runtime::runtime::{self, LoadOptions};

/// Instance marker retained in captured test output for diagnostics.
const INSTANCE_ID_MARKER: &str = "LOAD_OPTIONS_INSTANCE_ID";
const ROLE: &str = "load-options-e2e";

fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .output()
        .is_ok_and(|output| output.status.success())
}

#[tokio::test(flavor = "multi_thread")]
async fn load_options_launch() {
    // The programmatic API reads the runtime override from process env. Spawn
    // this test with an isolated override instead of mutating the async test
    // process's environment. This matches the DinD CLI harness's image choice.
    const CHILD_MARKER: &str = "JACKIN_E2E_LOAD_OPTIONS_CHILD";
    if std::env::var_os(CHILD_MARKER).is_none() {
        let image = std::env::var("JACKIN_E2E_CONSTRUCT_IMAGE")
            .unwrap_or_else(|_| "projectjackin/construct:trixie".to_owned());
        let output = Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", "load_options_launch", "--nocapture"])
            .env(CHILD_MARKER, "1")
            .env("JACKIN_CONSTRUCT_IMAGE", image)
            .output()
            .expect("run programmatic launch with isolated image override");
        print!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        assert!(
            output.status.success(),
            "isolated programmatic launch failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    assert!(
        docker_available(),
        "e2e tests require a running Docker daemon (`docker info` failed)"
    );

    let temp = tempfile::tempdir().expect("isolated launch fixture");
    let paths = JackinPaths::resolve_with_env(&temp.path().join("home"), None, None);
    paths.ensure_base_dirs().expect("jackin base dirs");
    let role_source = temp.path().join("role");
    seed_launch_fixture(&paths, &role_source).expect("isolated role fixture");
    let mut config = AppConfig::load_or_init(&paths).expect("synthetic config must load");
    let selector = RoleSelector::parse(ROLE).expect("role selector must parse");

    // A workspace that is not one of the operator's projects: an empty scratch
    // directory bind-mounted at /workspace.
    let workdir = tempfile::tempdir().expect("workspace tempdir");
    let host_src = workdir
        .path()
        .canonicalize()
        .expect("workspace path must canonicalize");
    let workspace = ResolvedWorkspace {
        name: host_src.display().to_string(),
        label: host_src.display().to_string(),
        workdir: "/workspace".to_owned(),
        mounts: vec![MountConfig {
            src: host_src.display().to_string(),
            dst: "/workspace".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        keep_awake_enabled: false,
        default_agent: None,
        git_pull_on_entry: false,
    };

    // Every decision pre-supplied; nothing here may open a dialog.
    let mut opts = LoadOptions::programmatic(Agent::Claude);
    opts.force = true;
    opts.account = Some("e2e-claude".into());
    opts.model = Some("claude-opus-5".to_owned());
    opts.effort = Some(ReasoningEffort::Medium);
    opts.env
        .insert("JACKIN_E2E_LOAD_OPTIONS".to_owned(), "1".to_owned());
    opts.extra_mounts = Vec::new();
    opts.on_demand_bindings = Vec::new();

    let docker = BollardDockerClient::connect().expect("docker client must connect");
    let mut runner = ShellRunner { debug: false };

    runtime::load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .expect("programmatic launch must succeed without a TTY");

    let launched = opts
        .launched_instance()
        .expect("a programmatic launch must report its instance identity");
    assert!(
        !launched.instance_id.is_empty(),
        "the launch must report a non-empty instance id"
    );
    println!("{INSTANCE_ID_MARKER}={}", launched.instance_id);
    println!("LOAD_OPTIONS_CONTAINER_BASE={}", launched.container_base);
    verify_launched_status(&paths, &launched.container_base, &launched.instance_id)
        .expect("launched instance must remain running and appear in status");
}

fn verify_launched_status(paths: &JackinPaths, container: &str, id: &str) -> anyhow::Result<()> {
    let status = Command::new(env!("CARGO_BIN_EXE_jackin"))
        .args(["status", "--format", "json"])
        .env("JACKIN_HOME_DIR", &paths.jackin_home)
        .env("JACKIN_CONFIG_DIR", &paths.config_dir)
        .output();
    let inspect = Command::new("docker")
        .args(["inspect", "--format", "{{json .State}}", container])
        .output();
    let logs = Command::new("docker")
        .args(["logs", "--tail", "40", container])
        .output();
    let cleanup = Command::new("docker")
        .args(["rm", "-f", container])
        .output()?;
    let status = status?;
    let inspect = inspect?;
    let logs = logs?;
    anyhow::ensure!(
        status.status.success(),
        "status failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );
    let status_json: serde_json::Value = serde_json::from_slice(&status.stdout)?;
    let running = status_json["workspaces"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|workspace| {
            workspace["instances"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|instance| instance["instance_id"] == id && instance["state"] == "running")
        });
    anyhow::ensure!(
        running,
        "launched instance {id} ({container}) must be running\nstatus: {status_json}\ncontainer state: {}{}\ncontainer logs: {}{}",
        String::from_utf8_lossy(&inspect.stdout),
        String::from_utf8_lossy(&inspect.stderr),
        String::from_utf8_lossy(&logs.stdout),
        String::from_utf8_lossy(&logs.stderr),
    );
    anyhow::ensure!(
        cleanup.status.success(),
        "smoke container cleanup failed: {}",
        String::from_utf8_lossy(&cleanup.stderr)
    );
    Ok(())
}

fn seed_launch_fixture(paths: &JackinPaths, role: &std::path::Path) -> anyhow::Result<()> {
    assert!(
        std::env::var_os("JACKIN_CAPSULE_BIN").is_some(),
        "e2e requires a locally built Linux JACKIN_CAPSULE_BIN"
    );
    std::fs::create_dir_all(role)?;
    // Role sources stay version-pinned. The runtime's JACKIN_CONSTRUCT_IMAGE
    // override selects the locally built e2e image after source validation.
    std::fs::write(
        role.join("Dockerfile"),
        jackin_manifest::BASE_DOCKERFILE_FROM,
    )?;
    std::fs::write(
        role.join(jackin_core::MANIFEST_FILENAME),
        "version = \"v1alpha7\"\ndockerfile = \"Dockerfile\"\nagents = [\"claude\"]\n\n[claude]\nplugins = []\n",
    )?;
    jackin_manifest::repo::validate_role_repo(role)?;
    for args in [
        vec!["init"],
        vec!["add", "."],
        vec![
            "-c",
            "user.name=Jackin E2E",
            "-c",
            "user.email=e2e@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-s",
            "-m",
            "Seed load options role\n\nCo-authored-by: Codex <codex@openai.com>",
        ],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(role)
                .output()?
                .status
                .success()
        );
    }
    std::fs::write(
        &paths.config_file,
        format!(
            r#"version = "v1alpha10"
[docker]
profile = "standard"
[accounts.e2e-claude]
name = "E2E Claude"
provider = "anthropic"
[accounts.e2e-claude.credential]
type = "api_key"
value = "synthetic-e2e-claude-key"
[roles.{ROLE}]
git = "{}"
trusted = true
"#,
            role.display()
        ),
    )?;
    jackin_image::agent_binary::install_test_stub(paths, Agent::Claude)?;
    let stub = paths.cache_dir.join("agent-binaries-test-stub/claude");
    std::fs::write(
        stub,
        r#"#!/bin/sh
set -eu
if [ "${1:-}" = "install" ]; then
  mkdir -p "$HOME/.local/bin"
  cp "$0" "$HOME/.local/bin/claude"
  chmod 0755 "$HOME/.local/bin/claude"
  exit 0
fi
if [ "${1:-}" = "--version" ]; then
  echo 'claude 0.0.0-e2e'
  exit 0
fi
sleep 300
"#,
    )?;
    Ok(())
}
