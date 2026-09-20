// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{ConfigEditor, WorkspaceConfig, WorkspaceRoleOverride};
use jackin_core::JackinPaths;

fn profile(name: &str) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: name.into(),
        provider: AiProvider::Anthropic,
        credential: AccountCredential::Profile {
            agent: Agent::Claude,
            directory: PathBuf::from("/profiles").join(name),
            xdg_roots: None,
            source_selector: None,
        },
    }
}
fn config() -> (AppConfig, WorkspaceName) {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert("personal".into(), profile("Personal"));
    cfg.accounts.insert("work".into(), profile("Work"));
    let ws = WorkspaceName::parse("project").unwrap();
    cfg.workspaces
        .insert(ws.as_str().into(), WorkspaceConfig::default());
    (cfg, ws)
}
#[test]
fn unauthorized_global_binding_does_not_fall_back_to_workspace_account() {
    let (mut cfg, ws) = config();
    cfg.workspaces.get_mut(ws.as_str()).unwrap().accounts = vec!["work".into()];
    cfg.account_bindings
        .insert(Agent::Claude, "personal".into());
    let error = resolve_account(&cfg, Agent::Claude, Some(&ws), "").unwrap_err();
    assert!(error.to_string().contains("not assigned"), "{error}");
    assert_eq!(
        resolve_account(&cfg, Agent::Claude, None, "")
            .unwrap()
            .unwrap()
            .name,
        "Personal"
    );
}

#[test]
fn authorized_global_binding_is_inherited_by_workspace() {
    let (mut cfg, ws) = config();
    cfg.workspaces.get_mut(ws.as_str()).unwrap().accounts = vec!["personal".into(), "work".into()];
    cfg.account_bindings
        .insert(Agent::Claude, "personal".into());

    let resolved = resolve_account(&cfg, Agent::Claude, Some(&ws), "")
        .unwrap()
        .unwrap();
    assert_eq!(resolved.name, "Personal");
}
#[test]
fn sole_allowed_account_selected_and_ambiguity_rejected() {
    let (mut cfg, ws) = config();
    cfg.workspaces.get_mut(ws.as_str()).unwrap().accounts = vec!["personal".into()];
    assert_eq!(
        resolve_account(&cfg, Agent::Claude, Some(&ws), "")
            .unwrap()
            .unwrap()
            .name,
        "Personal"
    );
    cfg.workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .accounts
        .push("work".into());
    resolve_account(&cfg, Agent::Claude, Some(&ws), "").unwrap_err();
}
#[test]
fn role_binding_wins_but_cannot_escape_allowlist() {
    let (mut cfg, ws) = config();
    let workspace = cfg.workspaces.get_mut(ws.as_str()).unwrap();
    workspace.accounts = vec!["personal".into(), "work".into()];
    workspace
        .account_bindings
        .insert(Agent::Claude, "personal".into());
    workspace.roles.insert(
        "role".into(),
        WorkspaceRoleOverride {
            account_bindings: BTreeMap::from([(Agent::Claude, "work".into())]),
            ..Default::default()
        },
    );
    assert_eq!(
        resolve_account(&cfg, Agent::Claude, Some(&ws), "role")
            .unwrap()
            .unwrap()
            .name,
        "Work"
    );
    cfg.workspaces.get_mut(ws.as_str()).unwrap().accounts = vec!["personal".into()];
    cfg.validate_accounts().unwrap_err();
    resolve_account(&cfg, Agent::Claude, Some(&ws), "role").unwrap_err();
}
#[test]
fn secrets_are_redacted_and_provider_routing_is_explicit() {
    let account = AccountConfig {
        enabled: true,
        name: "Work".into(),
        provider: AiProvider::Moonshot,
        credential: AccountCredential::ApiKey {
            value: EnvValue::from("SECRET-SENTINEL"),
            base_url: None,
            model: Some("k3".into()),
        },
    };
    assert!(!format!("{account:?}").contains("SECRET-SENTINEL"));
    let env = account.credential_env(Agent::Claude).unwrap();
    assert_eq!(
        env.get("ANTHROPIC_BASE_URL"),
        Some(&EnvValue::from("https://api.kimi.com/coding"))
    );
    assert_eq!(
        env.get("ANTHROPIC_AUTH_TOKEN"),
        Some(&EnvValue::from("SECRET-SENTINEL"))
    );
    assert!(!account.supports_agent(Agent::Amp));
}

#[test]
fn oauth_token_instance_endpoint_is_routed() {
    let account = AccountConfig {
        enabled: true,
        name: "Claude subscription".into(),
        provider: AiProvider::Anthropic,
        credential: AccountCredential::OAuthToken {
            agent: Agent::Claude,
            value: EnvValue::from("oauth-token"),
        },
    };

    let env = account
        .credential_env_for_instance(Agent::Claude, Some("https://proxy.example/v1"))
        .unwrap();

    assert_eq!(
        env,
        BTreeMap::from([
            (
                "ANTHROPIC_BASE_URL".into(),
                EnvValue::from("https://proxy.example/v1"),
            ),
            (
                "CLAUDE_CODE_OAUTH_TOKEN".into(),
                EnvValue::from("oauth-token"),
            ),
        ])
    );
}

#[test]
fn invalid_ids_and_on_demand_credentials_rejected() {
    for id in ["", "../x", "X", "-start", "with space"] {
        assert!(validate_account_id(id).is_err());
    }
    let mut cfg = AppConfig::default();
    cfg.accounts.insert(
        "bad".into(),
        AccountConfig {
            enabled: true,
            name: "Bad".into(),
            provider: AiProvider::Anthropic,
            credential: AccountCredential::ApiKey {
                value: EnvValue::from(""),
                base_url: None,
                model: None,
            },
        },
    );
    cfg.validate_accounts().unwrap_err();
}
#[test]
fn first_start_discovers_once_and_does_not_grant_workspace_access() {
    let temp = tempfile::TempDir::new().unwrap();
    let paths = JackinPaths::resolve_with_env(temp.path(), None, None);
    std::fs::create_dir_all(temp.path().join(".codex")).unwrap();
    std::fs::write(
        temp.path().join(".codex/auth.json"),
        r#"{"tokens":{"access_token":"fixture-token"}}"#,
    )
    .unwrap();
    let cfg = AppConfig::load_or_init(&paths).unwrap();
    assert!(cfg.accounts.contains_key("default-codex"));
    assert!(cfg.account_bindings.is_empty());
    assert_eq!(
        cfg.bootstrap,
        Some(BootstrapState {
            version: BOOTSTRAP_VERSION,
            fresh_install: false,
        })
    );
    let fresh_config = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
        fresh_config.contains("[bootstrap]"),
        "fresh config omitted bootstrap sentinel:\n{fresh_config}"
    );
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("default-codex").unwrap();
    editor.save().unwrap();
    let reloaded = AppConfig::load_or_init(&paths).unwrap();
    assert_eq!(
        reloaded.bootstrap,
        Some(BootstrapState {
            version: BOOTSTRAP_VERSION,
            fresh_install: false,
        })
    );
    assert!(
        !reloaded.accounts.contains_key("default-codex"),
        "removed discovered account was resurrected"
    );
}

#[test]
fn minimax_codex_account_routes_its_key_and_requires_a_model() {
    let mut account = AccountConfig {
        enabled: true,
        name: "MiniMax coding".into(),
        provider: AiProvider::Minimax,
        credential: AccountCredential::ApiKey {
            value: EnvValue::from("fixture-minimax-key"),
            base_url: None,
            model: Some("MiniMax-M3".into()),
        },
    };
    assert!(account.supports_agent(Agent::Codex));
    let env = account.credential_env(Agent::Codex).unwrap();
    assert_eq!(
        env.get("MINIMAX_API_KEY"),
        Some(&EnvValue::from("fixture-minimax-key"))
    );
    assert_eq!(
        env.get("OPENAI_BASE_URL"),
        Some(&EnvValue::from("https://api.minimax.io/v1"))
    );
    assert!(!env.contains_key("OPENAI_API_KEY"));
    if let AccountCredential::ApiKey { model, .. } = &mut account.credential {
        *model = None;
    }
    account.credential_env(Agent::Codex).unwrap_err();
}

#[test]
fn cross_provider_opencode_account_requires_model_and_native_does_not() {
    let mut account = AccountConfig {
        enabled: true,
        name: "Anthropic for OpenCode".into(),
        provider: AiProvider::Anthropic,
        credential: AccountCredential::ApiKey {
            value: EnvValue::from("fixture-anthropic-key"),
            base_url: None,
            model: Some("claude-3-7-sonnet".into()),
        },
    };
    assert!(account.supports_agent(Agent::Opencode));
    let env = account.credential_env(Agent::Opencode).unwrap();
    assert_eq!(
        env.get("ANTHROPIC_API_KEY"),
        Some(&EnvValue::from("fixture-anthropic-key"))
    );
    // OpenCode endpoints are written to its private configuration, not env
    assert_eq!(env.len(), 1);

    // Cross-provider account with None model must fail
    if let AccountCredential::ApiKey { model, .. } = &mut account.credential {
        *model = None;
    }
    let err = account.credential_env(Agent::Opencode).unwrap_err();
    assert!(
        err.to_string()
            .contains("requires an explicit model for opencode")
    );

    // Cross-provider account with empty model must also fail
    if let AccountCredential::ApiKey { model, .. } = &mut account.credential {
        *model = Some("   ".into());
    }
    let err = account.credential_env(Agent::Opencode).unwrap_err();
    assert!(
        err.to_string()
            .contains("requires an explicit model for opencode")
    );

    // Native OpenCode account does not require an explicit model
    let native_account = AccountConfig {
        enabled: true,
        name: "Native OpenCode".into(),
        provider: AiProvider::Opencode,
        credential: AccountCredential::ApiKey {
            value: EnvValue::from("fixture-opencode-key"),
            base_url: None,
            model: None,
        },
    };
    assert!(native_account.supports_agent(Agent::Opencode));
    let env = native_account.credential_env(Agent::Opencode).unwrap();
    assert_eq!(
        env.get("OPENCODE_API_KEY"),
        Some(&EnvValue::from("fixture-opencode-key"))
    );
}

#[test]
fn native_provider_mapping_covers_new_agents() {
    assert_eq!(
        AiProvider::for_agent(Agent::Claude),
        Some(AiProvider::Anthropic)
    );
    assert_eq!(
        AiProvider::for_agent(Agent::Antigravity),
        Some(AiProvider::Google)
    );
    assert_eq!(
        AiProvider::for_agent(Agent::Gemini),
        Some(AiProvider::Google)
    );
    assert_eq!(
        AiProvider::for_agent(Agent::Cursor),
        Some(AiProvider::Cursor)
    );
    assert_eq!(AiProvider::for_agent(Agent::Muse), Some(AiProvider::Meta));
    assert_eq!(AiProvider::for_agent(Agent::Omp), None);
    assert_eq!(AiProvider::for_agent(Agent::Hermes), None);
}

#[test]
fn new_provider_slugs_round_trip() {
    for (provider, slug) in [
        (AiProvider::Google, "google"),
        (AiProvider::Cursor, "cursor"),
        (AiProvider::Meta, "meta"),
        (AiProvider::OpenRouter, "openrouter"),
    ] {
        assert_eq!(provider.slug(), slug);
        assert_eq!(slug.parse::<AiProvider>().unwrap(), provider);
    }
}

#[test]
fn profile_selector_uses_canonical_wire_fields_without_aliases() {
    let selector = ProfileSelector {
        entry: "openai".into(),
        profile: Some("work".into()),
    };
    let serialized = toml::to_string(&selector).unwrap();
    assert_eq!(serialized, "entry = \"openai\"\nprofile = \"work\"\n");
    assert_eq!(
        toml::from_str::<ProfileSelector>(&serialized).unwrap(),
        selector
    );
    toml::from_str::<ProfileSelector>("provider = \"openai\"\nprofile = \"work\"\n").unwrap_err();
}

fn api_key(provider: AiProvider, model: Option<&str>) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: format!("{provider} key"),
        provider,
        credential: AccountCredential::ApiKey {
            value: EnvValue::from("fixture-key"),
            base_url: None,
            model: model.map(str::to_owned),
        },
    }
}

#[test]
fn single_provider_newcomers_accept_only_native_keys() {
    for (agent, provider) in [
        (Agent::Antigravity, AiProvider::Google),
        (Agent::Gemini, AiProvider::Google),
        (Agent::Cursor, AiProvider::Cursor),
        (Agent::Muse, AiProvider::Meta),
    ] {
        assert!(api_key(provider, None).supports_agent(agent), "{agent:?}");
        for other in [
            AiProvider::Anthropic,
            AiProvider::OpenAi,
            AiProvider::Google,
            AiProvider::Cursor,
            AiProvider::Meta,
            AiProvider::OpenRouter,
        ] {
            if other != provider {
                assert!(
                    !api_key(other, Some("model")).supports_agent(agent),
                    "{agent:?} vs {other:?}"
                );
            }
        }
    }
}

#[test]
fn multi_provider_clients_accept_every_provider_but_amp() {
    for agent in [Agent::Opencode, Agent::Omp, Agent::Hermes] {
        for provider in [
            AiProvider::Anthropic,
            AiProvider::OpenAi,
            AiProvider::Xai,
            AiProvider::Opencode,
            AiProvider::Moonshot,
            AiProvider::Zai,
            AiProvider::Minimax,
            AiProvider::Google,
            AiProvider::Cursor,
            AiProvider::Meta,
            AiProvider::OpenRouter,
        ] {
            assert!(
                api_key(provider, Some("model")).supports_agent(agent),
                "{agent:?} vs {provider:?}"
            );
        }
        assert!(!api_key(AiProvider::Amp, Some("model")).supports_agent(agent));
    }
}

#[test]
fn claude_and_codex_routing_is_unchanged_by_new_providers() {
    for agent in [Agent::Claude, Agent::Codex] {
        for provider in [AiProvider::Moonshot, AiProvider::Zai, AiProvider::Minimax] {
            assert!(api_key(provider, Some("model")).supports_agent(agent));
        }
        // OpenRouter reaches Claude/Codex-shaped workloads only through
        // OpenCode/Omp/Hermes, never directly.
        for provider in [
            AiProvider::Google,
            AiProvider::Cursor,
            AiProvider::Meta,
            AiProvider::OpenRouter,
        ] {
            assert!(!api_key(provider, Some("model")).supports_agent(agent));
        }
    }
}

#[test]
fn profile_compatibility_requires_owner_and_native_or_multi_provider_store() {
    let profile = |agent: Agent, provider: AiProvider| AccountConfig {
        enabled: true,
        name: "profile".into(),
        provider,
        credential: AccountCredential::Profile {
            agent,
            directory: PathBuf::from("/profiles/x"),
            xdg_roots: None,
            source_selector: None,
        },
    };
    // Native profiles still work.
    assert!(profile(Agent::Gemini, AiProvider::Google).supports_agent(Agent::Gemini));
    // Owner mismatch never works.
    assert!(!profile(Agent::Gemini, AiProvider::Google).supports_agent(Agent::Antigravity));
    // Multi-provider stores accept any provider under the owning agent.
    assert!(profile(Agent::Omp, AiProvider::Anthropic).supports_agent(Agent::Omp));
    assert!(profile(Agent::Hermes, AiProvider::OpenRouter).supports_agent(Agent::Hermes));
    assert!(profile(Agent::Opencode, AiProvider::Meta).supports_agent(Agent::Opencode));
    // ...but only under the owning agent.
    assert!(!profile(Agent::Omp, AiProvider::Anthropic).supports_agent(Agent::Hermes));
    // Single-provider agents reject non-native profile providers.
    assert!(!profile(Agent::Cursor, AiProvider::Google).supports_agent(Agent::Cursor));
}

#[test]
fn new_agents_route_native_key_variables() {
    let env = api_key(AiProvider::Google, None)
        .credential_env(Agent::Gemini)
        .unwrap();
    assert_eq!(
        env.get("GEMINI_API_KEY").unwrap().as_persisted_str(),
        "fixture-key"
    );
    let env = api_key(AiProvider::Cursor, None)
        .credential_env(Agent::Cursor)
        .unwrap();
    assert_eq!(
        env.get("CURSOR_API_KEY").unwrap().as_persisted_str(),
        "fixture-key"
    );
    let env = api_key(AiProvider::Meta, None)
        .credential_env(Agent::Muse)
        .unwrap();
    assert_eq!(
        env.get("META_API_KEY").unwrap().as_persisted_str(),
        "fixture-key"
    );
}

#[test]
fn omp_routing_requires_model_and_selects_provider_variable() {
    let account = api_key(AiProvider::OpenRouter, Some("org/model"));
    let env = account.credential_env(Agent::Omp).unwrap();
    assert_eq!(
        env.get("OPENROUTER_API_KEY").unwrap().as_persisted_str(),
        "fixture-key"
    );
    // Endpoints are provider-config material, never env, like OpenCode.
    assert_eq!(env.len(), 1);
    api_key(AiProvider::OpenRouter, None)
        .credential_env(Agent::Omp)
        .unwrap_err();
}

#[test]
fn omp_and_hermes_endpoint_overrides_fail_closed_until_provider_config_exists() {
    for agent in [Agent::Omp, Agent::Hermes] {
        let mut account = api_key(AiProvider::OpenRouter, Some("org/model"));
        if let AccountCredential::ApiKey { base_url, .. } = &mut account.credential {
            *base_url = Some("https://proxy.example/v1".into());
        }
        let err = account.credential_env(agent).unwrap_err();
        assert!(
            err.to_string()
                .contains("provider configuration is unsupported")
        );
    }
}

#[test]
fn omp_and_hermes_configuration_endpoint_overrides_fail_closed() {
    let mut accounts = BTreeMap::new();
    accounts.insert(
        "router".into(),
        api_key(AiProvider::OpenRouter, Some("org/model")),
    );
    for agent in [Agent::Omp, Agent::Hermes] {
        let config = AgentConfiguration {
            agent,
            account: "router".into(),
            model: Some("org/model".into()),
            base_url: Some("https://proxy.example/v1".into()),
            display_label: None,
            invoked_via_wrapper: None,
        };
        let err = config.validate("router-config", &accounts).unwrap_err();
        assert!(
            err.to_string()
                .contains("provider configuration is unsupported")
        );
    }
}

#[test]
fn endpoint_overrides_fail_closed_for_new_single_agents() {
    let mut account = api_key(AiProvider::Google, None);
    if let AccountCredential::ApiKey { base_url, .. } = &mut account.credential {
        *base_url = Some("https://proxy.example/v1".into());
    }
    let err = account.credential_env(Agent::Gemini).unwrap_err();
    assert!(
        err.to_string()
            .contains("endpoint overrides are unsupported")
    );
}

#[test]
fn disabled_accounts_keep_configuration_but_cannot_authenticate() {
    let (mut cfg, ws) = config();
    cfg.workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .accounts
        .push("work".into());
    cfg.accounts.get_mut("work").unwrap().enabled = false;
    cfg.validate_accounts().unwrap();
    assert!(!cfg.accounts["work"].supports_agent(Agent::Claude));
    cfg.accounts["work"]
        .credential_env(Agent::Claude)
        .unwrap_err();
    assert!(
        resolve_account(&cfg, Agent::Claude, Some(&ws), "smith")
            .unwrap()
            .is_none()
    );
    cfg.workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .account_bindings
        .insert(Agent::Claude, "work".into());
    cfg.validate_accounts().unwrap_err();
    resolve_account(&cfg, Agent::Claude, Some(&ws), "smith").unwrap_err();
    cfg.prune_account_bindings("work");
    cfg.validate_accounts().unwrap();
    assert!(
        resolve_account(&cfg, Agent::Claude, Some(&ws), "smith")
            .unwrap()
            .is_none()
    );
    let serialized = toml::to_string(&cfg.accounts["work"]).unwrap();
    assert!(serialized.contains("enabled = false"));
    let restored: AccountConfig = toml::from_str(&serialized).unwrap();
    assert!(!restored.enabled);
    assert!(
        toml::from_str::<AccountConfig>(&serialized.replace("enabled = false\n", ""))
            .unwrap()
            .enabled
    );
}

#[test]
fn validate_accounts_rejects_disabled_bindings_at_all_scopes() {
    let (mut cfg, ws) = config();
    cfg.accounts.get_mut("work").unwrap().enabled = false;
    cfg.workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .accounts
        .push("work".into());

    // Global binding to disabled account is rejected
    cfg.account_bindings.insert(Agent::Claude, "work".into());
    cfg.validate_accounts().unwrap_err();
    cfg.account_bindings.clear();
    cfg.validate_accounts().unwrap();

    // Workspace binding to disabled account is rejected
    cfg.workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .account_bindings
        .insert(Agent::Claude, "work".into());
    cfg.validate_accounts().unwrap_err();
    cfg.workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .account_bindings
        .clear();
    cfg.validate_accounts().unwrap();

    // Workspace-role binding to disabled account is rejected
    cfg.workspaces.get_mut(ws.as_str()).unwrap().roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            account_bindings: BTreeMap::from([(Agent::Claude, "work".into())]),
            ..Default::default()
        },
    );
    cfg.validate_accounts().unwrap_err();
    cfg.workspaces.get_mut(ws.as_str()).unwrap().roles.clear();
    cfg.validate_accounts().unwrap();
}

#[test]
fn prune_account_bindings_clears_all_scopes_and_enables_fallback() {
    let (mut cfg, ws) = config();
    // Allow personal and work in workspace
    cfg.workspaces.get_mut(ws.as_str()).unwrap().accounts = vec!["personal".into(), "work".into()];

    // Bind work at global, workspace, and role scopes
    cfg.account_bindings.insert(Agent::Claude, "work".into());
    cfg.workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .account_bindings
        .insert(Agent::Claude, "work".into());
    cfg.workspaces.get_mut(ws.as_str()).unwrap().roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            account_bindings: BTreeMap::from([(Agent::Claude, "work".into())]),
            ..Default::default()
        },
    );

    // Disable work
    cfg.accounts.get_mut("work").unwrap().enabled = false;
    cfg.validate_accounts().unwrap_err();

    // Prune disabled bindings across all scopes
    cfg.prune_account_bindings("work");
    cfg.validate_accounts().unwrap();

    assert!(cfg.account_bindings.is_empty());
    assert!(cfg.workspaces[ws.as_str()].account_bindings.is_empty());
    assert!(
        cfg.workspaces[ws.as_str()].roles["smith"]
            .account_bindings
            .is_empty()
    );

    // Fallback to the sole enabled account in workspace (personal)
    let resolved = resolve_account(&cfg, Agent::Claude, Some(&ws), "smith")
        .unwrap()
        .unwrap();
    assert_eq!(resolved.name, "Personal");
}

fn launch_fixture() -> (AppConfig, WorkspaceName) {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert("claude-work".into(), profile("Work"));
    let mut personal = profile("Personal");
    personal.provider = AiProvider::Anthropic;
    cfg.accounts.insert("claude-personal".into(), personal);
    cfg.accounts.insert(
        "codex-work".into(),
        AccountConfig {
            enabled: true,
            name: "Work".into(),
            provider: AiProvider::OpenAi,
            credential: AccountCredential::Profile {
                agent: Agent::Codex,
                directory: PathBuf::from("/profiles/codex-work"),
                xdg_roots: None,
                source_selector: None,
            },
        },
    );
    cfg.accounts
        .insert("zai-key".into(), api_key(AiProvider::Zai, None));
    for (id, agent, account) in [
        ("claude-a", Agent::Claude, "claude-work"),
        ("claude-b", Agent::Claude, "claude-personal"),
        ("codex-c", Agent::Codex, "codex-work"),
    ] {
        cfg.agent_configurations.insert(
            id.into(),
            AgentConfiguration {
                agent,
                account: account.into(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            },
        );
    }
    let ws = WorkspaceName::parse("project").unwrap();
    let workspace = WorkspaceConfig {
        accounts: vec![
            "claude-work".into(),
            "claude-personal".into(),
            "codex-work".into(),
        ],
        ..Default::default()
    };
    cfg.workspaces.insert(ws.as_str().into(), workspace);
    (cfg, ws)
}

#[test]
fn resolve_launch_one_launch_wins_and_validates_atomically() {
    let (cfg, ws) = launch_fixture();
    let instances = resolve_launch(
        &cfg,
        Some(&ws),
        "smith",
        Some(&["codex-c".to_owned(), "claude-a".to_owned()]),
        None,
    )
    .unwrap();
    assert_eq!(instances.len(), 2);
    assert_eq!(instances[0].config_id, "codex-c");
    assert_eq!(instances[0].label, "Codex · Work");
    assert!(!instances[0].synthesized);
    assert_eq!(instances[1].config_id, "claude-a");
    assert_eq!(instances[1].label, "Claude · Work");

    // Unknown ID fails the whole selection (no partial launch).
    resolve_launch(
        &cfg,
        Some(&ws),
        "smith",
        Some(&["codex-c".to_owned(), "nope".to_owned()]),
        None,
    )
    .unwrap_err();
    // Duplicate ID rejected.
    resolve_launch(
        &cfg,
        Some(&ws),
        "smith",
        Some(&["codex-c".to_owned(), "codex-c".to_owned()]),
        None,
    )
    .unwrap_err();
    // Explicit empty list resolves to no instances (shell-only).
    let empty = resolve_launch(&cfg, Some(&ws), "smith", Some(&[]), None).unwrap();
    assert!(empty.is_empty());
}

#[test]
fn resolve_launch_multi_instance_admission_follows_folder_var_kind() {
    let (mut cfg, ws) = launch_fixture();
    // Two Claude instances share a `Dir`-kind folder var → admitted.
    let instances = resolve_launch(
        &cfg,
        Some(&ws),
        "smith",
        Some(&["claude-a".to_owned(), "claude-b".to_owned()]),
        None,
    )
    .unwrap();
    assert_eq!(instances.len(), 2);

    let profile_for = |agent: Agent, name: &str| AccountConfig {
        enabled: true,
        name: name.into(),
        provider: AiProvider::for_agent(agent).unwrap(),
        credential: AccountCredential::Profile {
            agent,
            directory: PathBuf::from("/profiles").join(name),
            xdg_roots: None,
            source_selector: None,
        },
    };
    let add_pair = |cfg: &mut AppConfig, agent: Agent, prefix: &str| {
        for (id, account) in [
            (format!("{prefix}-a"), format!("{prefix}-work")),
            (format!("{prefix}-b"), format!("{prefix}-personal")),
        ] {
            cfg.accounts
                .insert(account.clone(), profile_for(agent, &account));
            cfg.agent_configurations.insert(
                id,
                AgentConfiguration {
                    agent,
                    account: account.clone(),
                    model: None,
                    base_url: None,
                    display_label: None,
                    invoked_via_wrapper: None,
                },
            );
            cfg.workspaces
                .get_mut(ws.as_str())
                .unwrap()
                .accounts
                .push(account);
        }
    };
    // Kimi has no folder var → two instances rejected.
    add_pair(&mut cfg, Agent::Kimi, "kimi");
    let error = resolve_launch(
        &cfg,
        Some(&ws),
        "smith",
        Some(&["kimi-a".to_owned(), "kimi-b".to_owned()]),
        None,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("no config-folder env var"),
        "unexpected kimi rejection: {error}"
    );
    // Amp is `XdgRoot`-kind → two instances rejected with the XDG reason.
    add_pair(&mut cfg, Agent::Amp, "amp");
    let error = resolve_launch(
        &cfg,
        Some(&ws),
        "smith",
        Some(&["amp-a".to_owned(), "amp-b".to_owned()]),
        None,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("XDG_DATA_HOME"),
        "unexpected amp rejection: {error}"
    );
    // OpenCode also exports an XDG root. Until each pane has a complete
    // process-wide XDG namespace, two provider-bound profiles in one
    // container are rejected rather than sharing unrelated OpenCode state.
    add_pair(&mut cfg, Agent::Opencode, "opencode");
    let error = resolve_launch(
        &cfg,
        Some(&ws),
        "smith",
        Some(&["opencode-a".to_owned(), "opencode-b".to_owned()]),
        None,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("XDG_DATA_HOME"),
        "unexpected OpenCode rejection: {error}"
    );
    // A lone second-agent instance still resolves.
    let instances =
        resolve_launch(&cfg, Some(&ws), "smith", Some(&["kimi-a".to_owned()]), None).unwrap();
    assert_eq!(instances.len(), 1);
}

#[test]
fn resolve_launch_scope_precedence_replaces_without_union() {
    let (mut cfg, ws) = launch_fixture();
    cfg.default_launch = Some(vec!["codex-c".into()]);
    cfg.workspaces.get_mut(ws.as_str()).unwrap().default_launch =
        Some(vec!["claude-a".into(), "claude-b".into()]);
    cfg.workspaces.get_mut(ws.as_str()).unwrap().roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            default_launch: Some(vec!["claude-b".into()]),
            ..Default::default()
        },
    );
    // Role scope replaces workspace + global entirely.
    let instances = resolve_launch(&cfg, Some(&ws), "smith", None, None).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].config_id, "claude-b");
    // Other roles fall through to the workspace scope.
    let instances = resolve_launch(&cfg, Some(&ws), "other", None, None).unwrap();
    assert_eq!(instances.len(), 2);
    assert_eq!(instances[0].config_id, "claude-a");
}

#[test]
fn resolve_launch_global_candidates_filter_by_authorization() {
    let (mut cfg, ws) = launch_fixture();
    cfg.agent_configurations.insert(
        "zai-codex".into(),
        AgentConfiguration {
            agent: Agent::Codex,
            account: "zai-key".into(),
            model: Some("glm-4".into()),
            base_url: None,
            display_label: None,
            invoked_via_wrapper: None,
        },
    );
    cfg.default_launch = Some(vec!["zai-codex".into(), "codex-c".into()]);
    // zai-key is outside the workspace allowlist: filtered, not an error.
    let instances = resolve_launch(&cfg, Some(&ws), "smith", None, None).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].config_id, "codex-c");
    // Workspace-scoped defaults validate atomically instead.
    cfg.workspaces.get_mut(ws.as_str()).unwrap().default_launch = Some(vec!["zai-codex".into()]);
    resolve_launch(&cfg, Some(&ws), "smith", None, None).unwrap_err();
}

#[test]
fn resolve_launch_fallback_needs_a_single_eligible_instance() {
    let (cfg, ws) = launch_fixture();
    // Several eligible instances: picker needed, never a silent pick.
    let err = resolve_launch(&cfg, Some(&ws), "smith", None, None).unwrap_err();
    assert!(err.to_string().contains("multiple accounts"), "{err}");
    // Sole eligible instance fast-starts with a synthesized ID.
    let mut solo = AppConfig::default();
    solo.accounts.insert("only".into(), profile("Only"));
    let instances = resolve_launch(&solo, None, "smith", None, None).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].config_id, "only@claude");
    assert!(instances[0].synthesized);
    assert_eq!(instances[0].label, "Claude · Only");
    // Zero eligible accounts is an actionable error.
    let empty = AppConfig::default();
    resolve_launch(&empty, None, "smith", None, None).unwrap_err();
}

#[test]
fn resolve_launch_committed_agent_honors_global_binding() {
    // E2E shape: many accounts, no launch lists, one per-agent default.
    // Fast start must honor the binding, never prompt.
    let (mut cfg, ws) = launch_fixture();
    cfg.account_bindings
        .insert(Agent::Claude, "claude-personal".into());
    for workspace in [None, Some(&ws)] {
        let instances =
            resolve_launch(&cfg, workspace, "smith", None, Some(Agent::Claude)).unwrap();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].account_id, "claude-personal");
        assert_eq!(instances[0].agent, Agent::Claude);
        assert_eq!(instances[0].config_id, "claude-personal@claude");
        assert_eq!(instances[0].label, "Claude · Personal");
        assert!(instances[0].synthesized);
    }
}

#[test]
fn resolve_launch_role_binding_beats_global_binding() {
    let (mut cfg, ws) = launch_fixture();
    cfg.account_bindings
        .insert(Agent::Claude, "claude-personal".into());
    cfg.workspaces.get_mut(ws.as_str()).unwrap().roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            account_bindings: BTreeMap::from([(Agent::Claude, "claude-work".into())]),
            ..Default::default()
        },
    );
    let instances = resolve_launch(&cfg, Some(&ws), "smith", None, Some(Agent::Claude)).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].account_id, "claude-work");
}

#[test]
fn resolve_launch_default_launch_beats_binding() {
    // A full launch list names the composition explicitly; a bare
    // per-agent account preference loses at any scope.
    let (mut cfg, ws) = launch_fixture();
    cfg.default_launch = Some(vec!["claude-a".into()]);
    cfg.account_bindings
        .insert(Agent::Claude, "claude-personal".into());
    let instances = resolve_launch(&cfg, Some(&ws), "smith", None, Some(Agent::Claude)).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].config_id, "claude-a");
    assert!(!instances[0].synthesized);
}

#[test]
fn resolve_launch_agent_scopes_sole_eligible_fallback() {
    // No bindings: only codex-work supports Codex, so the committed
    // agent resolves it alone; without an agent the same registry is
    // ambiguous and still needs a picker.
    let (cfg, ws) = launch_fixture();
    let instances = resolve_launch(&cfg, Some(&ws), "smith", None, Some(Agent::Codex)).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].config_id, "codex-work@codex");
    let err = resolve_launch(&cfg, Some(&ws), "smith", None, None).unwrap_err();
    assert!(err.to_string().contains("multiple accounts"), "{err}");
}

#[test]
fn resolve_launch_invalid_binding_fails_without_silent_fallback() {
    let (mut cfg, _) = launch_fixture();
    cfg.account_bindings.insert(Agent::Claude, "nope".into());
    let err = resolve_launch(&cfg, None, "smith", None, Some(Agent::Claude)).unwrap_err();
    assert!(err.to_string().contains("unknown account"), "{err}");
    // codex-work is a Codex-owned profile: incompatible with Claude.
    cfg.account_bindings
        .insert(Agent::Claude, "codex-work".into());
    let err = resolve_launch(&cfg, None, "smith", None, Some(Agent::Claude)).unwrap_err();
    assert!(err.to_string().contains("does not support"), "{err}");
}

#[test]
fn resolve_launch_model_chain_prefers_configuration_override() {
    let (mut cfg, ws) = launch_fixture();
    cfg.agent_configurations.insert(
        "zai-flash".into(),
        AgentConfiguration {
            agent: Agent::Codex,
            account: "zai-key".into(),
            model: Some("glm-4-flash".into()),
            base_url: Some("https://api.z.ai/api/v1".into()),
            display_label: Some("Codex · ZAI flash".into()),
            invoked_via_wrapper: None,
        },
    );
    cfg.workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .accounts
        .push("zai-key".into());
    let instances =
        resolve_launch(&cfg, Some(&ws), "smith", Some(&["zai-flash".into()]), None).unwrap();
    assert_eq!(instances[0].model.as_deref(), Some("glm-4-flash"));
    assert_eq!(
        instances[0].base_url.as_deref(),
        Some("https://api.z.ai/api/v1")
    );
    assert_eq!(instances[0].label, "Codex · ZAI flash");
}

#[test]
fn agent_configuration_validation_rejects_bad_references_and_overrides() {
    let (mut cfg, _) = launch_fixture();
    let accounts = cfg.accounts.clone();
    let good = AgentConfiguration {
        agent: Agent::Claude,
        account: "claude-work".into(),
        model: None,
        base_url: None,
        display_label: None,
        invoked_via_wrapper: None,
    };
    good.validate("ok-id", &accounts).unwrap();
    let wrapper_error = AgentConfiguration {
        invoked_via_wrapper: Some(WrapperSpec {
            identity: "credential-wrapper".into(),
            args: vec!["--profile".into(), "private".into()],
        }),
        ..good.clone()
    }
    .validate("wrapped", &accounts)
    .unwrap_err();
    assert!(
        wrapper_error
            .to_string()
            .contains("declares an unsupported shell wrapper"),
        "wrapper templates must fail closed during config validation: {wrapper_error}"
    );
    good.validate("Bad_ID!", &accounts).unwrap_err();
    AgentConfiguration {
        account: "missing".into(),
        ..good.clone()
    }
    .validate("x", &accounts)
    .unwrap_err();
    AgentConfiguration {
        agent: Agent::Codex,
        ..good.clone()
    }
    .validate("x", &accounts)
    .unwrap_err();
    AgentConfiguration {
        model: Some("  ".into()),
        ..good.clone()
    }
    .validate("x", &accounts)
    .unwrap_err();
    AgentConfiguration {
        base_url: Some("ftp://x".into()),
        ..good.clone()
    }
    .validate("x", &accounts)
    .unwrap_err();
    cfg.accounts.get_mut("claude-work").unwrap().enabled = false;
    good.validate("x", &cfg.accounts).unwrap_err();
}

#[test]
fn validate_launch_lists_rejects_unknown_duplicate_and_unauthorized() {
    let (mut cfg, ws) = launch_fixture();
    cfg.default_launch = Some(vec!["nope".into()]);
    cfg.validate_accounts().unwrap_err();
    cfg.default_launch = Some(vec!["codex-c".into(), "codex-c".into()]);
    cfg.validate_accounts().unwrap_err();
    cfg.default_launch = None;
    cfg.workspaces.get_mut(ws.as_str()).unwrap().default_launch =
        Some(vec!["codex-c".into(), "zzz".into()]);
    cfg.validate_accounts().unwrap_err();
}

#[test]
fn prune_agent_configurations_scrubs_launch_lists() {
    let (mut cfg, ws) = launch_fixture();
    cfg.default_launch = Some(vec!["claude-a".into(), "codex-c".into()]);
    cfg.prune_agent_configurations("claude-work");
    assert!(!cfg.agent_configurations.contains_key("claude-a"));
    assert_eq!(
        cfg.default_launch.as_deref(),
        Some(["codex-c".to_owned()].as_slice())
    );
    cfg.validate_accounts().unwrap();
    assert_eq!(ws.as_str(), "project");
}

#[test]
fn xdg_roots_validate_xdg_agents_and_absolute() {
    let roots = |data: &str| XdgRoots {
        data: PathBuf::from(data),
        config: PathBuf::from("/x/config"),
        cache: PathBuf::from("/x/cache"),
    };
    let mut amp = AccountConfig {
        enabled: true,
        name: "Amp".into(),
        provider: AiProvider::Amp,
        credential: AccountCredential::Profile {
            agent: Agent::Amp,
            directory: PathBuf::from("/x/amp"),
            xdg_roots: Some(roots("/x/data")),
            source_selector: None,
        },
    };
    amp.validate("amp").unwrap();
    amp.credential = AccountCredential::Profile {
        agent: Agent::Amp,
        directory: PathBuf::from("/x/amp"),
        xdg_roots: Some(roots("relative")),
        source_selector: None,
    };
    amp.validate("amp").unwrap_err();
    let claude = AccountConfig {
        enabled: true,
        name: "Claude".into(),
        provider: AiProvider::Anthropic,
        credential: AccountCredential::Profile {
            agent: Agent::Claude,
            directory: PathBuf::from("/x/claude"),
            xdg_roots: Some(roots("/x/data")),
            source_selector: None,
        },
    };
    claude.validate("claude").unwrap_err();
    let opencode = AccountConfig {
        enabled: true,
        name: "OpenCode".into(),
        provider: AiProvider::Opencode,
        credential: AccountCredential::Profile {
            agent: Agent::Opencode,
            directory: PathBuf::from("/x/opencode"),
            xdg_roots: Some(roots("/x/data")),
            source_selector: None,
        },
    };
    opencode.validate("opencode").unwrap();
}

#[test]
fn resolved_amp_profile_carries_explicit_xdg_roots() {
    let roots = XdgRoots {
        data: PathBuf::from("/srv/amp/data"),
        config: PathBuf::from("/srv/amp/config"),
        cache: PathBuf::from("/srv/amp/cache"),
    };
    let mut cfg = AppConfig::default();
    cfg.accounts.insert(
        "amp-profile".into(),
        AccountConfig {
            enabled: true,
            name: "Amp profile".into(),
            provider: AiProvider::Amp,
            credential: AccountCredential::Profile {
                agent: Agent::Amp,
                directory: PathBuf::from("/srv/amp/data/amp"),
                xdg_roots: Some(roots.clone()),
                source_selector: None,
            },
        },
    );
    cfg.account_bindings
        .insert(Agent::Amp, "amp-profile".into());

    let instances = resolve_launch(&cfg, None, "role", None, Some(Agent::Amp)).unwrap();

    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].xdg_roots, Some(roots));
}

#[test]
fn new_schema_round_trips_through_toml() {
    let (mut cfg, ws) = launch_fixture();
    cfg.default_launch = Some(vec!["claude-a".into()]);
    cfg.bootstrap = Some(BootstrapState::initialized());
    cfg.workspaces.get_mut(ws.as_str()).unwrap().default_launch = Some(vec![]);
    let raw = toml::to_string_pretty(&cfg).unwrap();
    assert!(raw.contains("agent_configurations"), "{raw}");
    assert!(raw.contains("default_launch"), "{raw}");
    assert!(raw.contains("[bootstrap]"), "{raw}");
    let back: AppConfig = toml::from_str(&raw).unwrap();
    assert_eq!(back.agent_configurations.len(), 3);
    assert_eq!(back.bootstrap, Some(BootstrapState::initialized()));
    back.validate_accounts().unwrap();
}

#[test]
fn provider_wire_spelling_matches_canonical_slug() {
    for provider in AiProvider::ALL {
        let slug = provider.slug();
        assert_eq!(provider.to_string(), slug);
        assert_eq!(slug.parse::<AiProvider>().unwrap(), *provider);
        assert_eq!(
            serde_json::to_string(provider).unwrap(),
            format!("{slug:?}")
        );
        assert_eq!(
            serde_json::from_str::<AiProvider>(&format!("{slug:?}")).unwrap(),
            *provider
        );
    }
}
