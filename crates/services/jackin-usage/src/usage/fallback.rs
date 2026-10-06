// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Unpollable and unsupported snapshot fallbacks.

use jackin_protocol::control::{
    FocusedUsageView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};

use super::{UsageSurface, UsageViewInput, bucket, usage_view};

pub(crate) fn opencode_snapshot(agent: &str, provider: Option<&str>, now: i64) -> FocusedUsageView {
    usage_view(UsageViewInput {
        agent,
        provider,
        surface: UsageSurface::OpenCode,
        account_label: "OpenCode account (unresolved)".to_owned(),
        username: None,
        plan_label: None,
        credential_origin: None,
        buckets: vec![bucket(
            "Usage",
            None,
            None,
            None,
            None,
            Some("OpenCode Go credential is unavailable"),
            UsageSnapshotStatus::Unsupported,
        )],
        status: UsageSnapshotStatus::Unsupported,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        now,
        last_error: Some(
            "OpenCode account identity is provisional until the provider exposes a non-secret identifier".to_owned(),
        ),
    })
}

/// Honest snapshot for a locally identified binding with no pollable usage
/// fetch by design (Muse identity, omp/hermes attribution adapters). This is
/// a deliberate no-poll, never a provider outage: `Unsupported` so the broker
/// records it as a data-bearing success, outside retry/backoff paths.
pub(crate) fn unpollable_snapshot(
    agent: &str,
    provider: Option<&str>,
    now: i64,
) -> FocusedUsageView {
    usage_view(UsageViewInput {
        agent,
        provider,
        surface: UsageSurface::Unsupported,
        // Empty until the broker binds the discovery account label; never a
        // fabricated identity.
        account_label: String::new(),
        username: None,
        plan_label: None,
        credential_origin: None,
        buckets: Vec::new(),
        status: UsageSnapshotStatus::Unsupported,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        now,
        last_error: Some("usage polling not supported for this provider".to_owned()),
    })
}

pub(crate) fn unsupported_snapshot(
    agent: &str,
    provider: Option<&str>,
    now: i64,
) -> FocusedUsageView {
    usage_view(UsageViewInput {
        agent,
        provider,
        surface: UsageSurface::Unsupported,
        account_label: "unsupported focused agent".to_owned(),
        username: None,
        plan_label: None,
        credential_origin: None,
        buckets: Vec::new(),
        status: UsageSnapshotStatus::Unsupported,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        now,
        last_error: Some(format!("no usage adapter for agent {agent:?}")),
    })
}
