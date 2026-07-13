// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `instance`.
use super::*;
use jackin_core::paths::JackinPaths;
use jackin_manifest::load_role_manifest;
use tempfile::tempdir;

fn ignoring_resolvers() -> PrepareResolvers<'static> {
    PrepareResolvers {
        auth_modes: &|_| AuthForwardMode::Ignore,
        sync_source_dirs: &|_| None,
    }
}

fn simple_manifest(temp: &tempfile::TempDir) -> RoleManifest {
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    load_role_manifest(temp.path()).unwrap()
}

#[test]
fn prepares_persisted_claude_state() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);

    let (state, _) = RoleState::prepare(
        &paths,
        "jk-k7p9m2xq-agentsmith",
        &manifest,
        &ignoring_resolvers(),
        &GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
    )
    .unwrap();

    // Fresh ignore-mode launches are lazy: no jackin-owned auth files
    // are created unless stale forwarded state must be wiped.
    assert!(!state.claude_account_json().unwrap().exists());
    assert!(!state.claude_credentials_json().unwrap().exists());
    assert!(
        !state.claude_forwards_auth(),
        "Ignore mode must not forward auth into the container",
    );
    assert!(state.claude_model().is_none());
    assert!(state.codex_model().is_none());

    // Pin the host-side grouped layout: a regression to the legacy
    // flat shape (`.claude/state/.credentials.json` at the data-dir
    // root) would still satisfy the accessor checks
    // above, since they only look up paths through the enum. These
    // assertions verify the actual host paths under
    // `<container>/claude/`.
    let container_root = paths.data_dir.join("jk-k7p9m2xq-agentsmith");
    assert_eq!(
        state.claude_account_json().unwrap(),
        container_root.join("claude").join("account.json"),
    );
    assert_eq!(
        state.claude_credentials_json().unwrap(),
        container_root.join("claude").join("credentials.json"),
    );
    assert!(!container_root.join("home/.claude").exists());
    assert!(!container_root.join("home/.claude.json").exists());
    assert!(container_root.join("state").is_dir());
}

#[test]
fn prepares_codex_state_carries_model_without_config_toml() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
model = "gpt-5"
"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();

    let (state, outcome) = RoleState::prepare(
        &paths,
        "jk-k7p9m2xq-agentsmith",
        &manifest,
        &ignoring_resolvers(),
        &GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Codex,
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert_eq!(state.codex_model(), Some("gpt-5"));
    assert!(
        !paths
            .data_dir
            .join("jk-k7p9m2xq-agentsmith")
            .join("codex")
            .join("auth.json")
            .exists()
    );
    assert!(
        !paths
            .data_dir
            .join("jk-k7p9m2xq-agentsmith")
            .join("codex")
            .join("config.toml")
            .exists()
    );
    assert!(
        !paths
            .data_dir
            .join("jk-k7p9m2xq-agentsmith")
            .join("home/.codex")
            .exists()
    );
    // Codex state carries no Claude auth paths — the typed enum
    // makes the absence structural rather than a runtime nil.
    assert!(state.claude_account_json().is_none());
    assert!(state.claude_credentials_json().is_none());
    assert!(!state.claude_forwards_auth());
}

/// Regression: a multi-agent role must apply each supported
/// agent's *own* configured `auth_forward` mode, not the selected
/// agent's mode. Before the fix, selecting Codex with
/// `codex.auth_forward = ApiKey` would call `provision_claude_auth`
/// with `ApiKey` and silently `wipe_claude_state`, destroying the
/// operator's durable Claude credentials and breaking the next
/// `hardline --new --agent claude` switch.
#[test]
fn prepare_resolves_auth_mode_per_supported_agent() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    std::fs::write(
        temp.path().join("jackin.role.toml"),
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
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();

    // Claude → Sync (host missing → HostMissing, forward_auth = true)
    // Codex → ApiKey (would wipe Claude state if applied cross-agent)
    let auth_modes = |agent: jackin_core::agent::Agent| match agent {
        jackin_core::agent::Agent::Claude => AuthForwardMode::Sync,
        jackin_core::agent::Agent::Codex => AuthForwardMode::ApiKey,
        jackin_core::agent::Agent::Amp
        | jackin_core::agent::Agent::Kimi
        | jackin_core::agent::Agent::Opencode
        | jackin_core::agent::Agent::Grok => AuthForwardMode::Ignore,
    };

    let (state, selected_outcome) = RoleState::prepare(
        &paths,
        "jk-k7p9m2xq-agentsmith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &auth_modes,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Codex,
    )
    .unwrap();

    // Selected agent is Codex with ApiKey → TokenMode (env-driven).
    // The selected-outcome attribution must follow the *selected*
    // agent, not the last-iterated one.
    assert_eq!(selected_outcome, AuthProvisionOutcome::TokenMode);

    // Both agents provisioned.
    assert!(
        state.auth.claude.is_some(),
        "claude home dirs should be provisioned"
    );
    assert!(
        state.auth.codex.is_some(),
        "codex home dirs should be provisioned"
    );

    // Critical assertion: Claude's mode (Sync) is honored, not
    // Codex's (ApiKey). A regression to applying Codex's mode to
    // Claude would wipe state and set forward_auth = false.
    assert!(
        state.claude_forwards_auth(),
        "claude.auth_forward = Sync must produce forward_auth = true even when Codex is the selected agent",
    );
    assert!(
        state.claude_account_json().unwrap().exists(),
        "Sync mode must leave an account.json placeholder on disk",
    );
}

#[test]
fn prepare_emits_per_auth_slot_timings() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    std::fs::write(
        temp.path().join("jackin.role.toml"),
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
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = load_role_manifest(temp.path()).unwrap();
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, true, "load").unwrap();
    let _active = run.activate();

    RoleState::prepare(
        &paths,
        "jk-k7p9m2xq-agentsmith",
        &manifest,
        &ignoring_resolvers(),
        &GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
    )
    .unwrap();

    let jsonl = std::fs::read_to_string(run.path()).unwrap();
    assert!(jsonl.contains("role_state_prepare:github_auth"), "{jsonl}");
    assert!(jsonl.contains("role_state_prepare:claude_auth"), "{jsonl}");
    assert!(jsonl.contains("role_state_prepare:codex_auth"), "{jsonl}");
    assert!(jsonl.contains("\"stage\":\"credentials\""), "{jsonl}");
    assert!(
        !jsonl.contains("oauth_token:"),
        "timing details must not include credential values: {jsonl}"
    );
}

#[test]
fn github_ignore_prepare_skips_absent_state() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, true, "load").unwrap();
    let _active = run.activate();

    RoleState::prepare(
        &paths,
        "jk-k7p9m2xq-agentsmith",
        &manifest,
        &ignoring_resolvers(),
        &GithubAuthContext {
            mode: GithubAuthMode::Ignore,
            token: None,
        },
        temp.path(),
        jackin_core::agent::Agent::Claude,
    )
    .unwrap();

    let jsonl = std::fs::read_to_string(run.path()).unwrap();
    assert!(jsonl.contains("role_state_prepare:github_auth"), "{jsonl}");
    assert!(jsonl.contains("skipped_no_state"), "{jsonl}");
    assert!(
        !paths
            .data_dir
            .join("jk-k7p9m2xq-agentsmith/.config/gh")
            .exists(),
        "no-state GitHub ignore mode should not create jackin-owned gh config state"
    );
}

#[test]
fn github_ignore_prepare_still_wipes_existing_state() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);
    let hosts_yml = paths
        .data_dir
        .join("jk-k7p9m2xq-agentsmith")
        .join(".config/gh/hosts.yml");
    std::fs::create_dir_all(hosts_yml.parent().unwrap()).unwrap();
    std::fs::write(&hosts_yml, "github.com:\n    oauth_token: stale\n").unwrap();

    RoleState::prepare(
        &paths,
        "jk-k7p9m2xq-agentsmith",
        &manifest,
        &ignoring_resolvers(),
        &GithubAuthContext {
            mode: GithubAuthMode::Ignore,
            token: None,
        },
        temp.path(),
        jackin_core::agent::Agent::Claude,
    )
    .unwrap();

    assert!(
        !hosts_yml.exists(),
        "ignore mode must still wipe stale jackin-owned GitHub auth state"
    );
}

#[test]
fn agent_ignore_prepare_skips_absent_state_without_host_or_home_work() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);

    let host_home = temp.path().join("missing-host-home");
    let (state, outcome) = RoleState::prepare_for_agents(
        &paths,
        "jk-k7p9m2xq-agentsmith",
        &manifest,
        &ignoring_resolvers(),
        &GithubAuthContext::default(),
        &host_home,
        jackin_core::agent::Agent::Claude,
        &[jackin_core::agent::Agent::Claude],
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(state.auth.claude.is_some());

    let container_root = paths.data_dir.join("jk-k7p9m2xq-agentsmith");
    assert!(
        !container_root.join("claude").exists(),
        "no-state ignore mode should not create jackin-owned Claude auth state"
    );
    assert!(
        !container_root.join("home/.claude").exists(),
        "no-state ignore mode should not prepare selected-agent home state"
    );
}

#[test]
fn agent_ignore_prepare_still_wipes_existing_state() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);
    let stale_account = paths
        .data_dir
        .join("jk-k7p9m2xq-agentsmith")
        .join("claude/account.json");
    let stale_credentials = paths
        .data_dir
        .join("jk-k7p9m2xq-agentsmith")
        .join("claude/credentials.json");
    std::fs::create_dir_all(stale_account.parent().unwrap()).unwrap();
    std::fs::write(&stale_account, r#"{"stale":true}"#).unwrap();
    std::fs::write(&stale_credentials, r#"{"stale":true}"#).unwrap();

    RoleState::prepare_for_agents(
        &paths,
        "jk-k7p9m2xq-agentsmith",
        &manifest,
        &ignoring_resolvers(),
        &GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Claude,
        &[jackin_core::agent::Agent::Claude],
    )
    .unwrap();

    assert_eq!(std::fs::read_to_string(&stale_account).unwrap(), "{}");
    assert!(!stale_credentials.exists());
}

#[test]
fn prewarm_auth_for_agents_skips_github_and_selected_slot() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    std::fs::write(
        temp.path().join("jackin.role.toml"),
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
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = load_role_manifest(temp.path()).unwrap();
    let codex_mode_resolved = std::cell::Cell::new(false);
    let resolvers = PrepareResolvers {
        auth_modes: &|agent| match agent {
            jackin_core::agent::Agent::Codex => {
                codex_mode_resolved.set(true);
                AuthForwardMode::Ignore
            }
            other => panic!("unexpected selected/sibling auth mode resolution for {other}"),
        },
        sync_source_dirs: &|agent| match agent {
            jackin_core::agent::Agent::Codex => None,
            other => panic!("unexpected selected/sibling sync-source resolution for {other}"),
        },
    };

    let count = RoleState::prewarm_auth_for_agents(
        &paths,
        "jk-k7p9m2xq-agentsmith",
        &manifest,
        &resolvers,
        temp.path(),
        &[jackin_core::agent::Agent::Codex],
    )
    .unwrap();

    assert_eq!(count, 1);
    assert!(codex_mode_resolved.get());

    let container_root = paths.data_dir.join("jk-k7p9m2xq-agentsmith");
    assert!(
        !container_root.join("home/.codex").exists(),
        "ignore-mode sibling prewarm should skip absent no-state auth slots"
    );
    assert!(
        !container_root.join("home/.claude").exists(),
        "background prewarm must not provision the selected/omitted auth slot"
    );
}

#[test]
fn prepare_provisions_all_supported_auth_slots_after_parallel_join() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha4"
dockerfile = "Dockerfile"
agents = ["claude", "codex", "amp", "kimi", "opencode", "grok"]

[claude]
plugins = []

[codex]

[amp]

[kimi]

[opencode]

[grok]
"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = load_role_manifest(temp.path()).unwrap();

    let (state, selected_outcome) = RoleState::prepare(
        &paths,
        "jk-k7p9m2xq-agentsmith",
        &manifest,
        &ignoring_resolvers(),
        &GithubAuthContext::default(),
        temp.path(),
        jackin_core::agent::Agent::Grok,
    )
    .unwrap();

    assert_eq!(selected_outcome, AuthProvisionOutcome::Skipped);
    assert!(state.auth.claude.is_some());
    assert!(state.auth.codex.is_some());
    assert!(state.auth.amp.is_some());
    assert!(state.auth.kimi.is_some());
    assert!(state.auth.opencode.is_some());
    assert!(state.auth.grok.is_some());
    for agent in [
        jackin_core::agent::Agent::Claude,
        jackin_core::agent::Agent::Codex,
        jackin_core::agent::Agent::Amp,
        jackin_core::agent::Agent::Kimi,
        jackin_core::agent::Agent::Opencode,
        jackin_core::agent::Agent::Grok,
    ] {
        assert_eq!(
            state.auth_outcomes.get(&agent),
            Some(&AuthProvisionOutcome::Skipped),
            "{} auth outcome missing from launch summary state",
            agent.slug()
        );
    }
}
