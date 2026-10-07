// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use std::collections::BTreeSet;

use std::fs;

use jackin_config::AppConfig;
use jackin_core::{UsageCredentialEnvName, WorkspaceName};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageGenerationView, UsageRefreshPhase,
};
use jackin_usage_discovery::{
    UsageDiscoveryScope, ValidatedCredentialBinding, ValidatedCredentialSource,
    ValidatedUsageDiscovery, capability_for_binding, discover_usage_sources,
    usage_broker_capabilities, validate_usage_sources,
};
use jackin_usage_host_credentials::{
    OpaqueCredentialHandle, ProviderCredentialEnvResolution, ProviderCredentialEnvResolver,
};

fn quota_view() -> FocusedUsageView {
    let mut view = FocusedUsageView::unavailable("claude", chrono::Utc::now().timestamp());
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    view.account.provider_label = "Claude".to_owned();
    view.account.account_label = "account@example.test".to_owned();
    view.buckets = vec![QuotaBucketView {
        label: "Weekly".to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: Some(75),
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }];
    view
}
struct FixedHandleResolver;

impl ProviderCredentialEnvResolver for FixedHandleResolver {
    fn resolve_provider_credentials(
        &self,
        config: &AppConfig,
        _workspace: Option<&WorkspaceName>,
        _role: Option<&str>,
        keys: &[UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution> {
        keys.iter()
            .filter(|entry| config.env.contains_key(entry.name))
            .map(|entry| ProviderCredentialEnvResolution {
                key: entry.name.to_owned(),
                outcome: ProviderCredentialEnvOutcome::Resolved(OpaqueCredentialHandle::new(
                    "fixture-credential-1",
                )),
            })
            .collect()
    }
}
fn failed_generation(
    capability: &UsageAccountCapability,
    kind: UsageCoordinationErrorKind,
    message: &str,
) -> UsageGenerationView {
    UsageGenerationView {
        capability: capability.clone(),
        generation: 1,
        phase: UsageRefreshPhase::Failed,
        snapshot: None,
        error: Some(UsageCoordinationError {
            kind,
            message: message.to_owned(),
        }),
        retry_at_epoch: None,
    }
}

#[test]
fn broker_failure_without_snapshot_surfaces_honest_gap_in_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let mut config = AppConfig::default();
    config.accounts.insert(
        "codex-key".to_owned(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "codex-key".to_owned(),
            provider: jackin_config::AiProvider::OpenAi,
            credential: jackin_config::AccountCredential::ApiKey {
                value: jackin_config::EnvValue::Plain("fixture-openai-key".to_owned()),
                base_url: None,
                model: None,
            },
        },
    );
    fs::create_dir_all(&config_root).unwrap();
    fs::write(
        config_root.join("config.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();
    let validated = validate_usage_sources(
        discover_usage_sources(
            &UsageDiscoveryScope::HostDesktop {
                config_root,
                operator_home: temp.path().join("home"),
            },
            &FixedHandleResolver,
        )
        .unwrap(),
        &FixedHandleResolver,
    );
    assert_eq!(validated.accounts.len(), 1);
    let capabilities = usage_broker_capabilities(&validated);
    assert_eq!(capabilities.len(), 1);

    let mut runtime = HostUsageRuntime::new();
    runtime
        .open(HostRuntimeConfig::under_data_dir(temp.path().join("data")))
        .unwrap();
    runtime.discovery = Some(validated);
    runtime
        .apply_broker_generation(failed_generation(
            &capabilities[0],
            UsageCoordinationErrorKind::ProviderUnavailable,
            "Grok billing requires an authenticated profile",
        ))
        .unwrap();

    let view = runtime.snapshot("codex").unwrap();
    assert!(!view.is_refreshing_placeholder());
    assert_eq!(view.status, UsageSnapshotStatus::Unavailable);
    assert_eq!(view.account.account_label, "codex-key");
    assert_eq!(
        view.last_error.as_deref(),
        Some("Grok billing requires an authenticated profile")
    );

    // A later success still replaces the recorded error view.
    let mut fresh = quota_view();
    fresh.account.provider_label = "OpenAI / Codex".to_owned();
    runtime
        .apply_broker_generation(UsageGenerationView {
            capability: capabilities[0].clone(),
            generation: 2,
            phase: UsageRefreshPhase::Completed,
            snapshot: Some(fresh),
            error: None,
            retry_at_epoch: None,
        })
        .unwrap();
    let view = runtime.snapshot("codex").unwrap();
    assert_eq!(view.status, UsageSnapshotStatus::Fresh);
}

#[test]
fn broker_failure_for_anonymous_source_stays_surface_scoped() {
    let validated = validate_usage_sources(
        discover_usage_sources(
            &UsageDiscoveryScope::Capsule {
                forwarded_accounts: vec![ForwardedUsageAccount {
                    surface_id: "codex".to_owned(),
                    capability_id: "capability-a".to_owned(),
                    account_label: None,
                }],
            },
            &FixedHandleResolver,
        )
        .unwrap(),
        &FixedHandleResolver,
    );
    assert!(validated.accounts.is_empty());
    assert!(validated.bindings[0].identity.is_none());
    let capabilities = usage_broker_capabilities(&validated);
    assert_eq!(capabilities.len(), 1);

    let temp = tempfile::tempdir().unwrap();
    let mut runtime = HostUsageRuntime::new();
    runtime
        .open(HostRuntimeConfig::under_data_dir(temp.path().join("data")))
        .unwrap();
    runtime.discovery = Some(validated);
    runtime
        .apply_broker_generation(failed_generation(
            &capabilities[0],
            UsageCoordinationErrorKind::NeedsSecret,
            "Gemini auth not available to Capsule",
        ))
        .unwrap();

    let view = runtime.snapshot("codex").unwrap();
    assert_eq!(view.status, UsageSnapshotStatus::NeedsSecret);
    assert_eq!(
        view.last_error.as_deref(),
        Some("Gemini auth not available to Capsule")
    );
    assert!(runtime.list_accounts(None).unwrap().is_empty());
}

#[test]
fn rotated_catalog_revision_rejects_in_flight_broker_result() {
    let binding = ValidatedCredentialBinding {
        surface: HostSurfaceId::Claude,
        identity: Some(CanonicalAccountIdentity {
            surface: HostSurfaceId::Claude,
            subject: CanonicalAccountSubject::ProviderId("provider-account".to_owned()),
        }),
        source_id: "source-0001".to_owned(),
        capability_id: "capability-0001".to_owned(),
        credential_revision: "credential-revision".to_owned(),
        provenance: BTreeSet::from(["account work".to_owned()]),
        source: ValidatedCredentialSource::Capability,
    };
    let old_capability = capability_for_binding(&binding, Some("generation-old"));
    let current_capability = capability_for_binding(&binding, Some("generation-current"));
    assert_ne!(old_capability, current_capability);

    let temp = tempfile::tempdir().unwrap();
    let mut runtime = HostUsageRuntime::new();
    runtime
        .open(HostRuntimeConfig::under_data_dir(temp.path()))
        .unwrap();
    runtime.discovery = Some(ValidatedUsageDiscovery {
        config_generation: Some("generation-current".to_owned()),
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: vec![binding],
    });

    runtime
        .apply_broker_generation(UsageGenerationView {
            capability: old_capability,
            generation: 1,
            phase: UsageRefreshPhase::Completed,
            snapshot: Some(quota_view()),
            error: None,
            retry_at_epoch: None,
        })
        .unwrap();

    assert!(runtime.discovered_views.is_empty());
    assert!(runtime.discovered_provider_views.is_empty());
}
