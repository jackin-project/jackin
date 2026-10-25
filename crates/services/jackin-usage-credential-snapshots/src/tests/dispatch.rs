// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    FocusedUsageView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use jackin_usage_provider_core::{
    ProviderFailureMetadata, ProviderRateLimit, UsageSurface, UsageViewInput, usage_view,
};

use crate::{
    CredentialSnapshotVendors, provider_credential_snapshot,
    provider_credential_snapshot_with_metadata,
};

struct MarkerVendors;

fn marker(agent: &'static str, now: i64) -> FocusedUsageView {
    usage_view(UsageViewInput {
        agent,
        provider: None,
        surface: UsageSurface::Unsupported,
        account_label: format!("{agent}-marker"),
        username: None,
        plan_label: None,
        credential_origin: None,
        buckets: Vec::new(),
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        now,
        last_error: None,
    })
}

impl CredentialSnapshotVendors for MarkerVendors {
    fn claude_oauth_wave_view(
        &self,
        _secret: &str,
        now: i64,
    ) -> (
        FocusedUsageView,
        Option<ProviderRateLimit>,
        Option<ProviderFailureMetadata>,
    ) {
        (marker("claude-wave", now), None, None)
    }

    fn claude_api_key_view(&self, _key_name: &str, _secret: &str, now: i64) -> FocusedUsageView {
        marker("claude-key", now)
    }

    fn openrouter_key_view(
        &self,
        _secret: &str,
        now: i64,
    ) -> (FocusedUsageView, Option<ProviderRateLimit>) {
        (marker("openrouter", now), None)
    }

    fn amp_key_view(&self, _secret: &str, now: i64) -> FocusedUsageView {
        marker("amp", now)
    }

    fn zai_key_view(&self, _key_name: &str, _secret: &str, now: i64) -> FocusedUsageView {
        marker("zai", now)
    }

    fn kimi_key_view(&self, _secret: &str, now: i64) -> FocusedUsageView {
        marker("kimi", now)
    }

    fn minimax_key_view(&self, _secret: &str, now: i64) -> FocusedUsageView {
        marker("minimax", now)
    }

    fn grok_key_view(&self, _key_name: &str, now: i64) -> FocusedUsageView {
        marker("grok", now)
    }

    fn gemini_key_view(&self, _key_name: &str, now: i64) -> FocusedUsageView {
        marker("gemini", now)
    }

    fn opencode_key_view(&self, _secret: &str, now: i64) -> FocusedUsageView {
        marker("opencode", now)
    }
}

#[test]
fn unknown_surface_never_consults_vendor_arms() {
    let view =
        provider_credential_snapshot("no-such-surface", "SOME_KEY", "secret", &MarkerVendors);

    assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
    assert_eq!(view.account.account_label, "unsupported focused agent");
}

#[test]
fn opencode_surface_with_api_key_routes_to_opencode_arm() {
    let view = provider_credential_snapshot(
        "opencode",
        jackin_core::OPENCODE_API_KEY_ENV_NAME,
        "secret",
        &MarkerVendors,
    );

    assert_eq!(view.account.account_label, "opencode-marker");
}

#[test]
fn claude_oauth_token_routes_to_wave_arm() {
    let (view, _, _) = provider_credential_snapshot_with_metadata(
        "claude",
        jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME,
        "secret",
        &MarkerVendors,
    );

    assert_eq!(view.account.account_label, "claude-wave-marker");
}
