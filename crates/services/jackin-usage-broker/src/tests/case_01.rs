// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::path::PathBuf;

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
            dispatch_key: "AMP_API_KEY".to_owned(),
            launch_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
            material: Some(staged.clone()),
        },
    };
    let executor = DiscoveryProviderExecutor {
        bindings: Mutex::new(BTreeMap::from([(capability.clone(), vec![binding])])),
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
        .first_mut()
        .unwrap()
        .source = ValidatedCredentialSource::Env {
        handle: OpaqueCredentialHandle::new("handle-a-rotated"),
        key: "AMP_API_KEY".to_owned(),
        dispatch_key: "AMP_API_KEY".to_owned(),
        launch_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
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
        .first_mut()
        .unwrap()
        .source = ValidatedCredentialSource::Env {
        handle: OpaqueCredentialHandle::new("handle-b-repointed"),
        key: "AMP_API_KEY".to_owned(),
        dispatch_key: "AMP_API_KEY".to_owned(),
        launch_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
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
            vec![ValidatedCredentialBinding {
                surface: HostSurfaceId::Zai,
                identity: None,
                source_id: "source-zai".to_owned(),
                capability_id: "capability-zai".to_owned(),
                credential_revision: "credential-revision-zai".to_owned(),
                provenance: BTreeSet::from(["account zhipu-account".to_owned()]),
                source: ValidatedCredentialSource::Env {
                    handle: OpaqueCredentialHandle::new("handle-zai"),
                    key: "ZAI_API_KEY".to_owned(),
                    dispatch_key: "ZAI_API_KEY".to_owned(),
                    launch_keys: BTreeSet::from(["ZHIPU_API_KEY".to_owned()]),
                    material: Some(staged.clone()),
                },
            }],
        )])),
        validated_catalog: Mutex::new(None),
        scope: UsageDiscoveryScope::HostDesktop {
            config_root: PathBuf::new(),
            operator_home: PathBuf::new(),
        },
        resolver: Arc::new(NoopCredentialResolver),
        probe_budget: Duration::from_secs(1),
    };

    for key in ["ZAI_API_KEY", "ZHIPU_API_KEY", "Z_AI_API_KEY"] {
        let scope = env_scope("zhipu-account", "zai", key, &staged);
        executor
            .authorize_credential_scope(&capability, &scope)
            .expect("Z.AI alias with exact source material should authorize");
    }
    let wrong_material = env_material("ZAI_HOST_SECRET", "different-secret");
    let rejected = env_scope("zhipu-account", "zai", "Z_AI_API_KEY", &wrong_material);
    assert!(
        executor
            .authorize_credential_scope(&capability, &rejected)
            .is_err()
    );
}

#[test]
fn discovery_provider_error_text_cannot_set_rate_limit_or_retry_deadline() {
    for text in [
        "provider HTTP 401 Unauthorized",
        "provider HTTP 403 Forbidden",
        "provider HTTP 429; Retry-After: 97",
        "transport failed while contacting port 429",
    ] {
        let mut view = quota_view();
        view.status = UsageSnapshotStatus::Stale;
        view.last_error = Some(text.to_owned());

        let ProviderProbeOutcome::Failure {
            kind,
            message,
            retry_at_epoch,
        } = provider_probe_outcome(view)
        else {
            panic!("provider view must not publish as success");
        };
        assert_eq!(kind, UsageCoordinationErrorKind::ProviderUnavailable);
        assert_eq!(message, text);
        assert_eq!(retry_at_epoch, None);
    }
}

#[test]
fn discovery_typed_rate_limit_reaches_broker_without_text_parsing() {
    let mut view = quota_view();
    view.status = UsageSnapshotStatus::Stale;
    view.last_error = Some("transport message mentions HTTP 429".to_owned());

    let ProviderProbeOutcome::Failure {
        kind,
        message,
        retry_at_epoch,
    } = provider_probe_outcome_with_rate_limit(
        view,
        Some(jackin_usage_provider_core::ProviderRateLimit {
            retry_at_epoch: Some(1_700_000_037),
        }),
    )
    else {
        panic!("typed rate limit must be a broker failure");
    };
    assert_eq!(kind, UsageCoordinationErrorKind::RateLimited);
    assert_eq!(message, "usage provider rate limit is active");
    assert_eq!(retry_at_epoch, Some(1_700_000_037));
}

#[test]
fn discovery_typed_provider_failures_keep_auth_timeout_and_transport_kinds() {
    let cases = [
        (
            jackin_usage_provider_core::ProviderFailureMetadata {
                kind: jackin_usage_provider_core::ProviderErrorKind::HttpStatus,
                http_status: Some(401),
            },
            UsageCoordinationErrorKind::NeedsSecret,
        ),
        (
            jackin_usage_provider_core::ProviderFailureMetadata {
                kind: jackin_usage_provider_core::ProviderErrorKind::HttpStatus,
                http_status: Some(403),
            },
            UsageCoordinationErrorKind::Unauthorized,
        ),
        (
            jackin_usage_provider_core::ProviderFailureMetadata {
                kind: jackin_usage_provider_core::ProviderErrorKind::HttpStatus,
                http_status: Some(429),
            },
            UsageCoordinationErrorKind::RateLimited,
        ),
        (
            jackin_usage_provider_core::ProviderFailureMetadata {
                kind: jackin_usage_provider_core::ProviderErrorKind::Timeout,
                http_status: None,
            },
            UsageCoordinationErrorKind::ProviderTimeout,
        ),
        (
            jackin_usage_provider_core::ProviderFailureMetadata {
                kind: jackin_usage_provider_core::ProviderErrorKind::Transport,
                http_status: None,
            },
            UsageCoordinationErrorKind::ProviderUnavailable,
        ),
        (
            jackin_usage_provider_core::ProviderFailureMetadata {
                kind: jackin_usage_provider_core::ProviderErrorKind::Decode,
                http_status: None,
            },
            UsageCoordinationErrorKind::ProviderUnavailable,
        ),
    ];

    for (metadata, expected_kind) in cases {
        let mut view = quota_view();
        view.status = UsageSnapshotStatus::Stale;
        view.last_error = Some("same fixture error text".to_owned());
        let ProviderProbeOutcome::Failure {
            kind,
            retry_at_epoch,
            ..
        } = provider_probe_outcome_with_metadata(view, None, Some(metadata))
        else {
            panic!("typed provider error must remain a broker failure");
        };
        assert_eq!(kind, expected_kind);
        assert_eq!(retry_at_epoch, None);
    }
}

#[test]
fn refresh_binding_outcome_carries_typed_rate_limit_into_broker() {
    let binding = ValidatedCredentialBinding {
        surface: HostSurfaceId::Claude,
        identity: None,
        source_id: "source-typed-rate-limit".to_owned(),
        capability_id: "capability-typed-rate-limit".to_owned(),
        credential_revision: "credential-revision-typed-rate-limit".to_owned(),
        provenance: BTreeSet::new(),
        source: ValidatedCredentialSource::Env {
            handle: OpaqueCredentialHandle::new("typed-rate-limit-handle"),
            key: "CLAUDE_API_KEY".to_owned(),
            dispatch_key: "CLAUDE_API_KEY".to_owned(),
            launch_keys: BTreeSet::from(["CLAUDE_API_KEY".to_owned()]),
            material: Some(env_material("CLAUDE_API_KEY", "fixture-secret")),
        },
    };

    let outcome = refresh_binding_outcome(&binding, &TypedRateLimitResolver);
    let ProviderProbeOutcome::Failure {
        kind,
        retry_at_epoch,
        ..
    } = outcome
    else {
        panic!("typed rate limit must remain a broker failure");
    };
    assert_eq!(kind, UsageCoordinationErrorKind::RateLimited);
    assert_eq!(retry_at_epoch, Some(1_700_000_037));
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
