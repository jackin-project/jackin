// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn disc_blocked_providers_mint_no_refresh_material() {
    let temp = tempfile::tempdir().unwrap();
    let reader = RecordingProfileReader::default();
    // omp/hermes: attribution-only adapters; presence never mints material.
    let omp = temp.path().join("omp");
    std::fs::create_dir_all(omp.join("agent")).unwrap();
    std::fs::write(omp.join("agent/agent.db"), b"sqlite fixture").unwrap();
    assert!(matches!(
        profile_identity(&reader, Agent::Omp, &omp, temp.path()),
        ProfileValidation::Anonymous(None)
    ));
    let hermes = temp.path().join("hermes");
    std::fs::create_dir_all(&hermes).unwrap();
    std::fs::write(hermes.join("auth.json"), "{}").unwrap();
    assert!(matches!(
        profile_identity(&reader, Agent::Hermes, &hermes, temp.path()),
        ProfileValidation::Anonymous(None)
    ));
    // Muse: local identity only; the secret stays in the platform store and
    // no pollable fetch exists, so no material.
    let muse = temp.path().join("muse");
    std::fs::create_dir_all(&muse).unwrap();
    std::fs::write(
        muse.join("auth.json"),
        r#"{"providers":{"meta":{"user_email":"m@example.test"}}}"#,
    )
    .unwrap();
    let ProfileValidation::Authenticated { material: None, .. } =
        profile_identity(&reader, Agent::Muse, &muse, temp.path())
    else {
        panic!("muse profile must carry identity without material");
    };
}

#[test]
fn disc_antigravity_grant_mints_cli_refresh_material() {
    let temp = tempfile::tempdir().unwrap();
    // Grant present (payload always empty; presence is the whole answer) →
    // anonymous binding with CLI refresh material.
    for grant in [
        ProfileReadOutcome::Bytes(Vec::new()),
        ProfileReadOutcome::Bytes(vec![1, 2, 3]),
    ] {
        let reader = AntigravityGrantReader { grant };
        let ProfileValidation::Anonymous(Some(material)) =
            profile_identity(&reader, Agent::Antigravity, temp.path(), temp.path())
        else {
            panic!("antigravity grant must mint anonymous CLI material");
        };
        assert!(matches!(*material, ProfileCredentialMaterial::Antigravity));
    }
    // Grant absent/denied propagates truthfully, never a phantom binding.
    for (grant, expected) in [
        (ProfileReadOutcome::Missing, "missing"),
        (ProfileReadOutcome::Denied, "denied"),
    ] {
        let reader = AntigravityGrantReader { grant };
        let outcome = profile_identity(&reader, Agent::Antigravity, temp.path(), temp.path());
        assert!(
            matches!(
                outcome,
                ProfileValidation::Missing | ProfileValidation::Denied
            ),
            "antigravity without grant must be {expected}"
        );
    }
}

#[test]
fn disc_material_less_profile_binding_is_unpollable() {
    let temp = tempfile::tempdir().unwrap();
    let reader = RecordingProfileReader::default();
    // Muse: local identity, no material → Unpollable, never Capability.
    let muse = temp.path().join("muse");
    std::fs::create_dir_all(&muse).unwrap();
    std::fs::write(
        muse.join("auth.json"),
        r#"{"providers":{"meta":{"user_email":"m@example.test"}}}"#,
    )
    .unwrap();
    let parts = validate_source(
        DiscoveredCredentialSource::Profile {
            surface: HostSurfaceId::Meta,
            agent: Agent::Muse,
            root: muse,
            operator_home: temp.path().to_path_buf(),
            account_label: None,
            source_id: "source-muse".to_owned(),
            capability_id: "cap-muse".to_owned(),
            provenance: BTreeSet::new(),
        },
        &NoEnvResolver,
        &reader,
    );
    assert!(matches!(parts.5, ValidatedCredentialSource::Unpollable));
    // omp: attribution-only presence, no material → Unpollable as well.
    let omp = temp.path().join("omp");
    std::fs::create_dir_all(omp.join("agent")).unwrap();
    std::fs::write(omp.join("agent/agent.db"), b"sqlite fixture").unwrap();
    let parts = validate_source(
        DiscoveredCredentialSource::Profile {
            surface: HostSurfaceId::OpenRouter,
            agent: Agent::Omp,
            root: omp,
            operator_home: temp.path().to_path_buf(),
            account_label: None,
            source_id: "source-omp".to_owned(),
            capability_id: "cap-omp".to_owned(),
            provenance: BTreeSet::new(),
        },
        &NoEnvResolver,
        &reader,
    );
    assert!(matches!(parts.5, ValidatedCredentialSource::Unpollable));
}

#[test]
fn refresh_unpollable_binding_returns_honest_unsupported() {
    let binding = test_binding(HostSurfaceId::Meta, ValidatedCredentialSource::Unpollable);
    match refresh_credential_binding(&binding, &NoEnvResolver) {
        ProviderCredentialRefreshOutcome::Snapshot { view, .. } => {
            assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
            assert_eq!(
                view.last_error.as_deref(),
                Some("usage polling not supported for this provider")
            );
            assert!(view.buckets.is_empty());
            assert!(view.account.account_label.is_empty());
        }
        other => panic!("unpollable refresh must return an honest snapshot: {other:?}"),
    }
}

#[test]
fn refresh_cursor_binding_dispatches_to_collector() {
    // Live provider RPC with a fixture token: the dashboard rejects it, so
    // the arm must return the collector's honest Stale view — never
    // Malformed/Unsupported, which would mean dispatch never happened. The
    // material carries the profile root; refresh re-reads it.
    let temp = tempfile::tempdir().unwrap();
    let auth_path = temp.path().join("auth.json");
    std::fs::write(&auth_path, r#"{"accessToken":"fixture-opaque-token"}"#).unwrap();
    let binding = test_binding(
        HostSurfaceId::Cursor,
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Cursor { auth_path }),
    );
    match refresh_credential_binding(&binding, &NoEnvResolver) {
        ProviderCredentialRefreshOutcome::Snapshot { view, .. } => {
            assert_eq!(view.status, UsageSnapshotStatus::Stale);
            assert_eq!(view.account.provider_label, "Cursor");
            assert_eq!(view.focused_agent.as_deref(), Some("cursor"));
            assert!(view.last_error.is_some());
        }
        other => panic!("cursor refresh must dispatch to the collector: {other:?}"),
    }
}

#[test]
fn refresh_antigravity_binding_dispatches_to_cli() {
    // Live `agy` shell-out (cursor precedent above): any collector view —
    // Fresh quota, Stale, or a version-gate NeedsSecret — proves dispatch.
    // Only Malformed/Unsupported-by-discovery would mean the arm never ran.
    let binding = test_binding(
        HostSurfaceId::Google,
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Antigravity),
    );
    match refresh_credential_binding(&binding, &NoEnvResolver) {
        ProviderCredentialRefreshOutcome::Snapshot { view, .. } => {
            assert_eq!(view.account.provider_label, "Antigravity");
            assert_eq!(view.focused_agent.as_deref(), Some("gemini"));
            assert!(!view.is_refreshing_placeholder());
        }
        other => panic!("antigravity refresh must dispatch to the CLI collector: {other:?}"),
    }
}

#[test]
fn disc_account_aliases_avoid_governed_names_and_round_trip() {
    let mut seen = BTreeSet::new();
    for entry in jackin_core::USAGE_CREDENTIAL_ENV_REGISTRY {
        let alias = usage_account_alias_entry(*entry, entry.owner);
        assert_eq!(alias.owner, entry.owner);
        assert_ne!(alias.name, entry.name);
        assert!(
            !jackin_core::is_account_env(alias.name),
            "alias must survive operator-env attribution: {}",
            alias.name
        );
        assert_eq!(governed_name_for_account_alias(alias.name), entry.name);
        assert!(seen.insert(alias.name), "duplicate alias: {}", alias.name);
    }
    assert_eq!(
        governed_name_for_account_alias("ZAI_API_KEY"),
        "ZAI_API_KEY"
    );
}

#[test]
fn disc_zai_aliases_keep_one_canonical_owner_and_dispatch_route() {
    for name in ["ZAI_API_KEY", "ZHIPU_API_KEY", "Z_AI_API_KEY"] {
        let entry = UsageCredentialEnvName {
            name,
            owner: UsageCredentialOwner::Zai,
        };
        let alias = usage_account_alias_entry(entry, UsageCredentialOwner::Zai);
        assert_eq!(alias.owner, UsageCredentialOwner::Zai);
        assert_eq!(
            jackin_usage_provider_core::dispatch_key_for_route(
                alias.owner,
                governed_name_for_account_alias(alias.name),
            ),
            "ZAI_API_KEY"
        );
    }
}

#[test]
fn disc_env_key_account_resolves_through_isolated_alias() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    write_accounts_config(
        &config_root,
        &[],
        &[("codex-key", AiProvider::OpenAi, "fixture-openai-key")],
    );
    let resolver = SecretDedupFakeResolver::default();
    let catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);

    assert!(catalog.diagnostics.is_empty(), "{:?}", catalog.diagnostics);
    assert_eq!(catalog.candidates.len(), 1);
    assert_eq!(catalog.candidates[0].surface_id, "codex");
    assert_eq!(
        catalog.candidates[0].credential_kind,
        UsageCredentialKind::ApiKey
    );
    let calls = resolver.calls.lock().unwrap();
    assert_eq!(calls.len(), 4);
    assert!(
        calls
            .iter()
            .all(|call| { call.len() == 1 && call[0] == "JACKIN_USAGE_ACCOUNT_OPENAI_API_KEY" })
    );

    let validated = validate_usage_sources(catalog, &resolver);
    assert_eq!(validated.accounts.len(), 1);
    assert_eq!(validated.bindings.len(), 1);
    assert!(validated.bindings[0].identity.is_some());
    // Canonical ownership remains separate from exact provider dispatch.
    assert!(matches!(
        validated.bindings[0].source,
        ValidatedCredentialSource::Env {
            ref key,
            ref dispatch_key,
            ..
        } if key == "OPENAI_API_KEY" && dispatch_key == "OPENAI_API_KEY"
    ));
    assert_eq!(usage_broker_capabilities(&validated).len(), 1);
    assert_eq!(validated.unresolved_capabilities().count(), 0);
}

#[test]
fn disc_oauth_token_account_resolves_through_isolated_alias() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let mut config = AppConfig::default();
    config.accounts.insert(
        "oa-claude".to_owned(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "oa-claude".to_owned(),
            provider: AiProvider::Anthropic,
            credential: AccountCredential::OAuthToken {
                agent: Agent::Claude,
                value: jackin_config::EnvValue::Plain("fixture-oauth-token".to_owned()),
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

    assert!(catalog.diagnostics.is_empty(), "{:?}", catalog.diagnostics);
    assert_eq!(catalog.candidates.len(), 1);
    assert_eq!(
        catalog.candidates[0].credential_kind,
        UsageCredentialKind::OAuthToken
    );
    let calls = resolver.calls.lock().unwrap();
    assert_eq!(
        calls[0],
        vec!["JACKIN_USAGE_ACCOUNT_CLAUDE_CODE_OAUTH_TOKEN"]
    );
    let validated = validate_usage_sources(catalog, &resolver);
    assert_eq!(validated.accounts.len(), 1);
    assert!(matches!(
        validated.bindings[0].source,
        ValidatedCredentialSource::Env { ref key, .. } if key == "ANTHROPIC_API_KEY"
    ));
    assert!(matches!(
        validated.bindings[0].source,
        ValidatedCredentialSource::Env { ref dispatch_key, .. }
            if dispatch_key == "CLAUDE_CODE_OAUTH_TOKEN"
    ));
}

#[test]
fn disc_mixed_profile_and_env_same_provider_merge_to_one_identity() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let profile = temp.path().join("codex-shared");
    write_accounts_config(
        &config_root,
        &[("codex-profile", Agent::Codex, &profile)],
        &[("codex-key", AiProvider::OpenAi, "fixture-openai-key")],
    );
    write_codex_auth(
        &profile,
        "same-provider-account",
        "eyJlbWFpbCI6InNhbWVAZXhhbXBsZS50ZXN0In0",
        "fixture-secret",
    );
    let resolver = SecretDedupFakeResolver::default();
    let catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);
    assert_eq!(catalog.candidates.len(), 2);

    let validated =
        validate_usage_sources_with_reader(catalog, &resolver, &RecordingProfileReader::default());

    assert!(
        validated.diagnostics.is_empty(),
        "{:?}",
        validated.diagnostics
    );
    assert_eq!(validated.accounts.len(), 1);
    assert_eq!(validated.accounts[0].account_label, "same@example.test");
    assert_eq!(
        validated.accounts[0].provenance,
        vec!["account codex-key", "account codex-profile"]
    );
    assert_eq!(validated.accounts[0].source_ids.len(), 2);
    assert!(matches!(
        validated.accounts[0].identity.subject,
        CanonicalAccountSubject::ProviderId(_)
    ));
    assert_eq!(validated.bindings.len(), 2);
    assert_eq!(
        validated.bindings[0].identity,
        validated.bindings[1].identity
    );
    assert_eq!(usage_broker_capabilities(&validated).len(), 1);
    assert_eq!(validated.unresolved_capabilities().count(), 0);
}

#[test]
fn disc_distinct_env_keys_same_provider_keep_distinct_identities() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    write_accounts_config(
        &config_root,
        &[],
        &[
            ("key-one", AiProvider::OpenAi, "fixture-key-one"),
            ("key-two", AiProvider::OpenAi, "fixture-key-two"),
        ],
    );
    let resolver = SecretDedupFakeResolver::default();
    let catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);
    assert_eq!(catalog.candidates.len(), 2);

    let validated = validate_usage_sources(catalog, &resolver);
    assert_eq!(validated.accounts.len(), 2);
    assert_eq!(usage_broker_capabilities(&validated).len(), 2);
}
