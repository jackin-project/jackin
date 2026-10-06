// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn diagnose_premature_exit_surfaces_stderr_only_docker_logs() {
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;

    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
            exit_code: 1,
            oom_killed: false,
        }])),
        ..Default::default()
    };
    let mut runner =
        FakeRunner::with_combined_queue(["Error: missing /jackin/run/agent.toml".to_owned()]);
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
        msg.contains("Error: missing /jackin/run/agent.toml"),
        "stderr-only reason must be surfaced, not discarded: {msg}"
    );
    assert!(
        !msg.contains("no log output"),
        "stderr-only logs must not take the empty-logs branch: {msg}"
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
    crate::runtime::stubs::install_all_test_stubs(&paths);
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

    let mounts = agent_mounts(&state).unwrap();
    assert!(
        mounts.iter().any(|m| m.contains(":/jackin/state")),
        "jackin state mount missing: {mounts:?}"
    );
    assert!(
        mounts.iter().any(|m| m.contains(":/home/agent/.claude")),
        "durable Claude home mount missing: {mounts:?}"
    );
    assert!(
        !mounts
            .iter()
            .any(|m| m.contains(":/home/agent/.claude.json")),
        "mutable Claude metadata must live within its directory mount: {mounts:?}"
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
        auth_mount_paths: std::collections::BTreeSet::new(),
        auth_mount_leases: Vec::new(),
        provider_config_mounts: Vec::new(),
    };

    assert!(
        github_config_mount(&state).unwrap().is_none(),
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
        auth_mount_paths: std::collections::BTreeSet::new(),
        auth_mount_leases: Vec::new(),
        provider_config_mounts: Vec::new(),
    };

    assert!(
        github_config_mount(&state)
            .unwrap()
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
    crate::runtime::stubs::install_all_test_stubs(&paths);
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

    assert!(
        state.auth.for_agent(Agent::Claude).is_some(),
        "selected Claude slot missing"
    );
    assert!(
        state.auth.for_agent(Agent::Codex).is_none(),
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
    crate::runtime::stubs::install_all_test_stubs(&paths);
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

    let mounts = agent_mounts(&state).unwrap();
    assert!(
        mounts
            .iter()
            .any(|m| m.contains("/jackin/claude/account.json") && m.ends_with(":ro")),
        "account.json mount missing under /jackin/claude/: {mounts:?}",
    );
    assert!(
        mounts
            .iter()
            .any(|m| m.contains("/jackin/claude/credentials.json") && m.ends_with(":ro")),
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
    crate::runtime::stubs::install_all_test_stubs(&paths);
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

    let mounts = agent_mounts(&state).unwrap();
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
