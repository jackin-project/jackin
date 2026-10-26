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

#[test]
fn resolve_saved_launch_preserves_declared_and_synthesized_tuple_order() {
    let (mut cfg, ws) = launch_fixture();
    cfg.default_launch = Some(vec!["claude-b".into()]);
    let saved = [
        SavedLaunchInstance {
            config_id: "codex-work@codex",
            agent: Agent::Codex,
            account_id: "codex-work",
        },
        SavedLaunchInstance {
            config_id: "claude-a",
            agent: Agent::Claude,
            account_id: "claude-work",
        },
    ];

    let resolved = resolve_saved_launch(&cfg, Some(&ws), &saved).unwrap();

    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].config_id, "codex-work@codex");
    assert_eq!(resolved[0].account_id, "codex-work");
    assert!(resolved[0].synthesized);
    assert_eq!(resolved[1].config_id, "claude-a");
    assert_eq!(resolved[1].account_id, "claude-work");
    assert!(!resolved[1].synthesized);
}

#[test]
fn resolve_saved_launch_rejects_invalid_tuples_and_declared_id_collision() {
    let (cfg, ws) = launch_fixture();
    let tuple = |config_id, agent, account_id| SavedLaunchInstance {
        config_id,
        agent,
        account_id,
    };

    let Err(error) = resolve_saved_launch(&cfg, Some(&ws), &[]) else {
        panic!("empty saved launch selection must be rejected");
    };
    assert_eq!(error.to_string(), "saved launch selection is empty");

    let Err(error) = resolve_saved_launch(
        &cfg,
        Some(&ws),
        &[
            tuple("claude-a", Agent::Claude, "claude-work"),
            tuple("claude-a", Agent::Claude, "claude-work"),
        ],
    ) else {
        panic!("duplicate saved configuration must be rejected");
    };
    assert!(error.to_string().contains("duplicate configuration"));

    let Err(error) = resolve_saved_launch(
        &cfg,
        Some(&ws),
        &[tuple("claude-a", Agent::Codex, "claude-work")],
    ) else {
        panic!("saved agent mismatch must be rejected");
    };
    assert!(error.to_string().contains("saved configuration"));

    let Err(error) = resolve_saved_launch(
        &cfg,
        Some(&ws),
        &[tuple("claude-a", Agent::Claude, "claude-personal")],
    ) else {
        panic!("saved account mismatch must be rejected");
    };
    assert!(error.to_string().contains("saved configuration"));

    let Err(error) = resolve_saved_launch(
        &cfg,
        Some(&ws),
        &[tuple("not-a-synthetic-id", Agent::Claude, "claude-work")],
    ) else {
        panic!("malformed synthesized identifier must be rejected");
    };
    assert!(
        error
            .to_string()
            .contains("does not match its recorded account/agent tuple")
    );

    let mut collision = cfg.clone();
    collision.agent_configurations.insert(
        "claude-work@claude".into(),
        AgentConfiguration {
            agent: Agent::Claude,
            account: "claude-personal".into(),
            model: None,
            base_url: None,
            display_label: None,
            invoked_via_wrapper: None,
        },
    );
    let Err(error) = resolve_saved_launch(
        &collision,
        Some(&ws),
        &[tuple("claude-work@claude", Agent::Claude, "claude-work")],
    ) else {
        panic!("declared identifier collision must not be synthesized");
    };
    assert!(error.to_string().contains("saved configuration"));
}

#[test]
fn resolve_saved_launch_rechecks_current_eligibility_and_full_set_isolation() {
    let (mut cfg, ws) = launch_fixture();
    let tuple = |config_id, agent, account_id| SavedLaunchInstance {
        config_id,
        agent,
        account_id,
    };
    let saved = [tuple("claude-a", Agent::Claude, "claude-work")];

    cfg.accounts.get_mut("claude-work").unwrap().enabled = false;
    let Err(error) = resolve_saved_launch(&cfg, Some(&ws), &saved) else {
        panic!("disabled account must be rejected");
    };
    assert!(error.to_string().contains("not authorized"));

    let (mut cfg, ws) = launch_fixture();
    cfg.workspaces
        .get_mut(ws.as_str())
        .unwrap()
        .accounts
        .retain(|account| account != "claude-work");
    let Err(error) = resolve_saved_launch(&cfg, Some(&ws), &saved) else {
        panic!("account outside the selected workspace must be rejected");
    };
    assert!(error.to_string().contains("not assigned to this workspace"));

    let (mut cfg, ws) = launch_fixture();
    cfg.agent_configurations
        .get_mut("claude-a")
        .unwrap()
        .invoked_via_wrapper = Some(WrapperSpec {
        identity: "unsupported-shell-wrapper".into(),
        args: Vec::new(),
    });
    let Err(error) = resolve_saved_launch(&cfg, Some(&ws), &saved) else {
        panic!("unsupported wrapper transport must be rejected");
    };
    assert!(error.to_string().contains("unsupported shell wrapper"));

    let (mut cfg, ws) = launch_fixture();
    for (id, account) in [("kimi-a", "kimi-work"), ("kimi-b", "kimi-personal")] {
        cfg.accounts.insert(
            account.into(),
            AccountConfig {
                enabled: true,
                name: account.into(),
                provider: AiProvider::Moonshot,
                credential: AccountCredential::Profile {
                    agent: Agent::Kimi,
                    directory: PathBuf::from("/profiles").join(account),
                    xdg_roots: None,
                    source_selector: None,
                },
            },
        );
        cfg.agent_configurations.insert(
            id.into(),
            AgentConfiguration {
                agent: Agent::Kimi,
                account: account.into(),
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
            .push(account.into());
    }
    let error = resolve_saved_launch(
        &cfg,
        Some(&ws),
        &[
            tuple("kimi-a", Agent::Kimi, "kimi-work"),
            tuple("kimi-b", Agent::Kimi, "kimi-personal"),
        ],
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("no config-folder env var"),
        "{error}"
    );
}
