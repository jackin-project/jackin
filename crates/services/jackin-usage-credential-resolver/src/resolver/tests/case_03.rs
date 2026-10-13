// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn openrouter_credential_snapshot_is_supported_and_scoped_when_missing() {
    let view = provider_credential_snapshot("openrouter", "OPENROUTER_API_KEY", "");

    assert_eq!(view.status, UsageSnapshotStatus::NeedsLogin);
    assert_eq!(view.source, UsageSource::None);
    assert_eq!(view.account.provider_label, "OpenRouter");
    assert_eq!(view.focused_agent.as_deref(), Some("opencode"));
    assert_eq!(view.focused_provider.as_deref(), Some("OpenRouter"));
    assert_ne!(view.status, UsageSnapshotStatus::Unsupported);
    assert_eq!(
        view.last_error.as_deref(),
        Some("OpenRouter API key missing")
    );
}

#[test]
fn credential_snapshot_arms_cover_newly_wired_surfaces() {
    // Cursor API keys cannot drive the personal dashboard: explicit gap.
    let view = provider_credential_snapshot("cursor", "CURSOR_API_KEY", "fixture-key");
    assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
    assert_eq!(view.account.provider_label, "Cursor");
    assert!(
        view.last_error
            .as_deref()
            .is_some_and(|error| error.contains("Cursor API-key"))
    );
    // Google keys route to the real Gemini collector with the key origin.
    let view = provider_credential_snapshot("google", "GEMINI_API_KEY", "fixture-key");
    assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
    assert_eq!(view.account.provider_label, "Google");
    assert_eq!(
        view.account.credential_origin.as_deref(),
        Some("API key · env GEMINI_API_KEY")
    );
    // Blocked surfaces keep the explicit generic fallback.
    let view = provider_credential_snapshot("meta", "META_API_KEY", "fixture-key");
    assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
    assert_eq!(view.account.provider_label, "Usage");
}

#[test]
fn credential_snapshot_openrouter_arm_reads_key_quota() {
    // Live provider read with a fixture key: `/key` rejects it, so the arm
    // must return the collector's honest NeedsLogin/Error view — never the
    // generic "no usage adapter" fallback, which would mean the arm is dead.
    let view = provider_credential_snapshot("openrouter", "OPENROUTER_API_KEY", "fixture-key");
    assert_ne!(view.status, UsageSnapshotStatus::Fresh);
    assert_ne!(view.status, UsageSnapshotStatus::Unsupported);
    assert_eq!(view.account.provider_label, "OpenRouter");
    assert!(view.last_error.is_some());
}

#[test]
fn grok_env_key_only_snapshot_denies_billing_without_rpc() {
    // The configured-source arm never attempts a billing RPC on an inference
    // key: it reports the honest billing gap with zero network.
    for (key, origin) in [
        (
            jackin_core::XAI_API_KEY_ENV_NAME,
            "API token · env XAI_API_KEY",
        ),
        (
            jackin_core::GROK_DEPLOYMENT_KEY_ENV_NAME,
            "API token · env GROK_DEPLOYMENT_KEY",
        ),
    ] {
        let view = provider_credential_snapshot("grok", key, "fixture-key");
        assert_eq!(view.status, UsageSnapshotStatus::Error);
        assert_eq!(view.source, UsageSource::None);
        assert_eq!(
            view.last_error.as_deref(),
            Some("Grok billing requires an authenticated profile")
        );
        assert_eq!(view.account.credential_origin.as_deref(), Some(origin));
    }
}

#[test]
fn openrouter_cache_preserves_exact_capability_and_last_good_rows_on_error() {
    let capability = jackin_protocol::usage_broker::UsageAccountCapability {
        account_id: "account-openrouter".to_owned(),
        surface_id: "openrouter".to_owned(),
    };
    assert!(capability_matches_surface(
        "opencode",
        Some("OpenRouter"),
        &capability
    ));

    let target = UsageRefreshTarget {
        agent: "opencode".to_owned(),
        provider: Some("OpenRouter".to_owned()),
        capability: capability.clone(),
    };
    let mut view = provider_credential_snapshot("openrouter", "OPENROUTER_API_KEY", "");
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;

    let mut cache = UsageCache::default();
    cache.insert_snapshot_for_capability_for_test(
        "opencode",
        Some("OpenRouter"),
        &capability,
        view,
    );
    cache.adopt_broker_error(
        &target,
        &jackin_protocol::usage_broker::UsageCoordinationError {
            kind: jackin_protocol::usage_broker::UsageCoordinationErrorKind::ProviderUnavailable,
            message: "OpenRouter key request failed".to_owned(),
        },
    );

    let adopted = cache.focused_snapshot_for_capability(
        Some("opencode"),
        Some("OpenRouter"),
        Some(&capability),
    );
    assert_eq!(adopted.status, UsageSnapshotStatus::Stale);
    assert_eq!(adopted.account.provider_label, "OpenRouter");
    assert_eq!(adopted.buckets[0].label, "Usage");
    assert_eq!(
        adopted.last_error.as_deref(),
        Some("OpenRouter key request failed")
    );

    let wrong_surface = jackin_protocol::usage_broker::UsageAccountCapability {
        account_id: capability.account_id.clone(),
        surface_id: "opencode".to_owned(),
    };
    assert!(!capability_matches_surface(
        "opencode",
        Some("OpenRouter"),
        &wrong_surface
    ));
    assert_eq!(
        cache
            .focused_snapshot_for_capability(
                Some("opencode"),
                Some("OpenRouter"),
                Some(&wrong_surface),
            )
            .status,
        UsageSnapshotStatus::Unavailable
    );
}
