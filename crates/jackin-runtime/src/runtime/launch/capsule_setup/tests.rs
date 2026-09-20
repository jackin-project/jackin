// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_config::{
    AccountConfig, AccountCredential, AiProvider, AppConfig, AuthForwardMode, ProfileSelector,
};
use jackin_core::Agent;

fn api_key_account(provider: AiProvider, model: Option<&str>) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: "Test".into(),
        provider,
        credential: AccountCredential::ApiKey {
            value: "fixture-key".into(),
            base_url: None,
            model: model.map(str::to_owned),
        },
    }
}

fn instance(config_id: &str, agent: Agent, account_id: &str) -> jackin_config::ResolvedInstance {
    jackin_config::ResolvedInstance {
        config_id: config_id.into(),
        agent,
        account_id: account_id.into(),
        model: None,
        base_url: None,
        xdg_roots: None,
        label: config_id.into(),
        synthesized: true,
    }
}

fn manifest_with(temp: &tempfile::TempDir, agents: &[&str]) -> jackin_manifest::RoleManifest {
    let mut role = format!(
        "version = \"v1alpha5\"\ndockerfile = \"Dockerfile\"\nagents = [{}]\n",
        agents
            .iter()
            .map(|agent| format!("\"{agent}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    for agent in agents {
        role.push_str(&format!("\n[{agent}]\n"));
        if *agent == "claude" {
            role.push_str("model = \"sonnet\"\n");
        }
    }
    std::fs::write(temp.path().join("jackin.role.toml"), role).unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    jackin_manifest::load_role_manifest(temp.path()).unwrap()
}

#[test]
fn selections_are_keyed_by_config_id() {
    let mut config = AppConfig::default();
    config
        .accounts
        .insert("work".into(), api_key_account(AiProvider::Anthropic, None));
    config.accounts.insert(
        "personal".into(),
        AccountConfig {
            enabled: true,
            name: "Personal".into(),
            provider: AiProvider::OpenAi,
            credential: AccountCredential::Profile {
                agent: Agent::Codex,
                directory: "/accounts/personal".into(),
                xdg_roots: None,
                source_selector: None,
            },
        },
    );
    let instances = vec![
        instance("claude-work", Agent::Claude, "work"),
        instance("codex-personal", Agent::Codex, "personal"),
    ];
    let selections = account_auth_selections(&config, &instances).unwrap();
    assert_eq!(selections.len(), 2);
    assert_eq!(selections["claude-work"], (AuthForwardMode::ApiKey, None));
    assert_eq!(
        selections["codex-personal"],
        (AuthForwardMode::Sync, Some("/accounts/personal".into()))
    );
}

#[test]
fn selections_reject_unknown_accounts() {
    let config = AppConfig::default();
    account_auth_selections(&config, &[instance("ghost@codex", Agent::Codex, "ghost")])
        .unwrap_err();
}

#[test]
fn auth_modes_are_keyed_by_config_id() {
    let mut config = AppConfig::default();
    config
        .accounts
        .insert("work".into(), api_key_account(AiProvider::Anthropic, None));
    config.accounts.insert(
        "personal".into(),
        AccountConfig {
            enabled: true,
            name: "Personal".into(),
            provider: AiProvider::Anthropic,
            credential: AccountCredential::OAuthToken {
                agent: Agent::Claude,
                value: "fixture-token".into(),
            },
        },
    );
    let modes = capsule_auth_modes(
        &config,
        &[
            instance("claude-work", Agent::Claude, "work"),
            instance("claude-personal", Agent::Claude, "personal"),
        ],
    )
    .unwrap();
    assert_eq!(modes.len(), 2);
    assert_eq!(modes["claude-work"], AuthForwardMode::ApiKey.to_string());
    assert_eq!(
        modes["claude-personal"],
        AuthForwardMode::OAuthToken.to_string()
    );
}

#[test]
fn account_models_use_effective_instance_model_per_instance() {
    let mut config = AppConfig::default();
    config.accounts.insert(
        "work".into(),
        api_key_account(AiProvider::OpenAi, Some("account-model")),
    );
    config.accounts.insert(
        "other".into(),
        api_key_account(AiProvider::OpenAi, Some("other-model")),
    );
    let mut first = instance("codex-work", Agent::Codex, "work");
    first.model = Some("config-override".into());
    let mut second = instance("codex-other", Agent::Codex, "other");
    second.model = Some("other-model".into());
    let mut launch = jackin_protocol::CapsuleConfig {
        models: std::collections::BTreeMap::from([("codex-work".into(), "native-model".into())]),
        ..Default::default()
    };
    apply_account_models(&mut launch, &config, &[first, second]).unwrap();
    assert_eq!(launch.models["codex-work"], "config-override");
    assert_eq!(launch.models["codex-other"], "other-model");
}

#[test]
fn account_models_prefix_opencode_and_skip_missing() {
    let mut config = AppConfig::default();
    config.accounts.insert(
        "work".into(),
        api_key_account(AiProvider::Zai, Some("glm-5.3")),
    );
    config
        .accounts
        .insert("plain".into(), api_key_account(AiProvider::OpenAi, None));
    let mut opencode = instance("opencode-work", Agent::Opencode, "work");
    opencode.model = Some("glm-5.3".into());
    let codex = instance("codex-plain", Agent::Codex, "plain");
    let mut launch = jackin_protocol::CapsuleConfig {
        models: std::collections::BTreeMap::from([("codex-plain".into(), "native-model".into())]),
        ..Default::default()
    };
    apply_account_models(&mut launch, &config, &[opencode, codex]).unwrap();
    assert_eq!(launch.models["opencode-work"], "zai-coding-plan/glm-5.3");
    assert_eq!(launch.models["codex-plain"], "native-model");
}

#[test]
fn launch_model_and_effort_overrides_fan_out_to_every_codex_slot() {
    let temp = tempfile::tempdir().unwrap();
    let manifest = manifest_with(&temp, &["codex"]);
    let mut config = AppConfig::default();
    config
        .accounts
        .insert("work".into(), api_key_account(AiProvider::OpenAi, None));
    config
        .accounts
        .insert("personal".into(), api_key_account(AiProvider::OpenAi, None));
    let instances = [
        instance("codex-work", Agent::Codex, "work"),
        instance("codex-personal", Agent::Codex, "personal"),
    ];

    let models = resolved_instance_models(
        &config,
        &manifest,
        &instances,
        Agent::Codex,
        Some("gpt-5.6-luna"),
    )
    .unwrap();
    let efforts = resolved_instance_efforts(
        &instances,
        Agent::Codex,
        Some(jackin_core::ReasoningEffort::Max),
    );

    assert_eq!(
        models,
        std::collections::BTreeMap::from([
            ("codex-work".into(), "gpt-5.6-luna".into()),
            ("codex-personal".into(), "gpt-5.6-luna".into()),
        ])
    );
    assert_eq!(
        efforts,
        std::collections::BTreeMap::from([
            ("codex-work".into(), "max".into()),
            ("codex-personal".into(), "max".into()),
        ])
    );
}

#[test]
fn capsule_config_fans_manifest_models_out_per_instance() {
    let temp = tempfile::tempdir().unwrap();
    let manifest = manifest_with(&temp, &["claude", "codex"]);
    let selector = jackin_core::RoleSelector::new(Some("chainargos"), "the-architect");
    let config = capsule_config(
        &selector,
        "/workspace",
        &manifest,
        "ask",
        Vec::new(),
        Vec::new(),
        Vec::new(),
        &[
            instance("claude-work", Agent::Claude, "work"),
            instance("claude-personal", Agent::Claude, "personal"),
        ],
    );
    assert_eq!(config.role, "chainargos/the-architect");
    assert_eq!(config.workdir, "/workspace");
    assert_eq!(config.instances, vec!["claude-work", "claude-personal"]);
    assert_eq!(
        config.agents,
        std::collections::BTreeMap::from([
            ("claude-work".into(), "claude".into()),
            ("claude-personal".into(), "claude".into()),
        ])
    );
    assert_eq!(config.agent_for_instance("claude-work"), Some("claude"));
    assert_eq!(config.agent_for_instance("unknown"), None);
    assert_eq!(
        config.models,
        std::collections::BTreeMap::from([
            ("claude-work".into(), "sonnet".into()),
            ("claude-personal".into(), "sonnet".into()),
        ])
    );
}

#[test]
fn capsule_config_carries_instance_accounts_and_labels() {
    let temp = tempfile::tempdir().unwrap();
    let manifest = manifest_with(&temp, &["claude"]);
    let selector = jackin_core::RoleSelector::new(Some("chainargos"), "the-architect");
    let mut work = instance("claude-work", Agent::Claude, "work");
    work.label = "Claude · Work".into();
    let mut personal = instance("claude-personal", Agent::Claude, "personal");
    personal.label = "Personal Claude".into();
    let config = capsule_config(
        &selector,
        "/workspace",
        &manifest,
        "ask",
        Vec::new(),
        Vec::new(),
        Vec::new(),
        &[work, personal],
    );
    assert_eq!(
        config.accounts,
        std::collections::BTreeMap::from([
            ("claude-work".into(), "work".into()),
            ("claude-personal".into(), "personal".into()),
        ])
    );
    assert_eq!(config.account_for_instance("claude-work"), Some("work"));
    assert_eq!(config.account_for_instance("unknown"), None);
    // Operator label overrides pass through verbatim; names only, never secrets.
    assert_eq!(
        config.labels,
        std::collections::BTreeMap::from([
            ("claude-work".into(), "Claude · Work".into()),
            ("claude-personal".into(), "Personal Claude".into()),
        ])
    );
    assert_eq!(
        config.label_for_instance("claude-personal"),
        Some("Personal Claude")
    );
    assert_eq!(config.label_for_instance("unknown"), None);
}

#[test]
fn capsule_config_carries_workspace_mounts_and_worktree_git_targets() {
    let temp = tempfile::tempdir().unwrap();
    let manifest = manifest_with(&temp, &["claude"]);
    let selector = jackin_core::RoleSelector::new(Some("chainargos"), "the-architect");
    let config = capsule_config(
        &selector,
        "/workspace/project",
        &manifest,
        "ask",
        vec!["/workspace/other".to_owned()],
        vec![
            "/workspace/project".to_owned(),
            "/workspace/other".to_owned(),
        ],
        vec!["/jackin/host/workspace/other/.git".to_owned()],
        &[instance("claude-work", Agent::Claude, "work")],
    );
    assert_eq!(config.isolated_worktrees, vec!["/workspace/other"]);
    assert_eq!(
        config.workspace_mounts,
        vec!["/workspace/project", "/workspace/other"]
    );
    assert_eq!(
        config.worktree_git_targets,
        vec!["/jackin/host/workspace/other/.git"]
    );
}

#[test]
fn instance_bindings_keep_launch_order_and_config_id_keys() {
    let mut config = AppConfig::default();
    config
        .accounts
        .insert("work".into(), api_key_account(AiProvider::Anthropic, None));
    config.accounts.insert(
        "personal".into(),
        AccountConfig {
            enabled: true,
            name: "Personal".into(),
            provider: AiProvider::OpenAi,
            credential: AccountCredential::Profile {
                agent: Agent::Codex,
                directory: "/accounts/personal".into(),
                xdg_roots: None,
                source_selector: None,
            },
        },
    );
    // Launch order deliberately differs from key sort order.
    let instances = vec![
        instance("codex-personal", Agent::Codex, "personal"),
        instance("claude-work", Agent::Claude, "work"),
    ];
    let bindings = instance_auth_bindings(&config, &instances).unwrap();
    assert_eq!(bindings.len(), 2);
    assert_eq!(bindings[0].key, "codex-personal");
    assert_eq!(bindings[0].mode, AuthForwardMode::Sync);
    assert_eq!(
        bindings[0].sync_source_dir,
        Some(std::path::PathBuf::from("/accounts/personal"))
    );
    assert_eq!(bindings[1].key, "claude-work");
    assert_eq!(bindings[1].mode, AuthForwardMode::ApiKey);

    let missing = vec![instance("ghost", Agent::Claude, "nope")];
    instance_auth_bindings(&config, &missing).unwrap_err();
}

#[test]
fn opencode_profile_binding_carries_source_provider_identity() {
    let mut config = AppConfig::default();
    config.accounts.insert(
        "zai-profile".into(),
        AccountConfig {
            enabled: true,
            name: "OpenCode Zai".into(),
            provider: AiProvider::Zai,
            credential: AccountCredential::Profile {
                agent: Agent::Opencode,
                directory: "/profiles/opencode".into(),
                xdg_roots: None,
                source_selector: None,
            },
        },
    );

    let bindings = instance_auth_bindings(
        &config,
        &[instance("opencode-zai", Agent::Opencode, "zai-profile")],
    )
    .unwrap();
    assert_eq!(bindings[0].source_provider, Some(AiProvider::Zai));
    assert_eq!(
        bindings[0].sync_source_dir,
        Some("/profiles/opencode".into())
    );
}

#[test]
fn omp_profile_binding_carries_immutable_store_selector() {
    let mut config = AppConfig::default();
    config.accounts.insert(
        "omp-work".into(),
        AccountConfig {
            enabled: true,
            name: "Omp work".into(),
            provider: AiProvider::OpenAi,
            credential: AccountCredential::Profile {
                agent: Agent::Omp,
                directory: "/profiles/omp".into(),
                xdg_roots: None,
                source_selector: Some(ProfileSelector {
                    entry: "openai".into(),
                    profile: Some("work".into()),
                }),
            },
        },
    );

    let bindings =
        instance_auth_bindings(&config, &[instance("omp-work", Agent::Omp, "omp-work")]).unwrap();
    assert_eq!(bindings[0].source_provider, Some(AiProvider::OpenAi));
    assert_eq!(
        bindings[0].source_selector,
        Some(ProfileSelector {
            entry: "openai".into(),
            profile: Some("work".into()),
        })
    );
}

#[test]
fn instance_bindings_carry_roots_only_for_selected_instances() {
    let roots = jackin_config::XdgRoots {
        data: "/selected/data".into(),
        config: "/selected/config".into(),
        cache: "/selected/cache".into(),
    };
    let mut config = AppConfig::default();
    for account_id in ["selected", "unselected"] {
        config.accounts.insert(
            account_id.into(),
            AccountConfig {
                enabled: true,
                name: account_id.into(),
                provider: AiProvider::Amp,
                credential: AccountCredential::Profile {
                    agent: Agent::Amp,
                    directory: format!("/{account_id}/amp").into(),
                    xdg_roots: Some(roots.clone()),
                    source_selector: None,
                },
            },
        );
    }

    let mut selected = instance("amp-selected", Agent::Amp, "selected");
    selected.xdg_roots = Some(roots.clone());
    let bindings = instance_auth_bindings(&config, &[selected]).unwrap();

    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].account_id, "selected");
    assert_eq!(bindings[0].xdg_roots, Some(roots));
    assert!(
        !bindings
            .iter()
            .any(|binding| binding.account_id == "unselected")
    );
}

#[test]
fn instance_dirs_come_from_slots_and_fail_closed() {
    use crate::instance::ProvisionedInstanceAuth;
    let slot = |suffix: Option<&str>, home_rel: &str, store_rel: &str| ProvisionedInstanceAuth {
        agent: Agent::Claude,
        account_id: "work".into(),
        mode: AuthForwardMode::Sync,
        home_dir: None,
        credential_paths: Vec::new(),
        forward_auth: true,
        slot_suffix: suffix.map(str::to_owned),
        container_home_rel: home_rel.into(),
        container_store_rel: store_rel.into(),
        folder_target: format!("/home/agent/{home_rel}"),
        cache_source_dir: None,
        container_cache_rel: None,
    };
    let slots = std::collections::BTreeMap::from([
        ("claude-work".to_owned(), slot(None, ".claude", "claude")),
        (
            "claude-personal".to_owned(),
            slot(
                Some("claude-personal"),
                ".claude-claude-personal",
                "claude-claude-personal",
            ),
        ),
    ]);
    let instances = vec![
        instance("claude-work", Agent::Claude, "work"),
        instance("claude-personal", Agent::Claude, "personal"),
    ];
    let mut config = jackin_protocol::CapsuleConfig::default();
    apply_instance_dirs(&mut config, &instances, &slots).unwrap();
    assert_eq!(
        config.home_for_instance("claude-work"),
        Some("/home/agent/.claude")
    );
    assert_eq!(
        config.home_for_instance("claude-personal"),
        Some("/home/agent/.claude-claude-personal")
    );
    assert_eq!(
        config.forwarded_for_instance("claude-work"),
        Some("/jackin/claude")
    );
    assert_eq!(
        config.forwarded_for_instance("claude-personal"),
        Some("/jackin/claude-claude-personal")
    );
    assert_eq!(
        config.credential_file_for_instance("claude-work"),
        Some("/jackin/account-credentials/acct-636c617564652d776f726b.json")
    );
    assert_eq!(
        config.identity_for_instance("claude-work").unwrap().uid,
        2_000
    );
    assert_eq!(
        config.identity_for_instance("claude-personal").unwrap().uid,
        2_001
    );
    assert_eq!(config.shell_identity.unwrap().uid, 2_002);
    assert!(
        config
            .mount_paths_for_instance("claude-work")
            .iter()
            .all(|path| !path.starts_with(jackin_protocol::ACCOUNT_CREDENTIALS_DIR))
    );

    let mut config = jackin_protocol::CapsuleConfig::default();
    let missing = vec![instance("ghost", Agent::Claude, "work")];
    apply_instance_dirs(&mut config, &missing, &slots).unwrap_err();
}

#[test]
fn amp_instance_dir_exports_the_durable_data_parent() {
    use crate::instance::ProvisionedInstanceAuth;

    let slot = ProvisionedInstanceAuth {
        agent: Agent::Amp,
        account_id: "amp".into(),
        mode: AuthForwardMode::Sync,
        home_dir: None,
        credential_paths: Vec::new(),
        forward_auth: true,
        slot_suffix: None,
        container_home_rel: ".local/share/amp".into(),
        container_store_rel: "amp".into(),
        folder_target: "/home/agent/.local/share".into(),
        cache_source_dir: Some("/tmp/amp-cache".into()),
        container_cache_rel: Some(".cache/amp".into()),
    };
    let slots = std::collections::BTreeMap::from([("amp".to_owned(), slot)]);
    let instances = vec![instance("amp", Agent::Amp, "amp")];
    let mut config = jackin_protocol::CapsuleConfig::default();

    apply_instance_dirs(&mut config, &instances, &slots).unwrap();

    assert_eq!(
        config.home_for_instance("amp"),
        Some("/home/agent/.local/share")
    );
    assert_eq!(config.forwarded_for_instance("amp"), Some("/jackin/amp"));
    assert_eq!(
        config.cache_for_instance("amp"),
        Some("/home/agent/.cache/amp")
    );
    assert!(
        config
            .mount_paths_for_instance("amp")
            .contains(&"/home/agent/.cache/amp".to_owned())
    );
}
