// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_host_accounts::{self as accounts, AccountLifecycle, HostAccountDescriptor};
use jackin_usage_host_presentation::{
    HostProviderGlanceRow, HostSurfaceId, SELECTED_ACCOUNT_UNAVAILABLE_NOTICE,
};

use jackin_protocol::control::{
    CanonicalRemainingPercent, CanonicalUsedPercent, CountQuota, FocusedUsageView,
    MeterPercentPoints, PercentObservationOrigin, ProviderQuotaState, QuotaBucketView,
    UsageMeterSide, UsageSeverity,
};

use jackin_usage_provider_core::{
    UsageFormatPrefs, compact_duration_label, count_summary_label, exact_reset_parenthetical,
    format_percent_points, money_used_percent, percent_headline, provider_display_label,
    quota_bucket_has_observation, quota_bucket_remaining_rank_points, reset_label_with_prefs,
    select_quota_meter, usage_display_status_label, usage_identity_presentation,
    usage_status_storage_label,
};

#[derive(Debug, Clone, Copy)]
pub struct DrivingBucket {
    pub used_percent: Option<CanonicalUsedPercent>,
    pub used_percent_origin: Option<PercentObservationOrigin>,
    pub remaining_percent: Option<CanonicalRemainingPercent>,
    pub remaining_percent_origin: Option<PercentObservationOrigin>,
    pub count_quota: Option<CountQuota>,
    pub provider_quota_state: Option<ProviderQuotaState>,
    pub meter_percent: Option<MeterPercentPoints>,
    pub meter_side: Option<UsageMeterSide>,
    pub resets_at: Option<i64>,
}

pub const STATUS_BAR_MAX_CHIPS: usize = 3;

#[must_use]
pub fn status_bar_rank_key(bucket: &QuotaBucketView) -> (i64, f64) {
    (
        bucket.resets_at.unwrap_or(i64::MAX),
        quota_bucket_remaining_rank_points(bucket).unwrap_or(f64::INFINITY),
    )
}

pub fn driving_bucket_from_view(view: &FocusedUsageView) -> Option<DrivingBucket> {
    view.buckets
        .iter()
        .filter(|bucket| bucket_has_summary(bucket))
        .min_by(|left, right| bucket_summary_order(left).total_cmp(&bucket_summary_order(right)))
        .map(|bucket| {
            let (meter_percent, meter_side) = select_quota_meter(bucket);
            DrivingBucket {
                used_percent: bucket.used_percent,
                used_percent_origin: bucket.used_percent_origin,
                remaining_percent: bucket.remaining_percent,
                remaining_percent_origin: bucket.remaining_percent_origin,
                count_quota: bucket.count_quota,
                provider_quota_state: bucket.provider_quota_state,
                meter_percent,
                meter_side,
                resets_at: bucket.resets_at,
            }
        })
}

pub fn drive_label_prefix<'a>(
    view: &'a FocusedUsageView,
    bucket: &QuotaBucketView,
) -> Option<&'a str> {
    view.buckets
        .iter()
        .find(|candidate| std::ptr::eq(*candidate, bucket) && candidate.status_slot.is_none())
        .map(|bucket| bucket.label.as_str())
        .filter(|label| !label.is_empty())
}

fn bucket_has_summary(bucket: &QuotaBucketView) -> bool {
    quota_bucket_has_observation(bucket)
}

fn bucket_summary_order(bucket: &QuotaBucketView) -> f64 {
    quota_bucket_remaining_rank_points(bucket).unwrap_or(f64::INFINITY)
}

pub fn selected_account_unavailable_view(surface: HostSurfaceId) -> FocusedUsageView {
    let mut view = FocusedUsageView::unavailable(
        SELECTED_ACCOUNT_UNAVAILABLE_NOTICE,
        chrono::Utc::now().timestamp(),
    );
    view.focused_agent = Some(surface.agent_slug().to_owned());
    view.focused_provider = Some(surface.label().to_owned());
    view.account.provider_label = surface.account_provider_label().to_owned();
    view
}

pub fn view_is_auto_detected(view: &FocusedUsageView) -> bool {
    let origin_affirmative = view
        .account
        .credential_origin
        .as_deref()
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .is_some_and(|origin| !origin.to_ascii_lowercase().starts_with("needs "));
    let bucket_evidence = view.buckets.iter().any(quota_bucket_has_observation);
    origin_affirmative || bucket_evidence
}

pub fn glance_bucket(surface: HostSurfaceId, view: &FocusedUsageView) -> Option<&QuotaBucketView> {
    let slot = if surface == HostSurfaceId::Amp {
        jackin_protocol::control::StatusSlot::Daily
    } else {
        jackin_protocol::control::StatusSlot::Weekly
    };
    view.buckets
        .iter()
        .find(|bucket| bucket.status_slot == Some(slot) && quota_bucket_has_observation(bucket))
        .or_else(|| accounts::summary_bucket(view))
}

pub fn build_provider_glance_row(
    surface: HostSurfaceId,
    view: &FocusedUsageView,
    is_updating: bool,
    now: i64,
    prefs: UsageFormatPrefs,
) -> HostProviderGlanceRow {
    use jackin_protocol::control::UsageSnapshotStatus as Status;
    let display_label = provider_display_label(surface.label()).to_owned();
    let glance = glance_bucket(surface, view).or_else(|| {
        view.buckets
            .iter()
            .find(|bucket| bucket_has_summary(bucket))
    });
    let (meter_percent, meter_side) = glance.map_or((None, None), select_quota_meter);
    let glance_used_percent = glance.and_then(|bucket| bucket.used_percent);
    let glance_used_percent_origin = glance.and_then(|bucket| bucket.used_percent_origin);
    let glance_remaining_percent = glance.and_then(|bucket| bucket.remaining_percent);
    let glance_remaining_percent_origin = glance.and_then(|bucket| bucket.remaining_percent_origin);
    let glance_count_quota = glance.and_then(|bucket| bucket.count_quota);
    let glance_provider_quota_state = glance.and_then(|bucket| bucket.provider_quota_state);
    let glance_resets_at = glance.and_then(|bucket| bucket.resets_at);
    let (bar_label, headline, reset_label, compact_reset_label, exact_reset) = {
        let label =
            glance.and_then(|bucket| quota_bucket_label(bucket, meter_percent, meter_side, prefs));
        let (reset_label, compact_reset_label, exact_reset) = glance
            .and_then(|bucket| bucket.resets_at)
            .map_or((None, None, None), |at| {
                (
                    Some(reset_label_with_prefs(at, now, prefs)),
                    Some(if at <= now {
                        "now".to_owned()
                    } else {
                        compact_duration_label(at.saturating_sub(now))
                    }),
                    Some(exact_reset_parenthetical(at)),
                )
            });
        match label {
            Some(label) => (
                label.clone(),
                label,
                reset_label,
                compact_reset_label,
                exact_reset,
            ),
            None => (
                "–".to_owned(),
                "–".to_owned(),
                reset_label,
                compact_reset_label,
                exact_reset,
            ),
        }
    };
    let identity = usage_identity_presentation(&display_label, view, is_updating);
    HostProviderGlanceRow {
        surface_id: surface.id().to_owned(),
        icon_key: surface.id().to_owned(),
        fallback_glyph: surface.fallback_glyph().to_owned(),
        usage_url: surface.usage_url().map(str::to_owned),
        display_label,
        account_label: identity.account_label.clone(),
        plan_label: view.account.plan_label.clone(),
        glance_used_percent,
        glance_used_percent_origin,
        glance_remaining_percent,
        glance_remaining_percent_origin,
        glance_count_quota,
        glance_provider_quota_state,
        glance_resets_at,
        glance_meter_percent: meter_percent,
        glance_meter_side: meter_side,
        bar_label,
        headline,
        reset_label,
        compact_reset_label,
        exact_reset,
        status_word: usage_status_storage_label(view.status).to_owned(),
        is_refreshing: is_updating || view.is_refreshing_placeholder(),
        status_label: usage_display_status_label(view.status).to_owned(),
        severity: worst_severity_label(view),
        updated_label: view.updated_label.clone(),
        activity_label: identity.activity_label,
        activity_kind: match identity.activity_kind {
            jackin_protocol::control::UsageActivityKind::Idle => "idle",
            jackin_protocol::control::UsageActivityKind::Updating => "updating",
            jackin_protocol::control::UsageActivityKind::Exceptional => "exceptional",
        }
        .to_owned(),
        accessibility_label: identity.accessibility_label,
        last_error: view.last_error.clone(),
        dimmed: matches!(view.status, Status::Stale | Status::Error),
    }
}

pub fn account_descriptor(
    surface: HostSurfaceId,
    entry: &accounts::AccountCatalogEntry,
    selected: bool,
    now: i64,
    prefs: UsageFormatPrefs,
) -> HostAccountDescriptor {
    use jackin_protocol::control::UsageSnapshotStatus as Status;

    let view = &entry.view;
    let bucket = glance_bucket(surface, view)
        .or_else(|| accounts::summary_bucket(view))
        .or_else(|| {
            view.buckets
                .iter()
                .find(|bucket| bucket_has_summary(bucket))
        });
    let remaining_percent = bucket.and_then(|bucket| bucket.remaining_percent);
    let used_percent = bucket.and_then(|bucket| bucket.used_percent);
    let (meter_percent, meter_side) = bucket.map_or((None, None), select_quota_meter);
    let remaining_label = bucket
        .and_then(|bucket| quota_bucket_label(bucket, meter_percent, meter_side, prefs))
        .unwrap_or_else(|| "—".to_owned());
    let headline = remaining_label.clone();
    let (reset_label, exact_reset) =
        bucket
            .and_then(|bucket| bucket.resets_at)
            .map_or((None, None), |reset| {
                (
                    Some(reset_label_with_prefs(reset, now, prefs)),
                    Some(exact_reset_parenthetical(reset)),
                )
            });
    let mut provenance = entry
        .provenance
        .iter()
        .map(|source| source.display_label().to_owned())
        .collect::<Vec<_>>();
    provenance.extend(entry.discovery_provenance.iter().cloned());
    provenance.sort();
    provenance.dedup();
    let provenance_label = provenance.join(" · ");
    let status_label = usage_display_status_label(view.status).to_owned();
    let plan_or_status_label = entry.plan_label.clone().unwrap_or_else(|| {
        if matches!(view.status, Status::Fresh) {
            "—".to_owned()
        } else {
            status_label.clone()
        }
    });
    let reset_display_label = reset_label.clone().unwrap_or_else(|| "—".to_owned());
    let accessibility_label = format!(
        "{}, {}, {}, {}, {}",
        provider_display_label(surface.label()),
        entry.account_label,
        plan_or_status_label,
        remaining_label,
        reset_display_label
    );
    HostAccountDescriptor {
        surface_id: surface.id().to_owned(),
        provider_column_label: "—".to_owned(),
        account_key: entry.account_key.clone(),
        account_label: entry.account_label.clone(),
        plan_label: entry.plan_label.clone(),
        selected,
        lifecycle: entry.lifecycle.label().to_owned(),
        lifecycle_label: match entry.lifecycle {
            AccountLifecycle::Current => "Current account",
            AccountLifecycle::Historical => "Historical account",
            AccountLifecycle::ProviderPresenceOnly => "Provider presence only",
        }
        .to_owned(),
        provenance,
        provenance_label,
        plan_or_status_label,
        used_percent,
        used_percent_origin: bucket.and_then(|bucket| bucket.used_percent_origin),
        remaining_percent,
        remaining_percent_origin: bucket.and_then(|bucket| bucket.remaining_percent_origin),
        count_quota: bucket.and_then(|bucket| bucket.count_quota),
        provider_quota_state: bucket.and_then(|bucket| bucket.provider_quota_state),
        resets_at: bucket.and_then(|bucket| bucket.resets_at),
        meter_percent,
        meter_side,
        remaining_label,
        headline,
        reset_label,
        reset_display_label,
        exact_reset,
        status_word: usage_status_storage_label(view.status).to_owned(),
        status_label,
        severity: worst_severity_label(view),
        updated_label: view.updated_label.clone(),
        last_error: view.last_error.clone(),
        dimmed: matches!(view.status, Status::Stale | Status::Error),
        accessibility_label,
    }
}

fn quota_bucket_label(
    bucket: &QuotaBucketView,
    meter_percent: Option<MeterPercentPoints>,
    meter_side: Option<UsageMeterSide>,
    prefs: UsageFormatPrefs,
) -> Option<String> {
    if let Some(summary) = jackin_usage_provider_core::provider_quota_state_summary(bucket) {
        return Some(summary);
    }
    if let Some(used) = bucket.used_percent {
        return Some(used.label());
    }
    if let Some(remaining) = bucket.remaining_percent {
        return Some(percent_headline(remaining, prefs));
    }
    if let Some(count) = count_summary_label(bucket) {
        return Some(count);
    }
    if let Some(money_used) = money_used_percent(bucket) {
        return Some(format!("{}% used", money_used.display_value()));
    }
    if let (Some(meter), Some(side)) = (meter_percent, meter_side) {
        let value = format_percent_points(meter.get());
        return Some(match side {
            UsageMeterSide::Used => format!("{value}% used"),
            UsageMeterSide::Remaining => format!("{value}% left"),
        });
    }
    match (&bucket.used_label, &bucket.limit_label) {
        (Some(used), Some(limit)) => Some(format!("{used} of {limit}")),
        (Some(used), None) | (None, Some(used)) => Some(used.clone()),
        (None, None) => None,
    }
}

pub fn worst_severity_label(view: &FocusedUsageView) -> String {
    let mut worst = UsageSeverity::Normal;
    for bucket in &view.buckets {
        match bucket.severity {
            UsageSeverity::Danger => worst = UsageSeverity::Danger,
            UsageSeverity::Warn if worst != UsageSeverity::Danger => {
                worst = UsageSeverity::Warn;
            }
            _ => {}
        }
    }
    match worst {
        UsageSeverity::Normal => "normal",
        UsageSeverity::Warn => "warn",
        UsageSeverity::Danger => "danger",
    }
    .to_owned()
}
