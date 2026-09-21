// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use crate::host::{HostSurfaceId, OpaqueCredentialHandle};
use jackin_config::AppConfig;
use jackin_core::{UsageCredentialEnvName, WorkspaceName};
use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
    UsageSource,
};
use jackin_protocol::usage_broker::{
    UsageCatalogEntry, UsageCredentialScope, UsageCredentialSourceIdentity,
    UsageCredentialSourceProof, UsageFreshnessPhaseV1, UsageIdentityKindV1,
    UsageProjectionRefreshStateV1, UsageRefreshPhase, usage_credential_material_fingerprint,
};

use super::*;
use crate::host::{ForwardedUsageAccount, ProviderCredentialEnvResolution};

struct CountingExecutor {
    calls: AtomicUsize,
}

struct RetryRecordingResolver {
    manual_retries: Arc<AtomicUsize>,
}

impl Default for RetryRecordingResolver {
    fn default() -> Self {
        Self {
            manual_retries: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl ProviderCredentialEnvResolver for RetryRecordingResolver {
    fn begin_manual_retry(&self) {
        self.manual_retries.fetch_add(1, Ordering::SeqCst);
    }

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

struct NoopCredentialResolver;

impl ProviderCredentialEnvResolver for NoopCredentialResolver {
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

impl UsageProviderExecutor for CountingExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        ProviderProbeOutcome::success(quota_view())
    }
}

fn capability() -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: "abc123".to_owned(),
        surface_id: "claude".to_owned(),
    }
}

fn second_capability() -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: "def456".to_owned(),
        surface_id: "codex".to_owned(),
    }
}

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

fn env_material(source_name: &str, material: &str) -> ProviderCredentialSourceMaterial {
    ProviderCredentialSourceMaterial {
        source: UsageCredentialSourceIdentity::HostEnv {
            name: source_name.to_owned(),
        },
        material_fingerprint: usage_credential_material_fingerprint(material),
    }
}

fn env_scope(
    account_id: &str,
    surface_id: &str,
    key: &str,
    material: &ProviderCredentialSourceMaterial,
) -> UsageCredentialScope {
    UsageCredentialScope {
        sources: BTreeSet::from([UsageCredentialSourceProof {
            account_id: account_id.to_owned(),
            surface_id: surface_id.to_owned(),
            key: key.to_owned(),
            source: material.source.clone(),
            material_fingerprint: material.material_fingerprint.clone(),
        }]),
    }
}

#[test]
fn launch_scope_fails_closed_on_rotation_repoint_and_mixed_agent_source() {
    let capability = UsageAccountCapability {
        account_id: "shared-account".to_owned(),
        surface_id: "amp".to_owned(),
    };
    let staged = env_material("JACKIN_AGENT_A_KEY", "S1");
    let binding = ValidatedCredentialBinding {
        surface: HostSurfaceId::Amp,
        identity: None,
        source_id: "source-a".to_owned(),
        capability_id: "capability-a".to_owned(),
        credential_revision: "credential-revision-a".to_owned(),
        provenance: BTreeSet::from(["account shared-account".to_owned()]),
        source: ValidatedCredentialSource::Env {
            handle: OpaqueCredentialHandle::new("handle-a"),
            key: "AMP_API_KEY".to_owned(),
            material: Some(staged.clone()),
        },
    };
    let executor = DiscoveryProviderExecutor {
        bindings: Mutex::new(BTreeMap::from([(capability.clone(), binding)])),
        validated_catalog: Mutex::new(None),
        scope: UsageDiscoveryScope::HostDesktop {
            config_root: PathBuf::new(),
            operator_home: PathBuf::new(),
        },
        resolver: Arc::new(NoopCredentialResolver),
        probe_budget: Duration::from_secs(1),
    };
    let staged_scope = env_scope("shared-account", "amp", "AMP_API_KEY", &staged);
    executor
        .authorize_credential_scope(&capability, &staged_scope)
        .expect("staged source should authorize");

    let rotated = env_material("JACKIN_AGENT_A_KEY", "S2");
    executor
        .bindings
        .lock()
        .unwrap()
        .get_mut(&capability)
        .unwrap()
        .source = ValidatedCredentialSource::Env {
        handle: OpaqueCredentialHandle::new("handle-a-rotated"),
        key: "AMP_API_KEY".to_owned(),
        material: Some(rotated),
    };
    let error = executor
        .authorize_credential_scope(&capability, &staged_scope)
        .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);

    let repointed = env_material("JACKIN_AGENT_B_KEY", "S1");
    executor
        .bindings
        .lock()
        .unwrap()
        .get_mut(&capability)
        .unwrap()
        .source = ValidatedCredentialSource::Env {
        handle: OpaqueCredentialHandle::new("handle-b-repointed"),
        key: "AMP_API_KEY".to_owned(),
        material: Some(repointed.clone()),
    };
    let error = executor
        .authorize_credential_scope(&capability, &staged_scope)
        .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);

    let mixed_scope = UsageCredentialScope {
        sources: staged_scope
            .sources
            .iter()
            .cloned()
            .chain(env_scope("shared-account", "amp", "AMP_API_KEY", &repointed).sources)
            .collect(),
    };
    let error = executor
        .authorize_credential_scope(&capability, &mixed_scope)
        .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
}

#[test]
fn launch_scope_accepts_provider_native_zhipu_alias_for_canonical_zai_binding() {
    let capability = UsageAccountCapability {
        account_id: "zhipu-account".to_owned(),
        surface_id: "zai".to_owned(),
    };
    let staged = env_material("ZAI_HOST_SECRET", "S1");
    let executor = DiscoveryProviderExecutor {
        bindings: Mutex::new(BTreeMap::from([(
            capability.clone(),
            ValidatedCredentialBinding {
                surface: HostSurfaceId::Zai,
                identity: None,
                source_id: "source-zai".to_owned(),
                capability_id: "capability-zai".to_owned(),
                credential_revision: "credential-revision-zai".to_owned(),
                provenance: BTreeSet::from(["account zhipu-account".to_owned()]),
                source: ValidatedCredentialSource::Env {
                    handle: OpaqueCredentialHandle::new("handle-zai"),
                    key: "ZAI_API_KEY".to_owned(),
                    material: Some(staged.clone()),
                },
            },
        )])),
        validated_catalog: Mutex::new(None),
        scope: UsageDiscoveryScope::HostDesktop {
            config_root: PathBuf::new(),
            operator_home: PathBuf::new(),
        },
        resolver: Arc::new(NoopCredentialResolver),
        probe_budget: Duration::from_secs(1),
    };

    let scope = env_scope("zhipu-account", "zai", "ZHIPU_API_KEY", &staged);
    executor
        .authorize_credential_scope(&capability, &scope)
        .expect("provider-native key alias should authorize");
}

#[test]
fn discovery_provider_rate_limit_preserves_retry_after() {
    let before = chrono::Utc::now().timestamp();
    let mut view = quota_view();
    view.status = UsageSnapshotStatus::Stale;
    view.last_error = Some("provider HTTP 429; Retry-After: 97".to_owned());

    let ProviderProbeOutcome::Failure {
        kind,
        message,
        retry_at_epoch,
    } = provider_probe_outcome(view)
    else {
        panic!("rate-limited view must not publish as success");
    };
    let after = chrono::Utc::now().timestamp();
    assert_eq!(kind, UsageCoordinationErrorKind::RateLimited);
    assert_eq!(message, "usage provider rate limit is active");
    assert!(
        retry_at_epoch.is_some_and(|deadline| { (before + 97..=after + 97).contains(&deadline) })
    );
}

#[test]
fn discovery_provider_stale_and_error_views_are_retryable_failures() {
    for status in [UsageSnapshotStatus::Stale, UsageSnapshotStatus::Error] {
        let mut view = quota_view();
        view.status = status;
        view.last_error = None;
        let ProviderProbeOutcome::Failure {
            kind,
            retry_at_epoch,
            ..
        } = provider_probe_outcome(view)
        else {
            panic!("{status:?} provider view must not publish as success");
        };
        assert_eq!(kind, UsageCoordinationErrorKind::ProviderUnavailable);
        assert_eq!(retry_at_epoch, None);
    }
}

#[test]
fn discovery_provider_unsupported_views_remain_publishable_unsupported() {
    let mut view = quota_view();
    view.status = UsageSnapshotStatus::Unsupported;
    assert!(matches!(
        provider_probe_outcome(view),
        ProviderProbeOutcome::Success(_)
    ));
}

#[test]
fn background_rediscovery_does_not_start_manual_retry_or_admit_mismatch() {
    let resolver = RetryRecordingResolver::default();
    let scope = UsageDiscoveryScope::Capsule {
        forwarded_accounts: vec![ForwardedUsageAccount {
            surface_id: "claude".to_owned(),
            capability_id: "different-capability".to_owned(),
            account_label: Some("other@example.test".to_owned()),
        }],
    };
    let (binding, refreshed) = rediscover_bindings(&scope, &resolver, &capability());

    assert!(binding.is_none());
    assert!(refreshed.is_none());
    assert_eq!(resolver.manual_retries.load(Ordering::SeqCst), 0);
}

#[test]
fn discovery_executor_rejects_catalog_that_does_not_match_current_scope() {
    let manual_retries = Arc::new(AtomicUsize::new(0));
    let resolver: Arc<dyn ProviderCredentialEnvResolver> = Arc::new(RetryRecordingResolver {
        manual_retries: Arc::clone(&manual_retries),
    });
    let executor = DiscoveryProviderExecutor {
        bindings: Mutex::new(BTreeMap::new()),
        validated_catalog: Mutex::new(None),
        scope: UsageDiscoveryScope::Capsule {
            forwarded_accounts: Vec::new(),
        },
        resolver: Arc::clone(&resolver),
        probe_budget: Duration::from_secs(1),
    };
    let error = executor
        .validate_catalog(&[UsageCatalogEntry {
            capability: capability(),
            revision: "mismatched".to_owned(),
        }])
        .expect_err("mismatched catalog must fail closed");

    assert_eq!(
        error.kind,
        UsageCoordinationErrorKind::CatalogRevisionConflict
    );
    assert_eq!(manual_retries.load(Ordering::SeqCst), 0);
}

#[test]
fn discovery_provider_failures_carry_each_gap_reason() {
    // Every gap kind keeps its failure category but renders the collector's
    // specific message instead of the generic fallback. Payloads below are
    // the collectors' real gap strings.
    for (status, kind, gap) in [
        (
            UsageSnapshotStatus::Error,
            UsageCoordinationErrorKind::ProviderUnavailable,
            "Grok billing requires an authenticated profile",
        ),
        (
            UsageSnapshotStatus::Unavailable,
            UsageCoordinationErrorKind::ProviderUnavailable,
            "Grok billing requires an authenticated profile",
        ),
        (
            UsageSnapshotStatus::Stale,
            UsageCoordinationErrorKind::ProviderUnavailable,
            "Grok billing requires an authenticated profile",
        ),
        (
            UsageSnapshotStatus::NeedsSecret,
            UsageCoordinationErrorKind::NeedsSecret,
            "Gemini auth not available to Capsule",
        ),
        (
            UsageSnapshotStatus::NeedsLogin,
            UsageCoordinationErrorKind::NeedsSecret,
            "Grok auth not available to Capsule",
        ),
    ] {
        let mut view = quota_view();
        view.status = status;
        view.last_error = Some(gap.to_owned());
        let ProviderProbeOutcome::Failure {
            kind: actual,
            message,
            ..
        } = provider_probe_outcome(view)
        else {
            panic!("{status:?} provider view must not publish as success");
        };
        assert_eq!(actual, kind);
        assert_eq!(message, gap);
    }
}

#[test]
fn discovery_provider_failures_without_reason_keep_generic_fallback() {
    for (status, kind, fallback) in [
        (
            UsageSnapshotStatus::Error,
            UsageCoordinationErrorKind::ProviderUnavailable,
            "usage provider quota is unavailable",
        ),
        (
            UsageSnapshotStatus::NeedsSecret,
            UsageCoordinationErrorKind::NeedsSecret,
            "usage provider credentials require operator action",
        ),
    ] {
        for last_error in [None, Some(String::new()), Some("   ".to_owned())] {
            let mut view = quota_view();
            view.status = status;
            view.last_error = last_error;
            let ProviderProbeOutcome::Failure {
                kind: actual,
                message,
                ..
            } = provider_probe_outcome(view)
            else {
                panic!("{status:?} provider view must not publish as success");
            };
            assert_eq!(actual, kind);
            assert_eq!(message, fallback);
        }
    }
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
                outcome: crate::host::ProviderCredentialEnvOutcome::Resolved(
                    OpaqueCredentialHandle::new("fixture-credential-1"),
                ),
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
        .open(crate::host::HostRuntimeConfig::under_data_dir(
            temp.path().join("data"),
        ))
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
        .open(crate::host::HostRuntimeConfig::under_data_dir(
            temp.path().join("data"),
        ))
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
#[expect(
    clippy::too_many_lines,
    reason = "One table-style forwarding matrix: six source fixtures share one discovery setup; splitting would duplicate the binding fixtures per case."
)]
fn forwarded_scope_selects_only_accounts_backed_by_forwarded_sources() {
    use crate::host::{CanonicalAccountIdentity, CanonicalAccountSubject, HostSurfaceId};

    let profile_identity = CanonicalAccountIdentity {
        surface: HostSurfaceId::Amp,
        subject: CanonicalAccountSubject::ProviderStableHandle("profile@example.test".to_owned()),
    };
    let env_identity = CanonicalAccountIdentity {
        surface: HostSurfaceId::Amp,
        subject: CanonicalAccountSubject::ProviderStableHandle("env@example.test".to_owned()),
    };
    let env_material = ProviderCredentialSourceMaterial {
        source: UsageCredentialSourceIdentity::HostEnv {
            name: "AMP_API_KEY".to_owned(),
        },
        material_fingerprint: usage_credential_material_fingerprint("env-secret"),
    };
    let scope = "workspace sample role test";
    let discovery = ValidatedUsageDiscovery {
        config_generation: Some("generation".to_owned()),
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: vec![
            ValidatedCredentialBinding {
                surface: HostSurfaceId::Amp,
                identity: Some(profile_identity),
                source_id: "profile-source".to_owned(),
                capability_id: "profile-capability".to_owned(),
                credential_revision: "profile-revision".to_owned(),
                provenance: BTreeSet::from([
                    scope.to_owned(),
                    "account account-profile".to_owned(),
                ]),
                source: ValidatedCredentialSource::Profile(
                    super::super::discovery::ProfileCredentialMaterial::Amp {
                        key: "profile-secret".to_owned(),
                    },
                ),
            },
            ValidatedCredentialBinding {
                surface: HostSurfaceId::Amp,
                identity: Some(env_identity),
                source_id: "env-source".to_owned(),
                capability_id: "env-capability".to_owned(),
                credential_revision: "env-revision".to_owned(),
                provenance: BTreeSet::from([scope.to_owned(), "account account-env".to_owned()]),
                source: ValidatedCredentialSource::Env {
                    handle: OpaqueCredentialHandle::new("env-handle"),
                    key: "AMP_API_KEY".to_owned(),
                    material: Some(env_material.clone()),
                },
            },
        ],
    };
    let profile_capability = capability_for_binding(
        &discovery.bindings[0],
        discovery.config_generation.as_deref(),
    );
    let env_capability = capability_for_binding(
        &discovery.bindings[1],
        discovery.config_generation.as_deref(),
    );

    let profile_only = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::new(),
            selected_account_surfaces: BTreeMap::new(),
            profile_surface_ids: BTreeSet::from(["amp".to_owned()]),
            env_keys: BTreeSet::new(),
            credential_scope: UsageCredentialScope::default(),
        },
    );
    assert_eq!(profile_only, vec![profile_capability.clone()]);

    let env_only = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::new(),
            selected_account_surfaces: BTreeMap::new(),
            profile_surface_ids: BTreeSet::new(),
            env_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
            credential_scope: UsageCredentialScope::default(),
        },
    );
    assert_eq!(env_only, vec![env_capability.clone()]);

    let selected_profile = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::from(["account-profile".to_owned()]),
            selected_account_surfaces: BTreeMap::from([(
                "account-profile".to_owned(),
                "amp".to_owned(),
            )]),
            profile_surface_ids: BTreeSet::from(["amp".to_owned()]),
            env_keys: BTreeSet::new(),
            credential_scope: UsageCredentialScope::default(),
        },
    );
    assert_eq!(selected_profile, vec![profile_capability.clone()]);

    let selected_env = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::from(["account-env".to_owned()]),
            selected_account_surfaces: BTreeMap::from([(
                "account-env".to_owned(),
                "amp".to_owned(),
            )]),
            profile_surface_ids: BTreeSet::new(),
            env_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
            credential_scope: UsageCredentialScope {
                sources: BTreeSet::from([UsageCredentialSourceProof {
                    account_id: "account-env".to_owned(),
                    surface_id: "amp".to_owned(),
                    key: "AMP_API_KEY".to_owned(),
                    source: env_material.source.clone(),
                    material_fingerprint: env_material.material_fingerprint.clone(),
                }]),
            },
        },
    );
    assert_eq!(selected_env, vec![env_capability]);

    let wrong_surface = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::from(["account-profile".to_owned()]),
            selected_account_surfaces: BTreeMap::from([(
                "account-profile".to_owned(),
                "codex".to_owned(),
            )]),
            profile_surface_ids: BTreeSet::from(["amp".to_owned()]),
            env_keys: BTreeSet::new(),
            credential_scope: UsageCredentialScope::default(),
        },
    );
    assert!(
        wrong_surface.is_empty(),
        "account proof must bind both configured account and provider surface"
    );

    let wrong_account = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::from(["account-does-not-exist".to_owned()]),
            selected_account_surfaces: BTreeMap::new(),
            profile_surface_ids: BTreeSet::from(["amp".to_owned()]),
            env_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
            credential_scope: UsageCredentialScope::default(),
        },
    );
    assert!(wrong_account.is_empty());

    assert_eq!(
        usage_capability_for_selected_account(&discovery, "account-profile", "amp"),
        Some(profile_capability.clone())
    );
    assert_eq!(
        usage_capability_for_selected_account(&discovery, "account-profile", "claude"),
        None
    );

    let publication = publication_identity_metadata(&discovery);
    assert_eq!(
        publication[&profile_capability].identity_kind,
        UsageIdentityKindV1::ProviderStableHandle
    );
    assert_eq!(publication[&profile_capability].provenance_count, 2);
}

#[test]
fn rotated_catalog_revision_rejects_in_flight_broker_result() {
    use crate::host::{CanonicalAccountIdentity, CanonicalAccountSubject, HostSurfaceId};

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
        .open(crate::host::HostRuntimeConfig::under_data_dir(temp.path()))
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

#[test]
fn broker_catalog_admits_current_identity_and_rejects_stale_identity() {
    use crate::host::{CanonicalAccountIdentity, CanonicalAccountSubject, HostSurfaceId};

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
    let stale = capability_for_binding(&binding, Some("generation-stale"));
    let current = capability_for_binding(&binding, Some("generation-current"));
    assert_ne!(stale, current);

    let temp = tempfile::tempdir().unwrap();
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        executor,
    )
    .unwrap();
    client
        .reconcile_catalog(
            "generation-current".to_owned(),
            vec![UsageCatalogEntry {
                capability: current.clone(),
                revision: "credential-current".to_owned(),
            }],
        )
        .unwrap();

    assert_eq!(client.current(current).unwrap().generation, 0);
    assert_eq!(
        client.current(stale).unwrap_err().kind,
        UsageCoordinationErrorKind::CatalogRevoked
    );
}

#[test]
fn broker_catalog_match_requires_full_revision_and_entry_revisions() {
    use crate::host::{CanonicalAccountIdentity, CanonicalAccountSubject, HostSurfaceId};

    let discovery = ValidatedUsageDiscovery {
        config_generation: Some("generation-current".to_owned()),
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: vec![ValidatedCredentialBinding {
            surface: HostSurfaceId::Claude,
            identity: Some(CanonicalAccountIdentity {
                surface: HostSurfaceId::Claude,
                subject: CanonicalAccountSubject::ProviderId("provider-account".to_owned()),
            }),
            source_id: "source-0001".to_owned(),
            capability_id: "capability-0001".to_owned(),
            credential_revision: "credential-revision-a".to_owned(),
            provenance: BTreeSet::from(["account work".to_owned()]),
            source: ValidatedCredentialSource::Capability,
        }],
    };
    let entries = usage_catalog_entries(&discovery);
    ensure_catalog_matches(&discovery, "generation-current", &entries).unwrap();

    let mut changed_entries = entries.clone();
    changed_entries[0].revision.push_str("-changed");
    assert_eq!(
        ensure_catalog_matches(&discovery, "generation-current", &changed_entries)
            .unwrap_err()
            .kind,
        UsageCoordinationErrorKind::CatalogRevisionConflict
    );
    assert_eq!(
        ensure_catalog_matches(&discovery, "generation-old", &entries)
            .unwrap_err()
            .kind,
        UsageCoordinationErrorKind::CatalogRevisionConflict
    );
}

#[test]
fn usage_broker_twenty_clients_join_one_generation_and_probe() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete_executor = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete_executor;
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        broker_executor,
    )
    .unwrap();
    let barrier = Arc::new(Barrier::new(20));
    let mut clients = Vec::new();
    for _ in 0..20 {
        let client = client.clone();
        let barrier = Arc::clone(&barrier);
        clients.push(thread::spawn(move || {
            barrier.wait();
            client.refresh(capability(), 0, true).unwrap()
        }));
    }
    let generations = clients
        .into_iter()
        .map(|client| client.join().unwrap())
        .collect::<Vec<_>>();
    assert!(generations.iter().all(|state| state.generation == 1));
    let terminal = client
        .join(capability(), 1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn concurrent_catalog_rotations_publish_one_complete_revision() {
    let temp = tempfile::tempdir().unwrap();
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        Arc::new(CountingExecutor {
            calls: AtomicUsize::new(0),
        }),
    )
    .unwrap();
    let account_a = capability();
    let account_b = second_capability();
    let entry_a = UsageCatalogEntry {
        capability: account_a.clone(),
        revision: "entry-a".to_owned(),
    };
    let entry_b = UsageCatalogEntry {
        capability: account_b.clone(),
        revision: "entry-b".to_owned(),
    };
    let lease = client.current_projection().unwrap().projection_id;
    let barrier = Arc::new(Barrier::new(3));
    let first = {
        let client = client.clone();
        let lease = lease.clone();
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            client.reconcile_catalog_if_projection(
                Some(lease),
                "catalog-a".to_owned(),
                vec![entry_a],
            )
        })
    };
    let second = {
        let client = client.clone();
        let lease = lease.clone();
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            client.reconcile_catalog_if_projection(
                Some(lease),
                "catalog-b".to_owned(),
                vec![entry_b],
            )
        })
    };
    barrier.wait();
    let first = first.join().unwrap();
    let second = second.join().unwrap();
    let (winner, rejected) = match (first, second) {
        (Ok(winner), Err(rejected)) | (Err(rejected), Ok(winner)) => (winner, rejected),
        (Ok(_), Ok(_)) => panic!("two catalog rotations committed"),
        (Err(first), Err(second)) => {
            panic!("both catalog rotations rejected: {first:?}; {second:?}")
        }
    };
    assert_eq!(
        rejected.kind,
        UsageCoordinationErrorKind::CatalogRevisionConflict
    );

    let final_projection = client.current_projection().unwrap();
    assert_eq!(
        final_projection.discovery_revision,
        winner.discovery_revision
    );
    match winner.discovery_revision.as_str() {
        "catalog-a" => {
            assert_eq!(client.current(account_a).unwrap().generation, 0);
            assert_eq!(
                client.current(account_b).unwrap_err().kind,
                UsageCoordinationErrorKind::CatalogRevoked
            );
        }
        "catalog-b" => {
            assert_eq!(client.current(account_b).unwrap().generation, 0);
            assert_eq!(
                client.current(account_a).unwrap_err().kind,
                UsageCoordinationErrorKind::CatalogRevoked
            );
        }
        revision => panic!("mixed or unknown catalog revision: {revision}"),
    }
}

#[test]
fn catalog_cas_rejects_a_stale_rotation_after_a_newer_winner() {
    let temp = tempfile::tempdir().unwrap();
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        Arc::new(CountingExecutor {
            calls: AtomicUsize::new(0),
        }),
    )
    .unwrap();
    let lease = client.current_projection().unwrap().projection_id;
    let winning = UsageCatalogEntry {
        capability: capability(),
        revision: "entry-winning".to_owned(),
    };
    let stale = UsageCatalogEntry {
        capability: second_capability(),
        revision: "entry-stale".to_owned(),
    };

    let winner = client
        .reconcile_catalog_if_projection(
            Some(lease.clone()),
            "catalog-winning".to_owned(),
            vec![winning],
        )
        .unwrap();
    let error = client
        .reconcile_catalog_if_projection(Some(lease), "catalog-stale".to_owned(), vec![stale])
        .unwrap_err();

    assert_eq!(
        error.kind,
        UsageCoordinationErrorKind::CatalogRevisionConflict
    );
    assert_eq!(client.current_projection().unwrap(), winner);
}

#[test]
fn usage_broker_handshake_mismatch_fails_before_provider_dispatch() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let concrete_executor = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete_executor;
    let client = ensure_usage_broker_with_executor(config.clone(), broker_executor).unwrap();
    let incompatible = UsageBrokerClient::at(client.socket_path, "other-build".to_owned());
    let error = incompatible.refresh(capability(), 0, true).unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::ProtocolMismatch);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn existing_broker_reconcile_revokes_without_returning_stale_projection() {
    let temp = tempfile::tempdir().unwrap();
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        executor,
    )
    .unwrap();
    let entry = UsageCatalogEntry {
        capability: capability(),
        revision: "credential-a".to_owned(),
    };

    let admitted = client
        .reconcile_catalog("catalog-a".to_owned(), vec![entry.clone()])
        .unwrap();
    let queued = client.refresh(capability(), 0, true).unwrap();
    let completed = client
        .join(capability(), queued.generation, Duration::from_secs(2))
        .unwrap();
    assert_eq!(completed.phase, UsageRefreshPhase::Completed);

    let removed = client
        .reconcile_catalog("catalog-b".to_owned(), Vec::new())
        .unwrap();
    assert_eq!(removed.broker_instance_id, admitted.broker_instance_id);
    assert_eq!(removed.discovery_revision, "catalog-b");
    assert_eq!(
        removed.providers[0].accounts[0].canonical_account_id,
        capability().account_id
    );
    assert_eq!(
        removed.providers[0].accounts[0].status_label.as_deref(),
        Some("removed")
    );
    assert_eq!(client.current_projection().unwrap(), removed);
}

#[test]
fn broker_client_scoped_operation_requires_relay_and_never_probes() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete;
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        broker_executor,
    )
    .unwrap();

    let error = client
        .current_for_capability(UsageAccountCapability {
            account_id: "account-a".to_owned(),
            surface_id: "claude".to_owned(),
        })
        .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn usage_broker_recovers_stale_guard_with_private_permissions() {
    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let run_dir = secure_run_directory(&config.data_dir).unwrap();
    let leader = run_dir.join(BROKER_LEADER);
    fs::write(&leader, "2147483647\n").unwrap();
    fs::set_permissions(&leader, fs::Permissions::from_mode(0o600)).unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });

    let client = ensure_usage_broker_with_executor(config.clone(), executor).unwrap();
    assert!(connect_probe(&client));
    assert_eq!(fs::metadata(run_dir).unwrap().mode() & 0o777, 0o700);
    assert_eq!(
        fs::metadata(config.socket_path()).unwrap().mode() & 0o777,
        0o600
    );
    assert_eq!(fs::metadata(leader).unwrap().mode() & 0o777, 0o600);
}

#[test]
fn broker_startup_failure_cleans_lease_and_socket_before_returning() {
    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let projection = temp.path().join(BROKER_DIR).join("projection.json");
    fs::create_dir_all(projection.parent().unwrap()).unwrap();
    fs::create_dir(&projection).unwrap();

    let error = ensure_usage_broker_with_executor(
        config.clone(),
        Arc::new(CountingExecutor {
            calls: AtomicUsize::new(0),
        }),
    )
    .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);

    let run_dir = temp.path().join(BROKER_DIR).join(BROKER_RUN_DIR);
    assert!(!run_dir.join(BROKER_LEADER).exists());
    assert!(!config.socket_path().exists());
}

#[test]
fn broker_lease_uses_expiry_and_build_identity_not_pid_reuse() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lease");
    let mut live = BrokerLease::new("build");
    fs::write(&path, serde_json::to_vec(&live).unwrap()).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(
        claim_leader(&path, "build", Duration::from_secs(30))
            .unwrap()
            .is_none()
    );

    live.renewed_at_epoch -= 31;
    fs::write(&path, serde_json::to_vec(&live).unwrap()).unwrap();
    let replacement = claim_leader(&path, "build", Duration::from_secs(30))
        .unwrap()
        .expect("expired lease is reclaimable");
    assert_ne!(replacement.lease.instance_id, live.instance_id);

    fs::write(&path, serde_json::to_vec(&replacement.lease).unwrap()).unwrap();
    assert!(
        claim_leader(&path, "other-build", Duration::from_secs(30))
            .unwrap()
            .is_none()
    );
}

#[test]
fn stale_lease_descriptor_cannot_renew_or_clean_successor_files() {
    let temp = tempfile::tempdir().unwrap();
    let lease_path = temp.path().join("lease");
    let socket_path = temp.path().join("socket");
    let mut stale = claim_leader(&lease_path, "build", Duration::from_secs(30))
        .unwrap()
        .expect("first claimant owns the lease");
    fs::write(&socket_path, b"successor socket").unwrap();

    fs::remove_file(&lease_path).unwrap();
    let successor = claim_leader(&lease_path, "build", Duration::from_secs(30))
        .unwrap()
        .expect("successor owns the replacement lease");
    let successor_id = successor.lease.instance_id.clone();

    assert!(!renew_lease(&mut stale, Duration::from_secs(30)));
    assert!(!cleanup_owned_files(&lease_path, &socket_path, &mut stale,));
    let current: BrokerLease = serde_json::from_slice(&fs::read(&lease_path).unwrap()).unwrap();
    assert_eq!(current.instance_id, successor_id);
    assert!(socket_path.exists());
}

#[test]
fn usage_broker_rejects_symlinked_run_tree_without_mutating_target() {
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("data");
    let target = temp.path().join("target");
    fs::create_dir(&data_dir).unwrap();
    fs::create_dir(&target).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
    symlink(&target, data_dir.join(BROKER_DIR)).unwrap();
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });

    let result =
        ensure_usage_broker_with_executor(UsageBrokerConfig::for_data_dir(data_dir), executor);
    result.unwrap_err();
    assert_eq!(fs::metadata(target).unwrap().mode() & 0o777, 0o755);
}

struct HeldExecutor {
    started: mpsc::SyncSender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl UsageProviderExecutor for HeldExecutor {
    fn probe(&self, _: &UsageAccountCapability, _: u64) -> ProviderProbeOutcome {
        self.started.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        ProviderProbeOutcome::success(quota_view())
    }
}

#[test]
fn saturated_join_waiters_do_not_block_refresh_or_current() {
    let temp = tempfile::tempdir().unwrap();
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(HeldExecutor {
        started: started_tx,
        release: Mutex::new(release_rx),
    });
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let client = ensure_usage_broker_with_executor(config.clone(), executor).unwrap();
    let active = client.refresh(capability(), 0, true).unwrap();
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let mut waiters = Vec::new();
    for _ in 0..BROKER_CONNECTION_WORKERS * 2 {
        let mut stream = UnixStream::connect(config.socket_path()).unwrap();
        let request = UsageBrokerRequest {
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
            build_id: config.build_id.clone(),
            operation: UsageBrokerOperation::Join {
                capability: capability(),
                generation: active.generation,
                timeout_ms: 10_000,
            },
            launch_credential_scope: None,
        };
        let mut bytes = serde_json::to_vec(&request).unwrap();
        bytes.push(b'\n');
        stream.write_all(&bytes).unwrap();
        waiters.push(stream);
    }
    let (response_tx, response_rx) = mpsc::sync_channel(1);
    let control = client.clone();
    let request = thread::spawn(move || {
        let started = Instant::now();
        let short_wait = control.join(capability(), active.generation, Duration::from_millis(1));
        let elapsed = started.elapsed();
        let result = control
            .refresh(capability(), 0, true)
            .and_then(|_| control.current(capability()));
        response_tx.send((short_wait, elapsed, result)).unwrap();
    });
    let response = response_rx.recv_timeout(Duration::from_secs(2));
    // Always release the provider before asserting, so a failed regression
    // cannot strand fixture threads or turn cleanup into another timeout.
    release_tx.send(()).unwrap();
    request.join().unwrap();
    let (short_wait, elapsed, response) =
        response.expect("long polls starved a short wait or control requests");
    assert_eq!(
        short_wait.unwrap_err().kind,
        UsageCoordinationErrorKind::WaitTimeout
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "short join queued behind unrelated long polls"
    );
    let response = response.unwrap();
    assert_eq!(response.generation, active.generation);
    assert!(response.phase.is_active());
    for mut waiter in waiters {
        let response: UsageBrokerResponse = read_frame(&mut waiter).unwrap();
        assert!(
            matches!(response, UsageBrokerResponse::State { state } if state.phase == UsageRefreshPhase::Completed)
        );
    }
}

#[test]
fn stalled_response_reader_does_not_hold_worker_shutdown() {
    let (mut server, client) = UnixStream::pair().unwrap();
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let bytes = vec![b'x'; 8 * 1024 * 1024];
        write_with_deadline(&mut server, &bytes, Duration::from_millis(50));
        done_tx.send(()).unwrap();
    });
    let finished = done_rx.recv_timeout(Duration::from_secs(1));
    // Even the failing implementation can be joined once the peer closes.
    drop(client);
    worker.join().unwrap();
    assert!(
        finished.is_ok(),
        "stalled reader prevented bounded worker shutdown"
    );
}

#[test]
fn subscribe_all_dedups_reuses_fresh_and_forces_only_on_demand() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete_executor = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete_executor;
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        broker_executor,
    )
    .unwrap();

    // Due-on-open with a duplicated capability issues one request per account.
    let opened = client.subscribe_all([capability(), second_capability(), capability()]);
    assert_eq!(opened.len(), 2);
    assert!(opened.iter().all(|(_, result)| result.is_ok()));
    assert_eq!(
        client.subscriptions(),
        vec![capability(), second_capability()]
    );
    for (_, result) in &opened {
        let view = result.as_ref().unwrap();
        client
            .join(
                view.capability.clone(),
                view.generation,
                Duration::from_secs(5),
            )
            .unwrap();
    }
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);

    // Still-fresh observations are reused; nothing new is forced.
    let reopened = client.subscribe_all([capability(), second_capability()]);
    assert!(reopened.iter().all(|(_, result)| result.is_ok()));
    let heartbeat = client.refresh_due(false);
    assert_eq!(heartbeat.len(), 2);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);

    // An explicit operator refresh bypasses the success cooldown exactly once.
    let forced = client.refresh_due(true);
    assert!(forced.iter().all(|(_, result)| result.is_ok()));
    for (_, result) in &forced {
        let view = result.as_ref().unwrap();
        assert_eq!(view.generation, 2);
        client
            .join(
                view.capability.clone(),
                view.generation,
                Duration::from_secs(5),
            )
            .unwrap();
    }
    assert_eq!(executor.calls.load(Ordering::SeqCst), 4);
}

#[test]
fn unsubscribe_releases_local_interest_without_cancelling_shared_work() {
    let temp = tempfile::tempdir().unwrap();
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(HeldExecutor {
        started: started_tx,
        release: Mutex::new(release_rx),
    });
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        executor,
    )
    .unwrap();

    let opened = client.subscribe(capability()).unwrap();
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();

    // Prompt unsubscribe performs no broker I/O and leaves the broker-owned
    // generation untouched.
    assert!(client.unsubscribe(&capability()));
    assert!(!client.unsubscribe(&capability()));
    assert!(client.subscriptions().is_empty());
    let active = client.current(capability()).unwrap();
    assert_eq!(active.generation, opened.generation);
    assert!(active.phase.is_active());

    // Another client awaiting the same generation still observes terminal.
    release_tx.send(()).unwrap();
    let waiter = client.clone();
    let terminal = waiter
        .join(capability(), opened.generation, Duration::from_secs(5))
        .unwrap();
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
    assert!(terminal.snapshot.is_some());
}

#[test]
fn client_clone_forks_subscription_set() {
    let temp = tempfile::tempdir().unwrap();
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        executor,
    )
    .unwrap();
    client.subscribe(capability()).unwrap();
    let fork = client.clone();

    assert!(fork.unsubscribe(&capability()));
    assert_eq!(fork.subscriptions(), Vec::new());
    assert_eq!(client.subscriptions(), vec![capability()]);
    assert_eq!(client.observed_generation(&capability()), Some(1));

    client.unsubscribe_all();
    assert!(client.subscriptions().is_empty());
}

struct StallOneExecutor {
    slow: UsageAccountCapability,
    release: Mutex<mpsc::Receiver<()>>,
}

impl UsageProviderExecutor for StallOneExecutor {
    fn probe(&self, capability: &UsageAccountCapability, _generation: u64) -> ProviderProbeOutcome {
        if *capability == self.slow {
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(15))
                .unwrap();
        }
        ProviderProbeOutcome::success(quota_view())
    }
}

#[test]
fn healthy_accounts_publish_while_one_account_stalls() {
    let temp = tempfile::tempdir().unwrap();
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(StallOneExecutor {
        slow: second_capability(),
        release: Mutex::new(release_rx),
    });
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        executor,
    )
    .unwrap();

    let before = client.current_projection().unwrap();
    let opened = client.subscribe_all([capability(), second_capability()]);
    assert!(opened.iter().all(|(_, result)| result.is_ok()));
    let fast = opened
        .iter()
        .find(|(item, _)| *item == capability())
        .unwrap()
        .1
        .as_ref()
        .unwrap()
        .clone();
    client
        .join(capability(), fast.generation, Duration::from_secs(5))
        .unwrap();

    // The healthy account is published with data while the stalled account
    // keeps its refreshing state; the catalog revision never changes.
    let partial = client.current_projection().unwrap();
    partial.validate().unwrap();
    assert_eq!(partial.discovery_revision, before.discovery_revision);
    assert!(partial.broker_generation > before.broker_generation);
    assert_eq!(
        partial.refresh_state,
        UsageProjectionRefreshStateV1::Refreshing
    );
    let providers = partial
        .providers
        .iter()
        .map(|provider| provider.provider_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(providers, vec!["claude", "codex"]);
    let fast_account = partial.providers[0]
        .accounts
        .iter()
        .find(|account| account.canonical_account_id == "abc123")
        .unwrap();
    assert_eq!(fast_account.freshness.phase, UsageFreshnessPhaseV1::Current);
    assert!(!fast_account.windows.is_empty());
    let slow_account = partial.providers[1]
        .accounts
        .iter()
        .find(|account| account.canonical_account_id == "def456")
        .unwrap();
    assert_eq!(
        slow_account.freshness.phase,
        UsageFreshnessPhaseV1::Refreshing
    );
    assert!(slow_account.windows.is_empty());

    release_tx.send(()).unwrap();
    let slow = opened
        .iter()
        .find(|(item, _)| *item == second_capability())
        .unwrap()
        .1
        .as_ref()
        .unwrap()
        .clone();
    client
        .join(second_capability(), slow.generation, Duration::from_secs(5))
        .unwrap();
    let settled = client.current_projection().unwrap();
    settled.validate().unwrap();
    assert_eq!(settled.discovery_revision, before.discovery_revision);
    assert!(settled.broker_generation > partial.broker_generation);
    assert_eq!(settled.refresh_state, UsageProjectionRefreshStateV1::Idle);
    assert!(
        settled
            .providers
            .iter()
            .flat_map(|provider| &provider.accounts)
            .all(|account| account.freshness.phase == UsageFreshnessPhaseV1::Current)
    );
}

#[test]
fn projection_refresh_runs_due_checks_and_join_settles() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete_executor = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete_executor;
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        broker_executor,
    )
    .unwrap();
    client.subscribe(capability()).unwrap();
    client
        .join(capability(), 1, Duration::from_secs(5))
        .unwrap();
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    // A non-forced projection refresh reuses the still-fresh observation.
    let reused = client.request_refresh(None, false).unwrap();
    reused.validate().unwrap();
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert!(
        reused
            .providers
            .iter()
            .flat_map(|provider| &provider.accounts)
            .any(|account| account.canonical_account_id == "abc123")
    );

    // A forced projection refresh starts one new generation and the join
    // observes it settle without cancelling broker ownership.
    //
    // Join returns a superseding publication immediately by design, and
    // every intermediate publish mints a fresh publication id, so a single
    // join can observe a still-Refreshing snapshot under load. Chase the
    // chain until Idle or the deadline, like any correct caller must.
    let refreshing = client.request_refresh(None, true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut target = refreshing.projection_id.clone();
    let settled = loop {
        let observed = client
            .join_publication(target.clone(), Duration::from_secs(5))
            .unwrap();
        if observed.refresh_state == UsageProjectionRefreshStateV1::Idle
            || Instant::now() >= deadline
        {
            break observed;
        }
        target = observed.projection_id.clone();
    };
    assert_eq!(settled.refresh_state, UsageProjectionRefreshStateV1::Idle);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);

    // A superseded or unknown publication id returns the latest publication.
    let latest = client
        .join_publication("usage-broker:unknown".to_owned(), Duration::from_secs(5))
        .unwrap();
    assert_eq!(latest.projection_id, settled.projection_id);
}

#[test]
fn join_publication_timeout_leaves_broker_ownership_intact() {
    let temp = tempfile::tempdir().unwrap();
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(HeldExecutor {
        started: started_tx,
        release: Mutex::new(release_rx),
    });
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        executor,
    )
    .unwrap();
    client.subscribe(capability()).unwrap();
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    // Wait for a quiesced refreshing publication: once the id is stable
    // across a ticker interval, no publish can interleave with the join below
    // until the probe is released.
    let refreshing = loop {
        let first = client.current_projection().unwrap();
        thread::park_timeout(Duration::from_millis(250));
        let second = client.current_projection().unwrap();
        if first.projection_id == second.projection_id
            && second.refresh_state == UsageProjectionRefreshStateV1::Refreshing
        {
            break second;
        }
    };

    let error = client
        .join_publication(refreshing.projection_id.clone(), Duration::from_millis(50))
        .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::WaitTimeout);

    // The timed-out join cancelled nothing: releasing the probe still settles
    // the same account generation into a newer publication.
    release_tx.send(()).unwrap();
    let settled = client
        .join_publication(refreshing.projection_id, Duration::from_secs(5))
        .unwrap();
    assert_eq!(settled.refresh_state, UsageProjectionRefreshStateV1::Idle);
    assert!(settled.broker_generation > refreshing.broker_generation);
}

#[test]
fn probe_budget_returns_fast_and_expires_without_waiting() {
    let fast = probe::run_probe_with_budget(Duration::from_secs(5), || 7_u32).unwrap();
    assert_eq!(fast, 7);

    let started = Instant::now();
    let expired = probe::run_probe_with_budget(Duration::from_millis(20), || {
        thread::park_timeout(Duration::from_secs(30));
        7_u32
    });
    assert_eq!(expired, Err(probe::ProbeBudgetExpired));
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "budget expiry waited for the probe"
    );

    let timeout = probe::probe_timeout_outcome();
    let ProviderProbeOutcome::Failure {
        kind,
        message,
        retry_at_epoch,
    } = timeout
    else {
        panic!("budget expiry must report failure, never empty success");
    };
    assert_eq!(kind, UsageCoordinationErrorKind::ProviderTimeout);
    assert!(!message.is_empty());
    assert_eq!(retry_at_epoch, None);
}

#[test]
fn probe_budget_propagates_worker_panic_to_coordinator_classification() {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        probe::run_probe_with_budget(Duration::from_secs(5), || {
            panic!("adapter panic must reach the coordinator")
        })
    }));
    assert!(
        outcome.is_err(),
        "worker panic must propagate to the caller"
    );
}

fn scripted_discovery(
    generation: Option<&str>,
    members: &[(&str, crate::host::HostSurfaceId)],
) -> ValidatedUsageDiscovery {
    use crate::host::{CanonicalAccountIdentity, CanonicalAccountSubject};

    ValidatedUsageDiscovery {
        config_generation: generation.map(str::to_owned),
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: members
            .iter()
            .enumerate()
            .map(|(index, (label, surface))| ValidatedCredentialBinding {
                surface: *surface,
                identity: Some(CanonicalAccountIdentity {
                    surface: *surface,
                    subject: CanonicalAccountSubject::ProviderStableHandle((*label).to_owned()),
                }),
                source_id: format!("source-{index}"),
                capability_id: format!("capability-{index}-{label}"),
                credential_revision: format!("credential-revision-{index}-{label}"),
                provenance: BTreeSet::from(["workspace sample role test".to_owned()]),
                source: ValidatedCredentialSource::Capability,
            })
            .collect(),
    }
}

fn activation_scope(temp: &tempfile::TempDir) -> UsageDiscoveryScope {
    UsageDiscoveryScope::HostDesktop {
        config_root: temp.path().join("config"),
        operator_home: temp.path().join("home"),
    }
}

fn counting_broker(data_dir: &Path) -> UsageBrokerClient {
    ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(data_dir.to_owned()),
        Arc::new(CountingExecutor {
            calls: AtomicUsize::new(0),
        }),
    )
    .unwrap()
}

#[test]
fn slow_activator_stale_caller_catalog_never_wins() {
    use crate::host::HostSurfaceId;

    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let client = counting_broker(temp.path());

    // The newer winner already published the current truth.
    let fresh = scripted_discovery(
        Some("generation-fresh"),
        &[("fresh", HostSurfaceId::Claude)],
    );
    let fresh_entries = usage_catalog_entries(&fresh);
    let winner = client
        .reconcile_catalog("generation-fresh".to_owned(), fresh_entries)
        .unwrap();
    let stale = scripted_discovery(Some("generation-stale"), &[("stale", HostSurfaceId::Codex)]);
    let stale_capability =
        capability_for_binding(&stale.bindings[0], stale.config_generation.as_deref());

    // A slow activator arrives holding a stale caller-side catalog, but its
    // post-lease scan observes the same current truth as the winner. The
    // ordering is driven explicitly through the seams: no timing involved.
    let published = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut discover =
        || -> Result<ValidatedUsageDiscovery, UsageCoordinationError> { Ok(fresh.clone()) };
    let mut reconcile = {
        let published = Arc::clone(&published);
        move |client: &UsageBrokerClient,
              expected_projection_id: Option<String>,
              catalog_revision: String,
              entries: Vec<UsageCatalogEntry>| {
            published.lock().unwrap().push(catalog_revision.clone());
            client.reconcile_catalog_if_projection(
                expected_projection_id,
                catalog_revision,
                entries,
            )
        }
    };
    let handle = ensure_usage_broker_with_hooks(
        &config,
        &activation_scope(&temp),
        stale,
        &mut discover,
        &mut reconcile,
    )
    .unwrap();

    let published = published.lock().unwrap();
    assert!(
        !published.is_empty()
            && published
                .iter()
                .all(|revision| revision == "generation-fresh"),
        "stale caller catalog must never be published: {published:?}"
    );
    let final_projection = client.current_projection().unwrap();
    assert_eq!(final_projection.discovery_revision, "generation-fresh");
    assert_eq!(
        final_projection.broker_instance_id,
        winner.broker_instance_id
    );
    assert_eq!(handle.catalog_lease, final_projection.projection_id);
    assert_eq!(handle.capabilities, usage_broker_capabilities(&fresh));
    assert_eq!(
        client.current(stale_capability).unwrap_err().kind,
        UsageCoordinationErrorKind::CatalogRevoked
    );
    assert!(
        temp.path()
            .join("usage-broker")
            .join("activate.lock")
            .exists(),
        "activation must hold the inter-process lock file"
    );
}

#[test]
fn catalog_conflict_retries_with_rediscovery_then_succeeds() {
    use crate::host::HostSurfaceId;

    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let client = counting_broker(temp.path());

    let fresh = scripted_discovery(
        Some("generation-fresh"),
        &[("fresh", HostSurfaceId::Claude)],
    );
    let winner_entries = usage_catalog_entries(&scripted_discovery(
        Some("generation-winner"),
        &[("winner", HostSurfaceId::Codex)],
    ));
    let scans = Arc::new(AtomicUsize::new(0));
    let reconciles = Arc::new(AtomicUsize::new(0));
    let mut discover = {
        let scans = Arc::clone(&scans);
        let fresh = fresh.clone();
        move || -> Result<ValidatedUsageDiscovery, UsageCoordinationError> {
            scans.fetch_add(1, Ordering::SeqCst);
            Ok(fresh.clone())
        }
    };
    let mut reconcile = {
        let reconciles = Arc::clone(&reconciles);
        move |client: &UsageBrokerClient,
              expected_projection_id: Option<String>,
              catalog_revision: String,
              entries: Vec<UsageCatalogEntry>| {
            // Deterministic interleaving: a winner commits between our lease
            // read and our first reconcile, so the first CAS attempt fails.
            if reconciles.fetch_add(1, Ordering::SeqCst) == 0 {
                client
                    .reconcile_catalog("generation-winner".to_owned(), winner_entries.clone())
                    .unwrap();
            }
            client.reconcile_catalog_if_projection(
                expected_projection_id,
                catalog_revision,
                entries,
            )
        }
    };
    let handle = ensure_usage_broker_with_hooks(
        &config,
        &activation_scope(&temp),
        scripted_discovery(Some("generation-stale"), &[("stale", HostSurfaceId::Amp)]),
        &mut discover,
        &mut reconcile,
    )
    .unwrap();

    assert_eq!(reconciles.load(Ordering::SeqCst), 2);
    assert_eq!(
        scans.load(Ordering::SeqCst),
        2,
        "every conflict retry must re-discover, not reuse the losing scan"
    );
    let final_projection = client.current_projection().unwrap();
    assert_eq!(final_projection.discovery_revision, "generation-fresh");
    assert_eq!(handle.catalog_lease, final_projection.projection_id);
}

#[test]
fn catalog_conflict_fails_closed_after_bounded_retries() {
    use crate::host::HostSurfaceId;

    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let _client = counting_broker(temp.path());

    let fresh = scripted_discovery(
        Some("generation-fresh"),
        &[("fresh", HostSurfaceId::Claude)],
    );
    let scans = Arc::new(AtomicUsize::new(0));
    let reconciles = Arc::new(AtomicUsize::new(0));
    let mut discover = {
        let scans = Arc::clone(&scans);
        move || -> Result<ValidatedUsageDiscovery, UsageCoordinationError> {
            scans.fetch_add(1, Ordering::SeqCst);
            Ok(fresh.clone())
        }
    };
    let mut reconcile = {
        let reconciles = Arc::clone(&reconciles);
        move |_client: &UsageBrokerClient,
              _expected_projection_id: Option<String>,
              _catalog_revision: String,
              _entries: Vec<UsageCatalogEntry>|
              -> Result<UsageProjectionV1, UsageCoordinationError> {
            reconciles.fetch_add(1, Ordering::SeqCst);
            Err(UsageCoordinationError {
                kind: UsageCoordinationErrorKind::CatalogRevisionConflict,
                message: "scripted conflict".to_owned(),
            })
        }
    };
    let error = ensure_usage_broker_with_hooks(
        &config,
        &activation_scope(&temp),
        scripted_discovery(None, &[]),
        &mut discover,
        &mut reconcile,
    )
    .unwrap_err();

    assert_eq!(
        error.kind,
        UsageCoordinationErrorKind::CatalogRevisionConflict
    );
    assert_eq!(
        reconciles.load(Ordering::SeqCst),
        BROKER_ACTIVATION_ATTEMPTS as usize
    );
    assert_eq!(
        scans.load(Ordering::SeqCst),
        BROKER_ACTIVATION_ATTEMPTS as usize,
        "every attempt must run its own post-lease discovery scan"
    );
}

#[test]
fn transient_empty_scan_does_not_wipe_live_catalog() {
    use crate::host::HostSurfaceId;

    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let client = counting_broker(temp.path());

    let good = scripted_discovery(Some("generation-good"), &[("good", HostSurfaceId::Claude)]);
    let good_capability =
        capability_for_binding(&good.bindings[0], good.config_generation.as_deref());
    client
        .reconcile_catalog("generation-good".to_owned(), usage_catalog_entries(&good))
        .unwrap();

    // First scan observes a transient empty catalog; the confirmation scan
    // re-observes the live catalog. Pops resolve in scan order.
    let scripted = Arc::new(Mutex::new(vec![
        good.clone(),
        scripted_discovery(None, &[]),
    ]));
    let mut discover = {
        let scripted = Arc::clone(&scripted);
        move || -> Result<ValidatedUsageDiscovery, UsageCoordinationError> {
            Ok(scripted.lock().unwrap().pop().unwrap())
        }
    };
    let published_sizes = Arc::new(Mutex::new(Vec::<usize>::new()));
    let mut reconcile = {
        let published_sizes = Arc::clone(&published_sizes);
        move |client: &UsageBrokerClient,
              expected_projection_id: Option<String>,
              catalog_revision: String,
              entries: Vec<UsageCatalogEntry>| {
            published_sizes.lock().unwrap().push(entries.len());
            client.reconcile_catalog_if_projection(
                expected_projection_id,
                catalog_revision,
                entries,
            )
        }
    };
    let handle = ensure_usage_broker_with_hooks(
        &config,
        &activation_scope(&temp),
        good.clone(),
        &mut discover,
        &mut reconcile,
    )
    .unwrap();

    let published_sizes = published_sizes.lock().unwrap();
    assert_eq!(
        published_sizes.as_slice(),
        &[1],
        "transient empty scan must yield to the confirmation scan, never publish: {published_sizes:?}"
    );
    let final_projection = client.current_projection().unwrap();
    assert_eq!(final_projection.discovery_revision, "generation-good");
    assert_eq!(handle.catalog_lease, final_projection.projection_id);
    client.current(good_capability).unwrap();
}

#[test]
fn confirmed_empty_scan_still_revokes_live_catalog() {
    use crate::host::HostSurfaceId;

    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let client = counting_broker(temp.path());

    let good = scripted_discovery(Some("generation-good"), &[("good", HostSurfaceId::Claude)]);
    let good_capability =
        capability_for_binding(&good.bindings[0], good.config_generation.as_deref());
    client
        .reconcile_catalog("generation-good".to_owned(), usage_catalog_entries(&good))
        .unwrap();

    // Two consecutive empty scans confirm a genuine removal: the revocation
    // must still publish.
    let mut discover = || -> Result<ValidatedUsageDiscovery, UsageCoordinationError> {
        Ok(scripted_discovery(None, &[]))
    };
    let mut reconcile = |client: &UsageBrokerClient,
                         expected_projection_id: Option<String>,
                         catalog_revision: String,
                         entries: Vec<UsageCatalogEntry>| {
        client.reconcile_catalog_if_projection(expected_projection_id, catalog_revision, entries)
    };
    let handle = ensure_usage_broker_with_hooks(
        &config,
        &activation_scope(&temp),
        good,
        &mut discover,
        &mut reconcile,
    )
    .unwrap();

    let final_projection = client.current_projection().unwrap();
    assert_eq!(final_projection.discovery_revision, "empty");
    assert_eq!(handle.catalog_lease, final_projection.projection_id);
    assert!(handle.capabilities.is_empty());
    assert_eq!(
        client.current(good_capability).unwrap_err().kind,
        UsageCoordinationErrorKind::CatalogRevoked
    );
}

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
fn ensure_usage_broker_publishes_fresh_discovery_not_stale_caller_input() {
    use crate::host::HostSurfaceId;

    let data_dir = tempfile::tempdir().unwrap();
    let config_root = tempfile::tempdir().unwrap();
    let operator_home = tempfile::tempdir().unwrap();
    let scope = UsageDiscoveryScope::HostDesktop {
        config_root: config_root.path().to_owned(),
        operator_home: operator_home.path().to_owned(),
    };
    let resolver: Arc<dyn ProviderCredentialEnvResolver> = Arc::new(NoEnvResolver);
    // Broker already serving (as after any prior activation).
    let _running = counting_broker(data_dir.path());

    // Stale caller generation: one admitted account at a caller-side
    // revision, simulating staged desktop discovery that predates the
    // current tree (the tree here is empty).
    let stale = scripted_discovery(Some("stale-caller-rev"), &[("stale", HostSurfaceId::Amp)]);
    let handle = ensure_usage_broker(
        UsageBrokerConfig::for_data_dir(data_dir.path().to_owned()),
        scope.clone(),
        stale,
        Arc::clone(&resolver),
    )
    .unwrap();

    // The published catalog derives from post-lease discovery (the empty
    // tree here), never the stale caller revision ...
    let fresh = validate_usage_sources(
        discover_usage_sources(&scope, resolver.as_ref()).unwrap(),
        resolver.as_ref(),
    );
    let expected_revision = fresh
        .config_generation
        .clone()
        .unwrap_or_else(|| "empty".to_owned());
    assert_ne!(expected_revision, "stale-caller-rev");
    let projection = handle.client.current_projection().unwrap();
    assert_eq!(projection.discovery_revision, expected_revision);
    assert_eq!(handle.catalog_lease, projection.projection_id);
    // ... and the returned handle matches the published generation, not the
    // caller's admitted set.
    assert_eq!(handle.capabilities, usage_broker_capabilities(&fresh));
}

#[test]
fn sequential_reconcile_after_fresh_read_still_accepts_last_writer() {
    // Documents the broker-level contract the activation ordering above
    // defends: the projection fence rejects CONCURRENT stale writers (see
    // `catalog_cas_rejects_a_stale_rotation_after_a_newer_winner`), but a
    // stale writer that reads AFTER the fresh publication still passes the
    // fence. That is why `ensure_usage_broker` must publish post-lease
    // discovery resolved under the activation lock rather than trusting
    // caller input of any age.
    let temp = tempfile::tempdir().unwrap();
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        Arc::new(CountingExecutor {
            calls: AtomicUsize::new(0),
        }),
    )
    .unwrap();
    let fresh = UsageCatalogEntry {
        capability: capability(),
        revision: "entry-fresh".to_owned(),
    };
    let stale = UsageCatalogEntry {
        capability: second_capability(),
        revision: "entry-stale".to_owned(),
    };

    let first = client.current_projection().unwrap().projection_id;
    let winner = client
        .reconcile_catalog_if_projection(Some(first), "catalog-fresh".to_owned(), vec![fresh])
        .unwrap();
    // Stale writer reads the fresh publication, then overwrites with older
    // data: the fence passes because the read was current.
    let read_after_fresh = client.current_projection().unwrap().projection_id;
    assert_eq!(read_after_fresh, winner.projection_id);
    let overwritten = client
        .reconcile_catalog_if_projection(
            Some(read_after_fresh),
            "catalog-stale".to_owned(),
            vec![stale],
        )
        .unwrap();
    assert_eq!(overwritten.discovery_revision, "catalog-stale");
}
