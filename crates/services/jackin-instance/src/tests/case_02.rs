// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
