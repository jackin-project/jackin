use super::*;

fn claude_api_key_view() -> FocusedUsageView {
    crate::usage::claude_api_key_snapshot(
        "claude",
        Some("Claude"),
        "ANTHROPIC_API_KEY",
        "fixture-not-a-real-key",
        1_800_000_000,
    )
}

#[test]
fn unsupported_configured_claude_api_key_never_mints_account_identity() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    write_accounts_config(
        &config_root,
        &[],
        &[(
            "named-api-account",
            AiProvider::Anthropic,
            "fixture-not-a-real-key",
        )],
    );
    let resolver = SecretDedupFakeResolver::default();
    let catalog = discover_with(&config_root, &temp.path().join("home"), &resolver);
    let validated = validate_usage_sources(catalog, &resolver);
    assert!(validated.accounts.is_empty());
    let binding = validated
        .bindings
        .first()
        .expect("configured key retained")
        .clone();
    assert!(binding.identity.is_none());
    let ValidatedCredentialSource::Env { launch_keys, .. } = &binding.source else {
        panic!("API-key source must retain launch capability");
    };
    assert!(launch_keys.contains("ANTHROPIC_API_KEY"));
    let view = claude_api_key_view();
    assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
    assert!(super::super::super::accounts::account_key_for_view(&view).is_none());
    assert!(super::super::super::accounts::canonical_account_id_for_view(&view).is_none());
    let mut runtime = HostUsageRuntime::new();
    runtime
        .open(crate::host::HostRuntimeConfig::under_data_dir(temp.path()))
        .unwrap();
    runtime.discovery = Some(validated);
    runtime.record_discovered_snapshot(&binding, view);
    assert!(runtime.discovery.as_ref().unwrap().accounts.is_empty());
    assert!(runtime.discovered_views.is_empty());
    let provider = runtime
        .discovered_provider_views
        .get(&HostSurfaceId::Claude)
        .unwrap();
    assert_eq!(provider.status, UsageSnapshotStatus::Unsupported);
    assert!(
        provider
            .buckets
            .iter()
            .all(|bucket| bucket.remaining_percent.is_none())
    );
    // Even provider-returned typed claims cannot authenticate an anonymous
    // credential. The host binding overwrites both identity channels.
    let mut forged = claude_api_key_view();
    forged.canonical_identity = Some(
        CanonicalAccountIdentity {
            surface: HostSurfaceId::Claude,
            subject: CanonicalAccountSubject::ProviderId("forged-provider-subject".to_owned()),
        }
        .protocol_identity(),
    );
    forged.account_identity = Some(jackin_protocol::control::UsageAccountIdentity {
        account_id: "forged-route".to_owned(),
        surface_id: "codex".to_owned(),
        source_revision: Some("forged-source-revision".to_owned()),
    });
    forged.confidence = UsageConfidence::Authoritative;
    runtime.record_discovered_snapshot(&binding, forged);
    assert!(runtime.discovered_views.is_empty());
    assert!(runtime.discovery.as_ref().unwrap().accounts.is_empty());
    let provider = runtime
        .discovered_provider_views
        .get(&HostSurfaceId::Claude)
        .unwrap();
    assert!(provider.canonical_identity.is_none());
    assert_ne!(
        provider.account_identity.as_ref().unwrap().account_id,
        "forged-route"
    );
    assert_eq!(
        provider.account_identity.as_ref().unwrap().surface_id,
        "claude"
    );
    let accepted =
        super::super::super::broker::usage_catalog_entries(runtime.discovery.as_ref().unwrap())
            .into_iter()
            .find(|entry| {
                entry.capability.account_id
                    == provider.account_identity.as_ref().unwrap().account_id
            })
            .unwrap();
    assert_eq!(
        provider
            .account_identity
            .as_ref()
            .unwrap()
            .source_revision
            .as_deref(),
        Some(accepted.revision.as_str())
    );
}

#[test]
fn quota_confidence_and_labels_cannot_supply_authentication_identity() {
    for label in ["", "Claude API key", "named@example.test"] {
        for confidence in [
            UsageConfidence::None,
            UsageConfidence::PresenceOnly,
            UsageConfidence::Authoritative,
            UsageConfidence::Estimated,
        ] {
            let mut view = claude_api_key_view();
            view.account.account_label = label.to_owned();
            view.confidence = confidence;
            assert!(CanonicalAccountIdentity::from_view(HostSurfaceId::Claude, &view).is_none());
        }
    }
    let identity = CanonicalAccountIdentity {
        surface: HostSurfaceId::Claude,
        subject: CanonicalAccountSubject::ProviderId("subscription-id".to_owned()),
    };
    for confidence in [
        UsageConfidence::None,
        UsageConfidence::PresenceOnly,
        UsageConfidence::Authoritative,
        UsageConfidence::Estimated,
    ] {
        let mut view = claude_api_key_view();
        view.account.account_label = "subscription@example.test".to_owned();
        view.confidence = confidence;
        view.canonical_identity = Some(identity.protocol_identity());
        assert_eq!(
            CanonicalAccountIdentity::from_view(HostSurfaceId::Claude, &view),
            Some(identity.clone())
        );
        view.account.account_label.clear();
        assert_eq!(
            CanonicalAccountIdentity::from_view(HostSurfaceId::Claude, &view),
            Some(identity.clone())
        );
    }
}

#[test]
fn unsupported_quota_retains_independently_authenticated_account_and_label() {
    let temp = tempfile::tempdir().unwrap();
    let identity = CanonicalAccountIdentity {
        surface: HostSurfaceId::Claude,
        subject: CanonicalAccountSubject::ProviderId("authenticated-organization".to_owned()),
    };
    let mut binding = test_binding(HostSurfaceId::Claude, ValidatedCredentialSource::Unpollable);
    binding.identity = Some(identity.clone());
    let account = DiscoveredAccountDescriptor {
        surface_id: HostSurfaceId::Claude.id().to_owned(),
        account_key: identity.account_key(),
        account_label: "subscription@example.test".to_owned(),
        provenance: vec![],
        source_ids: vec![binding.source_id.clone()],
        identity: identity.clone(),
    };
    let mut runtime = HostUsageRuntime::new();
    runtime
        .open(crate::host::HostRuntimeConfig::under_data_dir(temp.path()))
        .unwrap();
    runtime.discovery = Some(ValidatedUsageDiscovery {
        config_generation: None,
        accounts: vec![account],
        diagnostics: vec![],
        candidates: vec![],
        bindings: vec![binding.clone()],
    });
    runtime.record_discovered_snapshot(&binding, claude_api_key_view());
    let view = runtime
        .discovered_views
        .get(&(HostSurfaceId::Claude, identity.account_key()))
        .unwrap();
    assert_eq!(view.account.account_label, "subscription@example.test");
    assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
    assert!(
        view.buckets
            .iter()
            .all(|bucket| bucket.remaining_percent.is_none())
    );
    assert_eq!(runtime.discovery.as_ref().unwrap().accounts.len(), 1);
    assert!(runtime.discovered_provider_views.is_empty());
}
