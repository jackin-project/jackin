// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Tab metadata and bucket builders.

use super::super::{
    FocusedUsageView, QuotaBucketView, StatusSlot, UsageSeverity, UsageSnapshotStatus, UsageSource,
    reset_label,
};
use super::summary_bucket;

/// Parse the broker account id out of a broker cache key
/// (`{base}:account-id-v1:{surface_id}:{account_id}`, built by
/// `usage_cache_key_for_broker_account`). Surface ids are closed colon-free
/// tokens, so the first colon after the marker splits the pair.
pub(crate) fn broker_account_id_from_cache_key(key: &str) -> Option<String> {
    let (_, rest) = key.split_once(":account-id-v1:")?;
    let (surface_id, account_id) = rest.split_once(':')?;
    (!surface_id.is_empty() && !account_id.is_empty()).then(|| account_id.to_owned())
}

/// Freshness + source tag for the Overview row, e.g. "fresh · provider" or
/// "stale · local estimate", matching the CodexBar-style status column.
pub(crate) fn usage_tab_source_label(view: &FocusedUsageView) -> String {
    let freshness = match view.status {
        UsageSnapshotStatus::Fresh => "fresh",
        UsageSnapshotStatus::Stale => "stale",
        UsageSnapshotStatus::NeedsLogin => "needs login",
        UsageSnapshotStatus::NeedsSecret => "needs secret",
        UsageSnapshotStatus::Unsupported => "unsupported",
        UsageSnapshotStatus::Unavailable => "unavailable",
        UsageSnapshotStatus::Error => "error",
    };
    let source = match view.source {
        UsageSource::ProviderApi => "provider",
        UsageSource::Cli => "managed CLI",
        UsageSource::LocalLogs => "local estimate",
        UsageSource::Cache => "cache",
        UsageSource::None => "no source",
    };
    format!("{freshness} · {source}")
}

pub(crate) fn usage_tab_status_label(view: &FocusedUsageView) -> String {
    if view.status == UsageSnapshotStatus::Fresh
        && let Some(bucket) = summary_bucket(&view.buckets)
        && let Some(remaining) = bucket.remaining_percent
    {
        // The summary window is the first available Rust-ranked limit (D30),
        // shared with the console list summary. An unslotted window (a
        // model-scoped Fable/Sonnet limit, or any other provider bucket)
        // winning the headline is named, so the Overview/status row tells the
        // operator *which* limit the % traces to, not just the % left.
        // Headline windows (Session/Weekly) stay bare: their slot already
        // implies them and the status bar carries those separately.
        let mut label = String::new();
        if bucket.status_slot.is_none() && !bucket.label.is_empty() {
            label.push_str(&bucket.label);
            label.push(' ');
        }
        label.push_str(&format!("{remaining}% left"));
        if let Some(reset) = &bucket.reset_label {
            label.push_str(" · ");
            label.push_str(reset);
        }
        return label;
    }
    match view.status {
        UsageSnapshotStatus::Fresh => "fresh".to_owned(),
        UsageSnapshotStatus::Stale => "stale".to_owned(),
        UsageSnapshotStatus::NeedsLogin => "needs login".to_owned(),
        UsageSnapshotStatus::NeedsSecret => "needs secret".to_owned(),
        UsageSnapshotStatus::Unsupported => "unsupported".to_owned(),
        UsageSnapshotStatus::Unavailable => "unavailable".to_owned(),
        UsageSnapshotStatus::Error => "error".to_owned(),
    }
}

pub(crate) fn bucket(
    label: &str,
    used_label: Option<String>,
    limit_label: Option<String>,
    remaining_percent: Option<u8>,
    reset_label: Option<String>,
    pace_label: Option<&str>,
    status: UsageSnapshotStatus,
) -> QuotaBucketView {
    QuotaBucketView {
        label: label.to_owned(),
        used_label,
        limit_label,
        remaining_percent,
        reset_label,
        resets_at: None,
        status_slot: None,
        pace_label: pace_label.map(str::to_owned),
        status,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::default(),
    }
}

/// Stamp a quota bucket's status-bar slot at construction. Returns the bucket so
/// it can be tagged and pushed in one expression (`buckets.push(with_status_slot(
/// build(...), Some(StatusSlot::Session)))`) — the slot rides with the view it
/// belongs to, so no later `last_mut`/positional step can float the tag onto the
/// wrong bucket.
pub(crate) fn with_status_slot(
    mut view: QuotaBucketView,
    slot: Option<StatusSlot>,
) -> QuotaBucketView {
    view.status_slot = slot;
    view
}

/// Build a window bucket carrying both the formatted reset label and the raw
/// reset epoch (RC2), so the CLI report can emit `resets_at`. `reset_at` is the
/// authoritative timestamp; `reset_label` is derived from it.
#[expect(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) fn timed_bucket(
    label: &str,
    used_label: Option<String>,
    limit_label: Option<String>,
    remaining_percent: Option<u8>,
    reset_at: Option<i64>,
    now: i64,
    pace_label: Option<&str>,
    status: UsageSnapshotStatus,
) -> QuotaBucketView {
    let mut view = bucket(
        label,
        used_label,
        limit_label,
        remaining_percent,
        reset_at.map(|epoch| reset_label(epoch, now)),
        pace_label,
        status,
    );
    view.resets_at = reset_at;
    view
}
