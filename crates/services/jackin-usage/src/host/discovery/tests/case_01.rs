// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn opencode_profile_requires_one_auth_entry_and_ignores_sibling_database() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("opencode");
    std::fs::create_dir_all(&root).unwrap();
    let auth = root.join("auth.json");
    let reader = RecordingProfileReader::default();

    std::fs::write(
        &auth,
        r#"{"anthropic":{"type":"api","key":"fixture-a"},"opencode-go":{"type":"api","key":"fixture-go"}}"#,
    )
    .unwrap();
    assert!(matches!(
        opencode_profile_identity(&reader, &auth),
        ProfileValidation::Malformed
    ));

    std::fs::write(
        &auth,
        r#"{"opencode-go":{"type":"api","key":"fixture-go"}}"#,
    )
    .unwrap();
    std::fs::write(root.join("opencode.db"), b"database fixture").unwrap();
    assert!(matches!(
        opencode_profile_identity(&reader, &auth),
        ProfileValidation::Anonymous(Some(_))
    ));

    std::fs::remove_file(&auth).unwrap();
    assert!(matches!(
        opencode_profile_identity(&reader, &auth),
        ProfileValidation::Malformed
    ));
}

#[test]
fn opencode_profile_database_only_uses_reader_abstraction() {
    let reader = SyntheticDatabaseOnlyReader;
    let auth = Path::new("/synthetic/opencode/auth.json");

    assert!(matches!(
        opencode_profile_identity(&reader, auth),
        ProfileValidation::Malformed
    ));
}

#[test]
fn disc_claude_keychain_consent_is_not_reported_missing() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let root = home.join(".claude");
    let catalog = UsageDiscoveryCatalog {
        config_generation: None,
        candidates: Vec::new(),
        diagnostics: Vec::new(),
        sources: vec![DiscoveredCredentialSource::Profile {
            surface: HostSurfaceId::Claude,
            agent: Agent::Claude,
            root,
            operator_home: home,
            account_label: Some("work".to_owned()),
            source_id: "source-0001".to_owned(),
            capability_id: "capability-1".to_owned(),
            provenance: BTreeSet::from(["account work".to_owned()]),
        }],
    };

    let validated =
        validate_usage_sources_with_reader(catalog, &NoEnvResolver, &ConsentKeychainReader);

    assert!(validated.accounts.is_empty());
    assert_eq!(validated.diagnostics.len(), 1);
    assert_eq!(
        validated.diagnostics[0].issue,
        UsageDiscoveryIssue::KeychainConsentRequired
    );
    assert_eq!(
        validated.diagnostics[0].issue.id(),
        "keychain_consent_required"
    );
}

#[test]
fn env_capability_ids_isolate_distinct_opaque_credentials() {
    let first = CredentialSourceKey::Env {
        surface: HostSurfaceId::Zai,
        handle: OpaqueCredentialHandle::new("credential-1"),
        key: "ZAI_API_KEY".to_owned(),
        dispatch_key: "ZAI_API_KEY".to_owned(),
    };
    let second = CredentialSourceKey::Env {
        surface: HostSurfaceId::Zai,
        handle: OpaqueCredentialHandle::new("credential-2"),
        key: "ZAI_API_KEY".to_owned(),
        dispatch_key: "ZAI_API_KEY".to_owned(),
    };

    let first_id = source_capability_id(HostSurfaceId::Zai, &first);
    let second_id = source_capability_id(HostSurfaceId::Zai, &second);
    assert_ne!(first_id, second_id);
    assert_eq!(first_id, source_capability_id(HostSurfaceId::Zai, &first));
    assert!(!first_id.contains("credential-1"));
    assert!(!second_id.contains("credential-2"));
}

#[test]
fn disc_registry_enumerates_registered_sources_without_ambient_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let home = temp.path().join("home");
    write_registry(
        &config_root,
        &[
            ("work", Agent::Codex, Path::new("/profiles/codex-work")),
            (
                "personal",
                Agent::Codex,
                Path::new("/profiles/codex-personal"),
            ),
        ],
    );
    write_codex_auth(
        &home.join(".codex"),
        "ambient",
        "e30",
        "unregistered-secret",
    );
    let resolver = FakeEnvResolver::default();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root: config_root.clone(),
            operator_home: home.clone(),
        },
        &resolver,
    )
    .unwrap();
    assert!(catalog.diagnostics.is_empty(), "{:?}", catalog.diagnostics);
    assert_eq!(catalog.candidates.len(), 2);
    assert!(
        catalog
            .candidates
            .iter()
            .all(|candidate| candidate.surface_id == "codex")
    );
    assert!(resolver.calls.lock().unwrap().is_empty());
    write_registry(&config_root, &[]);
    let empty = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: home,
        },
        &resolver,
    )
    .unwrap();
    assert!(empty.candidates.is_empty());
}

#[test]
fn disc_registry_api_sources_are_isolated_from_ambient_env_declarations() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    std::fs::create_dir_all(&config_root).unwrap();
    let mut config = AppConfig::default();
    config.accounts.insert(
        "zai-work".to_owned(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Work".to_owned(),
            provider: AiProvider::Zai,
            credential: AccountCredential::ApiKey {
                value: jackin_config::EnvValue::Plain("fixture-key".to_owned()),
                base_url: None,
                model: None,
            },
        },
    );
    config
        .account_bindings
        .insert(Agent::Opencode, "zai-work".to_owned());
    config.env.insert(
        "MINIMAX_API_KEY".to_owned(),
        jackin_config::EnvValue::Plain("unregistered".to_owned()),
    );
    std::fs::write(
        config_root.join("config.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();
    let resolver = FakeEnvResolver::default();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: temp.path().join("home"),
        },
        &resolver,
    )
    .unwrap();
    assert_eq!(catalog.candidates.len(), 1);
    assert_eq!(catalog.candidates[0].surface_id, "zai");
    assert_eq!(
        catalog.candidates[0].credential_kind,
        UsageCredentialKind::ApiKey
    );
    let calls = resolver.calls.lock().unwrap();
    assert_eq!(calls.len(), 5, "all compatible Z.AI routes must resolve");
    let mut isolated_alias_counts = BTreeMap::<String, usize>::new();
    for (_, _, keys) in calls.iter() {
        assert_eq!(keys.len(), 1, "each route must resolve one isolated alias");
        assert!(!jackin_core::is_account_env(&keys[0]));
        *isolated_alias_counts.entry(keys[0].clone()).or_default() += 1;
    }
    assert_eq!(
        isolated_alias_counts,
        BTreeMap::from([
            ("JACKIN_USAGE_ACCOUNT_ANTHROPIC_AUTH_TOKEN".to_owned(), 1,),
            ("JACKIN_USAGE_ACCOUNT_OPENAI_API_KEY".to_owned(), 1),
            ("JACKIN_USAGE_ACCOUNT_ZHIPU_API_KEY".to_owned(), 3),
        ])
    );
    assert!(
        !calls
            .iter()
            .any(|(_, _, keys)| { keys.iter().any(|key| key == "MINIMAX_API_KEY") })
    );

    let launch_keys = catalog
        .sources
        .iter()
        .find_map(|source| match source {
            DiscoveredCredentialSource::Env { launch_keys, .. } => Some(launch_keys.clone()),
            DiscoveredCredentialSource::Profile { .. }
            | DiscoveredCredentialSource::Capability { .. } => None,
        })
        .expect("one canonical provider source");
    assert_eq!(
        launch_keys,
        BTreeSet::from([
            "ANTHROPIC_AUTH_TOKEN".to_owned(),
            "OPENAI_API_KEY".to_owned(),
            "ZHIPU_API_KEY".to_owned(),
        ])
    );
    assert!(!format!("{catalog:?}").contains("fixture-key"));
}

#[test]
fn disc_synthesized_routes_keep_launch_keys_separate() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let mut config = AppConfig::default();
    config.accounts.insert(
        "zai-routes".to_owned(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Z.AI routes".to_owned(),
            provider: AiProvider::Zai,
            credential: AccountCredential::ApiKey {
                value: jackin_config::EnvValue::Plain("fixture-key".to_owned()),
                base_url: None,
                model: Some("glm-5".to_owned()),
            },
        },
    );
    std::fs::create_dir_all(&config_root).unwrap();
    std::fs::write(
        config_root.join("config.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();

    let resolver = SecretDedupFakeResolver::default();
    let catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);
    let (canonical_key, dispatch_key, launch_keys) = catalog
        .sources
        .iter()
        .find_map(|source| match source {
            DiscoveredCredentialSource::Env {
                key,
                dispatch_key,
                launch_keys,
                ..
            } => Some((key.as_str(), dispatch_key.as_str(), launch_keys.clone())),
            DiscoveredCredentialSource::Profile { .. }
            | DiscoveredCredentialSource::Capability { .. } => None,
        })
        .expect("one canonical provider source");
    assert_eq!(canonical_key, "ZAI_API_KEY");
    assert_eq!(dispatch_key, "ZAI_API_KEY");
    assert_eq!(
        launch_keys,
        BTreeSet::from([
            "ANTHROPIC_AUTH_TOKEN".to_owned(),
            "OPENAI_API_KEY".to_owned(),
            "ZHIPU_API_KEY".to_owned(),
        ])
    );
    let validated = validate_usage_sources(catalog, &resolver);
    assert_eq!(validated.bindings.len(), 1);
    assert_eq!(crate::host::usage_broker_capabilities(&validated).len(), 1);
}

#[test]
fn disc_registry_openrouter_api_key_maps_to_usage_surface_and_governed_env() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    std::fs::create_dir_all(&config_root).unwrap();
    let mut config = AppConfig::default();
    config.accounts.insert(
        "openrouter-work".to_owned(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "OpenRouter work".to_owned(),
            provider: AiProvider::OpenRouter,
            credential: AccountCredential::ApiKey {
                value: jackin_config::EnvValue::Plain("fixture-openrouter-key".to_owned()),
                base_url: None,
                model: Some("openai/gpt-5".to_owned()),
            },
        },
    );
    std::fs::write(
        config_root.join("config.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();

    let resolver = FakeEnvResolver::default();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: temp.path().join("home"),
        },
        &resolver,
    )
    .unwrap();

    assert_eq!(catalog.candidates.len(), 1);
    assert_eq!(catalog.candidates[0].surface_id, "openrouter");
    assert_eq!(
        catalog.candidates[0].credential_kind,
        UsageCredentialKind::ApiKey
    );
    assert_eq!(
        resolver.calls.lock().unwrap()[0].2,
        vec!["JACKIN_USAGE_ACCOUNT_OPENROUTER_API_KEY"]
    );
    assert!(!jackin_core::is_account_env(
        &resolver.calls.lock().unwrap()[0].2[0]
    ));
    assert!(!format!("{catalog:?}").contains("fixture-openrouter-key"));

    let validated = validate_usage_sources(catalog, &resolver);
    let capabilities = crate::host::usage_broker_capabilities(&validated);
    assert_eq!(capabilities.len(), 1);
    assert_eq!(capabilities[0].surface_id, "openrouter");
}
