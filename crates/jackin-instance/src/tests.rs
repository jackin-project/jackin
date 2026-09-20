// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `instance`.
use super::*;
use jackin_core::JackinPaths;
use jackin_manifest::load_role_manifest;
use std::path::PathBuf;
use tempfile::tempdir;

fn ignoring_resolvers() -> PrepareResolvers<'static> {
    PrepareResolvers {
        auth_modes: &|_| AuthForwardMode::Ignore,
        sync_source_dirs: &|_| None,
    }
}

#[test]
fn auth_provision_events_are_bounded_once_and_private() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);

    tracing::subscriber::with_default(subscriber, || {
        emit_agent_auth_provision(
            jackin_core::Agent::Claude,
            AuthForwardMode::Sync,
            Ok(AuthProvisionOutcome::Synced),
        );
        emit_agent_auth_provision(
            jackin_core::Agent::Codex,
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
            jackin_core::Agent::Claude,
            &[jackin_core::Agent::Claude],
        )
    });
    export.force_flush();

    prepared.unwrap();
    assert_eq!(export.event_count("auth.provision"), 1);
    assert!(!export.contains_log_text("private-container-name"));
    assert!(!export.contains_log_text(temp.path().to_string_lossy().as_ref()));
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
        jackin_core::Agent::Claude,
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
        jackin_core::Agent::Codex,
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
    let auth_modes = |agent: jackin_core::Agent| match agent {
        jackin_core::Agent::Claude => AuthForwardMode::Sync,
        jackin_core::Agent::Codex => AuthForwardMode::ApiKey,
        jackin_core::Agent::Amp
        | jackin_core::Agent::Kimi
        | jackin_core::Agent::Opencode
        | jackin_core::Agent::Grok
        | jackin_core::Agent::Antigravity
        | jackin_core::Agent::Gemini
        | jackin_core::Agent::Cursor
        | jackin_core::Agent::Muse
        | jackin_core::Agent::Omp
        | jackin_core::Agent::Hermes => AuthForwardMode::Ignore,
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
        jackin_core::Agent::Codex,
    )
    .unwrap();

    // Selected agent is Codex with ApiKey → TokenMode (env-driven).
    // The selected-outcome attribution must follow the *selected*
    // agent, not the last-iterated one.
    assert_eq!(selected_outcome, AuthProvisionOutcome::TokenMode);

    // Both agents provisioned.
    assert!(
        state.auth.for_agent(jackin_core::Agent::Claude).is_some(),
        "claude home dirs should be provisioned"
    );
    assert!(
        state.auth.for_agent(jackin_core::Agent::Codex).is_some(),
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
        jackin_core::Agent::Claude,
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
        jackin_core::Agent::Claude,
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
        jackin_core::Agent::Claude,
        &[jackin_core::Agent::Claude],
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(state.auth.for_agent(jackin_core::Agent::Claude).is_some());

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
        jackin_core::Agent::Claude,
        &[jackin_core::Agent::Claude],
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
            jackin_core::Agent::Codex => {
                codex_mode_resolved.set(true);
                AuthForwardMode::Ignore
            }
            other => panic!("unexpected selected/sibling auth mode resolution for {other}"),
        },
        sync_source_dirs: &|agent| match agent {
            jackin_core::Agent::Codex => None,
            other => panic!("unexpected selected/sibling sync-source resolution for {other}"),
        },
    };

    let count = RoleState::prewarm_auth_for_agents(
        &paths,
        "jk-k7p9m2xq-agentsmith",
        &manifest,
        &resolvers,
        temp.path(),
        &[jackin_core::Agent::Codex],
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
        jackin_core::Agent::Grok,
    )
    .unwrap();

    assert_eq!(selected_outcome, AuthProvisionOutcome::Skipped);
    for agent in [
        jackin_core::Agent::Claude,
        jackin_core::Agent::Codex,
        jackin_core::Agent::Amp,
        jackin_core::Agent::Kimi,
        jackin_core::Agent::Opencode,
        jackin_core::Agent::Grok,
    ] {
        assert!(
            state.auth.for_agent(agent).is_some(),
            "{} slot missing after parallel provision",
            agent.slug()
        );
        assert!(
            state
                .auth
                .slots
                .contains_key(&ProvisionedAuth::instance_key("default", agent)),
            "{} slot key must be the default {{account}}@{{agent}} synthesis",
            agent.slug()
        );
    }
    for agent in [
        jackin_core::Agent::Claude,
        jackin_core::Agent::Codex,
        jackin_core::Agent::Amp,
        jackin_core::Agent::Kimi,
        jackin_core::Agent::Opencode,
        jackin_core::Agent::Grok,
    ] {
        assert_eq!(
            state.auth_outcomes.get(&agent),
            Some(&AuthProvisionOutcome::Skipped),
            "{} auth outcome missing from launch summary state",
            agent.slug()
        );
    }
}

#[test]
fn prepare_for_bindings_provisions_two_instances_of_same_agent_independently() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);

    // Ignore mode takes the lazy skip path (no filesystem writes),
    // so both same-agent bindings provision deterministically.
    let bindings = vec![
        InstanceAuthBinding::new(
            "work",
            jackin_core::Agent::Claude,
            AuthForwardMode::Ignore,
            None,
        ),
        InstanceAuthBinding::new(
            "personal",
            jackin_core::Agent::Claude,
            AuthForwardMode::Ignore,
            None,
        ),
    ];

    let (state, selected_outcome) = RoleState::prepare_for_bindings(
        &paths,
        "jk-k7p9m2xq-agentsmith",
        &manifest,
        &bindings,
        &GithubAuthContext::default(),
        temp.path(),
        jackin_core::Agent::Claude,
    )
    .unwrap();

    assert_eq!(selected_outcome, AuthProvisionOutcome::Skipped);
    assert_eq!(state.auth.slots.len(), 2);
    let work_key = ProvisionedAuth::instance_key("work", jackin_core::Agent::Claude);
    let personal_key = ProvisionedAuth::instance_key("personal", jackin_core::Agent::Claude);
    let work = state
        .auth
        .slots
        .get(&work_key)
        .expect("work slot missing after multi-instance provision");
    let personal = state
        .auth
        .slots
        .get(&personal_key)
        .expect("personal slot missing after multi-instance provision");
    for (slot, account_id) in [(&work, "work"), (&personal, "personal")] {
        assert_eq!(slot.agent, jackin_core::Agent::Claude);
        assert_eq!(slot.account_id, account_id);
        assert_eq!(slot.mode, AuthForwardMode::Ignore);
        assert!(!slot.forward_auth);
        assert!(slot.home_dir.is_none());
    }
    // First binding keeps the legacy layout; the second gets suffixed
    // dirs so the two accounts never share a store or home.
    assert_eq!(work.slot_suffix, None);
    assert_eq!(work.container_home_rel, ".claude");
    assert_eq!(work.container_store_rel, "claude");
    assert_eq!(work.folder_target, "/home/agent/.claude");
    assert_eq!(personal.slot_suffix.as_deref(), Some("personal-claude"));
    assert_eq!(personal.container_home_rel, ".claude-personal-claude");
    assert_eq!(personal.container_store_rel, "claude-personal-claude");
    assert_eq!(
        personal.folder_target,
        "/home/agent/.claude-personal-claude"
    );
    assert_ne!(
        work.credential_paths, personal.credential_paths,
        "same-agent slots must not share credential paths"
    );
}

#[test]
fn parent_kind_slots_isolate_under_a_unique_parent() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);

    let bindings = vec![
        InstanceAuthBinding::new(
            "g1",
            jackin_core::Agent::Gemini,
            AuthForwardMode::Ignore,
            None,
        ),
        InstanceAuthBinding::new(
            "g2",
            jackin_core::Agent::Gemini,
            AuthForwardMode::Ignore,
            None,
        ),
    ];

    let (state, _) = RoleState::prepare_for_bindings(
        &paths,
        "jk-k7p9m2xq-agentsmith",
        &manifest,
        &bindings,
        &GithubAuthContext::default(),
        temp.path(),
        jackin_core::Agent::Gemini,
    )
    .unwrap();

    // `GEMINI_CLI_HOME` names the parent: the primary keeps the legacy
    // home with the agent-home target, the secondary gets a unique
    // parent whose `.gemini` child is its home.
    let primary_key = ProvisionedAuth::instance_key("g1", jackin_core::Agent::Gemini);
    let primary = state.auth.slots.get(&primary_key).unwrap();
    assert_eq!(primary.slot_suffix, None);
    assert_eq!(primary.container_home_rel, ".gemini");
    assert_eq!(primary.folder_target, "/home/agent");

    let secondary_key = ProvisionedAuth::instance_key("g2", jackin_core::Agent::Gemini);
    let secondary = state.auth.slots.get(&secondary_key).unwrap();
    assert_eq!(
        secondary.slot_suffix.as_deref(),
        Some("g2-gemini"),
        "secondary parent-kind slot keeps the sanitized key suffix"
    );
    assert_eq!(secondary.container_home_rel, ".gemini-g2-gemini/.gemini");
    assert_eq!(secondary.folder_target, "/home/agent/.gemini-g2-gemini");
}

#[test]
fn xdg_root_slots_export_the_durable_data_parent() {
    let (amp_home, amp_target) =
        slot_home_and_target(jackin_core::Agent::Amp, ".local/share/amp", None);
    assert_eq!(amp_home, ".local/share/amp");
    assert_eq!(amp_target, "/home/agent/.local/share");

    let (opencode_home, opencode_target) =
        slot_home_and_target(jackin_core::Agent::Opencode, ".local/share/opencode", None);
    assert_eq!(opencode_home, ".local/share/opencode");
    assert_eq!(opencode_target, "/home/agent/.local/share");
}

fn amp_binding_with_cache(key: &str, cache: PathBuf) -> InstanceAuthBinding {
    let mut binding =
        InstanceAuthBinding::new(key, jackin_core::Agent::Amp, AuthForwardMode::Ignore, None);
    binding.xdg_roots = Some(jackin_config::XdgRoots {
        data: cache.join("data"),
        config: cache.join("config"),
        cache,
    });
    binding
}

#[test]
fn xdg_overlap_guard_still_rejects_repeated_cache_roots() {
    let temp = tempdir().unwrap();
    let cache = temp.path().join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    let bindings = [
        amp_binding_with_cache("first", cache.clone()),
        amp_binding_with_cache("second", cache),
    ];

    let error = validate_selected_account_sources(&bindings, temp.path()).unwrap_err();
    assert!(error.to_string().contains("overlap"), "{error}");
}

#[test]
fn xdg_overlap_guard_rejects_dotdot_aliases() {
    let temp = tempdir().unwrap();
    let cache = temp.path().join("cache");
    std::fs::create_dir_all(cache.join("nested")).unwrap();
    let alias = cache.join("nested/..");
    let bindings = [
        amp_binding_with_cache("first", cache),
        amp_binding_with_cache("second", alias),
    ];

    let error = validate_selected_account_sources(&bindings, temp.path()).unwrap_err();
    assert!(error.to_string().contains("parent traversal"), "{error}");
}

#[cfg(unix)]
#[test]
fn xdg_overlap_guard_resolves_symlink_aliases() {
    let temp = tempdir().unwrap();
    let cache = temp.path().join("cache");
    let alias = temp.path().join("cache-alias");
    std::fs::create_dir_all(&cache).unwrap();
    std::os::unix::fs::symlink(&cache, &alias).unwrap();
    let bindings = [
        amp_binding_with_cache("first", cache),
        amp_binding_with_cache("second", alias),
    ];

    let error = validate_selected_account_sources(&bindings, temp.path()).unwrap_err();
    assert!(error.to_string().contains("overlap"), "{error}");
}

#[test]
fn amp_binding_provisions_credentials_from_selected_xdg_roots() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);
    let data = temp.path().join("selected-xdg/data");
    let config = temp.path().join("selected-xdg/config");
    let cache = temp.path().join("selected-xdg/cache");
    std::fs::create_dir_all(data.join("amp")).unwrap();
    std::fs::create_dir_all(config.join("amp")).unwrap();
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(
        data.join("amp/secrets.json"),
        r#"{"apiKey@https://ampcode.com/":"fixture-key"}"#,
    )
    .unwrap();
    std::fs::write(config.join("amp/settings.json"), r#"{"theme":"fixture"}"#).unwrap();

    let mut binding = InstanceAuthBinding::new(
        "selected",
        jackin_core::Agent::Amp,
        AuthForwardMode::Sync,
        None,
    );
    binding.xdg_roots = Some(jackin_config::XdgRoots {
        data,
        config,
        cache: cache.clone(),
    });
    let (state, _) = RoleState::prepare_for_bindings(
        &paths,
        "jk-selected-xdg",
        &manifest,
        &[binding],
        &GithubAuthContext::default(),
        temp.path().join("host-home").as_path(),
        jackin_core::Agent::Amp,
    )
    .unwrap();

    let slot = state
        .auth
        .slots
        .get("selected@amp")
        .expect("selected Amp slot missing");
    assert_eq!(slot.folder_target, "/home/agent/.local/share");
    assert_eq!(slot.cache_source_dir.as_deref(), Some(cache.as_path()));
    assert_eq!(slot.container_cache_rel.as_deref(), Some(".cache/amp"));
    assert_eq!(
        std::fs::read_to_string(state.root.join("amp/secrets.json")).unwrap(),
        r#"{"apiKey@https://ampcode.com/":"fixture-key"}"#
    );
    assert_eq!(
        std::fs::read_to_string(state.root.join("home/.config/amp/settings.json")).unwrap(),
        r#"{"theme":"fixture"}"#
    );
}

#[test]
fn opencode_binding_uses_selected_xdg_data_and_cache_roots() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);
    let data = temp.path().join("selected-opencode/data");
    let config = temp.path().join("selected-opencode/config");
    let cache = temp.path().join("selected-opencode/cache");
    std::fs::create_dir_all(data.join("opencode")).unwrap();
    std::fs::create_dir_all(config.join("opencode")).unwrap();
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(
        data.join("opencode/auth.json"),
        r#"{"opencode-go":{"type":"api","key":"fixture-key"}}"#,
    )
    .unwrap();

    let mut binding = InstanceAuthBinding::new(
        "selected",
        jackin_core::Agent::Opencode,
        AuthForwardMode::Sync,
        None,
    );
    binding.xdg_roots = Some(jackin_config::XdgRoots {
        data,
        config,
        cache: cache.clone(),
    });
    let (state, _) = RoleState::prepare_for_bindings(
        &paths,
        "jk-selected-opencode",
        &manifest,
        &[binding],
        &GithubAuthContext::default(),
        temp.path().join("host-home").as_path(),
        jackin_core::Agent::Opencode,
    )
    .unwrap();

    let slot = state
        .auth
        .slots
        .get("selected@opencode")
        .expect("selected OpenCode slot missing");
    assert_eq!(slot.cache_source_dir.as_deref(), Some(cache.as_path()));
    assert_eq!(slot.container_cache_rel.as_deref(), Some(".cache/opencode"));
    let staged: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(state.root.join("opencode/auth.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        staged
            .pointer("/opencode-go/type")
            .and_then(|value| value.as_str()),
        Some("api")
    );
    assert_eq!(
        staged
            .pointer("/opencode-go/key")
            .and_then(|value| value.as_str()),
        Some("fixture-key")
    );
}

#[cfg(unix)]
#[test]
fn xdg_cache_overlap_rejects_parent_traversal_and_symlink_aliases() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let real = temp.path().join("real-cache");
    let alias = temp.path().join("cache-alias");
    std::fs::create_dir_all(&real).unwrap();
    symlink(&real, &alias).unwrap();

    let binding_for = |key: &str, cache: PathBuf| {
        let mut binding =
            InstanceAuthBinding::new(key, jackin_core::Agent::Amp, AuthForwardMode::Ignore, None);
        binding.xdg_roots = Some(jackin_config::XdgRoots {
            data: temp.path().join(format!("{key}-data")),
            config: temp.path().join(format!("{key}-config")),
            cache,
        });
        binding
    };

    let error = validate_selected_account_sources(
        &[
            binding_for("first", real.clone()),
            binding_for("second", alias),
        ],
        temp.path(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("overlap"), "{error:#}");

    let traversal = real.join("..").join("real-cache");
    let error =
        validate_selected_account_sources(&[binding_for("traversal", traversal)], temp.path())
            .unwrap_err();
    assert!(error.to_string().contains("parent traversal"), "{error:#}");
}

#[test]
fn colliding_sanitized_suffixes_get_numeric_tails() {
    // `a@b` and `a-b` sanitize identically; secondary slots must still
    // land in distinct dirs.
    let binding_for = |key: &str| {
        let mut binding = InstanceAuthBinding::new(
            "work",
            jackin_core::Agent::Claude,
            AuthForwardMode::Ignore,
            None,
        );
        binding.key = key.to_owned();
        binding
    };
    let bindings = [
        binding_for("primary"),
        binding_for("a@b"),
        binding_for("a-b"),
    ];
    let suffixes = slot_suffixes(&bindings);
    assert_eq!(suffixes[0], None);
    assert_eq!(suffixes[1].as_deref(), Some("a-b"));
    assert_eq!(suffixes[2].as_deref(), Some("a-b-2"));
    // Dedupe is per agent: a codex secondary keeps `a-b` even
    // though a claude secondary already owns it; store dirs are per
    // agent so they cannot collide.
    let mut codex_primary = binding_for("codex-primary");
    codex_primary.agent = jackin_core::Agent::Codex;
    let mut codex_secondary = binding_for("a@b");
    codex_secondary.agent = jackin_core::Agent::Codex;
    let mixed = [
        bindings[0].clone(),
        bindings[1].clone(),
        codex_primary,
        codex_secondary,
    ];
    let suffixes = slot_suffixes(&mixed);
    assert_eq!(suffixes[0], None);
    assert_eq!(suffixes[1].as_deref(), Some("a-b"));
    assert_eq!(suffixes[2], None);
    assert_eq!(suffixes[3].as_deref(), Some("a-b"));
}

#[test]
fn prepare_for_bindings_honors_explicit_config_id_keys() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);

    let mut binding = InstanceAuthBinding::new(
        "work",
        jackin_core::Agent::Claude,
        AuthForwardMode::Ignore,
        None,
    );
    binding.key = "work-claude".to_owned();

    let (state, _) = RoleState::prepare_for_bindings(
        &paths,
        "jk-k7p9m2xq-agentsmith",
        &manifest,
        std::slice::from_ref(&binding),
        &GithubAuthContext::default(),
        temp.path(),
        jackin_core::Agent::Claude,
    )
    .unwrap();

    assert_eq!(state.auth.slots.len(), 1);
    let slot = state
        .auth
        .slots
        .get("work-claude")
        .expect("explicit key lost");
    assert_eq!(slot.account_id, "work");
    // Agent-scoped lookups still resolve through the explicit key.
    assert!(state.claude_account_json().is_some());
    assert!(state.claude_credentials_json().is_some());
}

#[test]
fn prewarm_auth_for_bindings_provisions_each_binding_once() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    let bindings = vec![
        InstanceAuthBinding::new(
            "work",
            jackin_core::Agent::Codex,
            AuthForwardMode::Ignore,
            None,
        ),
        InstanceAuthBinding::new(
            "personal",
            jackin_core::Agent::Codex,
            AuthForwardMode::Ignore,
            None,
        ),
    ];

    let count = RoleState::prewarm_auth_for_bindings(
        &paths,
        "jk-k7p9m2xq-agentsmith",
        &bindings,
        temp.path(),
    )
    .unwrap();

    assert_eq!(count, 2);
}
