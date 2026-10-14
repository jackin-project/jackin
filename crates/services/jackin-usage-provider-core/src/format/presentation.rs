// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageBucketPresentation {
    pub remaining_label: Option<String>,
    pub display_segments: Vec<String>,
    pub display_label: String,
    pub meter_percent: Option<u8>,
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
    use jackin_protocol::control::{StatusSlot, UsageSnapshotStatus};

    let mut segments: Vec<String> = Vec::new();
    let mut remaining_label = None;
    let mut meter_percent = None;

    if bucket.status_slot == Some(StatusSlot::Spend) {
        if let Some(remaining) = bucket.remaining_percent {
            let money_raw = bucket
                .used_money
                .as_ref()
                .and_then(|used| used.raw_percent_of(bucket.limit_money.as_ref()?));
            let used_raw: i32 = match money_raw {
                Some(raw) if raw > 100 => raw,
                _ => i32::from(100u8.saturating_sub(remaining)),
            };
            let segment = format!("{used_raw}% used");
            remaining_label = Some(segment.clone());
            segments.push(segment);
            meter_percent = Some(u8::try_from((100 - used_raw).clamp(0, 100)).unwrap_or(0));
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
    } else {
        if let Some(remaining) = bucket.remaining_percent {
            let segment =
                if bucket.label == "Credits" && remaining == 0 && bucket.limit_label.is_some() {
                    "0 left".to_owned()
                } else {
                    format!("{remaining}% left")
                };
            remaining_label = Some(segment.clone());
            segments.push(segment);
            meter_percent = Some(remaining);
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
            && bucket.remaining_percent == Some(0)
            && let Some(limit) = &bucket.limit_label
        {
            segments.push(limit.clone());
        }
        if segments.is_empty()
            && let Some(limit) = &bucket.limit_label
        {
            segments.push(limit.clone());
        }
        if segments.is_empty() || bucket.status != UsageSnapshotStatus::Fresh {
            segments.push(usage_display_status_label(bucket.status).to_owned());
        }
    }

    let display_segments: Vec<String> = segments
        .iter()
        .flat_map(|segment| segment.split(" · ").map(str::to_owned))
        .collect();
    let display_label = display_segments.join(" · ");
    UsageBucketPresentation {
        remaining_label,
        display_segments,
        display_label,
        meter_percent,
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
        let presentation = usage_bucket_presentation(bucket);
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
