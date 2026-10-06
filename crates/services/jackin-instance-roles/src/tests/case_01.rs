// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn auth_provision_events_are_bounded_once_and_private() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);

    tracing::subscriber::with_default(subscriber, || {
        emit_agent_auth_provision(
            Agent::Claude,
            AuthForwardMode::Sync,
            Ok(AuthProvisionOutcome::Synced),
        );
        emit_agent_auth_provision(
            Agent::Codex,
            AuthForwardMode::ApiKey,
            Err(jackin_telemetry::schema::enums::ErrorType::IoError),
        );
    });
    export.force_flush();

    assert_eq!(export.event_count("auth.provision"), 2);
    assert!(export.contains_log_text("claude"));
    assert!(export.contains_log_text("sync"));
    assert!(export.contains_log_text("agent_home"));
    assert!(export.contains_log_text("codex"));
    assert!(export.contains_log_text("api_key"));
    assert!(export.contains_log_text("environment"));
    assert!(export.contains_log_text("io_error"));
    assert!(!export.contains_log_text("private-env-value"));
    assert!(!export.contains_log_text("private-vault-id"));
    assert!(!export.contains_log_text("private-credential-name"));
}

#[test]
fn prepare_for_agents_exports_one_auth_outcome_per_attempt() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);

    let prepared = tracing::subscriber::with_default(subscriber, || {
        RoleState::prepare_for_agents(
            &paths,
            "private-container-name",
            &manifest,
            &ignoring_resolvers(),
            &GithubAuthContext::default(),
            temp.path(),
            Agent::Claude,
            &[Agent::Claude],
        )
    });
    export.force_flush();

    prepared.unwrap();
    assert_eq!(export.event_count("auth.provision"), 1);
    assert!(!export.contains_log_text("private-container-name"));
    assert!(!export.contains_log_text(temp.path().to_string_lossy().as_ref()));
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
        Agent::Claude,
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
    // above, since they only look up paths through the slots map. These
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
        Agent::Codex,
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
    // Codex state carries no Claude auth paths — the slots map
    // holds no Claude entry rather than a runtime nil.
    assert!(state.claude_account_json().is_none());
    assert!(state.claude_credentials_json().is_none());
    assert!(!state.claude_forwards_auth());
}

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
    let auth_modes = |agent: Agent| match agent {
        Agent::Claude => AuthForwardMode::Sync,
        Agent::Codex => AuthForwardMode::ApiKey,
        Agent::Amp
        | Agent::Kimi
        | Agent::Opencode
        | Agent::Grok
        | Agent::Antigravity
        | Agent::Gemini
        | Agent::Cursor
        | Agent::Muse
        | Agent::Omp
        | Agent::Hermes => AuthForwardMode::Ignore,
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
        Agent::Codex,
    )
    .unwrap();

    // Selected agent is Codex with ApiKey → TokenMode (env-driven).
    // The selected-outcome attribution must follow the *selected*
    // agent, not the last-iterated one.
    assert_eq!(selected_outcome, AuthProvisionOutcome::TokenMode);

    // Both agents provisioned.
    assert!(
        state.auth.for_agent(Agent::Claude).is_some(),
        "claude home dirs should be provisioned"
    );
    assert!(
        state.auth.for_agent(Agent::Codex).is_some(),
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
fn github_ignore_prepare_skips_absent_state() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);

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
        Agent::Claude,
    )
    .unwrap();

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
        Agent::Claude,
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
        Agent::Claude,
        &[Agent::Claude],
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(state.auth.for_agent(Agent::Claude).is_some());

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
        Agent::Claude,
        &[Agent::Claude],
    )
    .unwrap();

    assert_eq!(std::fs::read_to_string(&stale_account).unwrap(), "{}");
    assert!(!stale_credentials.exists());
}
