// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::{QuotaBucketView, StatusSlot, UsageSnapshotStatus, UsageSurface};

pub fn spend_headline_label(buckets: &[QuotaBucketView]) -> Option<String> {
    let spend = buckets.iter().find(|bucket| {
        bucket.status_slot == Some(StatusSlot::Spend) && status_bar_fresh_or_stale(bucket)
    })?;
    let used = spend.used_money.as_ref()?;
    if used.amount_minor == 0 {
        return None;
    }
    Some(match spend.limit_money.as_ref() {
        Some(limit) => format!("{} of {}", used.format_compact(), limit.major_amount()),
        None => format!("{} spent", used.format_compact()),
    })
}

pub fn status_bar_label(
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

pub fn status_bar_headline_for_surface(
    surface: UsageSurface,
    buckets: &[QuotaBucketView],
) -> Option<String> {
    if surface == UsageSurface::Amp {
        amp_status_bar_headline(buckets)
    } else {
        let mut labels = status_bar_quota_labels(buckets);
        labels.extend(spend_headline_label(buckets));
        (!labels.is_empty()).then(|| labels.join(" · "))
    }
}

pub fn amp_status_bar_headline(buckets: &[QuotaBucketView]) -> Option<String> {
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

pub fn status_bar_quota_labels(buckets: &[QuotaBucketView]) -> Vec<String> {
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
                bucket
                    .remaining_percent
                    .filter(|&remaining| remaining != 0)
                    .map(|remaining| format!("{label} {remaining}%"))
            })
    })
    .collect()
}

pub fn status_bar_fresh_or_stale(bucket: &QuotaBucketView) -> bool {
    matches!(
        bucket.status,
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale
    )
}

pub fn compact_account_identity(account_label: &str) -> &str {
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

pub(crate) fn summary_slot_rank(slot: Option<StatusSlot>) -> u8 {
    match slot {
        Some(StatusSlot::Daily | StatusSlot::Weekly) => 0,
        Some(StatusSlot::Session) => 2,
        Some(StatusSlot::Spend) | None => 3,
    }
}

pub fn summary_bucket(buckets: &[QuotaBucketView]) -> Option<&QuotaBucketView> {
    buckets
        .iter()
        .enumerate()
        .filter(|(_, bucket)| bucket.status == UsageSnapshotStatus::Fresh)
        .filter(|(_, bucket)| bucket.remaining_percent.is_some())
        .min_by_key(|(index, bucket)| (summary_slot_rank(bucket.status_slot), *index))
        .map(|(_, bucket)| bucket)
}
