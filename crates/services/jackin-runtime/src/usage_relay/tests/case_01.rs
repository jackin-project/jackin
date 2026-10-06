// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn resolved_launch_inventory_is_closed_to_manifest_not_catalog() {
    let config = CapsuleConfig {
        instances: vec![
            "work@claude".to_owned(),
            "work@codex".to_owned(),
            "work@claude".to_owned(),
        ],
        usage_capabilities: BTreeMap::from([(
            "unlisted".to_owned(),
            capability("unlisted-account"),
        )]),
        ..CapsuleConfig::default()
    };

    assert_eq!(
        resolved_launch_usage_inventory(&config).instances,
        ["work@claude", "work@codex"]
    );
}

#[test]
fn launch_usage_capabilities_preserve_account_identity_and_provider_surface() {
    use jackin_config::{AccountConfig, AccountCredential, AiProvider};

    let mut config = AppConfig::default();
    for (id, provider) in [
        ("personal-openai", AiProvider::OpenAi),
        ("work-openai", AiProvider::OpenAi),
        ("routed-zai", AiProvider::Zai),
    ] {
        config.accounts.insert(
            id.to_owned(),
            AccountConfig {
                enabled: true,
                name: id.to_owned(),
                provider,
                credential: AccountCredential::ApiKey {
                    value: "fixture-key".into(),
                    base_url: None,
                    model: None,
                },
            },
        );
    }

    let mut launch_config = CapsuleConfig {
        instances: vec![
            "personal-codex".to_owned(),
            "work-codex".to_owned(),
            "routed-codex".to_owned(),
        ],
        agents: BTreeMap::from([
            ("personal-codex".to_owned(), "codex".to_owned()),
            ("work-codex".to_owned(), "codex".to_owned()),
            ("routed-codex".to_owned(), "codex".to_owned()),
        ]),
        accounts: BTreeMap::from([
            ("personal-codex".to_owned(), "personal-openai".to_owned()),
            ("work-codex".to_owned(), "work-openai".to_owned()),
            ("routed-codex".to_owned(), "routed-zai".to_owned()),
        ]),
        ..CapsuleConfig::default()
    };

    populate_launch_usage_capabilities(&config, &mut launch_config);

    assert_eq!(
        launch_config.usage_capabilities,
        BTreeMap::from([
            (
                "personal-codex".to_owned(),
                UsageAccountCapability {
                    account_id: "personal-openai".to_owned(),
                    surface_id: "codex".to_owned(),
                },
            ),
            (
                "work-codex".to_owned(),
                UsageAccountCapability {
                    account_id: "work-openai".to_owned(),
                    surface_id: "codex".to_owned(),
                },
            ),
            (
                "routed-codex".to_owned(),
                UsageAccountCapability {
                    account_id: "routed-zai".to_owned(),
                    surface_id: "zai".to_owned(),
                },
            ),
        ])
    );
}

#[test]
fn staged_scope_pins_zhipu_source_identity_and_material() -> Result<()> {
    use jackin_config::{AccountConfig, AccountCredential, AiProvider};

    let mut config = AppConfig::default();
    config.accounts.insert(
        "zhipu".to_owned(),
        AccountConfig {
            enabled: true,
            name: "Zhipu".to_owned(),
            provider: AiProvider::Zai,
            credential: AccountCredential::ApiKey {
                value: jackin_core::EnvValue::OpRef(jackin_core::OpRef {
                    op: "op://vault/item/field".to_owned(),
                    path: "Vault/Item/Field".to_owned(),
                    account: Some("work".to_owned()),
                    on_demand: false,
                }),
                base_url: None,
                model: None,
            },
        },
    );
    let instances = vec![jackin_config::ResolvedInstance {
        config_id: "zhipu-opencode".to_owned(),
        agent: jackin_core::Agent::Opencode,
        account_id: "zhipu".to_owned(),
        model: None,
        base_url: None,
        xdg_roots: None,
        label: "Zhipu".to_owned(),
        synthesized: false,
    }];
    let credentials = jackin_protocol::AgentCredentialEnv::new(BTreeMap::from([(
        "zhipu-opencode".to_owned(),
        jackin_protocol::InstanceCredentialEnv {
            agent: "opencode".to_owned(),
            account_id: "zhipu".to_owned(),
            env: BTreeMap::from([("ZHIPU_API_KEY".to_owned(), "S1".to_owned())]),
        },
    )]));

    let scope = usage_credential_scope_for_staged_launch(&config, &instances, &credentials)?;
    assert_eq!(scope.sources.len(), 1);
    let proof = scope.sources.iter().next().expect("one Zhipu proof");
    assert_eq!(proof.key, "ZHIPU_API_KEY");
    assert_eq!(proof.account_id, "zhipu");
    assert_eq!(proof.surface_id, "zai");
    assert_eq!(
        proof.source,
        UsageCredentialSourceIdentity::OnePassword {
            reference: "op://vault/item/field".to_owned(),
            account: Some("work".to_owned()),
        }
    );
    assert_eq!(
        proof.material_fingerprint,
        usage_credential_material_fingerprint("S1")
    );
    Ok(())
}

#[test]
fn staged_scope_audits_one_account_across_mixed_agent_consumers() -> Result<()> {
    use jackin_config::{AccountConfig, AccountCredential, AiProvider};

    let mut config = AppConfig::default();
    config.accounts.insert(
        "shared-zai".to_owned(),
        AccountConfig {
            enabled: true,
            name: "Shared Z.AI".to_owned(),
            provider: AiProvider::Zai,
            credential: AccountCredential::ApiKey {
                value: jackin_core::EnvValue::OpRef(jackin_core::OpRef {
                    op: "op://vault/shared/field".to_owned(),
                    path: "Vault/Shared/Field".to_owned(),
                    account: Some("work".to_owned()),
                    on_demand: false,
                }),
                base_url: None,
                model: Some("glm-4.5".to_owned()),
            },
        },
    );
    let instances = vec![
        jackin_config::ResolvedInstance {
            config_id: "shared-claude".to_owned(),
            agent: jackin_core::Agent::Claude,
            account_id: "shared-zai".to_owned(),
            model: None,
            base_url: None,
            xdg_roots: None,
            label: "Shared Claude".to_owned(),
            synthesized: false,
        },
        jackin_config::ResolvedInstance {
            config_id: "shared-codex".to_owned(),
            agent: jackin_core::Agent::Codex,
            account_id: "shared-zai".to_owned(),
            model: Some("glm-4.5".to_owned()),
            base_url: None,
            xdg_roots: None,
            label: "Shared Codex".to_owned(),
            synthesized: false,
        },
        jackin_config::ResolvedInstance {
            config_id: "shared-opencode".to_owned(),
            agent: jackin_core::Agent::Opencode,
            account_id: "shared-zai".to_owned(),
            model: None,
            base_url: None,
            xdg_roots: None,
            label: "Shared OpenCode".to_owned(),
            synthesized: false,
        },
    ];
    let credentials = jackin_protocol::AgentCredentialEnv::new(BTreeMap::from([
        (
            "shared-claude".to_owned(),
            jackin_protocol::InstanceCredentialEnv {
                agent: "claude".to_owned(),
                account_id: "shared-zai".to_owned(),
                env: BTreeMap::from([(
                    jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME.to_owned(),
                    "S1".to_owned(),
                )]),
            },
        ),
        (
            "shared-codex".to_owned(),
            jackin_protocol::InstanceCredentialEnv {
                agent: "codex".to_owned(),
                account_id: "shared-zai".to_owned(),
                env: BTreeMap::from([(
                    jackin_core::OPENAI_API_KEY_ENV_NAME.to_owned(),
                    "S1".to_owned(),
                )]),
            },
        ),
        (
            "shared-opencode".to_owned(),
            jackin_protocol::InstanceCredentialEnv {
                agent: "opencode".to_owned(),
                account_id: "shared-zai".to_owned(),
                env: BTreeMap::from([(
                    jackin_core::ZHIPU_API_KEY_ENV_NAME.to_owned(),
                    "S1".to_owned(),
                )]),
            },
        ),
    ]));

    let scope = usage_credential_scope_for_staged_launch(&config, &instances, &credentials)?;
    assert_eq!(scope.sources.len(), 3);
    assert_eq!(
        scope
            .sources
            .iter()
            .map(|proof| proof.key.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME,
            jackin_core::OPENAI_API_KEY_ENV_NAME,
            jackin_core::ZHIPU_API_KEY_ENV_NAME,
        ])
    );
    assert!(
        scope
            .sources
            .iter()
            .all(|proof| proof.account_id == "shared-zai" && proof.surface_id == "zai")
    );
    Ok(())
}

#[test]
fn launch_discovery_relay_uses_distinct_canonical_ids_for_same_surface() -> Result<()> {
    use jackin_config::{AccountConfig, AccountCredential, AiProvider};

    let temp = tempfile::tempdir()?;
    let config_root = temp.path().join("config");
    let home = temp.path().join("home");
    fs::create_dir_all(&config_root)?;
    let mut config = AppConfig::default();
    for (id, name, account_id, token) in [
        (
            "personal-openai",
            "Personal",
            "provider-personal",
            "fixture-personal-token",
        ),
        ("work-openai", "Work", "provider-work", "fixture-work-token"),
    ] {
        let profile = temp.path().join(id);
        fs::create_dir_all(&profile)?;
        fs::write(
            profile.join("auth.json"),
            format!(r#"{{"tokens":{{"access_token":"{token}","account_id":"{account_id}"}}}}"#),
        )?;
        config.accounts.insert(
            id.to_owned(),
            AccountConfig {
                enabled: true,
                name: name.to_owned(),
                provider: AiProvider::OpenAi,
                credential: AccountCredential::Profile {
                    agent: jackin_core::Agent::Codex,
                    directory: profile,
                    xdg_roots: None,
                    source_selector: None,
                },
            },
        );
    }
    fs::write(config_root.join("config.toml"), toml::to_string(&config)?)?;

    let resolver = CachedProviderCredentialResolver::new(RuntimeSecretSource);
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: home,
        },
        &resolver,
    )
    .map_err(anyhow::Error::msg)?;
    let discovery = validate_usage_sources(catalog, &resolver);
    let sources = ForwardedUsageSources {
        selected_account_ids: BTreeSet::from([
            "personal-openai".to_owned(),
            "work-openai".to_owned(),
        ]),
        selected_account_surfaces: BTreeMap::from([
            ("personal-openai".to_owned(), "codex".to_owned()),
            ("work-openai".to_owned(), "codex".to_owned()),
        ]),
        profile_surface_ids: BTreeSet::from(["codex".to_owned()]),
        env_keys: BTreeSet::new(),
        credential_scope: UsageCredentialScope::default(),
    };
    let forwarded = forwarded_usage_capabilities(&discovery, "unrelated scope", &sources);
    assert_eq!(forwarded.len(), 2);
    assert!(
        forwarded
            .iter()
            .all(|capability| capability.surface_id == "codex")
    );

    let allowed = forwarded.iter().cloned().collect::<BTreeSet<_>>();
    let canonical = canonical_capabilities_for_launch(&discovery, &sources, &allowed);
    let mut launch_config = CapsuleConfig {
        instances: vec!["personal@codex".to_owned(), "work@codex".to_owned()],
        accounts: BTreeMap::from([
            ("personal@codex".to_owned(), "personal-openai".to_owned()),
            ("work@codex".to_owned(), "work-openai".to_owned()),
        ]),
        usage_capabilities: BTreeMap::from([
            (
                "personal@codex".to_owned(),
                UsageAccountCapability {
                    account_id: "personal-openai".to_owned(),
                    surface_id: "codex".to_owned(),
                },
            ),
            (
                "work@codex".to_owned(),
                UsageAccountCapability {
                    account_id: "work-openai".to_owned(),
                    surface_id: "codex".to_owned(),
                },
            ),
        ]),
        ..CapsuleConfig::default()
    };

    canonical
        .apply_to_launch_config(&mut launch_config)
        .unwrap();
    let personal = &launch_config.usage_capabilities["personal@codex"];
    let work = &launch_config.usage_capabilities["work@codex"];
    assert_eq!(personal.surface_id, "codex");
    assert_eq!(work.surface_id, "codex");
    assert_ne!(personal.account_id, "personal-openai");
    assert_ne!(work.account_id, "work-openai");
    assert_ne!(personal.account_id, work.account_id);
    assert!(matches!(
        UsageCapabilitySet::new(forwarded).authorize(personal),
        Ok(())
    ));
    assert!(matches!(
        UsageCapabilitySet::new(allowed).authorize(&UsageAccountCapability {
            account_id: "personal-openai".to_owned(),
            surface_id: "codex".to_owned(),
        }),
        Err(error) if error.kind == UsageCoordinationErrorKind::Unauthorized
    ));
    Ok(())
}
