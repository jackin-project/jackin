// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageBucketPresentation {
    pub remaining_label: Option<String>,
    pub display_segments: Vec<String>,
    pub display_label: String,
    pub meter_percent: Option<jackin_protocol::control::MeterPercentPoints>,
    pub meter_side: Option<jackin_protocol::control::UsageMeterSide>,
}

#[must_use]
pub fn usage_display_status_label(
    status: jackin_protocol::control::UsageSnapshotStatus,
) -> &'static str {
    use jackin_protocol::control::UsageSnapshotStatus as S;
    match status {
        S::Fresh => "fresh",
        S::Stale => "stale",
        S::NeedsLogin => "needs login",
        S::NeedsSecret => "needs secret",
        S::Unsupported => "unsupported",
        S::Unavailable => "unavailable",
        S::Error => "error",
    }
}

#[must_use]
pub fn usage_identity_presentation(
    provider_title: &str,
    view: &jackin_protocol::control::FocusedUsageView,
    is_updating: bool,
) -> jackin_protocol::control::UsageIdentityPresentation {
    use jackin_protocol::control::{UsageActivityKind, UsageSnapshotStatus as Status};

    let account_label = if view.account.account_label.trim().is_empty() {
        "No authenticated account".to_owned()
    } else {
        view.account.account_label.clone()
    };
    let (activity_label, activity_kind) = if is_updating || view.is_refreshing_placeholder() {
        ("Updating…".to_owned(), UsageActivityKind::Updating)
    } else {
        match view.status {
            Status::Fresh => (view.updated_label.clone(), UsageActivityKind::Idle),
            Status::Stale => (
                format!("Update delayed · {}", view.updated_label),
                UsageActivityKind::Exceptional,
            ),
            Status::NeedsLogin => (
                "Sign in required".to_owned(),
                UsageActivityKind::Exceptional,
            ),
            Status::NeedsSecret => (
                "Credential required".to_owned(),
                UsageActivityKind::Exceptional,
            ),
            Status::Unsupported => (
                "Usage limits unsupported".to_owned(),
                UsageActivityKind::Exceptional,
            ),
            Status::Unavailable => (
                "Usage unavailable".to_owned(),
                UsageActivityKind::Exceptional,
            ),
            Status::Error => (
                format!("Update failed · {}", view.updated_label),
                UsageActivityKind::Exceptional,
            ),
        }
    };
    jackin_protocol::control::UsageIdentityPresentation {
        provider_title: provider_title.to_owned(),
        account_label: account_label.clone(),
        accessibility_label: format!("{provider_title}, {account_label}, {activity_label}"),
        activity_label,
        activity_kind,
    }
}

pub(crate) fn usage_money_cap_segment(
    used: Option<&str>,
    limit: Option<&str>,
    prefix: &str,
) -> Option<String> {
    match (used, limit) {
        (Some(used), Some(limit)) => Some(format!("{prefix}: {used} / {limit}")),
        (Some(label), None) | (None, Some(label)) => Some(label.to_owned()),
        (None, None) => None,
    }
}

#[must_use]
pub fn usage_bucket_presentation(
    bucket: &jackin_protocol::control::QuotaBucketView,
) -> UsageBucketPresentation {
    usage_bucket_presentation_with_count_label(bucket, crate::count_summary_label(bucket))
}

fn usage_bucket_detail_presentation(
    bucket: &jackin_protocol::control::QuotaBucketView,
) -> UsageBucketPresentation {
    let count_label = bucket
        .count_quota
        .map(count_observation_detail_label)
        .or_else(|| crate::count_summary_label(bucket));
    usage_bucket_presentation_with_count_label(bucket, count_label)
}

fn usage_bucket_presentation_with_count_label(
    bucket: &jackin_protocol::control::QuotaBucketView,
    count_label: Option<String>,
) -> UsageBucketPresentation {
    use crate::quota::provider_quota_state_summary_with_count_label;

    match provider_quota_state_summary_with_count_label(bucket, count_label.as_deref()) {
        Some(summary) => provider_state_bucket_presentation(bucket, summary),
        None => quantity_bucket_presentation(bucket, count_label),
    }
}

fn count_observation_detail_label(count: jackin_protocol::control::CountQuota) -> String {
    fn value_label(value: Option<u64>) -> String {
        value.map_or_else(|| "unknown".to_owned(), |value| value.to_string())
    }

    format!(
        "Reported count ({}/{}): used {}, limit {}, remaining {}",
        count.unit.wire_label(),
        count.period.wire_label(),
        value_label(count.used),
        value_label(count.limit),
        value_label(count.remaining),
    )
}

fn provider_state_bucket_presentation(
    bucket: &jackin_protocol::control::QuotaBucketView,
    summary: String,
) -> UsageBucketPresentation {
    let mut segments = vec![summary.clone()];
    append_provider_state_details(bucket, &mut segments);
    finish_bucket_presentation(bucket, Some(summary), segments)
}

fn append_provider_state_details(
    bucket: &jackin_protocol::control::QuotaBucketView,
    segments: &mut Vec<String>,
) {
    use jackin_protocol::control::{StatusSlot, UsageSnapshotStatus};

    if bucket.status_slot != Some(StatusSlot::Spend) {
        if let Some(pace) = &bucket.pace_label {
            segments.push(pace.clone());
        }
        if let Some(reset) = &bucket.reset_label {
            segments.push(reset.clone());
        }
    }
    if bucket.status != UsageSnapshotStatus::Fresh {
        segments.push(usage_display_status_label(bucket.status).to_owned());
    }
}

#[derive(Default)]
struct BucketPresentationLabels {
    used: Option<String>,
    remaining: Option<String>,
    money: Option<String>,
    count: Option<String>,
    provider_used: Option<String>,
    provider_remaining: Option<String>,
}

fn bucket_presentation_labels(
    bucket: &jackin_protocol::control::QuotaBucketView,
    count_label: Option<String>,
) -> BucketPresentationLabels {
    use crate::money_used_percent;
    use jackin_protocol::control::PercentObservationOrigin;

    let used = bucket
        .used_percent
        .map(jackin_protocol::control::CanonicalUsedPercent::label);
    let remaining = bucket.remaining_percent.map(|percent| {
        if bucket.label == "Credits" && percent.get() == 0.0 && bucket.limit_label.is_some() {
            "0 left".to_owned()
        } else {
            percent.label()
        }
    });
    let money =
        money_used_percent(bucket).map(|percent| format!("{}% used", percent.display_value()));
    let count = count_label;
    let provider_used = (bucket.used_percent_origin
        == Some(PercentObservationOrigin::ProviderReported))
    .then(|| used.clone())
    .flatten();
    let provider_remaining = (bucket.remaining_percent_origin
        == Some(PercentObservationOrigin::ProviderReported))
    .then(|| remaining.clone())
    .flatten();

    BucketPresentationLabels {
        used,
        remaining,
        money,
        count,
        provider_used,
        provider_remaining,
    }
}

fn quantity_bucket_presentation(
    bucket: &jackin_protocol::control::QuotaBucketView,
    count_label: Option<String>,
) -> UsageBucketPresentation {
    use jackin_protocol::control::StatusSlot;

    let labels = bucket_presentation_labels(bucket, count_label);
    let (headline, segments) = if bucket.status_slot == Some(StatusSlot::Spend) {
        spend_bucket_segments(bucket, &labels)
    } else {
        regular_bucket_segments(bucket, &labels)
    };
    finish_bucket_presentation(bucket, headline, segments)
}

fn spend_bucket_segments(
    bucket: &jackin_protocol::control::QuotaBucketView,
    labels: &BucketPresentationLabels,
) -> (Option<String>, Vec<String>) {
    use jackin_protocol::control::UsageSnapshotStatus;

    let headline = labels
        .count
        .clone()
        .or_else(|| labels.provider_used.clone())
        .or_else(|| labels.provider_remaining.clone())
        .or_else(|| labels.money.clone())
        .or_else(|| labels.used.clone())
        .or_else(|| labels.remaining.clone());
    let mut segments = Vec::new();
    if let Some(label) = &headline {
        segments.push(label.clone());
    }
    for label in [
        labels.used.as_ref(),
        labels.remaining.as_ref(),
        labels.money.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        if !segments.contains(label) {
            segments.push(label.clone());
        }
    }
    if let Some(cap) = usage_money_cap_segment(
        bucket.used_label.as_deref(),
        bucket.limit_label.as_deref(),
        "Monthly cap",
    ) {
        segments.push(cap);
    }
    if segments.is_empty() || bucket.status != UsageSnapshotStatus::Fresh {
        segments.push(usage_display_status_label(bucket.status).to_owned());
    }
    (headline, segments)
}

fn regular_bucket_segments(
    bucket: &jackin_protocol::control::QuotaBucketView,
    labels: &BucketPresentationLabels,
) -> (Option<String>, Vec<String>) {
    let mut segments = Vec::new();
    if let Some(label) = &labels.used {
        segments.push(label.clone());
    }
    if let Some(label) = &labels.remaining {
        segments.push(label.clone());
    }
    if let Some(label) = &labels.count {
        segments.push(label.clone());
    }
    if labels.used.is_none()
        && labels.remaining.is_none()
        && let Some(label) = &labels.money
    {
        segments.push(label.clone());
    }
    if let Some(pace) = &bucket.pace_label {
        segments.push(pace.clone());
    }
    if let Some(reset) = &bucket.reset_label {
        segments.push(reset.clone());
    }
    if (bucket.used_money.is_some() || bucket.limit_money.is_some())
        && let Some(budget) = usage_money_cap_segment(
            bucket.used_label.as_deref(),
            bucket.limit_label.as_deref(),
            "Budget",
        )
    {
        segments.push(budget);
    } else if bucket.label == "Credits"
        && bucket
            .remaining_percent
            .is_some_and(|percent| percent.get() == 0.0)
        && let Some(limit) = &bucket.limit_label
    {
        segments.push(limit.clone());
    }
    append_fallback_status(bucket, &mut segments);
    (
        labels
            .used
            .clone()
            .or(labels.remaining.clone())
            .or(labels.count.clone())
            .or(labels.money.clone()),
        segments,
    )
}

fn append_fallback_status(
    bucket: &jackin_protocol::control::QuotaBucketView,
    segments: &mut Vec<String>,
) {
    use jackin_protocol::control::UsageSnapshotStatus;

    if segments.is_empty()
        && let Some(limit) = &bucket.limit_label
    {
        segments.push(limit.clone());
    }
    if segments.is_empty() || bucket.status != UsageSnapshotStatus::Fresh {
        segments.push(usage_display_status_label(bucket.status).to_owned());
    }
}

fn finish_bucket_presentation(
    bucket: &jackin_protocol::control::QuotaBucketView,
    remaining_label: Option<String>,
    segments: Vec<String>,
) -> UsageBucketPresentation {
    use crate::select_quota_meter;

    let display_segments: Vec<String> = segments
        .iter()
        .flat_map(|segment| segment.split(" · ").map(str::to_owned))
        .collect();
    let display_label = display_segments.join(" · ");
    let (meter_percent, meter_side) = select_quota_meter(bucket);
    UsageBucketPresentation {
        remaining_label,
        display_segments,
        display_label,
        meter_percent,
        meter_side,
    }
}

pub(crate) fn metadata_row(
    row_id: &str,
    label: &str,
    value: String,
) -> jackin_protocol::control::UsageDetailRow {
    jackin_protocol::control::UsageDetailRow {
        row_id: row_id.to_owned(),
        kind: jackin_protocol::control::UsageDetailRowKind::Metadata,
        label: label.to_owned(),
        display_label: value.clone(),
        layout_lines: vec![jackin_protocol::control::UsagePresentationLine {
            leading: Some(value),
            trailing: None,
        }],
        meter_percent: None,
        meter_side: None,
        severity: jackin_protocol::control::UsageSeverity::Normal,
    }
}

#[must_use]
pub fn usage_detail_presentation(
    view: &jackin_protocol::control::FocusedUsageView,
) -> jackin_protocol::control::UsageDetailPresentation {
    use jackin_protocol::control::{UsageDetailRow, UsageDetailRowKind, UsagePresentationLine};

    let mut rows = Vec::new();
    if let Some(username) = &view.account.username
        && username.trim() != view.account.account_label.trim()
    {
        rows.push(metadata_row("username", "Username", username.clone()));
    }
    if let Some(plan) = &view.account.plan_label {
        rows.push(metadata_row("plan", "Plan", plan.clone()));
    }
    if let Some(origin) = &view.account.credential_origin {
        rows.push(metadata_row("auth", "Auth", origin.clone()));
    }

    for (index, bucket) in view.buckets.iter().enumerate() {
        let presentation = usage_bucket_detail_presentation(bucket);
        let layout_lines: Vec<UsagePresentationLine> = presentation
            .display_segments
            .iter()
            .map(|segment| {
                if bucket.reset_label.as_deref() == Some(segment.as_str()) {
                    UsagePresentationLine {
                        leading: None,
                        trailing: Some(segment.clone()),
                    }
                } else {
                    UsagePresentationLine {
                        leading: Some(segment.clone()),
                        trailing: None,
                    }
                }
            })
            .collect();
        rows.push(UsageDetailRow {
            row_id: format!("bucket:{index}"),
            kind: UsageDetailRowKind::Bucket,
            label: bucket.label.clone(),
            display_label: presentation.display_label,
            layout_lines,
            meter_percent: presentation.meter_percent,
            meter_side: presentation.meter_side,
            severity: bucket.severity,
        });
    }

    if let Some(error) = &view.last_error {
        let mut row = metadata_row("detail", "Detail", error.clone());
        row.kind = UsageDetailRowKind::Detail;
        rows.push(row);
    }

    jackin_protocol::control::UsageDetailPresentation { rows }
}

#[cfg(test)]
mod tests;
