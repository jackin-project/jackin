// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    FocusedUsageView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use jackin_usage_provider_core::{
    ProviderFailureMetadata, ProviderRateLimit, UsageSurface, UsageViewInput, now_epoch,
    unsupported_snapshot, usage_view,
};

pub trait CredentialSnapshotVendors {
    fn claude_oauth_wave_view(
        &self,
        secret: &str,
        now: i64,
    ) -> (
        FocusedUsageView,
        Option<ProviderRateLimit>,
        Option<ProviderFailureMetadata>,
    );
    fn claude_api_key_view(&self, key_name: &str, secret: &str, now: i64) -> FocusedUsageView;
    fn openrouter_key_view(
        &self,
        secret: &str,
        now: i64,
    ) -> (FocusedUsageView, Option<ProviderRateLimit>);
    fn amp_key_view(&self, secret: &str, now: i64) -> FocusedUsageView;
    fn zai_key_view(&self, key_name: &str, secret: &str, now: i64) -> FocusedUsageView;
    fn kimi_key_view(&self, secret: &str, now: i64) -> FocusedUsageView;
    fn minimax_key_view(&self, secret: &str, now: i64) -> FocusedUsageView;
    fn grok_key_view(&self, key_name: &str, now: i64) -> FocusedUsageView;
    fn gemini_key_view(&self, key_name: &str, now: i64) -> FocusedUsageView;
}

#[must_use]
pub fn provider_credential_snapshot<V: CredentialSnapshotVendors>(
    surface_id: &str,
    key_name: &str,
    secret: &str,
    vendors: &V,
) -> FocusedUsageView {
    provider_credential_snapshot_with_metadata(surface_id, key_name, secret, vendors).0
}

pub fn provider_credential_snapshot_with_metadata<V: CredentialSnapshotVendors>(
    surface_id: &str,
    key_name: &str,
    secret: &str,
    vendors: &V,
) -> (
    FocusedUsageView,
    Option<ProviderRateLimit>,
    Option<ProviderFailureMetadata>,
) {
    let now = now_epoch();
    if surface_id == "claude" {
        if key_name == jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME {
            return vendors.claude_oauth_wave_view(secret, now);
        }
        return (
            vendors.claude_api_key_view(key_name, secret, now),
            None,
            None,
        );
    }
    if surface_id == "openrouter" {
        let (view, rate_limit) = vendors.openrouter_key_view(secret, now);
        return (view, rate_limit, None);
    }
    let view = match surface_id {
        "amp" => vendors.amp_key_view(secret, now),
        "zai" => vendors.zai_key_view(key_name, secret, now),
        "kimi" => vendors.kimi_key_view(secret, now),
        "minimax" => vendors.minimax_key_view(secret, now),
        "grok" => vendors.grok_key_view(key_name, now),
        "codex" => usage_view(UsageViewInput {
            agent: "codex",
            provider: Some("OpenAI"),
            surface: UsageSurface::Codex,
            account_label: String::new(),
            username: None,
            plan_label: None,
            credential_origin: Some("API key · configured source".to_owned()),
            buckets: Vec::new(),
            status: UsageSnapshotStatus::Unsupported,
            source: UsageSource::None,
            confidence: UsageConfidence::None,
            now,
            last_error: Some("OpenAI API-key subscription quota is unavailable".to_owned()),
        }),
        "cursor" => usage_view(UsageViewInput {
            agent: "cursor",
            provider: Some("Cursor"),
            surface: UsageSurface::Cursor,
            account_label: String::new(),
            username: None,
            plan_label: None,
            credential_origin: Some("API key · configured source".to_owned()),
            buckets: Vec::new(),
            status: UsageSnapshotStatus::Unsupported,
            source: UsageSource::None,
            confidence: UsageConfidence::None,
            now,
            last_error: Some("Cursor API-key subscription quota is unavailable".to_owned()),
        }),
        "google" => vendors.gemini_key_view(key_name, now),
        _ => unsupported_snapshot(surface_id, None, now),
    };
    (view, None, None)
}
