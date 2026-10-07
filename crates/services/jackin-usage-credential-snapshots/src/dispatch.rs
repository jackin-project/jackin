// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Configured-provider credential snapshots (vendor dispatch, see `lib.rs`).

use jackin_protocol::control::{
    FocusedUsageView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use jackin_usage_provider_core::{
    ProviderRateLimit, UsageSurface, UsageViewInput, now_epoch, unsupported_snapshot, usage_view,
};

/// Per-vendor snapshot arms for [`provider_credential_snapshot_with_rate_limit`].
/// Implemented once by the `jackin-usage` coordinator, which alone may name
/// the T3 vendor crates. Each arm receives only the routing inputs plus the
/// caller-retained secret, and returns a view that never embeds the secret.
pub trait CredentialSnapshotVendors {
    /// `claude` arm with an OAuth token: wave view over the resolved grant.
    fn claude_oauth_wave_view(
        &self,
        secret: &str,
        now: i64,
    ) -> (FocusedUsageView, Option<ProviderRateLimit>);
    /// `claude` arm with an API key: key snapshot view.
    fn claude_api_key_view(&self, key_name: &str, secret: &str, now: i64) -> FocusedUsageView;
    /// `openrouter` arm: key snapshot with rate limit.
    fn openrouter_key_view(
        &self,
        secret: &str,
        now: i64,
    ) -> (FocusedUsageView, Option<ProviderRateLimit>);
    /// `amp` arm: API-key snapshot view.
    fn amp_key_view(&self, secret: &str, now: i64) -> FocusedUsageView;
    /// `zai` arm: provider-key snapshot view.
    fn zai_key_view(&self, key_name: &str, secret: &str, now: i64) -> FocusedUsageView;
    /// `kimi` arm: key snapshot view.
    fn kimi_key_view(&self, secret: &str, now: i64) -> FocusedUsageView;
    /// `minimax` arm: key snapshot view.
    fn minimax_key_view(&self, secret: &str, now: i64) -> FocusedUsageView;
    /// `grok` arm: key snapshot view (billing needs an authenticated profile).
    fn grok_key_view(&self, key_name: &str, now: i64) -> FocusedUsageView;
    /// `google` arm: Gemini presence snapshot view.
    fn gemini_key_view(&self, key_name: &str, now: i64) -> FocusedUsageView;
}

/// Build one explicit configured-provider snapshot while the caller retains the
/// credential. This is the tier-3 probe body used by tier-4 protected-source
/// adapters; the secret is never returned or persisted.
#[must_use]
pub fn provider_credential_snapshot<V: CredentialSnapshotVendors>(
    surface_id: &str,
    key_name: &str,
    secret: &str,
    vendors: &V,
) -> FocusedUsageView {
    provider_credential_snapshot_with_rate_limit(surface_id, key_name, secret, vendors).0
}

pub fn provider_credential_snapshot_with_rate_limit<V: CredentialSnapshotVendors>(
    surface_id: &str,
    key_name: &str,
    secret: &str,
    vendors: &V,
) -> (FocusedUsageView, Option<ProviderRateLimit>) {
    let now = now_epoch();
    if surface_id == "claude" {
        if key_name == jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME {
            return vendors.claude_oauth_wave_view(secret, now);
        }
        return (vendors.claude_api_key_view(key_name, secret, now), None);
    }
    if surface_id == "openrouter" {
        return vendors.openrouter_key_view(secret, now);
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
        // A Cursor API key cannot drive the personal dashboard RPC (that
        // needs the OAuth JWT from `auth.json`); the key route stays an
        // explicit gap like the OpenAI arm above, never a speculative fetch.
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
        // Explicitly blocked (no production dispatch): `meta` (Muse has no
        // pollable usage fetch by design), `omp`/`hermes`
        // (attribution-only adapters with no native endpoint), `copilot` (no
        // collector, registry, or discovery entry exists at all).
        _ => unsupported_snapshot(surface_id, None, now),
    };
    (view, None)
}
