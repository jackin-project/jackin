// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
