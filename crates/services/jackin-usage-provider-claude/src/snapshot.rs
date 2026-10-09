// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Claude API-key route: API keys cannot poll the OAuth usage endpoint.

use jackin_protocol::control::{
    FocusedUsageView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use jackin_usage_provider_core::{UsageSurface, UsageViewInput, bucket, usage_view};

/// Claude API keys do not authenticate the OAuth quota endpoint. Keep this
/// route explicit and unsupported rather than feeding an API key into the
/// OAuth adapter and reporting a misleading login/error state.
pub fn claude_api_key_snapshot(
    agent: &str,
    provider: Option<&str>,
    key_name: &str,
    secret: &str,
    now: i64,
) -> FocusedUsageView {
    let has_secret = !secret.trim().is_empty();
    let status = if has_secret {
        UsageSnapshotStatus::Unsupported
    } else {
        UsageSnapshotStatus::NeedsSecret
    };
    let message = if has_secret {
        "Claude API-key quota is unavailable; OAuth usage requires CLAUDE_CODE_OAUTH_TOKEN"
    } else {
        "Claude API key is missing"
    };
    usage_view(UsageViewInput {
        agent,
        provider: provider.or(Some("Claude")),
        surface: UsageSurface::Claude,
        account_label: "Claude API key".to_owned(),
        username: None,
        plan_label: None,
        credential_origin: Some(format!("API key · env {key_name}")),
        buckets: vec![bucket(
            "Usage",
            None,
            None,
            None,
            None,
            Some(message),
            status,
        )],
        status,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        now,
        last_error: Some(message.to_owned()),
    })
}
