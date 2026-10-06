// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn agent_mounts_for_codex_without_auth_mounts_state_but_no_auth_handoff() {
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

    let mounts = agent_mounts(&state).unwrap();
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
    crate::runtime::stubs::install_all_test_stubs(&paths);
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

    let mounts = agent_mounts(&state).unwrap();
    assert!(
        mounts.iter().any(|m| m.contains(":/home/agent/.codex")),
        "durable Codex home mount missing: {mounts:?}"
    );
    assert!(
        mounts
            .iter()
            .any(|m| m.contains("/jackin/codex/auth.json") && m.ends_with(":ro")),
        "auth.json handoff missing: {mounts:?}"
    );
}

#[tokio::test]
async fn agent_mounts_for_two_claude_slots_isolates_homes_and_handoffs() {
    use crate::instance::{InstanceAuthBinding, RoleState};
    use jackin_core::Agent;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let manifest_temp = tempdir().unwrap();
    std::fs::write(
        manifest_temp.path().join("jackin.role.toml"),
        "version = \"v1alpha3\"\ndockerfile = \"Dockerfile\"\nagents = [\"claude\"]\n\n[claude]\n",
    )
    .unwrap();
    std::fs::write(
        manifest_temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(manifest_temp.path()).unwrap();

    // Two distinct host Claude profiles plus a host onboarding file.
    let host_home = temp.path().join("host_home");
    std::fs::create_dir_all(&host_home).unwrap();
    std::fs::write(host_home.join(".claude.json"), "{}").unwrap();
    let mut bindings = Vec::new();
    for (config_id, account, marker) in [
        ("claude-work", "work", "work-oauth"),
        ("claude-personal", "personal", "personal-oauth"),
    ] {
        let source = temp.path().join(format!("src-{account}"));
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(
            source.join(".claude.json"),
            format!("{{\"oauthAccount\":{{\"accountId\":\"{marker}\"}}}}"),
        )
        .unwrap();
        std::fs::write(
            source.join(".credentials.json"),
            format!("{{\"claudeAiOauth\":{{\"marker\":\"{marker}\"}}}}"),
        )
        .unwrap();
        let mut binding = InstanceAuthBinding::new(
            account,
            Agent::Claude,
            jackin_config::AuthForwardMode::Sync,
            Some(source),
        );
        binding.key = config_id.to_owned();
        bindings.push(binding);
    }

    let (state, _) = RoleState::prepare_for_bindings(
        &paths,
        "jk-agent-smith",
        &manifest,
        &bindings,
        &crate::instance::GithubAuthContext::default(),
        &host_home,
        Agent::Claude,
    )
    .unwrap();

    let mounts = agent_mounts(&state).unwrap();
    // Primary keeps legacy destinations; the secondary gets suffixed
    // home + handoff dirs.
    for expected in [
        ":/home/agent/.claude",
        ":/home/agent/.claude-claude-personal",
        "/jackin/claude/credentials.json",
        "/jackin/claude-claude-personal/credentials.json",
        "/jackin/claude/account.json",
        "/jackin/claude-claude-personal/account.json",
    ] {
        assert!(
            mounts.iter().any(|m| m.contains(expected)),
            "mount {expected} missing: {mounts:?}"
        );
    }
    for mount in mounts
        .iter()
        .filter(|mount| mount.contains(":/jackin/claude"))
    {
        assert!(
            mount.ends_with(":ro"),
            "every Claude auth handoff must be read-only: {mount}"
        );
    }
    std::fs::create_dir_all(state.root.join("credentials")).unwrap();
    let apple_mounts = apple_agent_mounts(&state).unwrap();
    assert!(apple_mounts.iter().any(|mount| {
        mount.target == std::path::Path::new(jackin_protocol::ACCOUNT_CREDENTIALS_DIR)
            && mount.readonly
    }));
    for mount in apple_mounts
        .iter()
        .filter(|mount| mount.target.to_string_lossy().starts_with("/jackin/claude"))
    {
        assert!(
            mount.readonly,
            "Apple auth store must be read-only: {mount:?}"
        );
    }
    // The two slots stage their own source credentials, not copies
    // of each other.
    for (store, marker) in [
        ("claude", "work-oauth"),
        ("claude-claude-personal", "personal-oauth"),
    ] {
        let staged = std::fs::read_to_string(state.root.join(format!("{store}/credentials.json")))
            .unwrap_or_else(|_| panic!("{store}/credentials.json missing: {mounts:?}"));
        assert!(
            staged.contains(marker),
            "{store} staged the wrong account: {staged}"
        );
    }
}

#[tokio::test]
async fn agent_mounts_for_codex_host_missing_omits_auth_json() {
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

    let mounts = agent_mounts(&state).unwrap();
    assert!(
        mounts.iter().any(|m| m.contains(":/home/agent/.codex")),
        "durable Codex home mount missing: {mounts:?}"
    );
    assert!(
        !mounts.iter().any(|m| m.contains("/jackin/codex/auth.json")),
        "no auth.json handoff when host has no ~/.codex/auth.json: {mounts:?}"
    );
}

#[test]
fn codex_source_auth_rerun_and_prewarm_fail_closed_after_persisted_state() {
    use crate::instance::{AuthProvisionOutcome, PrepareResolvers, RoleState};
    use jackin_core::Agent;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
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

    let host_home = temp.path().join("host_home");
    std::fs::create_dir_all(host_home.join(".codex")).unwrap();
    let resolvers = PrepareResolvers {
        auth_modes: &|_| jackin_config::AuthForwardMode::Sync,
        sync_source_dirs: &|_| None,
    };
    let host_auth = host_home.join(".codex/auth.json");

    for invalid_source in ["", " \n\t"] {
        std::fs::write(&host_auth, "{\"token\":\"valid\"}").unwrap();
        let (state, outcome) = RoleState::prepare(
            &paths,
            "jk-agent-smith",
            &manifest,
            &resolvers,
            &crate::instance::GithubAuthContext::default(),
            &host_home,
            Agent::Codex,
        )
        .unwrap();
        assert_eq!(outcome, AuthProvisionOutcome::Synced);
        let target = state.root.join("codex/auth.json");
        assert!(
            agent_mounts(&state)
                .unwrap()
                .iter()
                .any(|mount| mount.contains("/jackin/codex/auth.json")),
            "valid persisted Codex auth must be mounted"
        );
        drop(state);

        std::fs::write(&host_auth, invalid_source).unwrap();
        assert_eq!(
            RoleState::prewarm_auth_for_agents(
                &paths,
                "jk-agent-smith",
                &manifest,
                &resolvers,
                &host_home,
                &[Agent::Codex],
            )
            .unwrap(),
            1
        );
        assert!(
            !target.exists(),
            "invalid source must invalidate persisted Codex auth during prewarm"
        );

        let (state, outcome) = RoleState::prepare(
            &paths,
            "jk-agent-smith",
            &manifest,
            &resolvers,
            &crate::instance::GithubAuthContext::default(),
            &host_home,
            Agent::Codex,
        )
        .unwrap();
        assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
        let mounts = agent_mounts(&state).unwrap();
        assert!(
            !mounts
                .iter()
                .any(|mount| mount.contains("/jackin/codex/auth.json")),
            "invalid Codex source must not regain a stale auth mount: {mounts:?}"
        );
        assert!(
            state.auth_mount_paths.is_empty(),
            "invalid Codex source must not acquire an auth mount lease"
        );
    }
}
