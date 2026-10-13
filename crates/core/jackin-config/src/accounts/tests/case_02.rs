// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
fn credential_descriptors_follow_explicit_account_workspace_and_role_routes() {
    let mut cfg = AppConfig::default();
    cfg.accounts
        .insert("zai".into(), api_key(AiProvider::Zai, Some("glm-5")));
    cfg.account_bindings.insert(Agent::Codex, "zai".into());
    let workspace_name = WorkspaceName::parse("project").unwrap();
    let mut workspace = WorkspaceConfig::default();
    workspace.accounts.push("zai".into());
    workspace.roles.insert(
        "review".into(),
        WorkspaceRoleOverride {
            account_bindings: BTreeMap::from([(Agent::Claude, "zai".into())]),
            ..Default::default()
        },
    );
    cfg.workspaces
        .insert(workspace_name.as_str().to_owned(), workspace);
    cfg.agent_configurations.insert(
        "zai-opencode".into(),
        AgentConfiguration {
            agent: Agent::Opencode,
            account: "zai".into(),
            model: Some("zai-coding/glm-5".into()),
            base_url: None,
            display_label: None,
            invoked_via_wrapper: None,
        },
    );

    let routes = cfg.credential_descriptors_for_account("zai").unwrap();
    assert_eq!(
        routes,
        vec![
            ResolvedCredentialDescriptor {
                agent: Agent::Claude,
                provider: AiProvider::Zai,
                mode: AuthForwardMode::ApiKey,
                env_name: "ANTHROPIC_AUTH_TOKEN",
            },
            ResolvedCredentialDescriptor {
                agent: Agent::Codex,
                provider: AiProvider::Zai,
                mode: AuthForwardMode::ApiKey,
                env_name: "OPENAI_API_KEY",
            },
            ResolvedCredentialDescriptor {
                agent: Agent::Opencode,
                provider: AiProvider::Zai,
                mode: AuthForwardMode::ApiKey,
                env_name: "ZHIPU_API_KEY",
            },
            ResolvedCredentialDescriptor {
                agent: Agent::Omp,
                provider: AiProvider::Zai,
                mode: AuthForwardMode::ApiKey,
                env_name: "ZHIPU_API_KEY",
            },
            ResolvedCredentialDescriptor {
                agent: Agent::Hermes,
                provider: AiProvider::Zai,
                mode: AuthForwardMode::ApiKey,
                env_name: "ZHIPU_API_KEY",
            },
        ]
    );
}

#[test]
fn credential_descriptor_matrix_uses_each_launch_route_key() {
    let account = api_key(AiProvider::Zai, Some("glm-5"));
    assert_eq!(
        [Agent::Claude, Agent::Codex, Agent::Opencode]
            .into_iter()
            .map(|agent| account.resolved_credential_descriptor(agent).unwrap())
            .collect::<Vec<_>>(),
        vec![
            ResolvedCredentialDescriptor {
                agent: Agent::Claude,
                provider: AiProvider::Zai,
                mode: AuthForwardMode::ApiKey,
                env_name: "ANTHROPIC_AUTH_TOKEN",
            },
            ResolvedCredentialDescriptor {
                agent: Agent::Codex,
                provider: AiProvider::Zai,
                mode: AuthForwardMode::ApiKey,
                env_name: "OPENAI_API_KEY",
            },
            ResolvedCredentialDescriptor {
                agent: Agent::Opencode,
                provider: AiProvider::Zai,
                mode: AuthForwardMode::ApiKey,
                env_name: "ZHIPU_API_KEY",
            },
        ]
    );

    let oauth = AccountConfig {
        enabled: true,
        name: "Claude OAuth".into(),
        provider: AiProvider::Anthropic,
        credential: AccountCredential::OAuthToken {
            agent: Agent::Claude,
            value: EnvValue::from("fixture-oauth"),
        },
    };
    assert_eq!(
        oauth.resolved_credential_descriptor(Agent::Claude).unwrap(),
        ResolvedCredentialDescriptor {
            agent: Agent::Claude,
            provider: AiProvider::Anthropic,
            mode: AuthForwardMode::OAuthToken,
            env_name: "CLAUDE_CODE_OAUTH_TOKEN",
        }
    );
    profile("profile")
        .resolved_credential_descriptor(Agent::Claude)
        .unwrap_err();
}

#[test]
fn credential_descriptors_use_native_default_when_unreferenced() {
    let mut cfg = AppConfig::default();
    cfg.accounts
        .insert("google".into(), api_key(AiProvider::Google, None));

    let routes = cfg.credential_descriptors_for_account("google").unwrap();
    assert_eq!(
        routes.iter().map(|route| route.agent).collect::<Vec<_>>(),
        vec![
            Agent::Opencode,
            Agent::Antigravity,
            Agent::Gemini,
            Agent::Omp,
            Agent::Hermes,
        ]
    );
    assert!(routes.iter().all(|route| {
        route.mode == AuthForwardMode::ApiKey && route.env_name == "GEMINI_API_KEY"
    }));
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
