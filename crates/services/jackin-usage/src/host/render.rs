// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Host label and row render helpers.

use super::{
    AccountLifecycle, HostAccountDescriptor, HostProviderGlanceRow, HostSurfaceId,
    SELECTED_ACCOUNT_UNAVAILABLE_NOTICE, accounts,
};

use jackin_protocol::control::{FocusedUsageView, UsageSeverity};

use crate::usage::{
    UsageFormatPrefs, compact_duration_label, exact_reset_parenthetical, percent_headline,
    provider_display_label, reset_label_with_prefs, usage_display_status_label,
    usage_identity_presentation, usage_status_storage_label,
};

/// Driving bucket for compact/overview labels: min remaining + its reset epoch.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DrivingBucket {
    pub(crate) remaining: u8,
    pub(crate) resets_at: Option<i64>,
}

/// Hard cap for burn-first status-bar chips (SB-3 / SB-14). Never more than three.
pub const STATUS_BAR_MAX_CHIPS: usize = 3;

/// SB-17 rank keys for ascending sort: **soonest reset first**, then **higher
/// remaining %** (invert remaining so ascending puts larger headroom first).
/// Missing `resets_at` sorts last on the time key.
#[must_use]
pub(crate) fn status_bar_rank_key(remaining: u8, resets_at: Option<i64>) -> (i64, u8) {
    let time_key = resets_at.unwrap_or(i64::MAX);
    let remaining_key = u8::MAX.saturating_sub(remaining);
    (time_key, remaining_key)
}

/// Min-`remaining_percent` bucket (same selection as the legacy compact label).
pub(crate) fn driving_bucket_from_view(view: &FocusedUsageView) -> Option<DrivingBucket> {
    let mut best: Option<(u8, Option<i64>)> = None;
    for bucket in &view.buckets {
        let Some(remaining) = bucket.remaining_percent else {
            continue;
        };
        match best {
            Some((best_remaining, _)) if remaining >= best_remaining => {}
            _ => best = Some((remaining, bucket.resets_at)),
        }
    }
    best.map(|(remaining, resets_at)| DrivingBucket {
        remaining,
        resets_at,
    })
}

/// Model-scoped bucket label when the driving bucket has no status slot.
pub(crate) fn drive_label_prefix(view: &FocusedUsageView, remaining: u8) -> Option<&str> {
    view.buckets
        .iter()
        .find(|bucket| bucket.remaining_percent == Some(remaining) && bucket.status_slot.is_none())
        .map(|bucket| bucket.label.as_str())
        .filter(|label| !label.is_empty())
}

pub(crate) fn selected_account_unavailable_view(surface: HostSurfaceId) -> FocusedUsageView {
    let mut view = FocusedUsageView::unavailable(
        SELECTED_ACCOUNT_UNAVAILABLE_NOTICE,
        chrono::Utc::now().timestamp(),
    );
    view.focused_agent = Some(surface.agent_slug().to_owned());
    view.focused_provider = Some(surface.label().to_owned());
    view.account.provider_label = surface.account_provider_label().to_owned();
    view
}

/// A view is auto-detected when it carries affirmative credential evidence (a
/// non-empty `credential_origin` that is not a `"needs …"` placeholder, even
/// under `Unsupported` status) or at least one bucket with a numeric/formatted
/// quota field. Bucket labels, pace/status prose, and non-Fresh status alone
/// are never evidence.
pub(crate) fn view_is_auto_detected(view: &FocusedUsageView) -> bool {
    let origin_affirmative = view
        .account
        .credential_origin
        .as_deref()
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .is_some_and(|origin| !origin.to_ascii_lowercase().starts_with("needs "));
    let bucket_evidence = view.buckets.iter().any(|bucket| {
        bucket.remaining_percent.is_some()
            || bucket.used_label.is_some()
            || bucket.limit_label.is_some()
            || bucket.used_money.is_some()
            || bucket.limit_money.is_some()
            || bucket.reset_label.is_some()
            || bucket.resets_at.is_some()
    });
    origin_affirmative || bucket_evidence
}

/// Select the required semantic glance bucket: Weekly for the six non-Amp
/// providers and Daily for Amp. Never a Spend/Session/min-remaining or label
/// match — one provider's missing slot yields `–`, never a whole-list failure.
pub(crate) fn glance_bucket(
    surface: HostSurfaceId,
    view: &FocusedUsageView,
) -> Option<&jackin_protocol::control::QuotaBucketView> {
    let slot = if surface == HostSurfaceId::Amp {
        jackin_protocol::control::StatusSlot::Daily
    } else {
        jackin_protocol::control::StatusSlot::Weekly
    };
    view.buckets
        .iter()
        .find(|bucket| bucket.status_slot == Some(slot))
}

pub(crate) fn build_provider_glance_row(
    surface: HostSurfaceId,
    view: &FocusedUsageView,
    is_updating: bool,
    now: i64,
    prefs: UsageFormatPrefs,
) -> HostProviderGlanceRow {
    use jackin_protocol::control::UsageSnapshotStatus as Status;
    let display_label = provider_display_label(surface.label()).to_owned();
    let glance = glance_bucket(surface, view);
    let (
        bar_label,
        headline,
        glance_remaining_percent,
        reset_label,
        compact_reset_label,
        exact_reset,
    ) = match glance.and_then(|bucket| bucket.remaining_percent) {
        Some(percent) => {
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
            (
                format!("{percent}%"),
                format!("{percent}% left"),
                Some(percent),
                reset_label,
                compact_reset_label,
                exact_reset,
            )
        }
        None => ("–".to_owned(), "–".to_owned(), None, None, None, None),
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
        glance_remaining_percent,
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

pub(crate) fn account_descriptor(
    surface: HostSurfaceId,
    entry: &accounts::AccountCatalogEntry,
    selected: bool,
    now: i64,
    prefs: UsageFormatPrefs,
) -> HostAccountDescriptor {
    use jackin_protocol::control::UsageSnapshotStatus as Status;

    let view = &entry.view;
    let bucket = glance_bucket(surface, view).or_else(|| {
        view.buckets
            .iter()
            .filter(|bucket| bucket.remaining_percent.is_some())
            .min_by_key(|bucket| bucket.remaining_percent)
    });
    let remaining_percent = bucket.and_then(|bucket| bucket.remaining_percent);
    let (remaining_label, headline) = remaining_percent.map_or_else(
        || ("—".to_owned(), "—".to_owned()),
        |percent| (format!("{percent}%"), percent_headline(percent, prefs)),
    );
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
        remaining_percent,
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

pub(crate) fn worst_severity_label(view: &FocusedUsageView) -> String {
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
