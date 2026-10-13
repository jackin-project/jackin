// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Antigravity` snapshot entry points and status.

use super::super::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSnapshotStatus, UsageSource,
    UsageSurface, UsageViewInput, bucket, split_fetch, usage_view,
};

use super::{
    AntigravityUsage, antigravity_buckets, antigravity_cli_version, antigravity_credits_bucket,
    fetch_antigravity_cli_credits, fetch_antigravity_cli_usage,
};

pub(crate) fn antigravity_snapshot(
    agent: &str,
    provider: Option<&str>,
    now: i64,
) -> FocusedUsageView {
    match antigravity_cli_version() {
        Ok(_) => {}
        Err(error) => {
            return antigravity_status_view(
                agent,
                provider,
                now,
                antigravity_version_error_status(&error),
                &error,
            );
        }
    }
    let (usage, usage_error) = split_fetch(Some(fetch_antigravity_cli_usage()));
    let (credits, credits_error) = split_fetch(Some(fetch_antigravity_cli_credits()));
    let mut buckets = usage
        .as_ref()
        .map(|usage| antigravity_buckets(usage, now))
        .unwrap_or_default();
    let credits_bucket = credits.as_ref().and_then(antigravity_credits_bucket);
    if let Some(bucket) = &credits_bucket {
        buckets.push(bucket.clone());
    }
    if buckets.is_empty() {
        let error = usage_error
            .as_deref()
            .or(Some("Antigravity CLI returned no quota pools"));
        buckets.push(bucket(
            "Quota",
            None,
            None,
            None,
            None,
            error,
            UsageSnapshotStatus::Stale,
        ));
    }
    let status = antigravity_snapshot_status(usage.as_ref(), credits_bucket.as_ref());
    let mut view = usage_view(UsageViewInput {
        agent,
        provider: provider.or(Some("Antigravity")),
        // No UsageSurface variant exists for Antigravity yet (this lane is
        // constrained to mod lines + re-exports in usage.rs); patch the
        // provider label until the surface wiring lands.
        surface: UsageSurface::Unsupported,
        account_label: usage
            .as_ref()
            .and_then(|usage| usage.identity.clone())
            .unwrap_or_default(),
        username: None,
        plan_label: usage.as_ref().and_then(|usage| usage.plan.clone()),
        credential_origin: Some("CLI · agy (identity unverified)".to_owned()),
        buckets,
        status,
        source: if status == UsageSnapshotStatus::Fresh {
            UsageSource::Cli
        } else {
            UsageSource::None
        },
        confidence: if status == UsageSnapshotStatus::Fresh {
            UsageConfidence::Authoritative
        } else {
            UsageConfidence::None
        },
        now,
        last_error: match status {
            UsageSnapshotStatus::Fresh => credits_error,
            _ => usage_error,
        },
    });
    view.account.provider_label = "Antigravity".to_owned();
    view
}

/// View status for a passed version gate: `Fresh` only when the response
/// carried quota signal (parsed pools or a credits row). A parsed-but-
/// pool-less response is `Stale` — never `Fresh` with a lone `Stale`
/// placeholder bucket inside.
pub(crate) fn antigravity_snapshot_status(
    usage: Option<&AntigravityUsage>,
    credits_bucket: Option<&QuotaBucketView>,
) -> UsageSnapshotStatus {
    let has_signal = usage.is_some_and(|usage| !usage.pools.is_empty()) || credits_bucket.is_some();
    if has_signal {
        UsageSnapshotStatus::Fresh
    } else {
        UsageSnapshotStatus::Stale
    }
}

/// Status for a version-gate failure: a too-old binary predates JSON usage
/// (`Unsupported`), but a missing/unparseable binary is `NeedsSecret` — a
/// login cannot install a binary (Cursor/Gemini lanes agree). Pure so the
/// mapping is unit-testable without a live `agy`.
pub(crate) fn antigravity_version_error_status(error: &str) -> UsageSnapshotStatus {
    if error.contains("predates JSON") {
        UsageSnapshotStatus::Unsupported
    } else {
        UsageSnapshotStatus::NeedsSecret
    }
}

pub(crate) fn antigravity_status_view(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    status: UsageSnapshotStatus,
    error: &str,
) -> FocusedUsageView {
    let mut view = usage_view(UsageViewInput {
        agent,
        provider: provider.or(Some("Antigravity")),
        surface: UsageSurface::Unsupported,
        account_label: String::new(),
        username: None,
        plan_label: None,
        credential_origin: Some("CLI · agy".to_owned()),
        buckets: vec![bucket("Quota", None, None, None, None, Some(error), status)],
        status,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        now,
        last_error: Some(error.to_owned()),
    });
    view.account.provider_label = "Antigravity".to_owned();
    view
}
