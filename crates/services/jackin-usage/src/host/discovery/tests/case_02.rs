// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn disc_scope_capsule_uses_only_forwarded_capabilities() {
    let resolver = FakeEnvResolver::default();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::Capsule {
            forwarded_accounts: vec![
                ForwardedUsageAccount {
                    surface_id: "claude".to_owned(),
                    capability_id: "cap-1".to_owned(),
                    account_label: Some("account@example.test".to_owned()),
                },
                ForwardedUsageAccount {
                    surface_id: "claude".to_owned(),
                    capability_id: "cap-1".to_owned(),
                    account_label: Some("account@example.test".to_owned()),
                },
                ForwardedUsageAccount {
                    surface_id: "opencode".to_owned(),
                    capability_id: "cap-2".to_owned(),
                    account_label: None,
                },
                ForwardedUsageAccount {
                    surface_id: "bogus".to_owned(),
                    capability_id: "skipped".to_owned(),
                    account_label: None,
                },
            ],
        },
        &resolver,
    )
    .unwrap();

    // Duplicate claude capabilities dedup; every known surface (not just the
    // Desktop glance order) is admitted; unknown ids still skip.
    assert_eq!(catalog.candidates.len(), 2);
    assert_eq!(catalog.candidates[0].surface_id, "claude");
    assert_eq!(
        catalog.candidates[0].credential_kind,
        UsageCredentialKind::ForwardedCapability
    );
    assert_eq!(catalog.candidates[1].surface_id, "opencode");
    assert!(resolver.calls.lock().unwrap().is_empty());
}

#[test]
fn disc_scope_capsule_admits_newly_wired_surfaces() {
    let resolver = FakeEnvResolver::default();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::Capsule {
            forwarded_accounts: vec![
                ForwardedUsageAccount {
                    surface_id: "cursor".to_owned(),
                    capability_id: "cap-cursor".to_owned(),
                    account_label: Some("cursor@example.test".to_owned()),
                },
                ForwardedUsageAccount {
                    surface_id: "google".to_owned(),
                    capability_id: "cap-google".to_owned(),
                    account_label: None,
                },
                ForwardedUsageAccount {
                    surface_id: "openrouter".to_owned(),
                    capability_id: "cap-openrouter".to_owned(),
                    account_label: None,
                },
                // No collector, registry, or discovery entry exists.
                ForwardedUsageAccount {
                    surface_id: "copilot".to_owned(),
                    capability_id: "skipped".to_owned(),
                    account_label: None,
                },
            ],
        },
        &resolver,
    )
    .unwrap();

    let mut ids: Vec<_> = catalog
        .candidates
        .iter()
        .map(|candidate| candidate.surface_id.as_str())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, ["cursor", "google", "openrouter"]);
    assert!(resolver.calls.lock().unwrap().is_empty());
}

#[test]
fn disc_same_provider_sources_with_same_labels_keep_source_capabilities_distinct() {
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::Capsule {
            forwarded_accounts: vec![
                ForwardedUsageAccount {
                    surface_id: "codex".to_owned(),
                    capability_id: "capability-a".to_owned(),
                    account_label: Some("same@example.test".to_owned()),
                },
                ForwardedUsageAccount {
                    surface_id: "codex".to_owned(),
                    capability_id: "capability-b".to_owned(),
                    account_label: Some("same@example.test".to_owned()),
                },
            ],
        },
        &NoEnvResolver,
    )
    .unwrap();

    let validated = validate_usage_sources(catalog, &NoEnvResolver);

    assert_eq!(validated.accounts.len(), 2);
    assert_eq!(validated.bindings.len(), 2);
    assert_eq!(
        validated
            .accounts
            .iter()
            .map(|account| account.account_key.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        2
    );
    assert!(validated.accounts.iter().all(|account| {
        matches!(
            account.identity.subject,
            CanonicalAccountSubject::SourceCapability(_)
        )
    }));
    assert!(
        validated
            .accounts
            .iter()
            .all(|account| account.source_ids.len() == 1)
    );
}

#[test]
fn disc_unresolved_same_labels_do_not_overwrite_discovered_views() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::Capsule {
            forwarded_accounts: vec![
                ForwardedUsageAccount {
                    surface_id: "codex".to_owned(),
                    capability_id: "capability-a".to_owned(),
                    account_label: None,
                },
                ForwardedUsageAccount {
                    surface_id: "codex".to_owned(),
                    capability_id: "capability-b".to_owned(),
                    account_label: None,
                },
            ],
        },
        &NoEnvResolver,
    )
    .unwrap();
    let validated = validate_usage_sources(catalog, &NoEnvResolver);
    let bindings = validated.bindings.clone();

    let mut runtime = HostUsageRuntime::new();
    runtime
        .open(crate::host::HostRuntimeConfig::under_data_dir(temp.path()))
        .unwrap();
    runtime.discovery = Some(validated);

    for (index, binding) in bindings.iter().enumerate() {
        let mut view = FocusedUsageView::unavailable("fixture", index as i64);
        view.focused_agent = Some("codex".to_owned());
        view.focused_provider = Some("OpenAI".to_owned());
        view.account.provider_label = "OpenAI / Codex".to_owned();
        view.account.account_label = "same@example.test".to_owned();
        view.confidence = UsageConfidence::Authoritative;
        view.status_bar_label = format!("source-{index}");
        runtime.record_discovered_snapshot(binding, view);
    }

    assert_eq!(runtime.discovered_views.len(), 2);
    assert_eq!(
        runtime
            .discovered_views
            .values()
            .map(|view| view.status_bar_label.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["source-0", "source-1"])
    );
}

#[test]
fn disc_source_valid_profiles_resolve_without_network_or_fake_presence() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let profile = temp.path().join("codex-profile");
    write_codex_only_global(&config_root, &profile);
    write_codex_auth(
        &profile,
        "account-1",
        "eyJlbWFpbCI6ImFsaWNlQGV4YW1wbGUudGVzdCJ9",
        "fixture-secret",
    );
    let reader = RecordingProfileReader::default();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root: config_root.clone(),
            operator_home: temp.path().join("home"),
        },
        &NoEnvResolver,
    )
    .unwrap();

    let validated = validate_usage_sources_with_reader(catalog, &NoEnvResolver, &reader);

    assert!(
        validated.diagnostics.is_empty(),
        "{:?}",
        validated.diagnostics
    );
    assert_eq!(validated.accounts.len(), 1);
    assert_eq!(validated.accounts[0].surface_id, "codex");
    assert_eq!(validated.accounts[0].account_label, "alice@example.test");
    let debug = format!("{validated:?}");
    assert!(!debug.contains("fixture-secret"));
    assert!(!debug.contains(profile.to_string_lossy().as_ref()));
}

#[test]
fn profile_material_rotation_at_same_path_changes_catalog_entry_revision() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let profile = temp.path().join("codex-profile");
    write_codex_only_global(&config_root, &profile);
    write_codex_auth(
        &profile,
        "account-1",
        "eyJlbWFpbCI6ImFsaWNlQGV4YW1wbGUudGVzdCJ9",
        "fixture-secret-a",
    );
    let scope = UsageDiscoveryScope::HostDesktop {
        config_root: config_root.clone(),
        operator_home: temp.path().join("home"),
    };
    let first_catalog = discover_usage_sources(&scope, &NoEnvResolver).unwrap();
    let first = validate_usage_sources(first_catalog, &NoEnvResolver);
    let first_entries = crate::host::broker::usage_catalog_entries(&first);

    write_codex_auth(
        &profile,
        "account-1",
        "eyJlbWFpbCI6ImFsaWNlQGV4YW1wbGUudGVzdCJ9",
        "fixture-secret-b",
    );
    let second_catalog = discover_usage_sources(&scope, &NoEnvResolver).unwrap();
    let second = validate_usage_sources(second_catalog, &NoEnvResolver);
    let second_entries = crate::host::broker::usage_catalog_entries(&second);

    assert_eq!(
        first.bindings[0].capability_id,
        second.bindings[0].capability_id
    );
    assert_ne!(
        first.bindings[0].credential_revision,
        second.bindings[0].credential_revision
    );
    assert_ne!(first_entries, second_entries);
}

#[test]
fn disc_config_generation_rotates_capability_for_same_credential_identity() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let profile = temp.path().join("codex-profile");
    let scope = UsageDiscoveryScope::HostDesktop {
        config_root: config_root.clone(),
        operator_home: temp.path().join("home"),
    };
    write_codex_only_global(&config_root, &profile);
    write_codex_auth(
        &profile,
        "account-1",
        "eyJlbWFpbCI6ImFsaWNlQGV4YW1wbGUudGVzdCJ9",
        "fixture-secret",
    );
    let reader = RecordingProfileReader::default();
    let first = validate_usage_sources_with_reader(
        discover_usage_sources(&scope, &NoEnvResolver).unwrap(),
        &NoEnvResolver,
        &reader,
    );
    let first_capability = crate::host::usage_broker_capabilities(&first)
        .into_iter()
        .next()
        .unwrap();

    write_registry(&config_root, &[("codex-renamed", Agent::Codex, &profile)]);
    let second = validate_usage_sources_with_reader(
        discover_usage_sources(&scope, &NoEnvResolver).unwrap(),
        &NoEnvResolver,
        &reader,
    );
    let second_capability = crate::host::usage_broker_capabilities(&second)
        .into_iter()
        .next()
        .unwrap();

    assert_eq!(
        first.accounts[0].account_key,
        second.accounts[0].account_key
    );
    assert_ne!(first.config_generation, second.config_generation);
    assert_ne!(first_capability, second_capability);
}

#[test]
fn disc_source_missing_and_malformed_profiles_are_isolated_diagnostics() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let workspaces = config_root.join("workspaces");
    let valid = temp.path().join("valid");
    let missing = temp.path().join("missing");
    let malformed = temp.path().join("malformed");
    write_codex_only_global(&config_root, &valid);
    std::fs::create_dir_all(&workspaces).unwrap();
    write_codex_workspace(&workspaces.join("missing.toml"), &missing);
    write_codex_workspace(&workspaces.join("malformed.toml"), &malformed);
    write_codex_auth(
        &valid,
        "account-valid",
        "eyJlbWFpbCI6InZhbGlkQGV4YW1wbGUudGVzdCJ9",
        "valid-secret",
    );
    std::fs::create_dir_all(&malformed).unwrap();
    std::fs::write(malformed.join("auth.json"), "{broken-secret").unwrap();
    let reader = RecordingProfileReader::default();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root: config_root.clone(),
            operator_home: temp.path().join("home"),
        },
        &NoEnvResolver,
    )
    .unwrap();

    let validated = validate_usage_sources_with_reader(catalog, &NoEnvResolver, &reader);

    assert_eq!(validated.accounts.len(), 1);
    assert!(validated.diagnostics.iter().any(|diagnostic| {
        diagnostic.surface_id.as_deref() == Some("codex")
            && diagnostic.issue == UsageDiscoveryIssue::CredentialMissing
    }));
    assert!(validated.diagnostics.iter().any(|diagnostic| {
        diagnostic.surface_id.as_deref() == Some("codex")
            && diagnostic.issue == UsageDiscoveryIssue::CredentialMalformed
    }));
    let debug = format!("{:?}", validated.diagnostics);
    assert!(!debug.contains("broken-secret"));
    assert!(!debug.contains(temp.path().to_string_lossy().as_ref()));
}

#[test]
fn disc_source_kimi_profile_requires_credentials_in_selected_root() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let kimi_root = temp.path().join("kimi-profile");
    std::fs::create_dir_all(&kimi_root).unwrap();
    std::fs::create_dir_all(&config_root).unwrap();
    write_registry(&config_root, &[("kimi", Agent::Kimi, &kimi_root)]);
    std::fs::create_dir_all(kimi_root.join("credentials")).unwrap();
    std::fs::write(
        kimi_root.join("credentials/kimi-code.json"),
        r#"{"access_token":"selected-kimi-token"}"#,
    )
    .unwrap();
    let catalog = discover_usage_sources(
        &UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: temp.path().join("home"),
        },
        &NoEnvResolver,
    )
    .unwrap();

    let validated = validate_usage_sources(catalog, &NoEnvResolver);

    assert!(
        validated.diagnostics.is_empty(),
        "{:?}",
        validated.diagnostics
    );
    assert_eq!(validated.accounts.len(), 1);
    assert_eq!(validated.accounts[0].account_label, "kimi");
    assert_eq!(validated.bindings.len(), 1);
}
