// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `runtime/launch.rs`: load pipeline behavioral verification.
#![expect(
    unused_qualifications,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
use super::*;
use crate::runtime::launch::launch_runtime::{
    CapsuleAuth, CapsuleEndpoint, CapsuleNetwork, capsule_export_coverage,
    capsule_otlp_allowlist_host, capsule_otlp_propagation, debug_runtime_envs,
    telemetry_runtime_envs_for,
};

#[test]
fn capsule_otlp_fails_closed_for_network_endpoint_and_auth() {
    use jackin_diagnostics::CapsuleExportCoverage;

    assert_eq!(
        capsule_export_coverage(
            CapsuleNetwork::Disabled,
            CapsuleEndpoint::Safe,
            CapsuleAuth::Complete,
        ),
        CapsuleExportCoverage::DisabledNetworkNone
    );
    assert_eq!(
        capsule_export_coverage(
            CapsuleNetwork::Enabled,
            CapsuleEndpoint::Missing,
            CapsuleAuth::Complete,
        ),
        CapsuleExportCoverage::DisabledNoEndpoint
    );
    assert_eq!(
        capsule_export_coverage(
            CapsuleNetwork::Enabled,
            CapsuleEndpoint::Unclassified,
            CapsuleAuth::Complete,
        ),
        CapsuleExportCoverage::DisabledUnclassifiedEndpoint
    );
    assert_eq!(
        capsule_export_coverage(
            CapsuleNetwork::Enabled,
            CapsuleEndpoint::Safe,
            CapsuleAuth::HostOnly,
        ),
        CapsuleExportCoverage::DisabledUnclassifiedAuth
    );
    assert_eq!(
        capsule_export_coverage(
            CapsuleNetwork::Enabled,
            CapsuleEndpoint::Safe,
            CapsuleAuth::Complete,
        ),
        CapsuleExportCoverage::Enabled
    );
    assert_eq!(
        capsule_export_coverage(
            CapsuleNetwork::Enabled,
            CapsuleEndpoint::Safe,
            CapsuleAuth::Complete,
        ),
        CapsuleExportCoverage::Enabled
    );
}

#[test]
fn disabled_capsule_export_injects_no_telemetry_or_firewall_host() {
    let env = capsule_otlp_propagation(
        None,
        Some("authorization=private-host-header"),
        Some("00-private-traceparent"),
    );
    assert!(env.is_empty());
    assert_eq!(capsule_otlp_allowlist_host(None), None);
}

#[test]
fn enabled_capsule_export_uses_only_explicit_safe_carriers() {
    let endpoint = jackin_diagnostics::ContainerOtlp {
        endpoint: "http://host.docker.internal:4317".to_owned(),
        needs_host_gateway: true,
    };
    let env = capsule_otlp_propagation(
        Some(&endpoint),
        Some("authorization=capsule-safe"),
        Some("00-bounded-traceparent"),
    );
    assert_eq!(env.len(), 3);
    assert!(
        env.iter()
            .any(|value| value == "OTEL_EXPORTER_OTLP_HEADERS=authorization=capsule-safe")
    );
    assert_eq!(
        capsule_otlp_allowlist_host(Some(&endpoint)),
        Some("host.docker.internal")
    );
    assert!(!env.iter().any(|value| value.contains("CLIENT_KEY")));
    assert!(
        !env.iter()
            .any(|value| value.contains("private-host-header"))
    );
}
use jackin_config::AppConfig;
use jackin_core::WorkspaceName;
use jackin_test_support::FakeRunner;
use std::collections::HashMap;

#[test]
fn sensitive_mount_prompt_lists_every_hit_src_and_reason() {
    let sensitive = vec![
        jackin_config::SensitiveMount {
            src: "/home/op/.ssh".to_owned(),
            reason: "SSH private keys".to_owned(),
        },
        jackin_config::SensitiveMount {
            src: "/home/op/.aws".to_owned(),
            reason: "AWS credentials".to_owned(),
        },
    ];
    let prompt = sensitive_mount_prompt(&sensitive);
    // Every flagged path and its reason must reach the operator — a
    // dropped hit would silently hide a credential exposure.
    for hit in &sensitive {
        assert!(prompt.contains(&hit.src), "missing src in: {prompt}");
        assert!(prompt.contains(&hit.reason), "missing reason in: {prompt}");
    }
    assert!(prompt.contains("Continue with these mounts?"));
}
use crate::isolation::MountIsolation;
use crate::isolation::materialize::{MaterializedMount, MaterializedWorkspace, WorktreeAuxMounts};
use jackin_core::ANTHROPIC_API_KEY_ENV_NAME;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::tempdir;

fn workspace_manifest(
    container_name: &str,
    role_key: &str,
    role_display_name: &str,
    agent: jackin_core::Agent,
) -> InstanceManifest {
    let role_source_git = format!("https://example.invalid/{role_key}.git");
    let image_tag = format!("{}{role_key}", crate::runtime::naming::IMAGE_PREFIX);
    InstanceManifest::new(NewInstanceManifest {
        container_base: container_name,
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key,
        role_display_name,
        agent_runtime: agent,
        role_source_git: &role_source_git,
        role_source_ref: None,
        image_tag: &image_tag,
        docker: DockerResources::from_container_name(container_name),
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    })
}

fn write_indexed_manifest(paths: &JackinPaths, manifest: &InstanceManifest) {
    manifest
        .write(&paths.data_dir.join(&manifest.container_base))
        .unwrap();
    InstanceIndex::update_manifest(&paths.data_dir, manifest).unwrap();
}

fn local_role_base_for_test(selector: &RoleSelector, head_sha: Option<&str>) -> String {
    crate::runtime::naming::role_base_image_name(selector, None, head_sha)
}

#[test]
fn docker_build_failure_cli_error_contains_no_local_artifact_paths() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let run = jackin_diagnostics::RunDiagnostics::start(
        &paths,
        false,
        "load",
        jackin_diagnostics::ServiceIdentity::HOST_INTERACTIVE,
    )
    .unwrap();
    let error = anyhow::anyhow!("Docker build command failed");
    let rendered = launch_failure_cli_error(
        crate::runtime::progress::LaunchStage::DerivedImage,
        &error,
        Some(run.as_ref()),
    )
    .to_string();

    assert_eq!(
        rendered,
        "Docker build command failed: Docker build command failed"
    );
}

#[test]
fn derived_image_cli_error_preserves_original_without_docker_output() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let run = jackin_diagnostics::RunDiagnostics::start(
        &paths,
        false,
        "load",
        jackin_diagnostics::ServiceIdentity::HOST_INTERACTIVE,
    )
    .unwrap();

    let error = anyhow::anyhow!("preparing capsule binary failed");
    let rendered = launch_failure_cli_error(
        crate::runtime::progress::LaunchStage::DerivedImage,
        &error,
        Some(run.as_ref()),
    )
    .to_string();

    assert_eq!(rendered, "preparing capsule binary failed");
    assert!(!rendered.contains("Docker build command failed"));
    assert!(!rendered.contains("docker output"));
}

async fn resolve_workspace_restore(
    paths: &JackinPaths,
    role_key: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<RestoreResolution> {
    resolve_restore_candidate(
        paths,
        Some("workspace"),
        "workspace",
        "/workspace",
        role_key,
        jackin_core::Agent::Claude,
        docker,
        None,
    )
    .await
}
#[test]
fn capsule_config_serializes_manifest_models() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
agents = ["claude", "codex", "amp", "kimi", "opencode"]

[claude]
model = "sonnet"

[codex]
model = "gpt-5"

[amp]

[kimi]
model = "kimi-k2"

[opencode]
model = "zai/glm"
"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();

    let manifest = jackin_manifest::load_role_manifest(temp.path()).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");
    let config = capsule_config(&selector, "/workspace", &manifest, None, "ask", Vec::new());
    let auth_modes = super::capsule_setup::capsule_auth_modes(
        &jackin_config::AppConfig::default(),
        None,
        &selector.key(),
        &manifest,
    );

    assert_eq!(config.role, "chainargos/the-architect");
    assert_eq!(config.workdir, "/workspace");
    assert_eq!(
        config.agents,
        vec!["claude", "codex", "amp", "kimi", "opencode"]
    );
    assert_eq!(config.models.get("claude").unwrap(), "sonnet");
    assert_eq!(config.models.get("codex").unwrap(), "gpt-5");
    assert_eq!(config.models.get("kimi").unwrap(), "kimi-k2");
    assert_eq!(config.models.get("opencode").unwrap(), "zai/glm");
    assert!(!config.models.contains_key("amp"));
    assert_eq!(auth_modes.len(), config.agents.len());
    assert!(auth_modes.values().all(|mode| mode == "sync"));
}
#[tokio::test]
async fn diagnose_premature_exit_returns_none_when_container_running() {
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Running])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();
    let result = diagnose_premature_exit(
        &docker,
        &mut runner,
        "jk-the-architect",
        ExitPhase::PreAttach,
    )
    .await;
    assert!(
        result.is_none(),
        "running container must not be diagnosed as a failure"
    );
}

#[tokio::test]
async fn diagnose_premature_exit_includes_logs_when_container_already_stopped() {
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 127,
            oom_killed: false,
        }])),

        ..Default::default()
    };
    let mut runner = FakeRunner::with_capture_queue([
        "/jackin/runtime/entrypoint.sh: line 85: exec: codex: not found".to_owned(),
    ]);
    let err = diagnose_premature_exit(
        &docker,
        &mut runner,
        "jk-the-architect",
        ExitPhase::PreAttach,
    )
    .await
    .expect("stopped container must produce a diagnostic error");
    let msg = err.to_string();
    assert!(
        msg.contains("exit 127"),
        "exit code missing from msg: {msg}"
    );
    assert!(
        msg.contains("codex: not found"),
        "logs missing from msg: {msg}"
    );
    assert!(
        runner
            .recorded
            .iter()
            .any(|c| c.contains("docker logs --tail 40 jk-the-architect")),
        "must shell out to `docker logs` to capture the entrypoint output"
    );
}

#[tokio::test]
async fn diagnose_premature_exit_flags_oom_kill_distinct_from_normal_exit() {
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 137,
            oom_killed: true,
        }])),

        ..Default::default()
    };
    let mut runner = FakeRunner::with_capture_queue([String::new()]);
    let err = diagnose_premature_exit(&docker, &mut runner, "jackin-x", ExitPhase::PreAttach)
        .await
        .expect("OOM-killed container is a premature exit");
    let msg = err.to_string();
    assert!(msg.contains("OOM killed"), "expected OOM marker in: {msg}");
    assert!(
        msg.contains("no log output"),
        "empty logs branch missing: {msg}"
    );
}

#[tokio::test]
async fn diagnose_premature_exit_passes_through_when_inspect_returns_notfound() {
    use jackin_test_support::FakeDockerClient;
    let docker = FakeDockerClient::default(); // empty queue → NotFound
    let mut runner = FakeRunner::default();
    assert!(
        diagnose_premature_exit(&docker, &mut runner, "jackin-x", ExitPhase::PreAttach)
            .await
            .is_none(),
        "NotFound must not abort launch before exec attempt"
    );
}

#[tokio::test]
async fn diagnose_premature_exit_swallows_post_attach_clean_exit() {
    // Operator typed `/exit` in the agent → multiplexer drained
    // the last live session → container shut itself down with
    // exit 0. The container-lifecycle policy treats this as the
    // happy path; the host CLI must not surface it as an error.
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 0,
            oom_killed: false,
        }])),
        ..Default::default()
    };
    let mut runner = FakeRunner::default();
    let result = diagnose_premature_exit(
        &docker,
        &mut runner,
        "jk-the-architect",
        ExitPhase::PostAttach,
    )
    .await;
    assert!(
        result.is_none(),
        "post-attach exit 0 is the lifecycle-policy clean-shutdown path, not an error"
    );
    assert!(
        runner.recorded.is_empty(),
        "no `docker logs` fetch when the post-attach exit is clean"
    );
}

#[tokio::test]
async fn diagnose_premature_exit_surfaces_post_attach_nonzero_exit() {
    // Post-attach exit with a non-zero code still indicates a
    // problem inside the multiplexer / agent — operator wants the
    // logs surfaced even though the container is gone now.
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 137,
            oom_killed: false,
        }])),
        ..Default::default()
    };
    let mut runner = FakeRunner::with_capture_queue(["panic: VT screen overflow".to_owned()]);
    let err = diagnose_premature_exit(
        &docker,
        &mut runner,
        "jk-the-architect",
        ExitPhase::PostAttach,
    )
    .await
    .expect("post-attach non-zero exit must produce a diagnostic error");
    let msg = err.to_string();
    assert!(
        msg.contains("exited during session"),
        "phase label missing in: {msg}"
    );
    assert!(msg.contains("exit 137"), "exit code missing in: {msg}");
    assert!(
        msg.contains("panic: VT screen overflow"),
        "logs missing in: {msg}"
    );
}

#[tokio::test]
async fn diagnose_premature_exit_surfaces_pre_attach_exit_zero() {
    // Pre-attach exit 0 is still suspicious — PID 1 exited
    // without doing anything, most likely a bad image or missing
    // entrypoint. Operator wants the heads-up even though the
    // exit code looks clean.
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 0,
            oom_killed: false,
        }])),
        ..Default::default()
    };
    let mut runner = FakeRunner::with_capture_queue([String::new()]);
    let err = diagnose_premature_exit(
        &docker,
        &mut runner,
        "jk-the-architect",
        ExitPhase::PreAttach,
    )
    .await
    .expect("pre-attach exit 0 must still flag a missing Capsule");
    let msg = err.to_string();
    assert!(
        msg.contains("exited before attach"),
        "phase label missing in: {msg}"
    );
    assert!(msg.contains("exit 0"), "exit code missing in: {msg}");
}

#[tokio::test]
async fn diagnose_premature_exit_reports_empty_docker_logs() {
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;

    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 1,
            oom_killed: false,
        }])),
        ..Default::default()
    };
    let mut runner = FakeRunner::with_capture_queue([String::new()]);
    let err = diagnose_premature_exit(
        &docker,
        &mut runner,
        "jk-the-architect",
        ExitPhase::PreAttach,
    )
    .await
    .expect("pre-attach exit 1 must produce a diagnostic error");
    let msg = err.to_string();
    assert!(
        msg.contains("no log output"),
        "empty-log detail missing: {msg}"
    );
}

#[tokio::test]
async fn agent_mounts_for_claude_ignore_mode_mounts_state_but_no_auth_handoff() {
    // Ignore mode must still mount durable Claude home state so
    // conversations/plugins survive a Docker delete, but auth handoff
    // files under /jackin/claude/ must not flow into the container.
    use crate::instance::{PrepareResolvers, RoleState};
    use jackin_core::Agent;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let manifest_temp = tempdir().unwrap();
    std::fs::write(
        manifest_temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();
    std::fs::write(
        manifest_temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(manifest_temp.path()).unwrap();

    let (state, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| jackin_config::AuthForwardMode::Ignore,
            sync_source_dirs: &|_| None,
        },
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    let mounts = agent_mounts(&state);
    assert!(
        mounts.iter().any(|m| m.contains(":/jackin/state")),
        "jackin state mount missing: {mounts:?}"
    );
    assert!(
        mounts.iter().any(|m| m.contains(":/home/agent/.claude")),
        "durable Claude home mount missing: {mounts:?}"
    );
    assert!(
        mounts
            .iter()
            .any(|m| m.contains(":/home/agent/.claude.json")),
        "durable Claude account file mount missing: {mounts:?}"
    );
    assert!(
        !mounts.iter().any(|m| m.contains("/jackin/claude/")),
        "ignore mode must not mount Claude auth handoff files: {mounts:?}"
    );
}

#[test]
fn github_config_mount_skips_absent_ignored_state() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("role-state");
    let state = RoleState {
        root: root.clone(),
        gh_config_dir: root.join(".config/gh"),
        gh_provision_outcome: crate::instance::GithubProvisionOutcome::Skipped,
        agent_runtime: crate::instance::AgentRuntimeState {
            agent: jackin_core::Agent::Claude,
            model: None,
        },
        auth: crate::instance::ProvisionedAuth::default(),
        auth_outcomes: std::collections::BTreeMap::new(),
    };

    assert!(
        github_config_mount(&state).is_none(),
        "ignored GitHub auth with no state should not make docker create an empty gh config dir"
    );
}

#[test]
fn github_config_mount_keeps_existing_ignored_state() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("role-state");
    let gh_config_dir = root.join(".config/gh");
    std::fs::create_dir_all(&gh_config_dir).unwrap();
    let state = RoleState {
        root,
        gh_config_dir,
        gh_provision_outcome: crate::instance::GithubProvisionOutcome::Skipped,
        agent_runtime: crate::instance::AgentRuntimeState {
            agent: jackin_core::Agent::Claude,
            model: None,
        },
        auth: crate::instance::ProvisionedAuth::default(),
        auth_outcomes: std::collections::BTreeMap::new(),
    };

    assert!(
        github_config_mount(&state)
            .as_deref()
            .is_some_and(|mount| mount.ends_with(":/home/agent/.config/gh")),
        "existing jackin-owned GitHub state should still mount"
    );
}

#[tokio::test]
async fn role_state_prepare_for_agents_skips_sibling_auth_slots() {
    use crate::instance::{PrepareResolvers, RoleState};
    use jackin_core::Agent;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let manifest_temp = tempdir().unwrap();
    std::fs::write(
        manifest_temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["claude", "codex"]

[claude]
plugins = []

[codex]
"#,
    )
    .unwrap();
    std::fs::write(
        manifest_temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(manifest_temp.path()).unwrap();
    let codex_mode_resolutions = AtomicUsize::new(0);
    let codex_sync_resolutions = AtomicUsize::new(0);

    let (state, _) = RoleState::prepare_for_agents(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|agent| {
                if agent == Agent::Codex {
                    codex_mode_resolutions.fetch_add(1, Ordering::SeqCst);
                }
                jackin_config::AuthForwardMode::Ignore
            },
            sync_source_dirs: &|agent| {
                if agent == Agent::Codex {
                    codex_sync_resolutions.fetch_add(1, Ordering::SeqCst);
                }
                None
            },
        },
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
        &[Agent::Claude],
    )
    .unwrap();

    assert!(state.auth.claude.is_some(), "selected Claude slot missing");
    assert!(
        state.auth.codex.is_none(),
        "sibling Codex auth slot must not be provisioned"
    );
    assert_eq!(
        codex_mode_resolutions.load(Ordering::SeqCst),
        0,
        "sibling auth mode must not be resolved"
    );
    assert_eq!(
        codex_sync_resolutions.load(Ordering::SeqCst),
        0,
        "sibling sync-source override must not be resolved"
    );
}

#[tokio::test]
async fn agent_mounts_for_claude_sync_mode_forwards_auth_files() {
    // Sync mode + host auth present → both account.json and
    // credentials.json flow under /jackin/claude/. Plugins are baked
    // into the image and do not need a runtime mount.
    use crate::instance::{PrepareResolvers, RoleState};
    use jackin_core::Agent;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let manifest_temp = tempdir().unwrap();
    std::fs::write(
        manifest_temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();
    std::fs::write(
        manifest_temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(manifest_temp.path()).unwrap();

    // Seed a fake host home with both Claude files so sync resolves.
    let host_home = temp.path().join("host_home");
    std::fs::create_dir_all(host_home.join(".claude")).unwrap();
    std::fs::write(
        host_home.join(".claude.json"),
        r#"{"oauthAccount":{"emailAddress":"test@example.com"}}"#,
    )
    .unwrap();
    std::fs::write(
        host_home.join(".claude/.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"t","refreshToken":"r"}}"#,
    )
    .unwrap();

    let (state, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| jackin_config::AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &crate::instance::GithubAuthContext::default(),
        &host_home,
        Agent::Claude,
    )
    .unwrap();

    let mounts = agent_mounts(&state);
    assert!(
        mounts
            .iter()
            .any(|m| m.contains("/jackin/claude/account.json") && !m.ends_with(":ro")),
        "account.json mount missing under /jackin/claude/: {mounts:?}",
    );
    assert!(
        mounts
            .iter()
            .any(|m| m.contains("/jackin/claude/credentials.json") && !m.ends_with(":ro")),
        "credentials.json mount missing under /jackin/claude/: {mounts:?}",
    );
}

#[tokio::test]
async fn agent_mounts_for_claude_oauth_token_mode_mounts_skeleton_only() {
    // OAuthToken mode writes a `{"hasCompletedOnboarding":true}`
    // skeleton at account.json (so the in-container CLI does not
    // run its login wizard) and removes credentials.json. The
    // launcher must mount the skeleton AND must not mount any
    // stale credentials.json that survived the provision step.
    use crate::instance::{PrepareResolvers, RoleState};
    use jackin_core::Agent;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let manifest_temp = tempdir().unwrap();
    std::fs::write(
        manifest_temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();
    std::fs::write(
        manifest_temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(manifest_temp.path()).unwrap();

    let (state, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| jackin_config::AuthForwardMode::OAuthToken,
            sync_source_dirs: &|_| None,
        },
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    let mounts = agent_mounts(&state);
    assert!(
        mounts
            .iter()
            .any(|m| m.contains("/jackin/claude/account.json")),
        "account.json skeleton must be mounted under oauth_token mode: {mounts:?}",
    );
    assert!(
        !mounts
            .iter()
            .any(|m| m.contains("/jackin/claude/credentials.json")),
        "credentials.json must NOT be mounted under oauth_token mode \
             (the env var is the credential): {mounts:?}",
    );
}

#[tokio::test]
async fn agent_mounts_for_codex_without_auth_mounts_state_but_no_auth_handoff() {
    use crate::instance::{PrepareResolvers, RoleState};
    use jackin_core::Agent;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let manifest_temp = tempdir().unwrap();
    std::fs::write(
        manifest_temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
"#,
    )
    .unwrap();
    std::fs::write(
        manifest_temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(manifest_temp.path()).unwrap();

    let (state, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| jackin_config::AuthForwardMode::Ignore,
            sync_source_dirs: &|_| None,
        },
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        Agent::Codex,
    )
    .unwrap();

    let mounts = agent_mounts(&state);
    assert!(
        mounts.iter().any(|m| m.contains(":/jackin/state")),
        "jackin state mount missing: {mounts:?}"
    );
    assert!(
        mounts.iter().any(|m| m.contains(":/home/agent/.codex")),
        "durable Codex home mount missing: {mounts:?}"
    );
    assert!(
        !mounts.iter().any(|m| m.contains("/jackin/codex/auth.json")),
        "no auth.json handoff when auth is ignored: {mounts:?}"
    );
}

#[tokio::test]
async fn agent_mounts_for_codex_synced_includes_auth_json() {
    use crate::instance::{PrepareResolvers, RoleState};
    use jackin_core::Agent;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let manifest_temp = tempdir().unwrap();
    std::fs::write(
        manifest_temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
"#,
    )
    .unwrap();
    std::fs::write(
        manifest_temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(manifest_temp.path()).unwrap();

    // Stage a host ~/.codex/auth.json so Sync mode succeeds.
    let host_home = temp.path().join("host_home");
    std::fs::create_dir_all(host_home.join(".codex")).unwrap();
    std::fs::write(
        host_home.join(".codex/auth.json"),
        "{\"auth_mode\":\"chatgpt\"}",
    )
    .unwrap();

    let (state, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| jackin_config::AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &crate::instance::GithubAuthContext::default(),
        &host_home,
        Agent::Codex,
    )
    .unwrap();

    let mounts = agent_mounts(&state);
    assert!(
        mounts.iter().any(|m| m.contains(":/home/agent/.codex")),
        "durable Codex home mount missing: {mounts:?}"
    );
    assert!(
        mounts
            .iter()
            .any(|m| m.contains("/jackin/codex/auth.json") && !m.ends_with(":ro")),
        "auth.json handoff missing: {mounts:?}"
    );
}

#[tokio::test]
async fn agent_mounts_for_codex_host_missing_omits_auth_json() {
    use crate::instance::{PrepareResolvers, RoleState};
    use jackin_core::Agent;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let manifest_temp = tempdir().unwrap();
    std::fs::write(
        manifest_temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
"#,
    )
    .unwrap();
    std::fs::write(
        manifest_temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(manifest_temp.path()).unwrap();

    let (state, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| jackin_config::AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &crate::instance::GithubAuthContext::default(),
        temp.path().join("empty_host_home").as_path(),
        Agent::Codex,
    )
    .unwrap();

    let mounts = agent_mounts(&state);
    assert!(
        mounts.iter().any(|m| m.contains(":/home/agent/.codex")),
        "durable Codex home mount missing: {mounts:?}"
    );
    assert!(
        !mounts.iter().any(|m| m.contains("/jackin/codex/auth.json")),
        "no auth.json handoff when host has no ~/.codex/auth.json: {mounts:?}"
    );
}

#[tokio::test]
async fn agent_mounts_for_amp_synced_includes_secrets_json() {
    use crate::instance::{PrepareResolvers, RoleState};
    use jackin_core::Agent;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let manifest_temp = tempdir().unwrap();
    std::fs::write(
        manifest_temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["amp"]

[amp]
"#,
    )
    .unwrap();
    std::fs::write(
        manifest_temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(manifest_temp.path()).unwrap();

    let host_home = temp.path().join("host_home");
    std::fs::create_dir_all(host_home.join(".local/share/amp")).unwrap();
    std::fs::write(
        host_home.join(".local/share/amp/secrets.json"),
        "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}",
    )
    .unwrap();

    let (state, _) = RoleState::prepare(
        &paths,
        "jk-the-architect",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| jackin_config::AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &crate::instance::GithubAuthContext::default(),
        &host_home,
        Agent::Amp,
    )
    .unwrap();

    let mounts = agent_mounts(&state);
    assert!(
        mounts
            .iter()
            .any(|m| m.contains(":/home/agent/.local/share/amp")),
        "durable Amp data mount missing: {mounts:?}"
    );
    assert!(
        mounts
            .iter()
            .any(|m| m.contains("/jackin/amp/secrets.json") && !m.ends_with(":ro")),
        "secrets.json handoff missing: {mounts:?}"
    );
}

#[tokio::test]
async fn agent_mounts_for_amp_ignore_mounts_state_but_no_auth_handoff() {
    use crate::instance::{PrepareResolvers, RoleState};
    use jackin_core::Agent;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let manifest_temp = tempdir().unwrap();
    std::fs::write(
        manifest_temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["amp"]

[amp]
"#,
    )
    .unwrap();
    std::fs::write(
        manifest_temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(manifest_temp.path()).unwrap();

    let (state, _) = RoleState::prepare(
        &paths,
        "jk-the-architect",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| jackin_config::AuthForwardMode::Ignore,
            sync_source_dirs: &|_| None,
        },
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        Agent::Amp,
    )
    .unwrap();

    let mounts = agent_mounts(&state);
    assert!(
        mounts.iter().any(|m| m.contains(":/jackin/state")),
        "jackin state mount missing: {mounts:?}"
    );
    assert!(
        mounts
            .iter()
            .any(|m| m.contains(":/home/agent/.local/share/amp")),
        "durable Amp data mount missing: {mounts:?}"
    );
    assert!(
        !mounts
            .iter()
            .any(|m| m.contains("/jackin/amp/secrets.json")),
        "ignore mode must not mount Amp auth handoff files: {mounts:?}"
    );
}

#[test]
fn exec_binding_names_joins_names_in_order() {
    let bindings = vec![
        jackin_protocol::ExecBinding {
            name: "A".to_owned(),
            kind: jackin_protocol::ExecKind::Op,
            source: "op://x".to_owned(),
        },
        jackin_protocol::ExecBinding {
            name: "B".to_owned(),
            kind: jackin_protocol::ExecKind::Literal,
            source: "v".to_owned(),
        },
    ];
    // This string is the contract the in-container picker reads; pin it so the
    // two launch paths can't drift on the format.
    assert_eq!(exec_binding_names(&bindings), "A,B");
    assert_eq!(exec_binding_names(&[]), "");
}

#[test]
fn resolve_backend_defaults_docker_and_workspace_overrides_config() {
    let mut config = jackin_config::AppConfig::default();
    // No selection anywhere → Docker.
    assert_eq!(resolve_backend(&config, None).unwrap(), Backend::Docker);
    // Host-wide default applies.
    config.runtime.default_backend = Some("apple-container".to_owned());
    assert_eq!(
        resolve_backend(&config, None).unwrap(),
        Backend::AppleContainer
    );
    // Per-workspace backend overrides the host-wide default.
    let mut ws = jackin_config::WorkspaceConfig::default();
    ws.runtime.backend = Some("docker".to_owned());
    config.workspaces.insert("prod".to_owned(), ws);
    assert_eq!(
        resolve_backend(&config, Some("prod")).unwrap(),
        Backend::Docker
    );
    // A workspace without an override falls back to the host-wide default.
    assert_eq!(
        resolve_backend(&config, Some("absent")).unwrap(),
        Backend::AppleContainer
    );
    // An unrecognised backend fails closed instead of silently launching Docker.
    config.runtime.default_backend = Some("aple-container".to_owned());
    resolve_backend(&config, None).unwrap_err();
}

/// Build the docker mounts for a single agent in Ignore mode (no auth handoff),
/// so assertions isolate the derived durable-home mounts from
/// `push_agent_home_mounts`. Covers the agent-enum consolidation: a regression
/// in any agent's `AgentStatePaths` (wrong/dropped data or config root) surfaces
/// here instead of shipping silently.
fn home_mounts_for(agent_slug: &str, agent: jackin_core::Agent) -> Vec<String> {
    use crate::instance::{PrepareResolvers, RoleState};
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let manifest_temp = tempdir().unwrap();
    std::fs::write(
        manifest_temp.path().join("jackin.role.toml"),
        format!(
            "version = \"v1alpha4\"\ndockerfile = \"Dockerfile\"\nagents = [\"{agent_slug}\"]\n\n[{agent_slug}]\n"
        ),
    )
    .unwrap();
    std::fs::write(
        manifest_temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(manifest_temp.path()).unwrap();
    let (state, _) = RoleState::prepare(
        &paths,
        "jk-the-architect",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| jackin_config::AuthForwardMode::Ignore,
            sync_source_dirs: &|_| None,
        },
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        agent,
    )
    .unwrap();
    agent_mounts(&state)
}

#[tokio::test]
async fn agent_mounts_derive_opencode_data_and_config_roots() {
    let mounts = home_mounts_for("opencode", jackin_core::Agent::Opencode);
    assert!(
        mounts
            .iter()
            .any(|m| m.ends_with(":/home/agent/.local/share/opencode")),
        "opencode data root mount missing: {mounts:?}"
    );
    assert!(
        mounts
            .iter()
            .any(|m| m.ends_with(":/home/agent/.config/opencode")),
        "opencode paired config root mount missing: {mounts:?}"
    );
}

#[tokio::test]
async fn agent_mounts_derive_amp_paired_config_root() {
    let mounts = home_mounts_for("amp", jackin_core::Agent::Amp);
    assert!(
        mounts
            .iter()
            .any(|m| m.ends_with(":/home/agent/.config/amp")),
        "amp paired config root mount missing: {mounts:?}"
    );
}

#[tokio::test]
async fn agent_mounts_derive_grok_home_root() {
    let mounts = home_mounts_for("grok", jackin_core::Agent::Grok);
    assert!(
        mounts.iter().any(|m| m.ends_with(":/home/agent/.grok")),
        "grok home root mount missing: {mounts:?}"
    );
}

#[tokio::test]
async fn agent_mounts_derive_kimi_home_root() {
    let mounts = home_mounts_for("kimi", jackin_core::Agent::Kimi);
    assert!(
        mounts
            .iter()
            .any(|m| m.ends_with(":/home/agent/.kimi-code")),
        "kimi home root mount missing: {mounts:?}"
    );
}

#[tokio::test]
async fn build_workspace_mount_strings_marks_overrides_readonly() {
    // One worktree-mode mount with all four bind sources populated.
    // Host `.git/` mount MUST stay rw (git writes refs/objects/
    // HEAD/index/logs all under it on every commit/branch/fetch).
    // Both override files MUST be `:ro`-suppressed.
    let mat = MaterializedWorkspace {
            workdir: "/workspace/jackin".into(),
            mounts: vec![MaterializedMount {
                bind_src:
                    "/data/jk-the-architect/git/worktree/repo/Users/donbeave/Projects/jackin-project/jackin/jk-the-architect"
                        .into(),
                dst: "/Users/donbeave/Projects/jackin-project/jackin".into(),
                readonly: false,
                isolation: MountIsolation::Worktree,
                worktree_aux: Some(WorktreeAuxMounts {
                    host_git_dir: "/Users/donbeave/Projects/jackin-project/jackin/.git".into(),
                    host_git_target:
                        "/jackin/host/Users/donbeave/Projects/jackin-project/jackin/.git".into(),
                    git_file_override:
                        "/data/jk-the-architect/git/overrides/Users/donbeave/Projects/jackin-project/jackin/.git"
                            .into(),
                    git_file_target: "/Users/donbeave/Projects/jackin-project/jackin/.git".into(),
                    gitdir_back_override:
                        "/data/jk-the-architect/git/overrides/Users/donbeave/Projects/jackin-project/jackin/gitdir"
                            .into(),
                    gitdir_back_target:
                        "/jackin/host/Users/donbeave/Projects/jackin-project/jackin/.git/worktrees/jk-the-architect/gitdir"
                            .into(),
                }),
            }],
            keep_awake_enabled: false,
        };

    let strings = build_workspace_mount_strings(&mat);
    assert_eq!(strings.len(), 4, "one worktree mount → four bind specs");

    // 1: worktree at <dst>, no :ro (writable).
    assert_eq!(
        strings[0],
        "/data/jk-the-architect/git/worktree/repo/Users/donbeave/Projects/jackin-project/jackin/jk-the-architect:/Users/donbeave/Projects/jackin-project/jackin"
    );
    assert!(!strings[0].ends_with(":ro"));

    // 2: host .git/, MUST stay rw — refs/objects/HEAD/index/logs
    // are all written under it. Both ends terminate in `.git`.
    assert_eq!(
        strings[1],
        "/Users/donbeave/Projects/jackin-project/jackin/.git:/jackin/host/Users/donbeave/Projects/jackin-project/jackin/.git"
    );
    assert!(
        !strings[1].ends_with(":ro"),
        "host .git mount must remain rw",
    );

    // 3: .git pointer override at <dst>/.git. :ro hardening.
    assert!(
        strings[2].ends_with(":ro"),
        "git-file override must be ro; got {}",
        strings[2],
    );
    assert!(
        strings[2].contains("/git/overrides/Users/donbeave/Projects/jackin-project/jackin/.git")
    );
    assert!(strings[2].contains(":/Users/donbeave/Projects/jackin-project/jackin/.git:ro"));

    // 4: gitdir back-pointer override at
    // `/jackin/host/<dst-tree>/.git/worktrees/<container>/gitdir`.
    // File-level overlay on top of the host `.git/` mount destination.
    // :ro hardening.
    assert!(
        strings[3].ends_with(":ro"),
        "gitdir-back override must be ro; got {}",
        strings[3],
    );
    assert!(
        strings[3].contains("/git/overrides/Users/donbeave/Projects/jackin-project/jackin/gitdir")
    );
    assert!(
            strings[3].contains(
                ":/jackin/host/Users/donbeave/Projects/jackin-project/jackin/.git/worktrees/jk-the-architect/gitdir:ro"
            )
        );
}

#[tokio::test]
async fn build_workspace_mount_strings_passthrough_for_shared_mounts() {
    // Shared mounts produce exactly one bind spec, no aux entries.
    let mat = MaterializedWorkspace {
        workdir: "/workspace".into(),
        mounts: vec![MaterializedMount {
            bind_src: "/host/shared".into(),
            dst: "/workspace/shared".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
            worktree_aux: None,
        }],
        keep_awake_enabled: false,
    };

    let strings = build_workspace_mount_strings(&mat);
    assert_eq!(strings, vec!["/host/shared:/workspace/shared".to_owned()]);
}

#[tokio::test]
async fn build_workspace_mount_strings_two_isolated_mounts_emits_eight_distinct_strings() {
    // A workspace with two isolated mounts on different host repos
    // (allowed by validate_isolation_layout) must emit a clean
    // 4-bind grouping per mount with no path collisions. This is
    // the production multi-mount path; finalize.rs's prompt loop
    // also handles this case (see multi_mount_force_delete_on_each_*).
    let mat = MaterializedWorkspace {
        workdir: "/workspace".into(),
        mounts: vec![
            MaterializedMount {
                bind_src: "/data/jackin-x/git/worktree/repo/workspace/a/jackin-x".into(),
                dst: "/workspace/a".into(),
                readonly: false,
                isolation: MountIsolation::Worktree,
                worktree_aux: Some(WorktreeAuxMounts {
                    host_git_dir: "/host/repo-a/.git".into(),
                    host_git_target: "/jackin/host/workspace/a/.git".into(),
                    git_file_override: "/data/jackin-x/git/overrides/workspace/a/.git".into(),
                    git_file_target: "/workspace/a/.git".into(),
                    gitdir_back_override: "/data/jackin-x/git/overrides/workspace/a/gitdir".into(),
                    gitdir_back_target: "/jackin/host/workspace/a/.git/worktrees/jackin-x/gitdir"
                        .into(),
                }),
            },
            MaterializedMount {
                bind_src: "/data/jackin-x/git/worktree/repo/workspace/b/jackin-x".into(),
                dst: "/workspace/b".into(),
                readonly: false,
                isolation: MountIsolation::Worktree,
                worktree_aux: Some(WorktreeAuxMounts {
                    host_git_dir: "/host/repo-b/.git".into(),
                    host_git_target: "/jackin/host/workspace/b/.git".into(),
                    git_file_override: "/data/jackin-x/git/overrides/workspace/b/.git".into(),
                    git_file_target: "/workspace/b/.git".into(),
                    gitdir_back_override: "/data/jackin-x/git/overrides/workspace/b/gitdir".into(),
                    gitdir_back_target: "/jackin/host/workspace/b/.git/worktrees/jackin-x/gitdir"
                        .into(),
                }),
            },
        ],
        keep_awake_enabled: false,
    };

    let strings = build_workspace_mount_strings(&mat);
    assert_eq!(
        strings.len(),
        8,
        "two isolated mounts → eight bind specs (4 per mount); got {strings:?}"
    );

    // No two emitted strings may be identical — distinct dsts
    // throughout, which is the disambiguation guarantee under
    // /jackin/host/<dst-tree>/.
    let mut sorted = strings.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        strings.len(),
        "no duplicate bind specs across mounts; got {strings:?}"
    );

    // Each mount's 4 bind specs reference its own dst tree.
    let first_mount_count = strings
        .iter()
        .filter(|s| s.contains("/workspace/a") || s.contains("/jackin/host/workspace/a/"))
        .count();
    let second_mount_count = strings
        .iter()
        .filter(|s| s.contains("/workspace/b") || s.contains("/jackin/host/workspace/b/"))
        .count();
    assert_eq!(first_mount_count, 4, "mount A should have 4 bind specs");
    assert_eq!(second_mount_count, 4, "mount B should have 4 bind specs");

    // Both override files for both mounts must remain :ro.
    let ro_count = strings.iter().filter(|s| s.ends_with(":ro")).count();
    assert_eq!(
        ro_count, 4,
        ":ro hardening must apply to both override files of both mounts; got {strings:?}"
    );
}

#[tokio::test]
async fn build_workspace_mount_strings_preserves_readonly_on_user_facing_mount() {
    // A user-configured `readonly = true` mount still gets `:ro` on
    // the user-facing dst — this is independent of the override
    // hardening.
    let mat = MaterializedWorkspace {
        workdir: "/workspace".into(),
        mounts: vec![MaterializedMount {
            bind_src: "/host/cache".into(),
            dst: "/workspace/cache".into(),
            readonly: true,
            isolation: MountIsolation::Shared,
            worktree_aux: None,
        }],
        keep_awake_enabled: false,
    };

    let strings = build_workspace_mount_strings(&mat);
    assert_eq!(strings, vec!["/host/cache:/workspace/cache:ro".to_owned()]);
}

#[tokio::test]
async fn workspace_mise_paths_cover_workdir_and_mount_destinations() {
    let workspace = jackin_config::ResolvedWorkspace {
        name: String::new(),
        label: "sample-workspace".to_owned(),
        workdir: "/workspace".to_owned(),
        mounts: vec![
            jackin_config::MountConfig {
                src: "/host/jackin".to_owned(),
                dst: "/workspace/jackin".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
            jackin_config::MountConfig {
                src: "/host/homebrew-tap".to_owned(),
                dst: "/workspace/homebrew-tap".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
        ],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
    };

    let value = workspace_mise_trusted_config_paths(&workspace).unwrap();

    assert_eq!(
        value,
        "/workspace:/workspace/homebrew-tap:/workspace/jackin"
    );
}

#[tokio::test]
async fn workspace_mise_env_does_not_override_operator_value() {
    let workspace = repo_workspace(Path::new("/host/repo"));
    let mut vars = vec![(
        MISE_TRUSTED_CONFIG_PATHS_ENV.to_owned(),
        "/operator/trusted".to_owned(),
    )];

    inject_workspace_mise_env(&mut vars, &workspace);

    assert_eq!(
        vars,
        vec![(
            MISE_TRUSTED_CONFIG_PATHS_ENV.to_owned(),
            "/operator/trusted".to_owned()
        )]
    );
}

#[test]
fn attach_failure_error_preserves_command_context() {
    let error = attach_failure_error(
        "jk-test",
        &anyhow::anyhow!("command failed: docker exec ..."),
    )
    .to_string();

    assert!(
        error.contains("capsule attach failed for jk-test"),
        "{error}"
    );
    assert!(error.contains("command failed: docker exec"), "{error}");
}

/// A Codex-authed role state rooted at `root` plus a workspace whose
/// workdir (`/workspace`) and single mount (`/workspace/repo`) are the two
/// paths `seed_codex_project_trust` should mark trusted.
fn codex_trust_fixture(root: &Path) -> (RoleState, jackin_config::ResolvedWorkspace) {
    let state = RoleState {
        root: root.to_path_buf(),
        gh_config_dir: root.join("gh"),
        gh_provision_outcome: crate::instance::GithubProvisionOutcome::Skipped,
        agent_runtime: crate::instance::AgentRuntimeState {
            agent: jackin_core::Agent::Codex,
            model: None,
        },
        auth: crate::instance::ProvisionedAuth {
            codex: Some(crate::instance::CodexAuth::default()),
            ..Default::default()
        },
        auth_outcomes: std::collections::BTreeMap::new(),
    };
    let workspace = jackin_config::ResolvedWorkspace {
        name: String::new(),
        label: "sample-workspace".to_owned(),
        workdir: "/workspace".to_owned(),
        mounts: vec![jackin_config::MountConfig {
            src: "/host/repo".to_owned(),
            dst: "/workspace/repo".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
    };
    (state, workspace)
}

#[test]
fn seed_codex_project_trust_preserves_existing_config() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("state");
    std::fs::create_dir_all(root.join("home/.codex")).unwrap();
    std::fs::write(
        root.join("home/.codex/config.toml"),
        "model = \"gpt-5\"\n\n[projects.\"/existing\"]\ntrust_level = \"trusted\"\n",
    )
    .unwrap();
    let (state, workspace) = codex_trust_fixture(&root);

    seed_codex_project_trust(&state, &workspace).unwrap();

    let codex_config = std::fs::read_to_string(root.join("home/.codex/config.toml")).unwrap();
    assert!(codex_config.contains("model = \"gpt-5\""));
    assert!(codex_config.contains("[projects.\"/existing\"]"));
    assert!(codex_config.contains("[projects.\"/workspace\"]"));
    assert!(codex_config.contains("[projects.\"/workspace/repo\"]"));
    assert_eq!(codex_config.matches("trust_level = \"trusted\"").count(), 3);
}

#[test]
fn seed_codex_project_trust_replaces_non_table_projects_value() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("state");
    std::fs::create_dir_all(root.join("home/.codex")).unwrap();
    std::fs::write(root.join("home/.codex/config.toml"), "projects = 5\n").unwrap();
    let (state, workspace) = codex_trust_fixture(&root);

    seed_codex_project_trust(&state, &workspace).unwrap();

    let codex_config = std::fs::read_to_string(root.join("home/.codex/config.toml")).unwrap();
    assert!(!codex_config.contains("projects = 5"));
    assert!(codex_config.contains("[projects.\"/workspace\"]"));
    assert!(codex_config.contains("trust_level = \"trusted\""));
}

#[test]
fn seed_codex_project_trust_replaces_non_table_project_entry() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("state");
    std::fs::create_dir_all(root.join("home/.codex")).unwrap();
    std::fs::write(
        root.join("home/.codex/config.toml"),
        "[projects]\n\"/workspace\" = \"oops\"\n",
    )
    .unwrap();
    let (state, workspace) = codex_trust_fixture(&root);

    seed_codex_project_trust(&state, &workspace).unwrap();

    let codex_config = std::fs::read_to_string(root.join("home/.codex/config.toml")).unwrap();
    assert!(!codex_config.contains("\"oops\""));
    let doc: toml_edit::DocumentMut = codex_config.parse().unwrap();
    let projects = doc.get("projects").and_then(|i| i.as_table_like()).unwrap();
    let workspace_entry = projects.get("/workspace").and_then(|i| i.as_table_like());
    assert_eq!(
        workspace_entry
            .and_then(|t| t.get("trust_level"))
            .and_then(|i| i.as_str()),
        Some("trusted")
    );
}

#[test]
fn seed_codex_project_trust_is_idempotent_across_relaunches() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("state");
    std::fs::create_dir_all(root.join("home/.codex")).unwrap();
    let (state, workspace) = codex_trust_fixture(&root);

    seed_codex_project_trust(&state, &workspace).unwrap();
    let first = std::fs::read_to_string(root.join("home/.codex/config.toml")).unwrap();
    seed_codex_project_trust(&state, &workspace).unwrap();
    let second = std::fs::read_to_string(root.join("home/.codex/config.toml")).unwrap();

    assert_eq!(first, second);
    assert_eq!(second.matches("trust_level = \"trusted\"").count(), 2);
}

#[test]
fn seed_codex_project_trust_errors_on_invalid_toml_without_clobbering() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("state");
    std::fs::create_dir_all(root.join("home/.codex")).unwrap();
    let original = "[unterminated\n";
    std::fs::write(root.join("home/.codex/config.toml"), original).unwrap();
    let (state, workspace) = codex_trust_fixture(&root);

    let err = seed_codex_project_trust(&state, &workspace).unwrap_err();
    assert!(err.to_string().contains("parsing Codex config"));
    let after = std::fs::read_to_string(root.join("home/.codex/config.toml")).unwrap();
    assert_eq!(after, original);
}

#[tokio::test]
async fn git_pull_on_entry_starts_all_repo_pulls_before_waiting() {
    let temp = tempdir().unwrap();
    let bin_dir = temp.path().join("bin");
    let marker_dir = temp.path().join("markers");
    std::fs::create_dir_all(&bin_dir).unwrap();
    std::fs::create_dir_all(&marker_dir).unwrap();

    let git_script = bin_dir.join("git");
    std::fs::write(
        &git_script,
        r#"#!/bin/sh
set -eu
marker_dir="$(dirname "$0")/../markers"
touch "$marker_dir/$(basename "$2").started"
i=0
while [ "$(find "$marker_dir" -name '*.started' | wc -l | tr -d ' ')" -lt 2 ]; do
  i=$((i + 1))
  if [ "$i" -gt 80 ]; then
    echo "timed out waiting for peer pull" >&2
    exit 42
  fi
  sleep 0.025
done
echo "pulled $2"
"#,
    )
    .unwrap();
    let mut perms = std::fs::metadata(&git_script).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&git_script, perms).unwrap();

    let repo_a = temp.path().join("repo-a");
    let repo_b = temp.path().join("repo-b");
    std::fs::create_dir_all(repo_a.join(".git")).unwrap();
    std::fs::create_dir_all(repo_b.join(".git")).unwrap();

    let workspace = jackin_config::ResolvedWorkspace {
        name: String::new(),
        label: "parallel".to_owned(),
        workdir: "/workspace".to_owned(),
        mounts: vec![
            jackin_config::MountConfig {
                src: repo_a.display().to_string(),
                dst: "/workspace/a".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
            jackin_config::MountConfig {
                src: repo_b.display().to_string(),
                dst: "/workspace/b".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
        ],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: true,
    };

    pull_workspace_repos_with_git(&workspace, false, &git_script);

    assert!(marker_dir.join("repo-a.started").is_file());
    assert!(marker_dir.join("repo-b.started").is_file());
}

#[test]
fn git_pull_exports_spawn_failure_without_repo_or_program_paths() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let results = pull_git_sources_with_git(
        vec!["/operator-secret/repository".to_owned()],
        false,
        Path::new("/operator-secret/missing-git"),
        false,
    );
    assert!(matches!(
        results.as_slice(),
        [super::git_pull::GitPullResult::SpawnError { .. }]
    ));
    export.force_flush();

    assert_eq!(export.finished_spans().len(), 1);
    assert_eq!(export.error_span_count(), 1);
    assert!(export.contains_span_text("process_spawn_error"));
    assert!(!export.contains_span_text("operator-secret"));
    assert!(!export.contains_span_text("repository"));
    assert!(!export.contains_span_text("missing-git"));
}

fn repo_workspace(repo_dir: &Path) -> jackin_config::ResolvedWorkspace {
    jackin_config::ResolvedWorkspace {
        name: String::new(),
        label: repo_dir.display().to_string(),
        workdir: "/workspace".to_owned(),
        mounts: vec![jackin_config::MountConfig {
            src: repo_dir.display().to_string(),
            dst: "/workspace".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
    }
}

fn fake_docker_for_clean_attached_exit() -> jackin_test_support::FakeDockerClient {
    jackin_test_support::FakeDockerClient {
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from([
            String::new(),
            String::new(),
            "Sessions: 1\n".to_owned(),
            "Sessions: 0\n".to_owned(),
        ])),
        ..Default::default()
    }
}

fn arg_after(command: &str, flag: &str) -> String {
    let mut args = command.split_whitespace();
    while let Some(arg) = args.next() {
        if arg == flag {
            return args.next().unwrap_or_default().to_owned();
        }
    }
    String::new()
}

fn launched_role_container_name(runner: &FakeRunner) -> String {
    let command = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d --name ") && call.contains("jackin.kind=role"))
        .expect("expected role docker run command");
    arg_after(command, "--name")
}

fn launched_dind_container(
    docker: &jackin_test_support::FakeDockerClient,
) -> (String, jackin_core::ContainerSpec) {
    docker
        .created_containers
        .borrow()
        .iter()
        .find(|(_, spec)| {
            spec.labels
                .get("jackin.kind")
                .is_some_and(|value| value == "dind")
        })
        .cloned()
        .expect("expected DinD container")
}

fn dind_env_from_run_cmd(run_cmd: &str) -> String {
    run_cmd
        .split_whitespace()
        .find_map(|arg| arg.strip_prefix("JACKIN_DIND_HOSTNAME="))
        .expect("expected JACKIN_DIND_HOSTNAME env")
        .to_owned()
}

fn compat_dind_load_options() -> LoadOptions {
    LoadOptions {
        docker_profile: Some(crate::runtime::docker_profile::DockerSecurityProfile::Compat),
        ..LoadOptions::default()
    }
}

#[test]
fn host_runtime_passthrough_env_keeps_only_explicit_runtime_knobs() {
    let passthrough = host_runtime_passthrough_env([
        ("JACKIN_DISABLE_TIRITH".to_owned(), "1".to_owned()),
        ("JACKIN_DHAT_ALLOC_LOG".to_owned(), "1".to_owned()),
        ("JACKIN_CAPSULE_FORCE_PANIC".to_owned(), "true".to_owned()),
        ("TZ".to_owned(), "Asia/Ho_Chi_Minh".to_owned()),
        ("PATH".to_owned(), "/bin".to_owned()),
    ]);

    assert_eq!(
        passthrough,
        vec![
            "JACKIN_DISABLE_TIRITH=1",
            "JACKIN_DHAT_ALLOC_LOG=1",
            "JACKIN_CAPSULE_FORCE_PANIC=true",
            "TZ=Asia/Ho_Chi_Minh",
        ]
    );
}

#[test]
fn debug_runtime_envs_do_not_propagate_file_configuration() {
    let debug_envs = debug_runtime_envs(true);
    assert!(debug_envs.is_empty());
}

#[test]
fn telemetry_runtime_envs_forward_effective_level_to_capsule() {
    assert_eq!(
        telemetry_runtime_envs_for(jackin_diagnostics::TelemetryLevel::Info),
        vec!["JACKIN_TELEMETRY_LEVEL=info".to_owned()]
    );
    assert_eq!(
        telemetry_runtime_envs_for(jackin_diagnostics::TelemetryLevel::Debug),
        vec!["JACKIN_TELEMETRY_LEVEL=debug".to_owned()]
    );
    assert_eq!(
        telemetry_runtime_envs_for(jackin_diagnostics::TelemetryLevel::Trace),
        vec!["JACKIN_TELEMETRY_LEVEL=trace".to_owned()]
    );
}

#[tokio::test]
async fn validate_agent_supported_rejects_unsupported_choice() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(temp.path()).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");

    let err =
        validate_agent_supported(&selector, &manifest, jackin_core::Agent::Codex).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("role \"agent-smith\""));
    assert!(message.contains("agent \"codex\""));
    assert!(message.contains("supported: [claude]"));
}

#[tokio::test]
async fn restore_role_source_override_uses_manifest_source_without_mutating_config() {
    let selector = RoleSelector::new(None, "agent-smith");
    let mut config = AppConfig::default();
    config.roles.insert(
        "agent-smith".to_owned(),
        jackin_config::RoleSource {
            git: "https://example.invalid/current.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );

    let (source, is_new, restore_override) = resolve_launch_role_source(
        &mut config,
        &selector,
        Some("https://example.invalid/recorded.git"),
    )
    .unwrap();

    assert_eq!(source.git, "https://example.invalid/recorded.git");
    assert!(source.trusted);
    assert!(!is_new);
    assert!(restore_override);
    assert_eq!(
        config.roles.get("agent-smith").unwrap().git,
        "https://example.invalid/current.git"
    );
}

/// Helper: trust callback that always accepts.
///
/// Signature matches `deny_trust` so both can be passed as the same
/// function-pointer type to the trust prompt; the `Ok(())` is therefore
/// load-bearing even though clippy flags it.
#[expect(
    clippy::unnecessary_wraps,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
fn auto_trust(_: &RoleSelector, _: &jackin_config::RoleSource) -> anyhow::Result<()> {
    Ok(())
}

/// Helper: trust callback that always declines.
fn deny_trust(_: &RoleSelector, _: &jackin_config::RoleSource) -> anyhow::Result<()> {
    anyhow::bail!("role source not trusted — aborting")
}

#[tokio::test]
async fn load_namespaced_agent_registers_source_and_trusts_on_accept() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");
    let mut runner =
        FakeRunner::for_load_agent(["false 0 false".to_owned(), "false 0 false".to_owned()]);

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
model = "sonnet"
plugins = ["code-review@claude-plugins-official"]
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
    load_role_with(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &compat_dind_load_options(),
        auto_trust,
        |_, _, _| Ok(()),
    )
    .await
    .unwrap();

    // Source was auto-registered and persisted with trust
    let persisted = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(persisted.contains("chainargos/the-architect"));
    assert!(persisted.contains("trusted = true"));
    assert!(
        runner
            .recorded
            .iter()
            .any(|call| call.contains("git -C") || call.contains("git clone"))
    );
    assert!(runner.recorded.iter().any(|call| {
        call.contains("buildx build ")
            && call.contains("--output type=docker,name=jk_chainargos_the-architect")
    }));
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| { call.contains("docker inspect jk-") && call.contains("thearchitect") })
    );
    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| {
            call.contains("docker run -d --name jk-")
                && call.contains("thearchitect")
                && call.contains("jackin.kind=role")
        })
        .unwrap();
    let container_name = launched_role_container_name(&runner);
    assert!(crate::instance::naming::is_dns_label(&container_name));
    assert!(!container_name.contains("__"));
    assert!(!container_name.contains("clone"));
    assert!(!run_cmd.contains("JACKIN_CODEX_MODEL"));
    assert!(!run_cmd.contains("JACKIN_AGENT_MODEL_OVERRIDES"));
    assert!(!run_cmd.contains("-e JACKIN_ROLE="));
    let capsule_config_path = paths
        .jackin_home
        .join("sockets")
        .join(&container_name)
        .join(jackin_protocol::CAPSULE_CONFIG_FILENAME);
    let capsule_config: jackin_protocol::CapsuleConfig =
        toml::from_str(&std::fs::read_to_string(capsule_config_path).unwrap()).unwrap();
    assert_eq!(capsule_config.role, "chainargos/the-architect");
    assert_eq!(capsule_config.workdir, workspace.workdir);
    assert_eq!(capsule_config.agents, vec!["claude"]);
    assert_eq!(capsule_config.models.get("claude").unwrap(), "sonnet");
    assert!(
        !runner
            .recorded
            .iter()
            .any(|call| call.contains("claude plugin install"))
    );

    let (dind, dind_spec) = launched_dind_container(&docker);
    assert!(crate::instance::naming::is_dns_label(&dind));
    assert!(!dind.contains("__"));
    assert!(
        dind_spec
            .env
            .contains(&format!("DOCKER_TLS_SAN=DNS:{dind}")),
        "DinD SAN must include the DNS-safe DinD name with a DNS: prefix"
    );
}

/// WP3 hard rule: the host Docker socket must never be bind-mounted into a
/// role container. Inner-Docker access is provided exclusively by the `DinD`
/// sidecar over TLS; mounting `docker.sock` would hand the agent the host
/// daemon (root-equivalent escape). This guards the rule against regression
/// across the whole launch path, not just one mount-builder.
#[tokio::test]
async fn role_container_never_mounts_host_docker_socket() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");
    let mut runner =
        FakeRunner::for_load_agent(["false 0 false".to_owned(), "false 0 false".to_owned()]);

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
model = "sonnet"
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
    load_role_with(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &LoadOptions::default(),
        auto_trust,
        |_, _, _| Ok(()),
    )
    .await
    .unwrap();

    let role_run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d --name jk-") && call.contains("jackin.kind=role"))
        .expect("expected role docker run command");
    assert!(
        !role_run_cmd.contains("docker.sock"),
        "role container must never bind-mount the host Docker socket; run cmd was: {role_run_cmd}"
    );
    // Belt and suspenders: no container the fake daemon created (the DinD
    // sidecar included) binds the host Docker socket.
    for (name, spec) in docker.created_containers.borrow().iter() {
        assert!(
            !spec.binds.iter().any(|b| b.contains("docker.sock")),
            "container {name} must not bind-mount docker.sock"
        );
    }
}

#[tokio::test]
async fn load_namespaced_agent_aborts_when_trust_declined() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(Some("evil-org"), "backdoor");
    let mut runner = FakeRunner::for_load_agent([String::new(), String::new()]);

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
plugins = []
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
    let error = load_role_with(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &LoadOptions::default(),
        deny_trust,
        |_, _, _| Ok(()),
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("not trusted"));

    // Source was NOT persisted when trust was declined
    let persisted = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!persisted.contains("evil-org/backdoor"));

    // No Docker build or run commands were issued
    assert!(
        !runner
            .recorded
            .iter()
            .any(|call| call.contains("docker build") || call.contains("docker run"))
    );
}

#[tokio::test]
async fn load_agent_injects_configured_mounts() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let selector = RoleSelector::new(Some("chainargos"), "agent-brown");
    let mut runner =
        FakeRunner::for_load_agent(["false 0 false".to_owned(), "false 0 false".to_owned()]);

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
plugins = []
"#,
    )
    .unwrap();

    let mount_src = temp.path().join("test-mount");
    std::fs::create_dir_all(&mount_src).unwrap();
    std::fs::create_dir_all(&paths.config_dir).unwrap();

    let config_content = r#"[roles."chainargos/agent-brown"]
git = "git@github.com:chainargos/jackin-agent-brown.git"
trusted = true
"#;
    std::fs::write(&paths.config_file, config_content).unwrap();
    let mut config = AppConfig::load_or_init(&paths).unwrap();

    let workspace = jackin_config::ResolvedWorkspace {
        name: String::new(),
        label: "/workspace".to_owned(),
        workdir: "/workspace".to_owned(),
        mounts: vec![
            jackin_config::MountConfig {
                src: repo_dir.display().to_string(),
                dst: "/workspace".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
            jackin_config::MountConfig {
                src: mount_src.display().to_string(),
                dst: "/test-data".to_owned(),
                readonly: true,
                isolation: MountIsolation::Shared,
            },
        ],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
    };

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
    assert!(run_cmd.contains(&format!("{}:/test-data:ro", mount_src.display())));
}

#[tokio::test]
async fn load_agent_runs_attached_without_runtime_plugins_mount() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
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
            .any(|call| call.contains("buildx build ")
                && call.contains("--output type=docker,name=jk_agent-smith"))
    );
    assert!(
        runner
            .run_recorded
            .iter()
            .any(|call| call.contains("buildx build "))
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
    crate::runtime::test_support::install_all_test_stubs(&paths);
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
    let mut config = AppConfig::load_or_init(&paths).unwrap();
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
        .find(|call| call.contains("buildx build ") && call.contains("DerivedDockerfile"))
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
        run_cmd.ends_with(" codex"),
        "initial agent must be passed as container argv"
    );
    assert!(!run_cmd.contains("/jackin/codex/config.toml"));
    // Multi-agent role `agents = ["claude", "codex"]` provisions and mounts
    // every supported agent's home state at `docker run`, so a later
    // `hardline --new --agent claude` tab finds its auth without relaunching.
    // Both mounts must be present; the initially-selected agent is Codex.
    assert!(run_cmd.contains("/home/agent/.claude"));
    assert!(run_cmd.contains("/home/agent/.codex"));
    let container_name = launched_role_container_name(&runner);
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

/// Codex CLI drives interactive `ChatGPT` login when no API key is
/// present, so jackin must not gate launch on `OPENAI_API_KEY`.
#[tokio::test]
async fn load_agent_launches_codex_without_openai_key() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
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
agents = ["codex"]

[codex]
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

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .expect("role docker run should fire even without OPENAI_API_KEY");
    assert!(
        !run_cmd.contains("JACKIN_AGENT="),
        "JACKIN_AGENT must not be a container env var"
    );
    assert!(
        run_cmd.ends_with(" codex"),
        "initial agent must be passed as container argv"
    );
    assert!(!run_cmd.contains("-e OPENAI_API_KEY="));
}

struct LoadAgentFixture {
    _temp: tempfile::TempDir,
    paths: JackinPaths,
    config: AppConfig,
    selector: RoleSelector,
    runner: FakeRunner,
    workspace: jackin_config::ResolvedWorkspace,
    docker: jackin_test_support::FakeDockerClient,
}

fn load_agent_fixture(manifest_body: &str) -> LoadAgentFixture {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
    )
    .unwrap();
    let config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let runner = FakeRunner::for_load_agent([String::new()]);
    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(repo_dir.join("jackin.role.toml"), manifest_body).unwrap();
    let workspace = repo_workspace(&repo_dir);
    LoadAgentFixture {
        _temp: temp,
        paths,
        config,
        selector,
        runner,
        workspace,
        docker: jackin_test_support::FakeDockerClient::default(),
    }
}

#[tokio::test]
async fn load_agent_uses_single_supported_agent_without_workspace_default() {
    let mut f = load_agent_fixture(CODEX_ONLY_MANIFEST);
    load_role(
        &f.paths,
        &mut f.config,
        &f.selector,
        &f.workspace,
        &f.docker,
        &mut f.runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    let run_cmd = f
        .runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .expect("role docker run should fire for single-agent role");
    let last_positional = run_cmd
        .split_whitespace()
        .last()
        .expect("docker run command must have at least one argument");
    assert_eq!(
        last_positional, "codex",
        "single supported agent must become the initial runtime: {run_cmd}"
    );
}

#[tokio::test]
async fn load_agent_bails_when_multi_agent_choice_has_no_rich_dialog() {
    let mut f = load_agent_fixture(MULTI_AGENT_MANIFEST);
    let error = load_role(
        &f.paths,
        &mut f.config,
        &f.selector,
        &f.workspace,
        &f.docker,
        &mut f.runner,
        &LoadOptions::default(),
    )
    .await
    .expect_err("multi-agent role without resolution must not silently fall back");
    let rendered = format!("{error:#}");
    assert!(
        rendered.contains("agent-smith"),
        "error must name the role: {rendered}"
    );
    assert!(
        rendered.contains("pass --agent") || rendered.contains("default_agent"),
        "error must name the operator-actionable fix: {rendered}"
    );
}

#[tokio::test]
async fn load_agent_bails_when_sensitive_mount_has_no_rich_dialog() {
    let mut f = load_agent_fixture(CODEX_ONLY_MANIFEST);
    f.workspace.mounts.push(jackin_config::MountConfig {
        src: "/home/operator/.ssh".to_owned(),
        dst: "/host/ssh".to_owned(),
        readonly: true,
        isolation: MountIsolation::Shared,
    });

    let error = load_role(
        &f.paths,
        &mut f.config,
        &f.selector,
        &f.workspace,
        &f.docker,
        &mut f.runner,
        &LoadOptions::default(),
    )
    .await
    .expect_err("sensitive mount confirmation must require the rich launch dialog");
    let rendered = format!("{error:#}");
    assert!(
        rendered.contains("sensitive mount confirmation requires the rich launch dialog"),
        "error should explain the rich dialog requirement: {rendered}"
    );
}

#[tokio::test]
async fn load_agent_bails_when_manifest_declares_no_supported_agents() {
    let mut f = load_agent_fixture(
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = []
"#,
    );
    let error = load_role(
        &f.paths,
        &mut f.config,
        &f.selector,
        &f.workspace,
        &f.docker,
        &mut f.runner,
        &LoadOptions::default(),
    )
    .await
    .expect_err("role manifest with no agents must fail load");
    let rendered = format!("{error:#}");
    // Manifest validation rejects `agents = []` before reaching the
    // launch-time bail. The defensive bail at the resolve site is
    // unreachable in practice — pinned here so a future refactor
    // that loosens manifest validation still surfaces the same
    // operator-facing failure.
    assert!(
        rendered.contains("agents") && rendered.contains("empty"),
        "error must name the empty-agents condition: {rendered}"
    );
}

struct ConsoleResolutionFixture {
    _temp: tempfile::TempDir,
    paths: JackinPaths,
    selector: RoleSelector,
    repo_dir: PathBuf,
    config: AppConfig,
    runner: FakeRunner,
}

const MULTI_AGENT_MANIFEST: &str = r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["claude", "codex"]

[claude]
plugins = []

[codex]
"#;
const CODEX_ONLY_MANIFEST: &str = r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
"#;

fn console_resolution_fixture() -> ConsoleResolutionFixture {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    let mut config = AppConfig::default();
    config.roles.insert(
        "agent-smith".to_owned(),
        jackin_config::RoleSource {
            git: "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    ConsoleResolutionFixture {
        _temp: temp,
        paths,
        selector,
        repo_dir,
        config,
        runner: FakeRunner::default(),
    }
}

fn write_role_repo(repo_dir: &Path, manifest: &str) {
    std::fs::create_dir_all(repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(repo_dir.join("jackin.role.toml"), manifest).unwrap();
}

fn seed_cached_repo(repo_dir: &Path, manifest: &str) {
    write_role_repo(repo_dir, manifest);
    std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
}

fn materialize_on_clone(runner: &mut FakeRunner, repo_dir: PathBuf, manifest: String) {
    runner.side_effects.push((
        "clone".to_owned(),
        Box::new(move || {
            write_role_repo(&repo_dir, &manifest);
            std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        }),
    ));
}

#[tokio::test]
async fn console_agent_resolution_fast_paths_cached_manifest() {
    // When the role repo + manifest are already on disk, the
    // console must skip git entirely — the actual launch path
    // re-fetches and re-validates anyway.
    let mut f = console_resolution_fixture();
    seed_cached_repo(&f.repo_dir, MULTI_AGENT_MANIFEST);

    let agents =
        resolve_supported_agents_for_console(&f.paths, &f.config, &f.selector, &mut f.runner)
            .await
            .unwrap();

    assert_eq!(
        agents,
        vec![jackin_core::Agent::Claude, jackin_core::Agent::Codex]
    );
    assert!(
        f.runner.recorded.is_empty(),
        "fast path must not invoke any git command: {:?}",
        f.runner.recorded
    );
}

#[tokio::test]
async fn console_agent_resolution_falls_through_when_manifest_present_but_git_absent() {
    // Orphan manifest (jackin.role.toml without `.git/`) must not
    // be trusted as a cache hit — the `.git/` guard forces a fresh
    // clone so half-cleaned caches never serve stale data.
    let mut f = console_resolution_fixture();
    write_role_repo(&f.repo_dir, CODEX_ONLY_MANIFEST);
    // Deliberately no `.git/` directory.
    let materialize_dir = f.repo_dir.clone();
    f.runner.side_effects.push((
        "clone".to_owned(),
        Box::new(move || {
            std::fs::create_dir_all(materialize_dir.join(".git")).unwrap();
        }),
    ));

    let _unused =
        resolve_supported_agents_for_console(&f.paths, &f.config, &f.selector, &mut f.runner)
            .await
            .unwrap();

    assert!(
        f.runner
            .run_recorded
            .iter()
            .any(|c| c.contains("git clone")),
        "orphan manifest must trigger a fresh clone, not a cache hit: {:?}",
        f.runner.run_recorded
    );
}

#[tokio::test]
async fn console_agent_resolution_falls_through_when_cached_manifest_unparseable() {
    // `.git/` present but manifest body cannot be parsed →
    // fast-path must defer to the real fetch instead of returning
    // a stale or partial agent list. The downstream fetch itself
    // may legitimately fail in the test harness; what matters is
    // that the runner is invoked at all.
    let mut f = console_resolution_fixture();
    std::fs::create_dir_all(f.repo_dir.join(".git")).unwrap();
    std::fs::write(f.repo_dir.join("jackin.role.toml"), "this is not toml = =").unwrap();

    let _unused =
        resolve_supported_agents_for_console(&f.paths, &f.config, &f.selector, &mut f.runner).await;

    assert!(
        !f.runner.recorded.is_empty(),
        "unparseable cached manifest must trigger fall-through to git: {:?}",
        f.runner.recorded
    );
}

#[tokio::test]
async fn console_agent_resolution_falls_through_to_git_when_uncached() {
    // No cached repo on disk → must fetch via non-interactive git
    // (null stdin, GIT_TERMINAL_PROMPT=0, quiet) so a hanging
    // credential helper cannot freeze the TUI.
    let mut f = console_resolution_fixture();
    materialize_on_clone(
        &mut f.runner,
        f.repo_dir.clone(),
        MULTI_AGENT_MANIFEST.to_owned(),
    );

    let agents =
        resolve_supported_agents_for_console(&f.paths, &f.config, &f.selector, &mut f.runner)
            .await
            .unwrap();

    assert_eq!(
        agents,
        vec![jackin_core::Agent::Claude, jackin_core::Agent::Codex]
    );
    assert!(
        f.runner
            .run_recorded
            .iter()
            .any(|c| c.contains("git clone")),
        "fall-through path must clone: {:?}",
        f.runner.run_recorded
    );
    assert!(
        !f.runner.run_options.is_empty(),
        "fall-through must record git RunOptions"
    );
    assert!(
        f.runner
            .run_options
            .iter()
            .all(|opts| opts.quiet && !opts.capture_stderr),
        "console role resolution must not stream git output over the TUI"
    );
    assert!(
        f.runner.run_options.iter().all(|opts| opts.null_stdin
            && opts
                .extra_env
                .contains(&("GIT_TERMINAL_PROMPT".to_owned(), "0".to_owned()))),
        "console role resolution must make git non-interactive"
    );
}

#[tokio::test]
async fn console_agent_resolution_propagates_git_failure() {
    let mut f = console_resolution_fixture();
    f.runner.fail_with.push((
        "git clone".to_owned(),
        "Could not resolve host: github.com".to_owned(),
    ));

    let error =
        resolve_supported_agents_for_console(&f.paths, &f.config, &f.selector, &mut f.runner)
            .await
            .expect_err("git clone failure must surface to caller");
    let rendered = format!("{error:#}");
    assert!(
        rendered.contains("Could not resolve host"),
        "wrapped error must preserve git failure cause: {rendered}"
    );
}

#[tokio::test]
async fn load_agent_uses_resolved_workspace_mounts_and_workdir() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
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
plugins = []
"#,
    )
    .unwrap();

    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir_all(&workspace_dir).unwrap();
    let workspace = jackin_config::ResolvedWorkspace {
        name: String::new(),
        label: workspace_dir.display().to_string(),
        workdir: workspace_dir.display().to_string(),
        mounts: vec![jackin_config::MountConfig {
            src: workspace_dir.display().to_string(),
            dst: workspace_dir.display().to_string(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
    };

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

    let run_call = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    assert!(run_call.contains(&format!("--workdir {}", workspace.workdir)));
    assert!(run_call.contains(&format!(
        "{}:{}",
        workspace_dir.display(),
        workspace_dir.display()
    )));
    assert!(!run_call.contains(&format!("{}:/workspace", repo_dir.display())));
}

#[tokio::test]
async fn load_agent_bakes_host_uid_not_gid_into_docker_build() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
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
plugins = []
"#,
    )
    .unwrap();

    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir_all(&workspace_dir).unwrap();
    let workspace = jackin_config::ResolvedWorkspace {
        name: String::new(),
        label: workspace_dir.display().to_string(),
        workdir: workspace_dir.display().to_string(),
        mounts: vec![jackin_config::MountConfig {
            src: workspace_dir.display().to_string(),
            dst: workspace_dir.display().to_string(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
    };

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

    let build_call = runner
        .recorded
        .iter()
        .find(|call| {
            call.contains("buildx build ")
                && call.contains("DerivedDockerfile")
                && call.contains("--output type=docker,name=jk_agent-smith")
        })
        .unwrap();
    assert!(build_call.contains("--build-arg JACKIN_RUN_UID="));
    assert!(!build_call.contains("--build-arg JACKIN_HOST_UID="));
    assert!(!build_call.contains("--build-arg JACKIN_HOST_GID="));
    assert!(!build_call.contains("--build-arg ROLE_GIT_SHA="));
    // The host-identity strategy is now folded into the master recipe hash
    // (no standalone label); its presence proves the recipe was stamped.
    assert!(build_call.contains("--label jackin.image.recipe.hash="));
    let recorded = runner.recorded.join("\n");
    assert!(
        !recorded.contains("gh auth token"),
        "Dockerfiles without id=github_token must skip build-token lookup; recorded:\n{recorded}"
    );
    assert!(
        !build_call.contains("--secret") && !build_call.contains("id=github_token"),
        "Dockerfiles without id=github_token must not inject a BuildKit secret; got:\n{build_call}"
    );
    assert!(!recorded.contains("id -u"));
    assert!(!recorded.contains("id -g"));

    let build_run_index = runner
        .run_recorded
        .iter()
        .position(|call| call.contains("buildx build ") && call.contains("DerivedDockerfile"))
        .unwrap();
    let build_opts = &runner.run_options[build_run_index];
    assert!(build_opts.capture_stdout);
    assert!(build_opts.capture_stderr);
    assert!(build_opts.null_stdin);
    assert!(build_opts.tee_to_build_log);
    assert!(
        build_opts
            .extra_env
            .contains(&("BUILDKIT_PROGRESS".to_owned(), "plain".to_owned()))
    );
    assert!(
        build_opts
            .extra_env
            .contains(&("DOCKER_BUILDKIT".to_owned(), "1".to_owned())),
        "Docker builds must use BuildKit even when no GitHub token secret is requested"
    );

    let run_call = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    if let Some(run_as_user) = crate::runtime::identity::host_run_as_user() {
        assert!(
            run_call.contains(&format!("--user {run_as_user} --group-add 0")),
            "role docker run must use host UID/GID plus supplementary group 0: {run_call}"
        );
        assert!(
            run_call.contains("/var/lib/extrausers/passwd:ro"),
            "role docker run must mount runtime passwd entry: {run_call}"
        );
        assert!(
            run_call.contains("/var/lib/extrausers/group:ro"),
            "role docker run must mount runtime group entry: {run_call}"
        );

        let passwd = std::fs::read_to_string(paths.jackin_home.join("extrausers/passwd")).unwrap();
        let group = std::fs::read_to_string(paths.jackin_home.join("extrausers/group")).unwrap();
        let (uid, gid) = run_as_user.split_once(':').unwrap();
        assert_eq!(
            passwd,
            format!("agent:x:{uid}:{gid}:agent:/home/agent:/bin/zsh\n")
        );
        assert_eq!(group, format!("agent-host:x:{gid}:agent\n"));
    }
}

#[tokio::test]
async fn load_agent_tags_fresh_published_image_as_local_base() {
    // A fresh published image is pulled, verified by Docker image labels, and
    // tagged into the local jk_<role>__base name. The overlay derives FROM that
    // local base without running a restamp Docker build.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let role_sha = "21a9002";
    let mut runner = FakeRunner::for_load_agent([role_sha.to_owned()]);

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
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    docker.inspect_image_labels_queue.borrow_mut().push_back(
        [
            (
                crate::runtime::naming::LABEL_IMAGE_ROLE_GIT_SHA.to_owned(),
                role_sha.to_owned(),
            ),
            (
                crate::runtime::naming::LABEL_IMAGE_CONSTRUCT_VERSION.to_owned(),
                "0.1-trixie".to_owned(),
            ),
        ]
        .into(),
    );
    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&repo_dir),
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    // The base is a local tag of the already verified published image.
    let base_tag = runner
        .recorded
        .iter()
        .find(|c| c.contains("docker tag docker.io/myorg/my-role:latest jk_agent-smith__base"))
        .expect("fresh published image must be tagged into a local base");
    assert!(
        base_tag.ends_with(&format!(":{role_sha}")),
        "base tag must use the role SHA; got: {base_tag}"
    );
    assert!(
        !runner
            .recorded
            .iter()
            .any(|c| c.contains("buildx build ") && c.contains("BaseDockerfile")),
        "fresh published images must not be restamped through a Docker build"
    );
    // The overlay derives FROM that local base, not the published image.
    assert!(
        runner
            .recorded
            .iter()
            .any(|c| c.contains("buildx build ") && c.contains("DerivedDockerfile")),
        "overlay must derive FROM the local base"
    );
}

#[tokio::test]
async fn load_agent_builds_local_role_base_then_derives_overlay_from_it() {
    // The workspace build is two-stage: first a role *base* image
    // (jk_<role>__base, the role Dockerfile, no overlay), then the derived image
    // (FROM that base + jackin overlay). The base carries the role-sha + construct
    // labels so it can be reused across overlay rebuilds.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
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

[claude]
plugins = []
"#,
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&repo_dir),
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    // Stage 1: the role base build.
    let base_build = runner
        .recorded
        .iter()
        .find(|c| c.contains("buildx build ") && c.contains("BaseDockerfile"))
        .expect("workspace build must first build the role base image");
    assert!(
        base_build.contains("--output type=docker,name=jk_agent-smith__base")
            && base_build.contains("compression=uncompressed"),
        "base build must load uncompressed jk_<role>__base; got: {base_build}"
    );
    assert!(
        base_build.contains("--builder default"),
        "base build consumes local images and must use the Docker-driver builder; got: {base_build}"
    );
    assert!(
        base_build.contains("docker --context default buildx build"),
        "base build must select the default Docker context for the default builder; got: {base_build}"
    );
    assert!(
        base_build.contains("--label jackin.construct.image=")
            && base_build.contains("--label jackin.role.git.sha="),
        "base build must stamp construct + role-sha labels for reuse; got: {base_build}"
    );
    assert!(
        !base_build.contains("DerivedDockerfile"),
        "base build must not include the jackin overlay; got: {base_build}"
    );

    // Stage 2: the derived overlay build, FROM the local base (not the construct).
    let derived_build = runner
        .recorded
        .iter()
        .find(|c| c.contains("buildx build ") && c.contains("DerivedDockerfile"))
        .expect("workspace build must derive the overlay after the base");
    assert!(
        !derived_build.contains("--pull"),
        "derived build is FROM a local base and must never --pull; got: {derived_build}"
    );
    assert!(
        derived_build.contains("--builder default"),
        "derived build consumes the local role base and must use the Docker-driver builder; got: {derived_build}"
    );
    assert!(
        derived_build.contains("docker --context default buildx build"),
        "derived build must select the default Docker context for the default builder; got: {derived_build}"
    );
    assert!(
        derived_build.contains("--label jackin.image.recipe.hash="),
        "derived build stamps the recipe labels; got: {derived_build}"
    );
}

#[tokio::test]
async fn load_agent_omits_pull_flag_in_normal_workspace_build() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
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

[claude]
plugins = []
"#,
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&repo_dir),
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    let build_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("buildx build ") && call.contains("DerivedDockerfile"))
        .unwrap();
    assert!(
        !build_cmd.contains("--pull"),
        "workspace mode without --rebuild must not pass --pull"
    );
    assert!(
        build_cmd.contains("--label jackin.image.recipe.version=v9"),
        "workspace build must stamp recipe version label; got: {build_cmd}"
    );
    assert!(
        build_cmd.contains("--label jackin.image.recipe.hash="),
        "workspace build must stamp recipe hash label; got: {build_cmd}"
    );
    // Agent-independence is now captured inside the recipe hash (the
    // supported-agent set is a recipe input) rather than a standalone label.
    assert!(
        build_cmd.contains("--label jackin.manifest.version="),
        "workspace build must stamp the manifest version label; got: {build_cmd}"
    );
}

#[tokio::test]
async fn load_agent_cleans_up_sidecar_when_derived_build_fails() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([String::new()]);
    runner.fail_with.push((
        "buildx build ".to_owned(),
        "derived build failed".to_owned(),
    ));

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
plugins = []
"#,
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    let error = load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&repo_dir),
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap_err();

    assert!(
        error.to_string().contains("derived build failed"),
        "unexpected error: {error:#}"
    );
    let docker_recorded = docker.recorded.borrow();
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.starts_with("docker rm -f jk-") && call.ends_with("-dind")),
        "DinD cleanup missing after build failure: {docker_recorded:?}"
    );
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.starts_with("docker volume rm jk-")),
        "cert volume cleanup missing after build failure: {docker_recorded:?}"
    );
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.starts_with("docker network rm jk-")),
        "network cleanup missing after build failure: {docker_recorded:?}"
    );
}

#[tokio::test]
async fn load_agent_reuses_valid_local_image_and_skips_build_work() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, Some("abc123"));
    let local_base = local_role_base_for_test(&selector, Some("abc123"));
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        Some(local_base.as_str()),
        "0",
    );
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        cached_repo.repo_dir.join("Dockerfile"),
        cached_repo.repo_dir.join("context-copy-poison"),
    )
    .unwrap();
    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    runner.fail_on = vec![
        "buildx build ".to_owned(),
        "gh auth token".to_owned(),
        "docker run --rm --entrypoint".to_owned(),
        "agent_binary".to_owned(),
    ];

    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    let recorded = runner.recorded.join("\n");
    assert!(
        !recorded.contains("buildx build "),
        "valid local recipe must skip docker build; recorded:\n{recorded}"
    );
    assert!(
        !recorded.contains("gh auth token"),
        "valid local recipe must skip GitHub token lookup; recorded:\n{recorded}"
    );
    assert!(
        !recorded.contains("docker run --rm --entrypoint"),
        "valid local recipe must skip foreground agent version probe; recorded:\n{recorded}"
    );
    assert!(
        !recorded.contains("agent_binary_resolve_started"),
        "valid local recipe must skip runtime binary preparation; recorded:\n{recorded}"
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call == &format!("docker inspect image:{image}")),
        "valid local image must still be inspected for recipe labels"
    );
}

#[tokio::test]
async fn load_agent_refresh_background_reuses_valid_local_image_and_skips_build_work() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, Some("abc123"));
    let local_base = local_role_base_for_test(&selector, Some("abc123"));
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        Some(local_base.as_str()),
        "0",
    );
    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    runner.fail_on = vec![
        "buildx build ".to_owned(),
        "gh auth token".to_owned(),
        "docker run --rm --entrypoint".to_owned(),
        "agent_binary".to_owned(),
    ];

    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    let recorded = runner.recorded.join("\n");
    assert!(
        !recorded.contains("buildx build "),
        "refresh-background decision must skip docker build; recorded:\n{recorded}"
    );
    assert!(
        !recorded.contains("gh auth token"),
        "reuse decision must skip GitHub token lookup; recorded:\n{recorded}"
    );
    assert!(
        !recorded.contains("docker run --rm --entrypoint"),
        "reuse decision must skip foreground version probe; recorded:\n{recorded}"
    );
    assert!(
        !recorded.contains("agent_binary_resolve_started"),
        "reuse decision must skip runtime binary preparation; recorded:\n{recorded}"
    );

    let docker_recorded = docker.recorded.borrow();
    assert!(
        !docker_recorded
            .iter()
            .any(|call| call == "docker pull docker.io/myorg/my-role:latest"),
        "reuse decision must not check published image freshness in the foreground: {docker_recorded:?}"
    );
    assert!(
        docker_recorded
            .iter()
            .any(|call| call == &format!("docker inspect image:{image}")),
        "reuse decision must inspect valid local recipe labels: {docker_recorded:?}"
    );
}

#[tokio::test]
async fn valid_image_decision_runs_before_operator_env_resolution() {
    struct FailingOpRunner;

    impl jackin_env::OpRunner for FailingOpRunner {
        fn read(&self, _reference: &str) -> anyhow::Result<String> {
            anyhow::bail!("operator env read intentionally failed")
        }

        fn probe(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    config.env.insert(
        "OPERATOR_IMAGE_ORDER".to_owned(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://vault/item/field".to_owned(),
            path: "Vault/Item/Field".to_owned(),
            account: None,
            on_demand: false,
        }),
    );
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, Some("abc123"));
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    let opts = LoadOptions {
        op_runner: Some(Box::new(FailingOpRunner)),
        ..LoadOptions::default()
    };

    let error = load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .unwrap_err();

    assert!(
        error.to_string().contains("operator env resolution failed"),
        "expected operator env failure after image decision, got {error:#}"
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call == &format!("docker inspect image:{image}")),
        "valid image must be inspected before operator env can fail"
    );
}

#[tokio::test]
async fn stale_agent_version_cache_does_not_force_foreground_update_probe() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, Some("abc123"));
    jackin_image::version_check::store_cache_bust(&paths, &image, "stored-bust");
    jackin_image::version_check::store_version(&paths, agent, &image, "1.0.0");
    let latest = jackin_image::agent_binary::AgentRelease {
        agent,
        version: "2.0.0".to_owned(),
        url: "https://example.invalid/claude".to_owned(),
        checksum: None,
        archive_member: None,
    };
    let latest_path = paths
        .cache_dir
        .join("agent-binaries")
        .join(agent.slug())
        .join("latest.json");
    std::fs::create_dir_all(latest_path.parent().unwrap()).unwrap();
    std::fs::write(latest_path, serde_json::to_string(&latest).unwrap()).unwrap();
    let stale_labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("oldsha"),
        None,
        None,
        "stored-bust",
    );
    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(stale_labels);
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);

    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    let build_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("buildx build ") && call.contains("DerivedDockerfile"))
        .expect("stale role SHA must trigger a derived image rebuild");
    assert!(
        build_cmd.contains("--build-arg JACKIN_CACHE_BUST=stored-bust"),
        "normal rebuild path must not run latest-release update probe and mint a fresh cache bust; got: {build_cmd}"
    );
}

#[tokio::test]
async fn load_agent_cleans_up_when_parallel_sidecar_start_fails() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, None);
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let mut docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    docker.fail_with = vec![(
        "create_container:".to_owned(),
        "dind create failed".to_owned(),
    )];
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);

    let error = load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &compat_dind_load_options(),
    )
    .await
    .unwrap_err();

    assert!(
        error.to_string().contains("dind create failed"),
        "unexpected error: {error:#}"
    );
    let docker_recorded = docker.recorded.borrow();
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.starts_with("docker rm -f jk-") && !call.ends_with("-dind")),
        "role container cleanup missing after sidecar failure: {docker_recorded:?}"
    );
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.starts_with("docker rm -f jk-") && call.ends_with("-dind")),
        "DinD cleanup missing after sidecar failure: {docker_recorded:?}"
    );
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.starts_with("docker volume rm jk-")),
        "cert volume cleanup missing after sidecar failure: {docker_recorded:?}"
    );
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.starts_with("docker network rm jk-")),
        "network cleanup missing after sidecar failure: {docker_recorded:?}"
    );
}

#[tokio::test]
async fn load_agent_skips_operator_env_resolution_when_no_env_layers_apply() {
    struct FailingOpRunner;

    impl jackin_env::OpRunner for FailingOpRunner {
        fn read(&self, _reference: &str) -> anyhow::Result<String> {
            anyhow::bail!("operator env should not be resolved")
        }

        fn probe(&self) -> anyhow::Result<()> {
            anyhow::bail!("operator env should not probe op")
        }
    }

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, None);
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    let opts = LoadOptions {
        agent: Some(agent),
        op_runner: Some(Box::new(FailingOpRunner)),
        ..LoadOptions::default()
    };

    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn load_agent_skips_non_required_operator_credential_refs() {
    struct FailingCredentialOpRunner;

    impl jackin_env::OpRunner for FailingCredentialOpRunner {
        fn read(&self, reference: &str) -> anyhow::Result<String> {
            anyhow::bail!("non-required credential ref should not be resolved: {reference}")
        }

        fn probe(&self) -> anyhow::Result<()> {
            anyhow::bail!("non-required credential refs should not probe op")
        }
    }

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    config.env.insert(
        "OPENAI_API_KEY".to_owned(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://vault/openai/key".to_owned(),
            path: "Vault/OpenAI/key".to_owned(),
            account: None,
            on_demand: false,
        }),
    );
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, None);
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    let opts = LoadOptions {
        agent: Some(agent),
        op_runner: Some(Box::new(FailingCredentialOpRunner)),
        ..LoadOptions::default()
    };

    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn load_agent_skips_non_required_manifest_credential_prompts() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[env.OPENAI_API_KEY]
interactive = true
prompt = "Codex API key"

[claude]
plugins = []
"#,
    )
    .unwrap();
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, None);
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    let opts = LoadOptions {
        agent: Some(agent),
        ..LoadOptions::default()
    };

    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .unwrap();

    assert!(
        runner
            .recorded
            .iter()
            .filter(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
            .all(|call| !call.contains("OPENAI_API_KEY")),
        "non-selected manifest credential leaked into docker run: {:?}",
        runner.recorded
    );
}

#[test]
fn credential_key_filter_resolves_every_supported_agent_credential() {
    use jackin_core::Agent;

    // Generic operator/manifest vars always resolve.
    assert!(launch_pipeline::credential_key_needed_for_role(
        &[Agent::Codex],
        "OPERATOR_SMOKE"
    ));
    // A credential for an agent the role cannot run stays lazy: launching a
    // Claude-only role never needs Codex's OPENAI_API_KEY.
    assert!(!launch_pipeline::credential_key_needed_for_role(
        &[Agent::Claude],
        "OPENAI_API_KEY"
    ));
    // A supported agent's own credential resolves even when that agent's
    // resolved mode (e.g. Sync) would not read it — the operator declared it
    // and an ApiKey tab for that agent must find it in the container env.
    assert!(launch_pipeline::credential_key_needed_for_role(
        &[Agent::Codex],
        "OPENAI_API_KEY"
    ));
    // Multi-agent role: every supported agent's credential resolves so any tab
    // can authenticate, not just the initially-selected agent.
    assert!(launch_pipeline::credential_key_needed_for_role(
        &[Agent::Claude, Agent::Codex],
        "OPENAI_API_KEY"
    ));
    assert!(launch_pipeline::credential_key_needed_for_role(
        &[Agent::Claude, Agent::Codex],
        "ANTHROPIC_API_KEY"
    ));
    // Empty slice: no agent can run, so known credential keys are skipped;
    // generic/unknown keys still pass through.
    assert!(!launch_pipeline::credential_key_needed_for_role(
        &[],
        "ANTHROPIC_API_KEY"
    ));
    assert!(launch_pipeline::credential_key_needed_for_role(
        &[],
        "MY_CUSTOM_VAR"
    ));
}

#[test]
fn manifest_env_timing_detail_distinguishes_skips_from_empty_results() {
    assert_eq!(manifest_env_timing_detail(true, 0), "skipped");
    assert_eq!(manifest_env_timing_detail(false, 0), "0 vars");
    assert_eq!(manifest_env_timing_detail(false, 2), "2 vars");
}

#[tokio::test]
async fn load_agent_skips_github_env_resolution_when_github_auth_ignored() {
    struct FailingGithubOpRunner;

    impl jackin_env::OpRunner for FailingGithubOpRunner {
        fn read(&self, _reference: &str) -> anyhow::Result<String> {
            anyhow::bail!("ignored github env should not be resolved")
        }
    }

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let mut github_env = std::collections::BTreeMap::new();
    github_env.insert(
        jackin_core::GH_TOKEN_ENV_NAME.to_owned(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://vault/github/token".to_owned(),
            path: "Vault/GitHub/token".to_owned(),
            account: None,
            on_demand: false,
        }),
    );
    config.github = Some(jackin_config::GithubAuthConfig {
        auth_forward: jackin_config::GithubAuthMode::Ignore,
        env: github_env,
    });
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, None);
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    let opts = LoadOptions {
        agent: Some(agent),
        op_runner: Some(Box::new(FailingGithubOpRunner)),
        ..LoadOptions::default()
    };

    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn load_agent_skips_unused_github_env_resolution() {
    struct FailingUnusedGithubOpRunner;

    impl jackin_env::OpRunner for FailingUnusedGithubOpRunner {
        fn read(&self, reference: &str) -> anyhow::Result<String> {
            anyhow::bail!("unused github env ref should not be resolved: {reference}")
        }
    }

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let mut github_env = std::collections::BTreeMap::new();
    github_env.insert(
        jackin_core::GH_TOKEN_ENV_NAME.to_owned(),
        jackin_core::EnvValue::Plain("ghp_test".to_owned()),
    );
    github_env.insert(
        "UNUSED_GITHUB_SECRET".to_owned(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://vault/github/unused".to_owned(),
            path: "Vault/GitHub/unused".to_owned(),
            account: None,
            on_demand: false,
        }),
    );
    config.github = Some(jackin_config::GithubAuthConfig {
        auth_forward: jackin_config::GithubAuthMode::Token,
        env: github_env,
    });
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, None);
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    let opts = LoadOptions {
        agent: Some(agent),
        op_runner: Some(Box::new(FailingUnusedGithubOpRunner)),
        ..LoadOptions::default()
    };

    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .unwrap();

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    assert!(
        run_cmd.contains("-e GH_TOKEN=ghp_test"),
        "required GitHub token must still inject; got: {run_cmd}"
    );
    assert!(
        !run_cmd.contains("UNUSED_GITHUB_SECRET"),
        "unused GitHub env keys are not runtime env; got: {run_cmd}"
    );
}

#[tokio::test]
async fn load_agent_rebuild_token_preflight_failure_tears_down_adopted_dind() {
    // Regression for the adopted-prewarm-DinD leak: `adopt_prewarmed_dind_sidecar`
    // takes over a *running* prewarmed DinD container/network/volume and deletes
    // its on-disk state, so nothing re-adopts it. A fallible preflight after
    // adoption (here Token-mode GitHub auth with no resolvable token) must tear
    // those resources down rather than orphan a live privileged container.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    // Token mode with an empty env => GH_TOKEN resolves to None =>
    // `verify_github_token_present` fails, after adoption.
    config.github = Some(jackin_config::GithubAuthConfig {
        auth_forward: jackin_config::GithubAuthMode::Token,
        env: std::collections::BTreeMap::new(),
    });

    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, None);
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        None,
        "0",
    );

    // Seed a kept, running prewarmed DinD so the launch adopts it.
    let prewarm_dind = "jk-prewarm-b4-dind";
    let prewarm_net = "jk-prewarm-b4-net";
    let prewarm_certs = "jk-prewarm-b4-certs";
    write_prewarmed_dind_state(
        &paths,
        &DindSidecarPrewarm {
            dind: prewarm_dind.to_owned(),
            network: prewarm_net.to_owned(),
            certs_volume: prewarm_certs.to_owned(),
            ready_ms: 12,
            kept: true,
        },
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    // Image-reuse path (no build needed to reach adoption).
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    // Adoption: pin the prewarmed dind to Running by name (the restore/claim
    // inspects that run first hit the default NotFound), and give its network
    // the prewarm labels so adoption accepts it.
    docker
        .inspect_state_by_name
        .borrow_mut()
        .insert(prewarm_dind.to_owned(), ContainerState::Running);
    let mut network_labels = HashMap::new();
    network_labels.insert("jackin.kind".to_owned(), "prewarm-dind".to_owned());
    network_labels.insert("jackin.prewarm".to_owned(), "true".to_owned());
    docker.inspect_network_queue.borrow_mut().push_back(Some(
        jackin_docker::docker_client::NetworkRow {
            name: prewarm_net.to_owned(),
            labels: network_labels,
        },
    ));
    docker
        .exec_capture_queue
        .borrow_mut()
        .push_back(String::new());
    docker
        .exec_capture_queue
        .borrow_mut()
        .push_back(String::new());

    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    let opts = LoadOptions {
        agent: Some(agent),
        ..LoadOptions::default()
    };

    let result = load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &opts,
    )
    .await;

    result.expect_err("missing Token-mode GitHub token must fail the launch");
    let recorded = docker.recorded.borrow();
    assert!(
        recorded
            .iter()
            .any(|call| call == &format!("docker rm -f {prewarm_dind}")),
        "adopted prewarm DinD must be torn down on post-adoption failure; recorded: {recorded:?}"
    );
    assert!(
        recorded
            .iter()
            .any(|call| call == &format!("docker network rm {prewarm_net}")),
        "adopted prewarm network must be torn down; recorded: {recorded:?}"
    );
}

#[tokio::test]
async fn load_agent_grant_validation_failure_tears_down_adopted_dind() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    config.docker.grants = Some(jackin_core::DockerGrants {
        user: Some("root".to_owned()),
        sudo: Some(true),
        ..Default::default()
    });

    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, None);
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        None,
        "0",
    );

    let prewarm_dind = "jk-prewarm-grants-dind";
    let prewarm_net = "jk-prewarm-grants-net";
    let prewarm_certs = "jk-prewarm-grants-certs";
    write_prewarmed_dind_state(
        &paths,
        &DindSidecarPrewarm {
            dind: prewarm_dind.to_owned(),
            network: prewarm_net.to_owned(),
            certs_volume: prewarm_certs.to_owned(),
            ready_ms: 12,
            kept: true,
        },
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    docker
        .inspect_state_by_name
        .borrow_mut()
        .insert(prewarm_dind.to_owned(), ContainerState::Running);
    let mut network_labels = HashMap::new();
    network_labels.insert("jackin.kind".to_owned(), "prewarm-dind".to_owned());
    network_labels.insert("jackin.prewarm".to_owned(), "true".to_owned());
    docker.inspect_network_queue.borrow_mut().push_back(Some(
        jackin_docker::docker_client::NetworkRow {
            name: prewarm_net.to_owned(),
            labels: network_labels,
        },
    ));
    docker
        .exec_capture_queue
        .borrow_mut()
        .push_back(String::new());
    docker
        .exec_capture_queue
        .borrow_mut()
        .push_back(String::new());

    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    let opts = LoadOptions {
        agent: Some(agent),
        ..LoadOptions::default()
    };

    let result = load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &opts,
    )
    .await;

    let error = result.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("docker grants validation failed"),
        "unexpected error: {error:#}"
    );
    let recorded = docker.recorded.borrow();
    assert!(
        recorded
            .iter()
            .any(|call| call == &format!("docker rm -f {prewarm_dind}")),
        "adopted prewarm DinD must be torn down after grant validation failure; recorded: {recorded:?}"
    );
    assert!(
        recorded
            .iter()
            .any(|call| call == &format!("docker volume rm {prewarm_certs}")),
        "adopted prewarm cert volume must be torn down after grant validation failure; recorded: {recorded:?}"
    );
    assert!(
        recorded
            .iter()
            .any(|call| call == &format!("docker network rm {prewarm_net}")),
        "adopted prewarm network must be torn down after grant validation failure; recorded: {recorded:?}"
    );
}

#[tokio::test]
async fn load_agent_does_not_short_circuit_on_running_instance() {
    // D13 reversal of PR #576: launch must NOT auto-attach to a live container.
    // The full build pipeline must run (`docker build`) even when a current-role
    // container is Running. Two inspect entries: the early-attach probe and the
    // in-pipeline probe both see Running and both reject it.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    std::fs::create_dir_all(&cached_repo.repo_dir).unwrap();
    std::fs::write(
        cached_repo.repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        "version = \"v1alpha3\"\ndockerfile = \"Dockerfile\"\n\n[claude]\nmodel = \"sonnet\"\n",
    )
    .unwrap();
    config.workspaces.insert(
        "workspace".to_owned(),
        jackin_config::WorkspaceConfig {
            workdir: "/workspace".to_owned(),
            default_agent: Some(jackin_core::Agent::Claude),
            ..jackin_config::WorkspaceConfig::default()
        },
    );
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let mut manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Claude,
    );
    manifest.mark_status(InstanceStatus::Running);
    write_indexed_manifest(&paths, &manifest);
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Running,
            ContainerState::Running,
            ContainerState::Running,
        ])),
        ..Default::default()
    };
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
    ]);
    let mut workspace = repo_workspace(&cached_repo.repo_dir);
    workspace.label = "workspace".to_owned();
    workspace.default_agent = Some(jackin_core::Agent::Claude);
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

    let recorded = runner.recorded.join("\n");
    assert!(
        recorded.contains("buildx build "),
        "D13: build must run even when current-role container is running; recorded:\n{recorded}"
    );
    assert!(
        !recorded.starts_with(&format!("docker exec {container_name}")),
        "D13: launch must not auto-attach to running container; recorded:\n{recorded}"
    );
}

#[tokio::test]
async fn load_agent_attaches_explicit_restore_container_before_role_repo() {
    struct FailingOpRunner;

    impl jackin_env::OpRunner for FailingOpRunner {
        fn read(&self, _reference: &str) -> anyhow::Result<String> {
            anyhow::bail!("explicit restore path should not resolve operator env")
        }

        fn probe(&self) -> anyhow::Result<()> {
            anyhow::bail!("explicit restore path should not probe op")
        }
    }

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    config.env.insert(
        "OPERATOR_RESTORE_SECRET".to_owned(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://vault/item/restore-secret".to_owned(),
            path: "Vault/Item/restore secret".to_owned(),
            account: None,
            on_demand: false,
        }),
    );
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Running,
            ContainerState::Running,
        ])),
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from([
            String::new(),
            "Sessions: 1\n".to_owned(),
        ])),
        ..Default::default()
    };
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
    ]);
    let opts = LoadOptions {
        op_runner: Some(Box::new(FailingOpRunner)),
        restore_container_base: Some(container_name.to_owned()),
        role_branch: Some("restore-ref".to_owned()),
        ..LoadOptions::default()
    };

    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .unwrap();

    let recorded = runner.recorded.join("\n");
    assert!(
        recorded.contains("docker exec")
            && recorded.contains(container_name)
            && recorded.contains("jackin-capsule"),
        "explicit restore container must attach through Capsule; recorded:\n{recorded}"
    );
    for forbidden in [
        &cached_repo.repo_dir.display().to_string(),
        "buildx build ",
        "gh auth token",
        "docker inspect image:",
        "docker run --rm --entrypoint",
    ] {
        assert!(
            !recorded.contains(forbidden),
            "explicit restore path must skip {forbidden}; recorded:\n{recorded}"
        );
    }
}

#[tokio::test]
async fn load_agent_starts_stopped_current_instance_before_credentials_and_build() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    config.workspaces.insert(
        "workspace".to_owned(),
        jackin_config::WorkspaceConfig {
            workdir: "/workspace".to_owned(),
            default_agent: Some(jackin_core::Agent::Claude),
            ..jackin_config::WorkspaceConfig::default()
        },
    );
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let mut manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Claude,
    );
    manifest.mark_status(InstanceStatus::Running);
    write_indexed_manifest(&paths, &manifest);
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Stopped {
                exit_code: 137,
                oom_killed: false,
            },
            ContainerState::Stopped {
                exit_code: 137,
                oom_killed: false,
            },
            ContainerState::Running,
            ContainerState::Running,
        ])),
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from([
            String::new(),
            "Sessions: 1\n".to_owned(),
        ])),
        ..Default::default()
    };
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
    ]);
    let mut workspace = repo_workspace(&cached_repo.repo_dir);
    workspace.label = "workspace".to_owned();
    workspace.name = "workspace".to_owned();
    workspace.default_agent = Some(jackin_core::Agent::Claude);

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

    let docker_recorded = docker.recorded.borrow();
    assert!(
        docker_recorded
            .iter()
            .any(|call| call == &format!("start_container:{container_name}")),
        "stopped current-role instance must be started; recorded: {docker_recorded:?}"
    );
    let recorded = runner.recorded.join("\n");
    assert!(
        recorded.contains("docker exec")
            && recorded.contains(container_name)
            && recorded.contains("jackin-capsule"),
        "started current-role instance must attach through Capsule; recorded:\n{recorded}"
    );
    for forbidden in [
        "buildx build ",
        "gh auth token",
        "docker inspect image:",
        "docker run --rm --entrypoint",
    ] {
        assert!(
            !recorded.contains(forbidden),
            "stopped restore path must skip {forbidden}; recorded:\n{recorded}"
        );
    }
    assert!(
        !recorded.contains(&cached_repo.repo_dir.display().to_string()),
        "stopped restore path must not touch the cached role repo before hardline; recorded:\n{recorded}"
    );
}

#[tokio::test]
async fn load_agent_recreates_missing_current_instance_from_valid_image_without_build() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    config.workspaces.insert(
        "workspace".to_owned(),
        jackin_config::WorkspaceConfig {
            workdir: "/workspace".to_owned(),
            ..jackin_config::WorkspaceConfig::default()
        },
    );
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let mut manifest = workspace_manifest(container_name, "agent-smith", "Agent Smith", agent);
    manifest.mark_status(InstanceStatus::Running);
    write_indexed_manifest(&paths, &manifest);
    let image = crate::runtime::naming::image_name(&selector, None);
    let local_base = local_role_base_for_test(&selector, Some("abc123"));
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        Some(local_base.as_str()),
        "0",
    );
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::NotFound,
            ContainerState::NotFound,
        ])),
        list_image_tags_queue: std::cell::RefCell::new(VecDeque::from([vec![image.clone()]])),
        inspect_image_labels_queue: std::cell::RefCell::new(VecDeque::from([labels])),
        ..Default::default()
    };
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    let mut workspace = repo_workspace(&cached_repo.repo_dir);
    workspace.label = "workspace".to_owned();
    workspace.name = "workspace".to_owned();

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

    let recorded = runner.recorded.join("\n");
    assert!(
        recorded.contains("docker run -d")
            && recorded.contains(&format!("--name {container_name}"))
            && recorded.contains(&image),
        "valid-image recreate path must run the missing role container from the reusable image; recorded:\n{recorded}"
    );
    for forbidden in [
        "buildx build ",
        "gh auth token",
        "docker run --rm --entrypoint",
    ] {
        assert!(
            !recorded.contains(forbidden),
            "valid-image recreate path must skip {forbidden}; recorded:\n{recorded}"
        );
    }
}

#[tokio::test]
async fn load_agent_passes_pull_flag_when_rebuild() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
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

[claude]
plugins = []
"#,
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&repo_dir),
        &docker,
        &mut runner,
        &LoadOptions {
            rebuild: true,
            ..LoadOptions::default()
        },
    )
    .await
    .unwrap();

    let build_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("buildx build "))
        .unwrap();
    assert!(
        build_cmd.contains("--pull"),
        "--rebuild must pass --pull to refresh the base image"
    );
}

#[tokio::test]
async fn load_agent_rebuild_does_not_attach_running_current_instance() {
    // Regression for the `--rebuild` fast-path bypass: the early restore gate
    // is guarded by `!opts.rebuild`, but a forced rebuild then falls through to
    // the *second* restore resolution. Without the matching guard there,
    // `resolve_restore_candidate` returns `AttachCurrentRole` for a running
    // current-role container and `return`s into it — silently skipping the
    // build the operator asked for. A running current-role container is seeded
    // here (inspect queue returns `Running`) so that if the guard regresses the
    // launch attaches and records no `docker build`, failing this test.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    config.workspaces.insert(
        "workspace".to_owned(),
        jackin_config::WorkspaceConfig {
            workdir: "/workspace".to_owned(),
            default_agent: Some(jackin_core::Agent::Claude),
            ..jackin_config::WorkspaceConfig::default()
        },
    );
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

[claude]
plugins = []
"#,
    )
    .unwrap();

    // Index a running current-role container that the resolver would attach to.
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let mut manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Claude,
    );
    manifest.mark_status(InstanceStatus::Running);
    write_indexed_manifest(&paths, &manifest);

    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Running,
            ContainerState::Running,
        ])),
        ..Default::default()
    };
    let mut workspace = repo_workspace(&repo_dir);
    workspace.label = "workspace".to_owned();
    workspace.default_agent = Some(jackin_core::Agent::Claude);
    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &LoadOptions {
            rebuild: true,
            ..LoadOptions::default()
        },
    )
    .await
    .unwrap();

    let recorded = runner.recorded.join("\n");
    assert!(
        recorded.contains("buildx build "),
        "--rebuild must build even when a running current-role container exists \
         (must not take the attach/start fast path); recorded:\n{recorded}"
    );
}

#[tokio::test]
async fn load_agent_passes_pull_flag_with_published_image() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let role_sha = "21a9002";
    let mut runner = FakeRunner::for_load_agent([role_sha.to_owned()]);

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
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(HashMap::from([(
            crate::runtime::naming::LABEL_IMAGE_ROLE_GIT_SHA.to_owned(),
            role_sha.to_owned(),
        )]));
    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&repo_dir),
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call == "docker pull docker.io/myorg/my-role:latest"),
        "pre-built image mode must pull to check for registry updates"
    );
    assert!(
        runner
            .recorded
            .iter()
            .any(|call| call.contains("docker tag docker.io/myorg/my-role:latest")),
        "fresh published image must be tagged as the local base"
    );
    assert!(
        runner
            .recorded
            .iter()
            .any(|call| call.contains("buildx build ") && call.contains("DerivedDockerfile")),
        "derived overlay build must still run"
    );
}

#[tokio::test]
async fn load_agent_uses_prebuilt_when_construct_version_matches() {
    // When the published image's jackin.role.git.sha label matches the role
    // checkout, the pre-built image is used.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let role_sha = "21a9002";
    let mut runner = FakeRunner::for_load_agent([role_sha.to_owned()]);

    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(HashMap::from([
            (
                crate::runtime::naming::LABEL_IMAGE_ROLE_GIT_SHA.to_owned(),
                role_sha.to_owned(),
            ),
            (
                crate::runtime::naming::LABEL_IMAGE_CONSTRUCT_VERSION.to_owned(),
                "0.1-trixie".to_owned(),
            ),
        ]));
    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&repo_dir),
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
            .any(|call| call.contains("docker tag docker.io/myorg/my-role:latest")),
        "pre-built mode must tag the verified image as the local base; got: {:?}",
        runner.recorded
    );
}

#[tokio::test]
async fn load_agent_falls_back_to_workspace_when_role_sha_label_missing() {
    // When the published image cannot prove it was built for the current role
    // SHA, jackin falls back to workspace mode.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    // The published image does not carry the current role SHA, triggering
    // workspace fallback.
    let mut runner = FakeRunner::for_load_agent(["abc123".to_owned()]);

    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(HashMap::new());
    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&repo_dir),
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    let build_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("buildx build "))
        .unwrap();
    // A stale published image falls back to a workspace role-base build, but it
    // is not an operator-requested rebuild: keep Docker's layer cache and do
    // not use the stale published image as base.
    assert!(
        !build_cmd.contains("--pull"),
        "published-stale fallback should preserve layer cache; got: {build_cmd}"
    );
    assert!(
        !build_cmd.contains("docker.io/myorg/my-role:latest"),
        "stale published image must not be used as base; got: {build_cmd}"
    );
}

#[tokio::test]
async fn load_agent_uses_prebuilt_when_role_sha_matches_without_construct_version() {
    // The role SHA label is authoritative for current published images. A
    // matching SHA is enough even when construct-version is absent.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let role_sha = "21a9002";
    let mut runner = FakeRunner::for_load_agent([role_sha.to_owned()]);

    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(HashMap::from([(
            crate::runtime::naming::LABEL_IMAGE_ROLE_GIT_SHA.to_owned(),
            role_sha.to_owned(),
        )]));
    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&repo_dir),
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
            .any(|call| call.contains("docker tag docker.io/myorg/my-role:latest")),
        "prebuilt mode must tag the verified image as the local base"
    );
    // In prebuilt mode rebuild=false, so the construct-mismatch guard calls
    // inspect_image_labels on the derived image (bollard). Workspace-rebuild mode skips it.
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker inspect image:jk_agent-smith")),
        "prebuilt mode must run docker inspect_image_label on derived image (construct-mismatch check)"
    );
}

#[tokio::test]
async fn load_agent_ignores_published_image_when_rebuild() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
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
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&repo_dir),
        &docker,
        &mut runner,
        &LoadOptions {
            rebuild: true,
            ..LoadOptions::default()
        },
    )
    .await
    .unwrap();

    // With --rebuild the workspace Dockerfile is used even when published_image is set.
    // The DerivedDockerfile must contain the workspace FROM, not the published image.
    let recorded = runner.recorded.join("\n");
    assert!(
        !recorded.contains("docker.io/myorg/my-role:latest"),
        "--rebuild must bypass published_image and build from the workspace Dockerfile"
    );
}

#[tokio::test]
async fn load_agent_rolls_back_runtime_on_attached_run_failure() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner {
        fail_on: vec!["jackin.kind=role".to_owned()],
        capture_queue: VecDeque::from(vec![
            String::new(),
            String::new(),
            String::new(),
            String::new(), // identity
            String::new(), // git pull
        ]),
        ..Default::default()
    };

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
    let docker = jackin_test_support::FakeDockerClient::default();
    let error = load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("docker run -d --name jk-"));
    let container_name = launched_role_container_name(&runner);
    let dind = format!("{container_name}-dind");
    let certs_volume = format!("{container_name}-dind-certs");
    let network = format!("{container_name}-net");
    // Cleanup uses docker (bollard) for rm operations
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call == &format!("docker rm -f {container_name}"))
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call == &format!("docker rm -f {dind}"))
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call == &format!("docker volume rm {certs_volume}"))
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call == &format!("docker network rm {network}"))
    );
}

#[tokio::test]
async fn load_agent_checks_dind_readiness() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
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
plugins = []
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
        &compat_dind_load_options(),
    )
    .await
    .unwrap();

    let (dind, _) = launched_dind_container(&docker);
    // DinD readiness check polls via docker exec (bollard)
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call.contains(&format!("docker exec {dind} docker info")))
    );

    // DinD container is created/started through DockerApi before readiness checks.
    let docker_recorded = docker.recorded.borrow();
    let dind_start = docker_recorded
        .iter()
        .position(|call| call == &format!("start_container:{dind}"))
        .unwrap();
    // docker exec calls go through bollard docker.exec_capture
    let dind_info = docker_recorded
        .iter()
        .position(|call| call.contains(&format!("docker exec {dind} docker info")))
        .unwrap();
    assert!(
        dind_start < dind_info,
        "DinD must start before readiness polling; recorded: {docker_recorded:?}"
    );
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.contains(&format!("docker exec {dind} docker info"))),
        "DinD readiness docker info check must be recorded; recorded: {docker_recorded:?}"
    );

    // TLS cert verification also via docker.exec_capture
    assert!(docker_recorded.iter().any(|call| {
        call.contains(&format!("docker exec {dind} test -f /certs/client/ca.pem"))
    }));
}

#[tokio::test]
async fn load_agent_configures_dind_with_tls() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
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
plugins = []
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &compat_dind_load_options(),
    )
    .await
    .unwrap();

    let (dind, dind_spec) = launched_dind_container(&docker);
    let certs_volume = dind.strip_suffix("-dind").unwrap().to_owned() + "-dind-certs";
    assert!(crate::instance::naming::is_dns_label(&dind), "{dind}");

    // DinD sidecar: TLS enabled with cert volume.
    assert!(
        dind_spec
            .env
            .contains(&"DOCKER_TLS_CERTDIR=/certs".to_owned()),
        "DinD must enable TLS cert generation"
    );
    assert!(
        dind_spec
            .binds
            .contains(&format!("{certs_volume}:/certs/client")),
        "DinD must mount cert volume"
    );
    // DinD's auto-generated server cert must include the container name as a
    // Subject Alternative Name, because the role connects via
    // DOCKER_HOST=tcp://{dind}:2376. Without this, the TLS
    // handshake fails because the default SANs only cover the short
    // container ID, `docker`, and `localhost`.
    //
    // The `DNS:` prefix is mandatory: `dockerd-entrypoint.sh` passes
    // `DOCKER_TLS_SAN` through to openssl verbatim (without adding a type
    // prefix), and openssl rejects SAN entries that lack a type tag with
    // `v2i_GENERAL_NAME_ex: missing value`.
    assert!(
        dind_spec
            .env
            .contains(&format!("DOCKER_TLS_SAN=DNS:{dind}")),
        "DinD SAN value must be prefixed with `DNS:` so openssl accepts it"
    );
    assert!(dind_spec.privileged, "DinD must run privileged");
    let expected_network = dind.strip_suffix("-dind").unwrap().to_owned() + "-net";
    assert_eq!(dind_spec.network, expected_network);
    assert_eq!(dind_spec.image, "docker:29-dind");

    // Role container: TLS client config
    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    assert!(
        run_cmd.contains(&format!("DOCKER_HOST=tcp://{dind}:2376")),
        "role must use TLS port 2376"
    );
    assert!(
        run_cmd.contains(&format!("TESTCONTAINERS_HOST_OVERRIDE={dind}")),
        "Testcontainers must receive the same DNS-safe DinD hostname"
    );
    assert!(
        run_cmd.contains("DOCKER_TLS_VERIFY=1"),
        "role must verify TLS"
    );
    assert!(
        run_cmd.contains("DOCKER_CERT_PATH=/jackin/run/dind-certs/client"),
        "role must know cert path"
    );
    assert!(
        run_cmd.contains(&format!("{certs_volume}:/jackin/run/dind-certs/client:ro")),
        "role must mount cert volume read-only"
    );
}

#[tokio::test]
async fn load_agent_adds_dind_to_no_proxy_when_proxy_is_configured() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    config.env.insert(
        "HTTPS_PROXY".to_owned(),
        jackin_core::EnvValue::Plain("http://proxy.internal:8305".to_owned()),
    );
    config.env.insert(
        "NO_PROXY".to_owned(),
        jackin_core::EnvValue::Plain("localhost,127.0.0.1".to_owned()),
    );
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
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
plugins = []
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &compat_dind_load_options(),
    )
    .await
    .unwrap();

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    let dind = dind_env_from_run_cmd(run_cmd);
    assert!(run_cmd.contains("HTTPS_PROXY=http://proxy.internal:8305"));
    // Both casings carry the merged list — operator's localhost,127.0.0.1
    // must survive into the lowercase synthesized variant for tools that
    // only read `no_proxy`.
    assert!(run_cmd.contains(&format!("NO_PROXY=localhost,127.0.0.1,{dind}")));
    assert!(run_cmd.contains(&format!("no_proxy=localhost,127.0.0.1,{dind}")));
}

#[tokio::test]
async fn load_agent_synthesizes_both_no_proxy_casings_when_only_proxy_set() {
    let (run_cmd, _temp) =
        run_load_with_env(&[("HTTPS_PROXY", "http://proxy.internal:8305")]).await;
    let dind = dind_env_from_run_cmd(&run_cmd);
    assert!(run_cmd.contains(&format!("NO_PROXY={dind}")));
    assert!(run_cmd.contains(&format!("no_proxy={dind}")));
}

#[tokio::test]
async fn load_agent_mirrors_no_proxy_to_missing_lower_casing() {
    let (run_cmd, _temp) = run_load_with_env(&[
        ("HTTPS_PROXY", "http://proxy.internal:8305"),
        ("NO_PROXY", "internal.corp"),
    ])
    .await;
    let dind = dind_env_from_run_cmd(&run_cmd);
    assert!(run_cmd.contains(&format!("NO_PROXY=internal.corp,{dind}")));
    assert!(run_cmd.contains(&format!("no_proxy=internal.corp,{dind}")));
}

#[tokio::test]
async fn load_agent_mirrors_lower_no_proxy_to_missing_upper_casing() {
    let (run_cmd, _temp) = run_load_with_env(&[
        ("https_proxy", "http://proxy.internal:8305"),
        ("no_proxy", "internal.corp"),
    ])
    .await;
    let dind = dind_env_from_run_cmd(&run_cmd);
    assert!(run_cmd.contains(&format!("NO_PROXY=internal.corp,{dind}")));
    assert!(run_cmd.contains(&format!("no_proxy=internal.corp,{dind}")));
}

#[tokio::test]
async fn load_agent_synthesizes_both_casings_when_only_no_proxy_declared() {
    // Operator may have proxy injected by /etc/environment, transparent
    // proxy, or container-injected vars; jackin only sees NO_PROXY.
    // Both casings must still receive the DinD bypass.
    let (run_cmd, _temp) = run_load_with_env(&[("NO_PROXY", "internal.corp")]).await;
    let dind = dind_env_from_run_cmd(&run_cmd);
    assert!(run_cmd.contains(&format!("NO_PROXY=internal.corp,{dind}")));
    assert!(run_cmd.contains(&format!("no_proxy=internal.corp,{dind}")));
}

#[tokio::test]
async fn load_agent_omits_no_proxy_when_no_proxy_env_declared() {
    let (run_cmd, _temp) = run_load_with_env(&[]).await;
    assert!(!run_cmd.contains("NO_PROXY="));
    assert!(!run_cmd.contains("no_proxy="));
}

async fn run_load_with_env(entries: &[(&str, &str)]) -> (String, tempfile::TempDir) {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    for (k, v) in entries {
        config.env.insert(
            (*k).to_owned(),
            jackin_core::EnvValue::Plain((*v).to_owned()),
        );
    }
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
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
plugins = []
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &compat_dind_load_options(),
    )
    .await
    .unwrap();

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap()
        .clone();
    (run_cmd, temp)
}

#[tokio::test]
async fn append_no_proxy_host_is_idempotent() {
    assert_eq!(
        append_no_proxy_host("localhost,jk-agent-smith-dind", "jk-agent-smith-dind"),
        "localhost,jk-agent-smith-dind"
    );
    assert_eq!(
        append_no_proxy_host("", "jk-agent-smith-dind"),
        "jk-agent-smith-dind"
    );
}

#[tokio::test]
async fn load_agent_sets_display_name_label() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
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

[identity]
name = "Agent Smith"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
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
    assert!(run_cmd.contains("jackin.display.name=Agent Smith"));
}

#[tokio::test]
async fn load_agent_emits_keep_awake_label_when_workspace_opted_in() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
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

[identity]
name = "Agent Smith"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let mut workspace = repo_workspace(&repo_dir);
    workspace.keep_awake_enabled = true;
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
    assert!(
        run_cmd.contains("--label jackin.keep.awake=true"),
        "role container with keep_awake_enabled must carry the keep_awake label, \
             so runtime::caffeinate::reconcile can detect it via docker ps --filter; \
             actual run command: {run_cmd}"
    );
}

#[tokio::test]
async fn load_agent_omits_keep_awake_label_when_workspace_opted_out() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
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

[identity]
name = "Agent Smith"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir); // keep_awake_enabled defaults false
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
    assert!(
        !run_cmd.contains("jackin.keep.awake"),
        "role container without keep_awake_enabled must not carry the label, \
             else the reconciler would hold caffeinate for opted-out workspaces; \
             actual run command: {run_cmd}"
    );
}

#[tokio::test]
async fn load_agent_sets_claude_env_to_jackin() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
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
plugins = []
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &compat_dind_load_options(),
    )
    .await
    .unwrap();

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    let dind = dind_env_from_run_cmd(run_cmd);
    assert!(run_cmd.contains("-e JACKIN=1"));
    assert!(run_cmd.contains(&format!("-e JACKIN_DIND_HOSTNAME={dind}")));
    assert!(run_cmd.contains("-e JACKIN_CONTAINER_NAME="));
    assert!(run_cmd.contains("-e JACKIN_INSTANCE_ID="));
    assert!(run_cmd.contains(&format!("-e TESTCONTAINERS_HOST_OVERRIDE={dind}")));
    assert!(!run_cmd.contains("JACKIN_DEBUG"));
}

#[tokio::test]
async fn load_agent_writes_instance_manifest() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        "true 0 false".to_owned(),
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
plugins = []
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
        &compat_dind_load_options(),
    )
    .await
    .unwrap();

    let container_name = launched_role_container_name(&runner);
    // D9: clean exit purges per-instance data inline; state dir must be gone.
    let state_dir = paths.data_dir.join(&container_name);
    assert!(
        !state_dir.exists(),
        "D9: state dir must be removed inline after clean exit, found {state_dir:?}"
    );
    // Index entry transitions to `purged` (entry removed at next explicit prune).
    let index_body = std::fs::read_to_string(paths.data_dir.join("instances.json")).unwrap();
    if index_body.contains(&format!(r#""container_base": "{container_name}""#)) {
        assert!(
            index_body.contains(r#""status": "purged""#),
            "D9: if index entry remains it must be marked purged; index: {index_body}"
        );
    }
}

#[tokio::test]
async fn load_agent_forwards_telemetry_without_debug_alias() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
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
plugins = []
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
    let opts = LoadOptions {
        debug: true,
        ..LoadOptions::default()
    };
    let docker = jackin_test_support::FakeDockerClient::default();
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
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    assert!(run_cmd.contains("-e JACKIN_TELEMETRY_LEVEL="));
    assert!(!run_cmd.contains("JACKIN_DEBUG"));
}

#[tokio::test]
async fn load_agent_injects_coauthor_trailer_env_when_enabled() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    config.git.coauthor_trailer = true;
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

[claude]
plugins = []
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
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
    assert!(
        run_cmd.contains("-e JACKIN_GIT_COAUTHOR_TRAILER=1"),
        "{run_cmd}"
    );
}

#[tokio::test]
async fn load_agent_omits_coauthor_trailer_env_when_disabled() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
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

[claude]
plugins = []
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
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
    assert!(
        !run_cmd.contains("JACKIN_GIT_COAUTHOR_TRAILER"),
        "{run_cmd}"
    );
}

#[tokio::test]
async fn load_agent_injects_dco_env_when_enabled() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    config.git.dco = true;
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

[claude]
plugins = []
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
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
    assert!(run_cmd.contains("-e JACKIN_GIT_DCO=1"), "{run_cmd}");
}

#[tokio::test]
async fn load_options_for_launch_carries_debug() {
    let opts = LoadOptions::for_launch(true);
    assert!(opts.debug);

    let opts = LoadOptions::for_launch(false);
    assert!(!opts.debug);
}

#[tokio::test]
async fn render_exit_clears_universe_marker_only_when_no_instances_remain() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();
    super::super::universe::mark_start(&paths, super::super::universe::StartKind::FreshConstruct);
    let marker = paths.data_dir.join("universe-since");
    let docker = jackin_test_support::FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
        ..Default::default()
    };
    render_exit(&paths, &docker).await;

    assert!(!marker.exists(), "last-instance exit clears the marker");
}

#[tokio::test]
async fn render_exit_preserves_universe_marker_when_instances_remain() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();
    super::super::universe::mark_start(&paths, super::super::universe::StartKind::FreshConstruct);
    let marker = paths.data_dir.join("universe-since");
    let docker = jackin_test_support::FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![
            jackin_docker::docker_client::ContainerRow {
                name: "jk-still-running".to_owned(),
                labels: HashMap::new(),
            },
        ]])),
        ..Default::default()
    };
    render_exit(&paths, &docker).await;

    assert!(
        marker.exists(),
        "leaving one of multiple instances keeps the universe open"
    );
}

#[tokio::test]
async fn render_exit_preserves_universe_marker_when_running_list_fails() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();
    super::super::universe::mark_start(&paths, super::super::universe::StartKind::FreshConstruct);
    let marker = paths.data_dir.join("universe-since");
    let docker = jackin_test_support::FakeDockerClient {
        fail_with: vec![("docker ps".to_owned(), "daemon down".to_owned())],
        ..Default::default()
    };
    render_exit(&paths, &docker).await;

    assert!(
        marker.exists(),
        "unknown Docker state must not close the universe"
    );
}

#[tokio::test]
async fn load_agent_injects_global_operator_env_literal() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();

    // Seed a config.toml with a global operator env map.
    std::fs::write(
        &paths.config_file,
        r#"[env]
OPERATOR_SMOKE = "smoke-literal"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
    )
    .unwrap();

    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
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
plugins = []
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
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
    assert!(
        run_cmd.contains("-e OPERATOR_SMOKE=smoke-literal"),
        "docker run must inject operator env; got: {run_cmd}"
    );
}

#[tokio::test]
async fn load_agent_keeps_zai_secret_out_of_capsule_config() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();

    std::fs::write(
        &paths.config_file,
        r#"[env]
ZAI_API_KEY = "super-secret-zai-key"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
    )
    .unwrap();

    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
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
plugins = []
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
    let opts = LoadOptions {
        provider: Some(jackin_protocol::Provider::Zai),
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

    let container_name = launched_role_container_name(&runner);
    let capsule_config_path = paths
        .jackin_home
        .join("sockets")
        .join(container_name)
        .join(jackin_protocol::CAPSULE_CONFIG_FILENAME);
    let capsule_config = std::fs::read_to_string(capsule_config_path).unwrap();
    assert!(
        !capsule_config.contains("super-secret-zai-key"),
        "CapsuleConfig must not persist resolved provider secrets: {capsule_config}"
    );
    assert!(
        !capsule_config.contains("zai_key"),
        "CapsuleConfig must not contain a provider secret field: {capsule_config}"
    );

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    assert!(
        run_cmd.contains("-e ZAI_API_KEY=super-secret-zai-key"),
        "ZAI_API_KEY should still reach the container process env; got: {run_cmd}"
    );
}

#[tokio::test]
async fn load_agent_injects_mise_trusted_paths_for_any_workspace() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();

    std::fs::write(
        &paths.config_file,
        r#"[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true

[workspaces.sample-workspace]
workdir = "/workspace"

[[workspaces.sample-workspace.mounts]]
src = "/tmp"
dst = "/workspace"
"#,
    )
    .unwrap();

    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
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
plugins = []
"#,
    )
    .unwrap();

    let workspace = jackin_config::ResolvedWorkspace {
        name: String::new(),
        label: "sample-workspace".to_owned(),
        workdir: "/workspace".to_owned(),
        mounts: vec![
            jackin_config::MountConfig {
                src: repo_dir.display().to_string(),
                dst: "/workspace/jackin".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
            jackin_config::MountConfig {
                src: repo_dir.display().to_string(),
                dst: "/workspace/homebrew-tap".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
        ],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
    };

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
    assert!(
        run_cmd.contains(
            "-e MISE_TRUSTED_CONFIG_PATHS=/workspace:/workspace/homebrew-tap:/workspace/jackin"
        ),
        "workspace must inject mise trusted paths; got: {run_cmd}"
    );
}

#[tokio::test]
async fn load_agent_operator_env_overrides_manifest_env() {
    // Spec: on conflict between manifest-declared env and operator
    // env, operator wins. The manifest below declares OPERATOR_SMOKE
    // as a literal "manifest-default"; the global operator env
    // declares the same key as "operator-wins". The docker run
    // command must inject the operator value.
    //
    // The `[env.OPERATOR_SMOKE]` manifest shape below matches the
    // existing EnvEntry schema in `src/env_model.rs` — if that
    // schema has diverged (e.g. `kind`/`default` field names), the
    // implementer should update the TOML fixture to match the
    // current schema; the test's *assertions* (operator-wins /
    // manifest-default not present) are unchanged.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();

    std::fs::write(
        &paths.config_file,
        r#"[env]
OPERATOR_SMOKE = "operator-wins"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
    )
    .unwrap();

    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
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

[env.OPERATOR_SMOKE]
default = "manifest-default"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
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
    assert!(
        run_cmd.contains("-e OPERATOR_SMOKE=operator-wins"),
        "operator env must win over manifest env on conflict; got: {run_cmd}"
    );
    assert!(
        !run_cmd.contains("-e OPERATOR_SMOKE=manifest-default"),
        "manifest value must NOT leak when operator overrides it; got: {run_cmd}"
    );
}

#[tokio::test]
async fn load_agent_injects_host_ref_operator_env() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();

    // No process-env mutation anywhere — the host env for the
    // resolver is supplied via `LoadOptions::host_env`, a plain
    // `BTreeMap<String, String>`. This keeps the test free of
    // any `std::env` write, which the crate-level
    // `unsafe_code = "forbid"` lint forbids.
    std::fs::write(
        &paths.config_file,
        r#"[env]
FROM_HOST = "$JACKIN_PR2_SMOKE_HOST_VAR"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
    )
    .unwrap();

    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
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
plugins = []
"#,
    )
    .unwrap();

    let mut host_env = std::collections::BTreeMap::new();
    host_env.insert(
        "JACKIN_PR2_SMOKE_HOST_VAR".to_owned(),
        "from-host-env".to_owned(),
    );

    let opts = LoadOptions {
        host_env: Some(host_env),
        ..LoadOptions::default()
    };

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
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
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    assert!(
        run_cmd.contains("-e FROM_HOST=from-host-env"),
        "host-ref operator env must resolve and inject; got: {run_cmd}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn load_agent_injects_op_cli_resolved_value() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();

    let bin_dir = temp.path().join("fake-bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let bin_path = bin_dir.join("op");
    // The resolver first runs `op --version` as a reachability probe
    // when any value carries an OpRef, then calls `op read -- op://...`
    // with the canonical UUID URI. The fake must handle both.
    std::fs::write(
            &bin_path,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo '2.30.0'; exit 0; fi\nif [ \"$1\" = \"read\" ]; then\n  for arg in \"$@\"; do\n    if [ \"$arg\" = \"op://abc-vault/abc-item/api-token\" ]; then printf '%s' 'resolved-op-token'; exit 0; fi\n  done\nfi\nexit 99\n",
        )
        .unwrap();
    let mut perms = std::fs::metadata(&bin_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&bin_path, perms).unwrap();

    std::fs::write(
        &paths.config_file,
        r#"[env]
OPERATOR_TOKEN = {op = "op://abc-vault/abc-item/api-token", path = "Personal/api/token"}

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
    )
    .unwrap();

    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
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
plugins = []
"#,
    )
    .unwrap();

    // Inject the fake `op` binary path via `LoadOptions::op_runner`.
    // No process env mutation — `OpCli::with_binary` takes the path
    // as a direct argument, so the `unsafe_code = "forbid"`
    // crate-level lint stays intact and sibling tests running in
    // parallel via cargo-nextest cannot race on any shared env var.
    let op_runner: Box<dyn jackin_env::OpRunner> = Box::new(jackin_env::OpCli::with_binary(
        bin_path.to_string_lossy().to_string(),
    ));
    let opts = LoadOptions {
        op_runner: Some(op_runner),
        ..LoadOptions::default()
    };

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
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
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    assert!(
        run_cmd.contains("-e OPERATOR_TOKEN=resolved-op-token"),
        "op:// ref must resolve via the injected OpCli and inject; got: {run_cmd}"
    );
}

// ── claim_container_name tests ────────────────────────────────────────────

/// `NotFound` → claim a unique ad-hoc name directly (no docker rm issued).
#[tokio::test]
async fn claim_container_name_not_found_claims_unique_ad_hoc_name() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let selector = RoleSelector::new(None, "agent-smith");
    // inspect returns NotFound (empty queue)
    let docker = jackin_test_support::FakeDockerClient::default();
    let (name, _lock) = claim_container_name(&paths, None, &selector, &docker)
        .await
        .unwrap();

    assert!(name.starts_with("jk-"), "{name}");
    assert!(name.contains("agentsmith"), "{name}");
    assert!(!name.contains("clone"), "{name}");
    assert!(crate::instance::naming::is_dns_label(&name), "{name}");
    assert!(
        crate::instance::naming::is_dns_label(&format!("{name}-dind")),
        "{name}"
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call.contains("docker inspect"))
    );
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call.contains("docker rm"))
    );
}

#[tokio::test]
async fn claim_container_name_docker_unavailable_errors() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let selector = RoleSelector::new(None, "agent-smith");
    let docker = jackin_test_support::FakeDockerClient {
        fail_with: vec![(
            "docker inspect".to_owned(),
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_owned(),
        )],
        ..Default::default()
    };
    let err = claim_container_name(&paths, None, &selector, &docker)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("cannot claim container name"));
    assert!(err.to_string().contains("Docker is unavailable"));
}

/// Running collision → skip that random name and claim another one.
#[tokio::test]
async fn claim_container_name_running_collision_tries_another_unique_name() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let selector = RoleSelector::new(None, "agent-smith");
    // First inspect → Running (occupied), second → NotFound (claimed)
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Running,
            ContainerState::NotFound,
        ])),
        ..Default::default()
    };
    let (name, _lock) = claim_container_name(&paths, None, &selector, &docker)
        .await
        .unwrap();

    assert!(name.starts_with("jk-"), "{name}");
    assert!(name.ends_with("-agentsmith"), "{name}");
    assert!(!name.contains("clone"), "{name}");
    assert_eq!(
        docker
            .recorded
            .borrow()
            .iter()
            .filter(|c| c.contains("docker inspect"))
            .count(),
        2
    );
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rm"))
    );
}

/// Stopped / exit 0 collision → docker rm issued, same random slot reclaimed.
#[tokio::test]
async fn claim_container_name_clean_exit_removes_and_reclaims() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let selector = RoleSelector::new(None, "agent-smith");
    // Stopped with exit_code=0, oom_killed=false → remove and reclaim
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 0,
            oom_killed: false,
        }])),
        ..Default::default()
    };
    let (name, _lock) = claim_container_name(&paths, None, &selector, &docker)
        .await
        .unwrap();

    assert!(name.starts_with("jk-"), "{name}");
    assert!(name.ends_with("-agentsmith"), "{name}");
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rm -f") && c.contains("agentsmith"))
    );
}

/// Stopped / non-zero collision → skip it and claim another random name.
#[tokio::test]
async fn claim_container_name_crashed_collision_tries_another_unique_name() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let selector = RoleSelector::new(None, "agent-smith");
    // Stopped with exit_code=1 → skip (no rm), then NotFound → claim
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Stopped {
                exit_code: 1,
                oom_killed: false,
            },
            ContainerState::NotFound,
        ])),
        ..Default::default()
    };
    let (name, _lock) = claim_container_name(&paths, None, &selector, &docker)
        .await
        .unwrap();

    assert!(name.starts_with("jk-"), "{name}");
    assert!(name.ends_with("-agentsmith"), "{name}");
    assert!(!name.contains("clone"), "{name}");
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.contains("docker rm"))
    );
}

#[tokio::test]
async fn claim_container_name_saved_workspace_includes_workspace_component() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let selector = RoleSelector::new(None, "agent-smith");
    let docker = jackin_test_support::FakeDockerClient::default();
    let (name, _lock) = claim_container_name(
        &paths,
        Some(&WorkspaceName::parse("my-workspace").unwrap()),
        &selector,
        &docker,
    )
    .await
    .unwrap();

    assert!(name.starts_with("jk-"), "{name}");
    assert!(
        name.contains("myworkspace") && name.ends_with("-agentsmith"),
        "{name}"
    );
    assert!(name.len() <= 58, "{name}");
}

#[tokio::test]
async fn missing_matching_instance_recreates_current_role() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Claude,
    );
    manifest
        .write(&paths.data_dir.join(container_name))
        .unwrap();
    // Missing current-role containers can be recreated in-place. The image
    // decision later decides whether that recreate can reuse the local image
    // or must rebuild.
    let docker = jackin_test_support::FakeDockerClient::default();

    let candidate = resolve_workspace_restore(&paths, "agent-smith", &docker)
        .await
        .unwrap();

    assert_eq!(
        candidate,
        RestoreResolution::RecreateCurrentRole(container_name.to_owned())
    );
}

#[tokio::test]
async fn running_matching_instance_is_skipped_by_launch_path() {
    // D13: launch never reconnects to a live instance. Running container →
    // StartFresh (let launch proceed to create a new container).
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Claude,
    );
    write_indexed_manifest(&paths, &manifest);
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Running])),
        ..Default::default()
    };

    let candidate = resolve_workspace_restore(&paths, "agent-smith", &docker)
        .await
        .unwrap();

    assert_eq!(candidate, RestoreResolution::StartFresh);
}

#[tokio::test]
async fn stopped_matching_instance_starts_current_role() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Claude,
    );
    write_indexed_manifest(&paths, &manifest);
    // Stopped current-role containers can be started and reconnected without
    // rebuilding or resolving launch credentials.
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 137,
            oom_killed: false,
        }])),
        ..Default::default()
    };

    let candidate = resolve_workspace_restore(&paths, "agent-smith", &docker)
        .await
        .unwrap();

    assert_eq!(
        candidate,
        RestoreResolution::StartCurrentRole(container_name.to_owned())
    );
}

#[tokio::test]
async fn related_restore_candidate_requires_rich_dialog_for_fresh_load() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-thearchitect";
    let manifest = workspace_manifest(
        container_name,
        "the-architect",
        "The Architect",
        jackin_core::Agent::Claude,
    );
    write_indexed_manifest(&paths, &manifest);
    // inspect -> NotFound -> matching but different role, but no rich
    // progress dialog is available in this direct unit-test call.
    let docker = jackin_test_support::FakeDockerClient::default();

    let error = resolve_workspace_restore(&paths, "agent-smith", &docker)
        .await
        .unwrap_err();

    // The related-only case flows through the unified rich restore dialog
    // instead of silently starting fresh.
    let message = error.to_string();
    assert!(
        message.contains("rich launch dialog"),
        "unexpected error: {message}"
    );
    assert!(message.contains("agent-smith"), "{message}");
}

#[tokio::test]
async fn running_related_instance_does_not_block_fresh_load() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-thearchitect";
    let manifest = workspace_manifest(
        container_name,
        "the-architect",
        "The Architect",
        jackin_core::Agent::Claude,
    );
    write_indexed_manifest(&paths, &manifest);
    // Related container is Running → skip → StartFresh
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Running])),
        ..Default::default()
    };

    let candidate = resolve_workspace_restore(&paths, "agent-smith", &docker)
        .await
        .unwrap();

    assert_eq!(candidate, RestoreResolution::StartFresh);
}

#[tokio::test]
async fn stopped_related_instance_does_not_block_fresh_load() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-thearchitect";
    let manifest = workspace_manifest(
        container_name,
        "the-architect",
        "The Architect",
        jackin_core::Agent::Claude,
    );
    write_indexed_manifest(&paths, &manifest);
    // Related container stopped non-cleanly → skip → StartFresh
    let docker = jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 137,
            oom_killed: false,
        }])),
        ..Default::default()
    };

    let candidate = resolve_workspace_restore(&paths, "agent-smith", &docker)
        .await
        .unwrap();

    assert_eq!(candidate, RestoreResolution::StartFresh);
}

#[tokio::test]
async fn related_restore_candidates_ignore_finished_instances() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-thearchitect";
    let mut manifest = workspace_manifest(
        container_name,
        "the-architect",
        "The Architect",
        jackin_core::Agent::Claude,
    );
    manifest.mark_status(InstanceStatus::CleanExited);
    write_indexed_manifest(&paths, &manifest);
    // Manifest is CleanExited → not a restore candidate, no docker call
    let docker = jackin_test_support::FakeDockerClient::default();

    let candidate = resolve_workspace_restore(&paths, "agent-smith", &docker)
        .await
        .unwrap();

    assert_eq!(candidate, RestoreResolution::StartFresh);
    assert!(docker.recorded.borrow().is_empty());
}

#[tokio::test]
async fn related_restore_candidate_with_container_recovers_in_place() {
    let container_name = "jk-k7p9m2xq-workspace-thearchitect";
    let candidate = RelatedRestoreCandidate {
        manifest: workspace_manifest(
            container_name,
            "the-architect",
            "The Architect",
            jackin_core::Agent::Claude,
        ),
        docker_state: ContainerState::Running,
    };

    let resolution = recover_related_restore_candidate(&candidate).unwrap();

    assert_eq!(
        resolution,
        RestoreResolution::RecoverRelatedRole(container_name.to_owned())
    );
}

#[tokio::test]
async fn missing_related_restore_candidate_rebuilds_in_place() {
    let container_name = "jk-k7p9m2xq-workspace-thearchitect";
    let candidate = RelatedRestoreCandidate {
        manifest: workspace_manifest(
            container_name,
            "the-architect",
            "The Architect",
            jackin_core::Agent::Claude,
        ),
        docker_state: ContainerState::NotFound,
    };

    let resolution = recover_related_restore_candidate(&candidate).unwrap();

    assert!(matches!(
        resolution,
        RestoreResolution::RebuildRelatedRole(ref manifest)
            if manifest.container_base == container_name
    ));
}

#[tokio::test]
async fn related_restore_load_options_use_manifest_source_ref_and_agent() {
    let container_name = "jk-k7p9m2xq-workspace-thearchitect";
    let mut manifest = workspace_manifest(
        container_name,
        "the-architect",
        "The Architect",
        jackin_core::Agent::Codex,
    );
    manifest.agent_runtime = "codex".to_owned();
    manifest.role_source_ref = Some("restore-ref".to_owned());
    let current = LoadOptions::for_load(true, false);

    let opts = related_restore_load_options(&current, &manifest).unwrap();

    assert!(opts.debug);
    assert_eq!(opts.agent, Some(jackin_core::Agent::Codex));
    assert_eq!(opts.role_branch.as_deref(), Some("restore-ref"));
    assert_eq!(opts.restore_container_base.as_deref(), Some(container_name));
    assert_eq!(
        opts.restore_role_source_git.as_deref(),
        Some("https://example.invalid/the-architect.git")
    );
}

#[tokio::test]
async fn supersede_restore_candidates_updates_manifest_and_index() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Claude,
    );
    write_indexed_manifest(&paths, &manifest);

    supersede_restore_candidates(&paths, vec![manifest]).unwrap();

    let manifest = InstanceManifest::read(&paths.data_dir.join(container_name)).unwrap();
    assert_eq!(manifest.status, InstanceStatus::Superseded);
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
    assert_eq!(index.instances[0].status, InstanceStatus::Superseded);
}

#[tokio::test]
async fn restore_candidate_label_includes_manifest_and_mount_state() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let mut manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Codex,
    );
    manifest.mark_status(InstanceStatus::PreservedDirty);
    manifest.last_attach_outcome = Some("exit:137".into());
    crate::isolation::state::write_records(
        &paths.data_dir.join(container_name),
        &[crate::isolation::state::IsolationRecord {
            workspace: "workspace".into(),
            mount_dst: "/workspace".into(),
            original_src: "/host/workspace".into(),
            isolation: MountIsolation::Worktree,
            worktree_path: "/tmp/worktree".into(),
            scratch_branch: "jackin/test".into(),
            base_commit: "abc123".into(),
            selector_key: "agent-smith".into(),
            container_name: container_name.into(),
            cleanup_status: crate::isolation::state::CleanupStatus::PreservedDirty,
        }],
    )
    .unwrap();

    let label = restore_candidate_label(&paths, &manifest);

    assert!(label.contains("k7p9m2xq"), "{label}");
    assert!(label.contains("status:preserved_dirty"), "{label}");
    assert!(label.contains("agent:codex"), "{label}");
    assert!(label.contains("role:agent-smith"), "{label}");
    assert!(label.contains("mounts:1 dirty:1 unpushed:0"), "{label}");
    assert!(label.contains("attach:exit:137"), "{label}");
    assert!(!label.contains(container_name), "{label}");
}

#[tokio::test]
async fn record_instance_attach_outcome_updates_manifest() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Claude,
    );
    manifest
        .write(&paths.data_dir.join(container_name))
        .unwrap();

    record_instance_attach_outcome(
        &paths,
        container_name,
        crate::isolation::finalize::AttachOutcome::stopped(137),
    )
    .unwrap();

    let manifest = InstanceManifest::read(&paths.data_dir.join(container_name)).unwrap();
    assert_eq!(manifest.last_attach_outcome.as_deref(), Some("exit:137"));
}

#[tokio::test]
async fn record_running_attach_outcome_restores_running_status() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);
    let container_name = "jk-k7p9m2xq-workspace-agentsmith";
    let mut manifest = workspace_manifest(
        container_name,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Claude,
    );
    manifest.mark_status(InstanceStatus::RestoreAvailable);
    write_indexed_manifest(&paths, &manifest);

    record_instance_attach_outcome(
        &paths,
        container_name,
        crate::isolation::finalize::AttachOutcome::still_running(),
    )
    .unwrap();

    let manifest = InstanceManifest::read(&paths.data_dir.join(container_name)).unwrap();
    assert_eq!(manifest.status, InstanceStatus::Running);
    assert_eq!(manifest.last_attach_outcome.as_deref(), Some("running"));
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
    assert_eq!(index.instances[0].status, InstanceStatus::Running);
}

#[tokio::test]
async fn format_attach_outcome_names_running_exit_and_oom() {
    use crate::isolation::finalize::AttachOutcome;

    assert_eq!(
        format_attach_outcome(AttachOutcome::still_running()),
        "running"
    );
    assert_eq!(format_attach_outcome(AttachOutcome::stopped(0)), "exit:0");
    assert_eq!(
        format_attach_outcome(AttachOutcome::oom_killed()),
        "oom_killed"
    );
}

#[tokio::test]
async fn verify_credential_sync_returns_ok_regardless() {
    use jackin_config::AuthForwardMode;
    use jackin_core::Agent;
    let merged: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let layers: Vec<(String, EnvLayerState)> = vec![];
    let r = verify_credential_env_present(
        Agent::Claude,
        AuthForwardMode::Sync,
        &merged,
        &[],
        &layers,
        &WorkspaceName::parse("proj").unwrap(),
        "smith",
    );
    r.unwrap();
}

#[tokio::test]
async fn verify_credential_ignore_returns_ok_regardless() {
    use jackin_config::AuthForwardMode;
    use jackin_core::Agent;
    let merged: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let layers: Vec<(String, EnvLayerState)> = vec![];
    let r = verify_credential_env_present(
        Agent::Claude,
        AuthForwardMode::Ignore,
        &merged,
        &[],
        &layers,
        &WorkspaceName::parse("proj").unwrap(),
        "smith",
    );
    r.unwrap();
}

#[tokio::test]
async fn verify_credential_api_key_present_ok() {
    use jackin_config::AuthForwardMode;
    use jackin_core::{ANTHROPIC_API_KEY_ENV_NAME, Agent};
    let mut merged = std::collections::BTreeMap::new();
    merged.insert(ANTHROPIC_API_KEY_ENV_NAME.into(), "sk-ant-xxx".into());
    let layers: Vec<(String, EnvLayerState)> = vec![];
    let r = verify_credential_env_present(
        Agent::Claude,
        AuthForwardMode::ApiKey,
        &merged,
        &[],
        &layers,
        &WorkspaceName::parse("proj").unwrap(),
        "smith",
    );
    r.unwrap();
}

#[tokio::test]
async fn verify_credential_api_key_missing_returns_structured_error() {
    use jackin_config::AuthForwardMode;
    use jackin_core::{ANTHROPIC_API_KEY_ENV_NAME, Agent};
    let mut merged = std::collections::BTreeMap::new();
    merged.insert(ANTHROPIC_API_KEY_ENV_NAME.into(), String::new());
    let layers = vec![
        ("[env]".into(), EnvLayerState::Unset),
        ("[roles.smith.env]".into(), EnvLayerState::Unset),
        ("[workspaces.proj.env]".into(), EnvLayerState::Unset),
        (
            "[workspaces.proj.roles.smith.env]".into(),
            EnvLayerState::Unset,
        ),
    ];
    let mode_resolution = vec![
        (
            "workspace × role × claude".into(),
            Some(AuthForwardMode::ApiKey),
        ),
        ("workspace × claude".into(), None),
        ("global × claude".into(), None),
    ];
    let r = verify_credential_env_present(
        Agent::Claude,
        AuthForwardMode::ApiKey,
        &merged,
        &mode_resolution,
        &layers,
        &WorkspaceName::parse("proj").unwrap(),
        "smith",
    );
    let err = r.unwrap_err();
    match err {
        LaunchError::AuthCredentialMissing {
            env_var,
            agent,
            mode,
            workspace,
            role,
            env_layers,
            mode_resolution,
            ..
        } => {
            assert_eq!(env_var, ANTHROPIC_API_KEY_ENV_NAME);
            assert_eq!(agent, Agent::Claude);
            assert_eq!(mode, AuthForwardMode::ApiKey);
            assert_eq!(workspace, "proj");
            assert_eq!(role, "smith");
            // Helper passes the caller's traces through verbatim.
            assert_eq!(env_layers.len(), 4);
            assert_eq!(mode_resolution.len(), 3);
            assert_eq!(mode_resolution[0].1, Some(AuthForwardMode::ApiKey));
        }
    }
}

#[tokio::test]
async fn verify_credential_api_key_unset_returns_structured_error() {
    use jackin_config::AuthForwardMode;
    use jackin_core::Agent;
    // ANTHROPIC_API_KEY not in map at all.
    let merged: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let layers: Vec<(String, EnvLayerState)> = vec![];
    let r = verify_credential_env_present(
        Agent::Claude,
        AuthForwardMode::ApiKey,
        &merged,
        &[],
        &layers,
        &WorkspaceName::parse("proj").unwrap(),
        "smith",
    );
    assert!(matches!(r, Err(LaunchError::AuthCredentialMissing { .. })));
}

#[tokio::test]
async fn verify_credential_oauth_token_missing_for_claude() {
    use jackin_config::AuthForwardMode;
    use jackin_core::Agent;
    let merged: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let layers = vec![("[env]".into(), EnvLayerState::Unset)];
    let r = verify_credential_env_present(
        Agent::Claude,
        AuthForwardMode::OAuthToken,
        &merged,
        &[],
        &layers,
        &WorkspaceName::parse("proj").unwrap(),
        "smith",
    );
    let err = r.unwrap_err();
    match err {
        LaunchError::AuthCredentialMissing { env_var, .. } => {
            assert_eq!(env_var, "CLAUDE_CODE_OAUTH_TOKEN");
        }
    }
}

#[tokio::test]
async fn verify_credential_codex_api_key_missing() {
    use jackin_config::AuthForwardMode;
    use jackin_core::Agent;
    let merged: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let layers: Vec<(String, EnvLayerState)> = vec![];
    let r = verify_credential_env_present(
        Agent::Codex,
        AuthForwardMode::ApiKey,
        &merged,
        &[],
        &layers,
        &WorkspaceName::parse("proj").unwrap(),
        "smith",
    );
    let err = r.unwrap_err();
    match err {
        LaunchError::AuthCredentialMissing { env_var, agent, .. } => {
            assert_eq!(env_var, "OPENAI_API_KEY");
            assert_eq!(agent, Agent::Codex);
        }
    }
}

#[tokio::test]
async fn verify_credential_amp_api_key_missing() {
    use jackin_config::AuthForwardMode;
    use jackin_core::Agent;
    let merged: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let layers: Vec<(String, EnvLayerState)> = vec![];
    let r = verify_credential_env_present(
        Agent::Amp,
        AuthForwardMode::ApiKey,
        &merged,
        &[],
        &layers,
        &WorkspaceName::parse("proj").unwrap(),
        "smith",
    );
    let err = r.unwrap_err();
    match err {
        LaunchError::AuthCredentialMissing { env_var, agent, .. } => {
            assert_eq!(env_var, "AMP_API_KEY");
            assert_eq!(agent, Agent::Amp);
        }
    }
}

#[tokio::test]
async fn build_mode_resolution_populates_all_3_layers() {
    use jackin_config::WorkspaceConfig;
    use jackin_config::{AgentAuthConfig, AuthForwardMode};
    use jackin_core::Agent;

    let ws = WorkspaceConfig {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
            ..Default::default()
        }),
        ..WorkspaceConfig::default()
    };
    let mut cfg = AppConfig {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::Sync,
            ..Default::default()
        }),
        ..AppConfig::default()
    };
    cfg.workspaces.insert("proj".into(), ws);

    let proj = jackin_core::WorkspaceName::parse("proj").unwrap();
    let trace = build_mode_resolution(&cfg, Agent::Claude, Some(&proj), "smith");
    assert_eq!(trace.len(), 3);
    // Ordered most-specific first: ws × role × claude (no override),
    // then ws × claude (api_key), then global × claude (sync).
    assert_eq!(trace[0].0, "workspace × role × claude");
    assert_eq!(trace[0].1, None);
    assert_eq!(trace[1].0, "workspace × claude");
    assert_eq!(trace[1].1, Some(AuthForwardMode::ApiKey));
    assert_eq!(trace[2].0, "global × claude");
    assert_eq!(trace[2].1, Some(AuthForwardMode::Sync));
}

#[tokio::test]
async fn build_mode_resolution_role_override_wins() {
    use jackin_config::{AgentAuthConfig, AuthForwardMode};
    use jackin_config::{WorkspaceConfig, WorkspaceRoleOverride};
    use jackin_core::Agent;

    let ro = WorkspaceRoleOverride {
        claude: Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::OAuthToken,
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut ws = WorkspaceConfig::default();
    ws.roles.insert("smith".into(), ro);
    let mut cfg = AppConfig::default();
    cfg.workspaces.insert("proj".into(), ws);

    let proj = jackin_core::WorkspaceName::parse("proj").unwrap();
    let trace = build_mode_resolution(&cfg, Agent::Claude, Some(&proj), "smith");
    assert_eq!(trace[0].1, Some(AuthForwardMode::OAuthToken));
    assert_eq!(trace[1].1, None);
    assert_eq!(trace[2].1, None);
}

#[tokio::test]
async fn sync_source_resolution_uses_workspace_role_scope_per_agent() {
    use jackin_config::{AgentAuthConfig, WorkspaceConfig, WorkspaceRoleOverride};
    use jackin_core::Agent;
    use std::path::PathBuf;

    let mut cfg = AppConfig {
        codex: Some(AgentAuthConfig {
            sync_source_dir: Some(PathBuf::from("/global/codex")),
            ..Default::default()
        }),
        amp: Some(AgentAuthConfig {
            sync_source_dir: Some(PathBuf::from("/global/amp")),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut workspace = WorkspaceConfig {
        codex: Some(AgentAuthConfig {
            sync_source_dir: Some(PathBuf::from("/workspace/codex")),
            ..Default::default()
        }),
        amp: Some(AgentAuthConfig {
            sync_source_dir: Some(PathBuf::from("/workspace/amp")),
            ..Default::default()
        }),
        ..Default::default()
    };
    workspace.roles.insert(
        "architect".to_owned(),
        WorkspaceRoleOverride {
            codex: Some(AgentAuthConfig {
                sync_source_dir: Some(PathBuf::from("/role/architect/codex")),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    workspace.roles.insert(
        "builder".to_owned(),
        WorkspaceRoleOverride {
            amp: Some(AgentAuthConfig {
                sync_source_dir: Some(PathBuf::from("/role/builder/amp")),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    cfg.workspaces.insert("proj".to_owned(), workspace);

    let proj = jackin_core::WorkspaceName::parse("proj").unwrap();
    let architect_source =
        |agent| jackin_config::resolve_sync_source_dir(&cfg, agent, Some(&proj), "architect");
    let builder_source =
        |agent| jackin_config::resolve_sync_source_dir(&cfg, agent, Some(&proj), "builder");

    assert_eq!(
        architect_source(Agent::Codex),
        Some(PathBuf::from("/role/architect/codex"))
    );
    assert_eq!(
        architect_source(Agent::Amp),
        Some(PathBuf::from("/workspace/amp"))
    );
    assert_eq!(
        builder_source(Agent::Codex),
        Some(PathBuf::from("/workspace/codex"))
    );
    assert_eq!(
        builder_source(Agent::Amp),
        Some(PathBuf::from("/role/builder/amp"))
    );
}

#[tokio::test]
async fn build_env_layer_states_classifies_present_vs_absent() {
    use jackin_config::{WorkspaceConfig, WorkspaceRoleOverride};
    use jackin_core::{EnvValue, OpRef}; // env_model flattened

    let mut ro = WorkspaceRoleOverride::default();
    ro.env.insert(
        ANTHROPIC_API_KEY_ENV_NAME.into(),
        EnvValue::OpRef(OpRef {
            op: "op://uuid/test/field".into(),
            path: "Test/api/key".into(),
            account: None,
            on_demand: false,
        }),
    );
    let mut ws = WorkspaceConfig::default();
    ws.roles.insert("smith".into(), ro);
    let mut cfg = AppConfig::default();
    cfg.workspaces.insert("proj".into(), ws);

    let proj = jackin_core::WorkspaceName::parse("proj").unwrap();
    let layers = build_env_layer_states(&cfg, Some(&proj), "smith", ANTHROPIC_API_KEY_ENV_NAME);
    assert_eq!(layers.len(), 4);
    assert_eq!(layers[0].0, "[env]");
    assert_eq!(layers[0].1, EnvLayerState::Unset);
    assert_eq!(layers[1].0, "[roles.smith.env]");
    assert_eq!(layers[1].1, EnvLayerState::Unset);
    assert_eq!(layers[2].0, "[workspaces.proj.env]");
    assert_eq!(layers[2].1, EnvLayerState::Unset);
    assert_eq!(layers[3].0, "[workspaces.proj.roles.smith.env]");
    assert_eq!(layers[3].1, EnvLayerState::ResolvedOpRef);
}

#[tokio::test]
async fn build_env_layer_states_classifies_literal_at_global() {
    use jackin_core::EnvValue; // env_model flattened

    let mut env = std::collections::BTreeMap::new();
    env.insert(
        ANTHROPIC_API_KEY_ENV_NAME.into(),
        EnvValue::Plain(format!("${ANTHROPIC_API_KEY_ENV_NAME}")),
    );
    let cfg = AppConfig {
        env,
        ..AppConfig::default()
    };

    let proj = jackin_core::WorkspaceName::parse("proj").unwrap();
    let layers = build_env_layer_states(&cfg, Some(&proj), "smith", ANTHROPIC_API_KEY_ENV_NAME);
    assert_eq!(layers[0].1, EnvLayerState::ResolvedLiteral);
    assert_eq!(layers[1].1, EnvLayerState::Unset);
    assert_eq!(layers[2].1, EnvLayerState::Unset);
    assert_eq!(layers[3].1, EnvLayerState::Unset);
}

#[tokio::test]
async fn inspect_attach_outcome_capture_failure_returns_still_running() {
    // Docker unavailable or container removed mid-inspect must NOT route
    // through finalize_clean_exit's auto-cleanup path — still_running
    // keeps records preserved for `jackin hardline` to recover.
    use crate::isolation::finalize::AttachOutcome;
    use jackin_docker::docker_client::ContainerState;
    for state in [
        ContainerState::NotFound,
        ContainerState::InspectUnavailable("daemon down".into()),
    ] {
        let docker = jackin_test_support::FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(VecDeque::from([state])),
            ..Default::default()
        };
        let outcome = inspect_attach_outcome(&docker, "jackin-x").await.unwrap();
        assert_eq!(outcome, AttachOutcome::still_running());
    }
}

fn inspect_docker(state: ContainerState) -> jackin_test_support::FakeDockerClient {
    jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([state])),
        ..Default::default()
    }
}

/// `Stopped { exit_code: 0 }` → stopped(0) → enters `finalize_clean_exit`
/// which is the documented happy path for clean container exits.
#[tokio::test]
async fn inspect_attach_outcome_exited_zero_returns_stopped() {
    use crate::isolation::finalize::AttachOutcome;
    use jackin_docker::docker_client::ContainerState;
    let docker = inspect_docker(ContainerState::Stopped {
        exit_code: 0,
        oom_killed: false,
    });
    let outcome = inspect_attach_outcome(&docker, "jackin-x").await.unwrap();
    assert_eq!(outcome, AttachOutcome::stopped(0));
}

/// `Stopped { exit_code: 137 }` → stopped(137) → preserved by finalize.
#[tokio::test]
async fn inspect_attach_outcome_exited_nonzero_returns_stopped_with_code() {
    use crate::isolation::finalize::AttachOutcome;
    use jackin_docker::docker_client::ContainerState;
    let docker = inspect_docker(ContainerState::Stopped {
        exit_code: 137,
        oom_killed: false,
    });
    let outcome = inspect_attach_outcome(&docker, "jackin-x").await.unwrap();
    assert_eq!(outcome, AttachOutcome::stopped(137));
}

/// `Stopped { oom_killed: true }` → `oom_killed`.
#[tokio::test]
async fn inspect_attach_outcome_exited_oom_returns_oom_killed() {
    use crate::isolation::finalize::AttachOutcome;
    use jackin_docker::docker_client::ContainerState;
    let docker = inspect_docker(ContainerState::Stopped {
        exit_code: 137,
        oom_killed: true,
    });
    let outcome = inspect_attach_outcome(&docker, "jackin-x").await.unwrap();
    assert_eq!(outcome, AttachOutcome::oom_killed());
}

/// `Running` → `still_running`. The basic happy detach case.
#[tokio::test]
async fn inspect_attach_outcome_running_returns_still_running() {
    use crate::isolation::finalize::AttachOutcome;
    use jackin_docker::docker_client::ContainerState;
    let docker = inspect_docker(ContainerState::Running);
    let outcome = inspect_attach_outcome(&docker, "jackin-x").await.unwrap();
    assert_eq!(outcome, AttachOutcome::still_running());
}

/// `Paused` → `still_running`. The container hasn't exited; treating
/// it as stopped(0) would let `finalize_clean_exit` auto-delete its
/// worktrees while the container is paused but recoverable.
#[tokio::test]
async fn inspect_attach_outcome_paused_returns_still_running() {
    use crate::isolation::finalize::AttachOutcome;
    use jackin_docker::docker_client::ContainerState;
    let docker = inspect_docker(ContainerState::Paused);
    let outcome = inspect_attach_outcome(&docker, "jackin-x").await.unwrap();
    assert_eq!(
        outcome,
        AttachOutcome::still_running(),
        "paused containers must NOT route through finalize_clean_exit's auto-cleanup path"
    );
}

/// `Restarting`, `Removing`, `Created` → `still_running` for the same
/// reason as `Paused`: not exited, no real exit code to act on.
#[tokio::test]
async fn inspect_attach_outcome_transient_states_return_still_running() {
    use crate::isolation::finalize::AttachOutcome;
    use jackin_docker::docker_client::ContainerState;
    for state in [
        ContainerState::Restarting,
        ContainerState::Removing,
        ContainerState::Created,
    ] {
        let docker = inspect_docker(state.clone());
        let outcome = inspect_attach_outcome(&docker, "jackin-x").await.unwrap();
        assert_eq!(
            outcome,
            AttachOutcome::still_running(),
            "{state:?} must map to still_running",
        );
    }
}

/// `Dead` → `still_running` (conservative: daemon failed to
/// deinitialize; records preserved for inspection).
#[tokio::test]
async fn inspect_attach_outcome_dead_returns_still_running() {
    use crate::isolation::finalize::AttachOutcome;
    use jackin_docker::docker_client::ContainerState;
    let docker = inspect_docker(ContainerState::Dead);
    let outcome = inspect_attach_outcome(&docker, "jackin-x").await.unwrap();
    assert_eq!(outcome, AttachOutcome::still_running());
}

/// `InspectUnavailable` → `still_running`. Conservative direction so a
/// daemon error never accidentally triggers data deletion.
#[tokio::test]
async fn inspect_attach_outcome_unknown_status_returns_still_running() {
    use crate::isolation::finalize::AttachOutcome;
    use jackin_docker::docker_client::ContainerState;
    let docker = inspect_docker(ContainerState::InspectUnavailable("unexpected".into()));
    let outcome = inspect_attach_outcome(&docker, "jackin-x").await.unwrap();
    assert_eq!(outcome, AttachOutcome::still_running());
}

#[tokio::test]
async fn auth_credential_missing_displays_layer_trace() {
    let err = LaunchError::AuthCredentialMissing {
        agent: jackin_core::Agent::Claude,
        mode: jackin_config::AuthForwardMode::ApiKey,
        env_var: ANTHROPIC_API_KEY_ENV_NAME,
        workspace: "proj".into(),
        role: "smith".into(),
        mode_resolution: vec![
            (
                "workspace × role × claude".into(),
                Some(jackin_config::AuthForwardMode::ApiKey),
            ),
            ("workspace × claude".into(), None),
            (
                "global × claude".into(),
                Some(jackin_config::AuthForwardMode::Sync),
            ),
        ],
        env_layers: vec![
            ("[env]".into(), EnvLayerState::Unset),
            ("[roles.smith.env]".into(), EnvLayerState::Unset),
            ("[workspaces.proj.env]".into(), EnvLayerState::Unset),
            (
                "[workspaces.proj.roles.smith.env]".into(),
                EnvLayerState::Unset,
            ),
        ],
    };
    let s = err.to_string();
    assert!(s.contains("auth_forward is 'api_key'"), "got: {s}");
    assert!(s.contains(ANTHROPIC_API_KEY_ENV_NAME), "got: {s}");
    assert!(
        s.contains("workspace × role × claude    -> api_key"),
        "got: {s}"
    );
    assert!(s.contains("[workspaces.proj.roles.smith.env]"), "got: {s}");
    assert!(s.contains("Open the Auth panel"), "got: {s}");
}

#[tokio::test]
async fn auth_credential_missing_codex_api_key_renders() {
    let err = LaunchError::AuthCredentialMissing {
        agent: jackin_core::Agent::Codex,
        mode: jackin_config::AuthForwardMode::ApiKey,
        env_var: "OPENAI_API_KEY",
        workspace: "proj".into(),
        role: "smith".into(),
        mode_resolution: vec![],
        env_layers: vec![],
    };
    let s = err.to_string();
    assert!(s.contains("codex"), "got: {s}");
    assert!(s.contains("OPENAI_API_KEY"), "got: {s}");
}

#[tokio::test]
async fn auth_credential_missing_amp_api_key_renders() {
    let err = LaunchError::AuthCredentialMissing {
        agent: jackin_core::Agent::Amp,
        mode: jackin_config::AuthForwardMode::ApiKey,
        env_var: "AMP_API_KEY",
        workspace: "proj".into(),
        role: "smith".into(),
        mode_resolution: vec![],
        env_layers: vec![],
    };
    let s = err.to_string();
    assert!(s.contains("amp"), "got: {s}");
    assert!(s.contains("AMP_API_KEY"), "got: {s}");
}

// ── verify_github_token_present (Token-mode pre-flight) ──────

#[tokio::test]
async fn verify_github_token_present_ok_when_token_resolves() {
    let r = verify_github_token_present(
        jackin_config::GithubAuthMode::Token,
        Some("ghp_real"),
        &WorkspaceName::parse("proj").unwrap(),
        "smith",
    );
    r.unwrap();
}

#[tokio::test]
async fn verify_github_token_present_ok_for_sync_and_ignore_regardless_of_token() {
    // Sync / Ignore have no pre-flight invariant on GH_TOKEN —
    // Sync sources its token from the host, Ignore exports nothing.
    let r = verify_github_token_present(
        jackin_config::GithubAuthMode::Sync,
        None,
        &WorkspaceName::parse("proj").unwrap(),
        "smith",
    );
    r.unwrap();
    let r = verify_github_token_present(
        jackin_config::GithubAuthMode::Ignore,
        None,
        &WorkspaceName::parse("proj").unwrap(),
        "smith",
    );
    r.unwrap();
}

#[tokio::test]
async fn verify_github_token_present_errors_when_token_missing() {
    let err = verify_github_token_present(
        jackin_config::GithubAuthMode::Token,
        None,
        &WorkspaceName::parse("customer-acme").unwrap(),
        "release-bot",
    )
    .unwrap_err();
    let s = err.to_string();
    assert!(s.contains("auth_forward = \"token\""), "got: {s}");
    assert!(s.contains("workspace 'customer-acme'"), "got: {s}");
    assert!(s.contains("role 'release-bot'"), "got: {s}");
    assert!(s.contains("GH_TOKEN"), "got: {s}");
    // Operator-actionable remediation suggestions.
    assert!(s.contains("[github.env]"), "got: {s}");
    assert!(
        s.contains("[workspaces.customer-acme.github.env]"),
        "got: {s}"
    );
    assert!(
        s.contains("[workspaces.customer-acme.roles.release-bot.github.env]"),
        "got: {s}"
    );
    assert!(s.contains("auth_forward = \"sync\""), "got: {s}");
    assert!(s.contains("\"ignore\""), "got: {s}");
}

#[tokio::test]
async fn verify_github_token_present_errors_when_token_empty_string() {
    // Empty string must be rejected the same as missing — `gh`
    // reads `GH_TOKEN=""` as no token, and we don't want to
    // launch DinD just for the agent to fail at first push.
    let err = verify_github_token_present(
        jackin_config::GithubAuthMode::Token,
        Some(""),
        &WorkspaceName::parse("proj").unwrap(),
        "smith",
    )
    .unwrap_err();
    assert!(err.to_string().contains("GH_TOKEN"));
}

// ── resolve_github_env_map ───────────────────────────────────

#[tokio::test]
async fn resolve_github_env_map_returns_empty_for_no_declarations() {
    use std::collections::BTreeMap;
    let decls: BTreeMap<String, jackin_core::EnvValue> = BTreeMap::new();
    let resolved = resolve_github_env_map(&decls, &LoadOptions::default()).unwrap();
    assert!(resolved.is_empty());
}

#[tokio::test]
async fn resolve_github_env_map_resolves_plain_values() {
    use std::collections::BTreeMap;
    let mut decls: BTreeMap<String, jackin_core::EnvValue> = BTreeMap::new();
    decls.insert(
        "GH_TOKEN".into(),
        jackin_core::EnvValue::Plain("ghp_test".into()),
    );
    decls.insert(
        "GH_HOST".into(),
        jackin_core::EnvValue::Plain("ghe.acme.com".into()),
    );
    let resolved = resolve_github_env_map(&decls, &LoadOptions::default()).unwrap();
    assert_eq!(
        resolved.get("GH_TOKEN").map(String::as_str),
        Some("ghp_test")
    );
    assert_eq!(
        resolved.get("GH_HOST").map(String::as_str),
        Some("ghe.acme.com"),
    );
}

#[tokio::test]
async fn resolve_github_env_map_aggregates_failures() {
    use std::collections::BTreeMap;
    // Two host-env references, both unset → both reported in
    // one structured error rather than aborting on the first.
    let mut decls: BTreeMap<String, jackin_core::EnvValue> = BTreeMap::new();
    decls.insert(
        "GH_TOKEN".into(),
        jackin_core::EnvValue::Plain("$JACKIN_TEST_MISSING_TOKEN".into()),
    );
    decls.insert(
        "GH_HOST".into(),
        jackin_core::EnvValue::Plain("$JACKIN_TEST_MISSING_HOST".into()),
    );
    let opts = LoadOptions {
        // Empty host-env map so `$NAME` references fail to resolve.
        host_env: Some(BTreeMap::new()),
        ..LoadOptions::default()
    };
    let err = resolve_github_env_map(&decls, &opts).unwrap_err();
    let s = err.to_string();
    assert!(
        s.contains("github env resolution failed for 2 var(s)"),
        "expected aggregated count, got: {s}"
    );
    assert!(s.contains("GH_TOKEN"), "got: {s}");
    assert!(s.contains("GH_HOST"), "got: {s}");
}

struct ConcurrentGithubOpRunner {
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
}

impl ConcurrentGithubOpRunner {
    fn record_active(&self, active: usize) {
        let mut observed = self.max_active.load(Ordering::SeqCst);
        while active > observed {
            match self.max_active.compare_exchange(
                observed,
                active,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return,
                Err(next) => observed = next,
            }
        }
    }
}

impl jackin_env::OpRunner for ConcurrentGithubOpRunner {
    fn read(&self, reference: &str) -> anyhow::Result<String> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.record_active(active);
        #[expect(
            clippy::disallowed_methods,
            reason = "test runner deliberately holds worker OS threads open to prove overlap"
        )]
        std::thread::sleep(std::time::Duration::from_millis(25));
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(format!("secret-for-{reference}"))
    }
}

#[tokio::test]
async fn resolve_github_env_map_reads_independent_op_refs_concurrently() {
    use std::collections::BTreeMap;
    let mut decls: BTreeMap<String, jackin_core::EnvValue> = BTreeMap::new();
    for key in ["GH_TOKEN", "GH_ENTERPRISE_TOKEN", "GH_HOST"] {
        decls.insert(
            key.into(),
            jackin_core::EnvValue::OpRef(jackin_core::OpRef {
                op: format!("op://vault/item/{key}"),
                path: format!("Vault/Item/{key}"),
                account: None,
                on_demand: false,
            }),
        );
    }
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));
    let runner = ConcurrentGithubOpRunner {
        active,
        max_active: Arc::clone(&max_active),
    };
    let opts = LoadOptions {
        op_runner: Some(Box::new(runner)),
        ..LoadOptions::default()
    };

    let resolved = resolve_github_env_map(&decls, &opts).unwrap();

    assert_eq!(resolved.len(), 3);
    assert!(
        max_active.load(Ordering::SeqCst) > 1,
        "expected overlapping github env op reads"
    );
}

#[test]
fn early_scan_skips_current_inspect_only_for_matching_empty_scan() {
    use super::restore_resolve::{
        EarlyCurrentRestoreScan, RestoreResolution, early_scan_reused_current,
        early_scan_skips_current_inspect,
    };
    use jackin_core::Agent;

    let early = EarlyCurrentRestoreScan::Scanned {
        agent: Agent::Claude,
        current: None,
    };
    assert!(early_scan_skips_current_inspect(&early, Agent::Claude));
    assert!(!early_scan_skips_current_inspect(&early, Agent::Codex));
    assert!(!early_scan_skips_current_inspect(
        &EarlyCurrentRestoreScan::NotRun,
        Agent::Claude
    ));
    assert!(!early_scan_skips_current_inspect(
        &EarlyCurrentRestoreScan::Scanned {
            agent: Agent::Claude,
            current: Some(RestoreResolution::RecreateCurrentRole("jk-x".into())),
        },
        Agent::Claude
    ));
    // Unselected-empty scope skips current inspect for any later agent.
    assert!(early_scan_skips_current_inspect(
        &EarlyCurrentRestoreScan::ScannedUnselectedEmpty,
        Agent::Claude
    ));
    assert!(early_scan_skips_current_inspect(
        &EarlyCurrentRestoreScan::ScannedUnselectedEmpty,
        Agent::Codex
    ));
    // Non-empty typed hit is reused (Some(Some(...))), not treated as skip-empty.
    assert_eq!(
        early_scan_reused_current(
            &EarlyCurrentRestoreScan::Scanned {
                agent: Agent::Claude,
                current: Some(RestoreResolution::RecreateCurrentRole("jk-x".into())),
            },
            Agent::Claude
        ),
        Some(Some(RestoreResolution::RecreateCurrentRole("jk-x".into())))
    );
}

/// Common path: early selected empty scan + later resolve must not re-inspect
/// current-role containers (launch-speed 008c residual).
#[tokio::test]
async fn early_empty_scan_avoids_second_current_role_inspect() {
    use super::restore_resolve::{
        EarlyCurrentRestoreScan, RestoreResolution, resolve_restore_candidate_reusing_early,
    };
    use jackin_core::Agent;
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    use std::collections::VecDeque;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);

    let container_name = "jk-early-empty-scan";
    let mut manifest =
        workspace_manifest(container_name, "agent-smith", "Agent Smith", Agent::Claude);
    manifest.mark_status(InstanceStatus::Crashed);
    write_indexed_manifest(&paths, &manifest);

    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            // First call: early selected scan sees NotFound → Recreate would
            // be returned by a live scan; we stash empty instead to prove reuse
            // path. Drive reusing_early with an empty Scanned so any second
            // inspect would pull this queue entry.
            ContainerState::NotFound,
            ContainerState::NotFound,
        ])),
        ..Default::default()
    };

    // Empty early scan for Claude: later resolve must not call inspect again.
    let early = EarlyCurrentRestoreScan::Scanned {
        agent: Agent::Claude,
        current: None,
    };
    let resolution = resolve_restore_candidate_reusing_early(
        &paths,
        Some("workspace"),
        "workspace",
        "/workspace",
        "agent-smith",
        Agent::Claude,
        &docker,
        None,
        &early,
    )
    .await
    .unwrap();

    assert_eq!(resolution, RestoreResolution::StartFresh);
    let inspects: Vec<_> = docker
        .recorded
        .borrow()
        .iter()
        .filter(|c| c.starts_with("docker inspect "))
        .cloned()
        .collect();
    assert!(
        inspects.is_empty(),
        "empty early scan must not re-inspect current-role; recorded: {inspects:?}"
    );
}

/// Non-empty early hit must reuse typed `Scanned.current` without a second
/// current-role Docker inspect (008c residual #2).
#[tokio::test]
async fn early_nonempty_scan_reuses_typed_current_without_reinspect() {
    use super::restore_resolve::{
        EarlyCurrentRestoreScan, RestoreResolution, resolve_restore_candidate_reusing_early,
    };
    use jackin_core::Agent;
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    use std::collections::VecDeque;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);

    let container_name = "jk-early-nonempty-reuse";
    let mut manifest =
        workspace_manifest(container_name, "agent-smith", "Agent Smith", Agent::Claude);
    manifest.mark_status(InstanceStatus::Crashed);
    write_indexed_manifest(&paths, &manifest);

    let docker = FakeDockerClient {
        // Any inspect would consume this; reuse must leave the queue untouched.
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::NotFound])),
        ..Default::default()
    };

    let early = EarlyCurrentRestoreScan::Scanned {
        agent: Agent::Claude,
        current: Some(RestoreResolution::RecreateCurrentRole(
            container_name.to_owned(),
        )),
    };
    let resolution = resolve_restore_candidate_reusing_early(
        &paths,
        Some("workspace"),
        "workspace",
        "/workspace",
        "agent-smith",
        Agent::Claude,
        &docker,
        None,
        &early,
    )
    .await
    .unwrap();

    assert_eq!(
        resolution,
        RestoreResolution::RecreateCurrentRole(container_name.to_owned())
    );
    let inspects: Vec<_> = docker
        .recorded
        .borrow()
        .iter()
        .filter(|c| c.starts_with("docker inspect "))
        .cloned()
        .collect();
    assert!(
        inspects.is_empty(),
        "typed non-empty early hit must not re-inspect; recorded: {inspects:?}"
    );
    // Queue still full proves we never called inspect.
    assert_eq!(docker.inspect_queue.borrow().len(), 1);
}

/// Unselected-empty early scan lets a later selected agent skip current-role
/// re-inspect when the role truly has no restore candidates (008c residual #1).
#[tokio::test]
async fn unselected_empty_early_scan_skips_later_agent_current_inspect() {
    use super::restore_resolve::{
        EarlyCurrentRestoreScan, RestoreResolution, resolve_restore_candidate_reusing_early,
    };
    use jackin_core::Agent;
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    use std::collections::VecDeque;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);

    // No indexed manifests → role-scope empty.
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::NotFound,
            ContainerState::NotFound,
        ])),
        ..Default::default()
    };

    let early = EarlyCurrentRestoreScan::ScannedUnselectedEmpty;
    let resolution = resolve_restore_candidate_reusing_early(
        &paths,
        Some("workspace"),
        "workspace",
        "/workspace",
        "agent-smith",
        Agent::Claude,
        &docker,
        None,
        &early,
    )
    .await
    .unwrap();

    assert_eq!(resolution, RestoreResolution::StartFresh);
    let inspects: Vec<_> = docker
        .recorded
        .borrow()
        .iter()
        .filter(|c| c.starts_with("docker inspect "))
        .cloned()
        .collect();
    assert!(
        inspects.is_empty(),
        "ScannedUnselectedEmpty must skip current-role inspect; recorded: {inspects:?}"
    );
}

/// Full early+later common path: one current-role inspect only (`FakeDocker`
/// call count), not a double inspect when the early scan was empty.
#[tokio::test]
async fn common_path_single_current_inspect_with_early_then_reuse() {
    use super::restore_resolve::{
        EarlyCurrentRestoreScan, RestoreResolution, resolve_current_restore_candidate_timed,
        resolve_restore_candidate_reusing_early,
    };
    use jackin_core::Agent;
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    use std::collections::VecDeque;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::test_support::install_all_test_stubs(&paths);

    let container_name = "jk-common-single-inspect";
    let mut manifest =
        workspace_manifest(container_name, "agent-smith", "Agent Smith", Agent::Claude);
    // Running is a restore candidate but launch never attaches (D13) → empty hit.
    manifest.mark_status(InstanceStatus::Running);
    write_indexed_manifest(&paths, &manifest);

    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Running,
            // Would be consumed by a wasteful second current-role inspect.
            ContainerState::Running,
        ])),
        ..Default::default()
    };

    // Early selected scan (mirrors launch_pipeline pre-role-repo probe).
    let early_hit = resolve_current_restore_candidate_timed(
        &paths,
        Some("workspace"),
        "workspace",
        "/workspace",
        "agent-smith",
        Agent::Claude,
        &docker,
    )
    .await
    .unwrap();
    assert_eq!(early_hit, None);
    let early = EarlyCurrentRestoreScan::Scanned {
        agent: Agent::Claude,
        current: None,
    };

    let resolution = resolve_restore_candidate_reusing_early(
        &paths,
        Some("workspace"),
        "workspace",
        "/workspace",
        "agent-smith",
        Agent::Claude,
        &docker,
        None,
        &early,
    )
    .await
    .unwrap();
    assert_eq!(resolution, RestoreResolution::StartFresh);

    let inspects: Vec<_> = docker
        .recorded
        .borrow()
        .iter()
        .filter(|c| c.starts_with(&format!("docker inspect {container_name}")))
        .cloned()
        .collect();
    assert_eq!(
        inspects.len(),
        1,
        "common path must inspect current-role candidate once, not twice; recorded: {inspects:?}"
    );
}
