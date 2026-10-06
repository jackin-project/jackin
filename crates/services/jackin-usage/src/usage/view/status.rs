// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Status bar and summary labels.

use super::super::{QuotaBucketView, StatusSlot, UsageSnapshotStatus, UsageSurface};

/// Monetary spend for the status-bar headline, read from the `Spend`-slot
/// bucket and rendered `<used> of <limit>` with the currency shown once
/// (e.g. `SGD 78 of 260`). `None` unless a fresh/stale bucket carries
/// structured [`Money`], so the headline shows nothing rather than a stale or
/// zeroed figure.
pub(crate) fn spend_headline_label(buckets: &[QuotaBucketView]) -> Option<String> {
    let spend = buckets.iter().find(|bucket| {
        bucket.status_slot == Some(StatusSlot::Spend) && status_bar_fresh_or_stale(bucket)
    })?;
    let used = spend.used_money.as_ref()?;
    // Drop zero spend from the compact headline (Bug 8): `$0 spent` / `$0 of N`
    // carries no signal in the status bar. The dialog still shows `$0.00 spent`.
    if used.amount_minor == 0 {
        return None;
    }
    Some(match spend.limit_money.as_ref() {
        Some(limit) => format!("{} of {}", used.format_compact(), limit.major_amount()),
        None => format!("{} spent", used.format_compact()),
    })
}

pub(crate) fn status_bar_label(
    surface: UsageSurface,
    _account_label: &str,
    status: UsageSnapshotStatus,
    buckets: &[QuotaBucketView],
) -> String {
    if let Some(headline) = status_bar_headline_for_surface(surface, buckets) {
        return headline;
    }
    match status {
        UsageSnapshotStatus::Fresh => "usage cached".to_owned(),
        UsageSnapshotStatus::Stale => "stale".to_owned(),
        UsageSnapshotStatus::NeedsLogin => "login".to_owned(),
        UsageSnapshotStatus::NeedsSecret => "secret".to_owned(),
        UsageSnapshotStatus::Unsupported => "unsupported".to_owned(),
        UsageSnapshotStatus::Unavailable => "usage unavailable".to_owned(),
        UsageSnapshotStatus::Error => "error".to_owned(),
    }
}

pub(crate) fn status_bar_headline_for_surface(
    surface: UsageSurface,
    buckets: &[QuotaBucketView],
) -> Option<String> {
    if surface == UsageSurface::Amp {
        amp_status_bar_headline(buckets)
    } else {
        // Session/Weekly percentages, then the monetary spend, all in one
        // ` · `-joined headline (e.g. `Session 89% · Weekly 73% · SGD 78 of 260`).
        let mut labels = status_bar_quota_labels(buckets);
        labels.extend(spend_headline_label(buckets));
        (!labels.is_empty()).then(|| labels.join(" · "))
    }
}

pub(crate) fn amp_status_bar_headline(buckets: &[QuotaBucketView]) -> Option<String> {
    // Daily is the only Amp glance headline; credit/workspace bounds stay
    // detail-only and never leak into the status bar or infer availability
    // from a bucket title.
    buckets
        .iter()
        .find(|bucket| {
            status_bar_fresh_or_stale(bucket) && bucket.status_slot == Some(StatusSlot::Daily)
        })
        .and_then(|bucket| {
            bucket
                .remaining_percent
                .map(|remaining| format!("Free {remaining}%"))
        })
}

pub(crate) fn status_bar_quota_labels(buckets: &[QuotaBucketView]) -> Vec<String> {
    // Read the semantic slot the provider tagged at construction, not the
    // free-text label — a window rename can't silently break the headline.
    [
        (StatusSlot::Session, "Session"),
        (StatusSlot::Weekly, "Weekly"),
    ]
    .into_iter()
    .filter_map(|(slot, label)| {
        buckets
            .iter()
            .find(|bucket| bucket.status_slot == Some(slot) && status_bar_fresh_or_stale(bucket))
            .and_then(|bucket| {
                // Drop a zero window from the compact headline (Bug 8, operator
                // decision: omit every zero-value segment from the status bar;
                // the dialog still shows `0% left`).
                bucket
                    .remaining_percent
                    .filter(|&remaining| remaining != 0)
                    .map(|remaining| format!("{label} {remaining}%"))
            })
    })
    .collect()
}

pub(crate) fn status_bar_fresh_or_stale(bucket: &QuotaBucketView) -> bool {
    matches!(
        bucket.status,
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale
    )
}

pub(crate) fn compact_account_identity(account_label: &str) -> &str {
    let trimmed = account_label.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("needs ")
        || trimmed.ends_with(" unavailable")
        || trimmed.contains(" not available")
    {
        "account unavailable"
    } else {
        trimmed
    }
}

/// Rank of one bucket in the settled Overview-summary order (D30:
/// long-range weekly/daily, model-specific, session, then other). The slot
/// mapping mirrors the projection's `window_category` exactly
/// (`Spend`/`None` read as `Other`), so the capsule and the console select
/// the same window; no producer emits the `Model` category yet, so
/// model-specific windows rank as `Other` on both surfaces until one does.
pub(crate) fn summary_slot_rank(slot: Option<StatusSlot>) -> u8 {
    match slot {
        Some(StatusSlot::Daily | StatusSlot::Weekly) => 0,
        // No `StatusSlot` marks a model-specific window; unslotted buckets
        // rank as `Other`, exactly like the projection maps them.
        Some(StatusSlot::Session) => 2,
        Some(StatusSlot::Spend) | None => 3,
    }
}

pub(crate) fn summary_bucket(buckets: &[QuotaBucketView]) -> Option<&QuotaBucketView> {
    // First available Rust-ranked limit (D30): lowest category rank wins,
    // ties break to provider order, and only fresh buckets carrying a
    // remaining percent qualify. Spend ranks last as `Other` (Bug 5: a
    // reset-less spend bucket must not win the headline over a real limit),
    // but still wins over nothing, so a spend-only account shows its quota.
    buckets
        .iter()
        .enumerate()
        .filter(|(_, bucket)| bucket.status == UsageSnapshotStatus::Fresh)
        .filter(|(_, bucket)| bucket.remaining_percent.is_some())
        .min_by_key(|(index, bucket)| (summary_slot_rank(bucket.status_slot), *index))
        .map(|(_, bucket)| bucket)
}
