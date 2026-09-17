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
fn workspace_empty_allowlist_never_inherits_global_credentials() {
    let (mut cfg, ws) = config();
    cfg.account_bindings
        .insert(Agent::Claude, "personal".into());
    assert!(
        resolve_account(&cfg, Agent::Claude, Some(&ws), "")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        resolve_account(&cfg, Agent::Claude, None, "")
            .unwrap()
            .unwrap()
            .name,
        "Personal"
    );
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
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("default-codex").unwrap();
    editor.save().unwrap();
    assert!(
        !AppConfig::load_or_init(&paths)
            .unwrap()
            .accounts
            .contains_key("default-codex")
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
    )
    .unwrap_err();
    // Duplicate ID rejected.
    resolve_launch(
        &cfg,
        Some(&ws),
        "smith",
        Some(&["codex-c".to_owned(), "codex-c".to_owned()]),
    )
    .unwrap_err();
    // Explicit empty list resolves to no instances (shell-only).
    let empty = resolve_launch(&cfg, Some(&ws), "smith", Some(&[])).unwrap();
    assert!(empty.is_empty());
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
    let instances = resolve_launch(&cfg, Some(&ws), "smith", None).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].config_id, "claude-b");
    // Other roles fall through to the workspace scope.
    let instances = resolve_launch(&cfg, Some(&ws), "other", None).unwrap();
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
    let instances = resolve_launch(&cfg, Some(&ws), "smith", None).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].config_id, "codex-c");
    // Workspace-scoped defaults validate atomically instead.
    cfg.workspaces.get_mut(ws.as_str()).unwrap().default_launch = Some(vec!["zai-codex".into()]);
    resolve_launch(&cfg, Some(&ws), "smith", None).unwrap_err();
}

#[test]
fn resolve_launch_fallback_needs_a_single_eligible_instance() {
    let (cfg, ws) = launch_fixture();
    // Several eligible instances: picker needed, never a silent pick.
    let err = resolve_launch(&cfg, Some(&ws), "smith", None).unwrap_err();
    assert!(err.to_string().contains("multiple accounts"), "{err}");
    // Sole eligible instance fast-starts with a synthesized ID.
    let mut solo = AppConfig::default();
    solo.accounts.insert("only".into(), profile("Only"));
    let instances = resolve_launch(&solo, None, "smith", None).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].config_id, "only@claude");
    assert!(instances[0].synthesized);
    assert_eq!(instances[0].label, "Claude · Only");
    // Zero eligible accounts is an actionable error.
    let empty = AppConfig::default();
    resolve_launch(&empty, None, "smith", None).unwrap_err();
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
    let instances = resolve_launch(&cfg, Some(&ws), "smith", Some(&["zai-flash".into()])).unwrap();
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
fn xdg_roots_validate_amp_only_and_absolute() {
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
        },
    };
    amp.validate("amp").unwrap();
    amp.credential = AccountCredential::Profile {
        agent: Agent::Amp,
        directory: PathBuf::from("/x/amp"),
        xdg_roots: Some(roots("relative")),
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
        },
    };
    claude.validate("claude").unwrap_err();
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
