// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use std::collections::{BTreeMap, BTreeSet};

use jackin_config::AppConfig;
use jackin_core::{UsageCredentialEnvName, WorkspaceName};
use jackin_usage_discovery::{
    DiscoveredAccountDescriptor, UsageDiscoveryScope, discover_usage_sources,
    validate_usage_sources,
};
use jackin_usage_host_credentials::{
    ForwardedUsageAccount, ProviderCredentialEnvResolution, ProviderCredentialEnvResolver,
};

/// Discovery-suite resolver fake, duplicated for the two runtime-glue tests
/// that stay in `jackin-usage` because they open a `HostUsageRuntime`.
struct NoEnvResolver;

impl ProviderCredentialEnvResolver for NoEnvResolver {
    fn resolve_provider_credentials(
        &self,
        _config: &AppConfig,
        _workspace: Option<&WorkspaceName>,
        _role: Option<&str>,
        _keys: &[UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution> {
        Vec::new()
    }
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
        .open(HostRuntimeConfig::under_data_dir(temp.path()))
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
fn disc_dedup_legacy_shared_snapshot_never_creates_active_row() {
    let temp = tempfile::tempdir().unwrap();
    let shared = temp.path().join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    let mut historical = FocusedUsageView::unavailable("stale", 1);
    historical.focused_agent = Some("codex".to_owned());
    historical.focused_provider = Some("Codex".to_owned());
    historical.account.provider_label = "OpenAI / Codex".to_owned();
    historical.account.account_label = "removed@example.test".to_owned();
    std::fs::write(
        shared.join("usage-old.snapshot.json"),
        serde_json::to_vec(&historical).unwrap(),
    )
    .unwrap();
    let store = temp.path().join("missing.db");

    let empty_membership: &[DiscoveredAccountDescriptor] = &[];
    let catalog = accounts::materialize_account_catalog(
        &[],
        &BTreeMap::new(),
        &BTreeMap::new(),
        &store,
        Some(empty_membership),
        &HostAccountCatalogStores,
    )
    .unwrap();

    assert!(catalog.entries_for_surface(HostSurfaceId::Codex).is_empty());
}
