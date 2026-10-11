// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
fn breadcrumb_migration_preserves_account_source_fingerprint() {
    let migrated_path = "Vault/Item/Team%252FBlue/Token";
    let account = AccountConfig {
        enabled: true,
        name: "Work".into(),
        provider: AiProvider::Anthropic,
        credential: AccountCredential::ApiKey {
            value: EnvValue::OpRef(jackin_core::OpRef {
                op: "op://vault-id/item-id/field-id".to_owned(),
                path: migrated_path.to_owned(),
                account: Some("work".to_owned()),
                on_demand: false,
            }),
            base_url: None,
            model: None,
        },
    };

    // Prior binaries fingerprinted the literal legacy path bytes. The v1
    // decoder must restore those exact bytes before hashing so a schema bump
    // does not resurrect an intentionally removed account source.
    let mut expected = Sha256::new();
    hash_component(&mut expected, account.provider.slug());
    hash_component(&mut expected, "api_key");
    hash_component(&mut expected, "op_ref");
    hash_component(&mut expected, "op://vault-id/item-id/field-id");
    hash_component(&mut expected, "Vault/Item/Team%2FBlue/Token");
    hash_optional_component(&mut expected, Some("work"));
    hash_component(&mut expected, "false");
    hash_optional_component(&mut expected, None);

    assert_eq!(
        account_source_fingerprint(&account),
        hex::encode(expected.finalize())
    );
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
