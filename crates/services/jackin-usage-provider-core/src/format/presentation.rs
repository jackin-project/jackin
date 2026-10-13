// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Usage bucket and detail presentation.

/// Rust-owned, limits-only presentation of one quota bucket. Shared by the
/// Capsule usage dialog and every native Desktop surface so semantic segment
/// choice and order live in Rust, never in Swift or a per-surface copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageBucketPresentation {
    /// Provider percentage text (segment 0), when the bucket has one.
    pub remaining_label: Option<String>,
    /// Complete semantic segments in display order (a Rust pace composite is
    /// flattened onto the canonical `" · "` separator).
    pub display_segments: Vec<String>,
    /// `display_segments` joined with the canonical `" · "` separator.
    pub display_label: String,
    /// Percentage usable only as presentation geometry (meter fill): remaining
    /// on every slot, including Spend, so all capsule meters agree with the
    /// console windows. The Spend *text* still reads used (`{n}% used`).
    pub meter_percent: Option<u8>,
}

/// Stable human status label for a snapshot status (limits-only; no price).
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

/// Build the one provider/account/activity identity block consumed by Capsule
/// and native Desktop surfaces. All visible copy is complete before crossing
/// the FFI boundary.
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

/// Build the shared limits-only presentation for one quota bucket. The segment
/// choice/order matches the Capsule usage dialog exactly; the Capsule meter is
/// prepended by the caller from [`UsageBucketPresentation::meter_percent`].
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
            // A remaining percent saturates at zero, so money over-100%
            // overage is invisible to it; the structured money ratio recovers
            // the raw magnitude through the same checked rule the projection
            // uses, keeping both surfaces on one "{raw}% used" text.
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
        // Balance-only quota (no percent, pace, reset, or money) surfaces its
        // limit label as the primary segment — the generic seam Grok's prepaid
        // balance consumes (plan 003). Buckets with any other segment are
        // unaffected, so existing Capsule output stays byte-identical.
        if segments.is_empty()
            && let Some(limit) = &bucket.limit_label
        {
            segments.push(limit.clone());
        }
        if segments.is_empty() || bucket.status != UsageSnapshotStatus::Fresh {
            segments.push(usage_display_status_label(bucket.status).to_owned());
        }
    }

    // Flatten a Rust pace composite (e.g. `"13% in deficit · Runs out in 2d"`)
    // onto the canonical separator so every segment is atomic.
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

/// One leading-only metadata line plus its `display_label`.
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

/// Build the single Rust-owned provider-detail card shared by the Capsule usage
/// dialog and the native Desktop Usage window. Identity, activity, and ordinary
/// freshness live in the separate identity presentation. This card emits only
/// distinct `username`/`plan`/`auth`, one `bucket:<zero-based index>` per source bucket
/// (so duplicate provider labels stay distinct), then optional `detail`
/// (`last_error`, appended after the last-good bucket rows — errors never
/// replace data). Every visible string is produced here; consumers render the
/// rows mechanically.
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
        // Canonical semantic order is already flattened in `display_segments`
        // (remaining, pace/run-out, reset, quota-bound/status). The reset
        // segment moves to the trailing column so the window can right-align it;
        // every other segment is a leading line. Order — and therefore the
        // joined `display_label` — is preserved either way.
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
