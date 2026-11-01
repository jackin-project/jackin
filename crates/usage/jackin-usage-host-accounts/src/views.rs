// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    FocusedUsageView, MeterPercentPoints, QuotaBucketView, UsageMeterSide,
};

use super::CanonicalAccountIdentity;
use jackin_usage_host_presentation::HostSurfaceId;
use jackin_usage_provider_core::{quota_bucket_remaining_rank_points, select_quota_meter};

#[must_use]
pub fn account_key_for_view(view: &FocusedUsageView) -> Option<String> {
    let surface = surface_for_view(view)?;
    CanonicalAccountIdentity::from_view(surface, view).map(|identity| identity.account_key())
}

#[must_use]
pub fn canonical_account_id_for_view(view: &FocusedUsageView) -> Option<String> {
    let surface = surface_for_view(view)?;
    CanonicalAccountIdentity::from_view(surface, view).map(|identity| identity.canonical_id_v1())
}

#[must_use]
pub fn short_account_identity(account_label: &str) -> String {
    let Some(trimmed) = stable_account_label(account_label) else {
        return String::new();
    };
    if let Some((local, _)) = trimmed.split_once('@')
        && !local.is_empty()
    {
        return local.to_owned();
    }
    if trimmed.chars().count() > 12 {
        return trimmed.chars().take(10).collect::<String>() + "…";
    }
    trimmed.to_owned()
}

pub(crate) fn stable_account_label(account_label: &str) -> Option<&str> {
    let label = account_label.trim();
    if label.is_empty()
        || label.eq_ignore_ascii_case("account unavailable")
        || label.eq_ignore_ascii_case("unknown")
        || label.eq_ignore_ascii_case("current host login")
        || label.eq_ignore_ascii_case("local amp auth")
    {
        None
    } else {
        Some(label)
    }
}

#[must_use]
pub fn min_meter_fill(view: &FocusedUsageView) -> Option<MeterPercentPoints> {
    view.buckets
        .iter()
        .filter_map(|bucket| {
            let rank_points = quota_bucket_remaining_rank_points(bucket)?;
            let (meter, side) = select_quota_meter(bucket);
            meter.map(|meter| {
                let remaining = match side {
                    Some(UsageMeterSide::Used) => meter.complement(),
                    Some(UsageMeterSide::Remaining) | None => meter,
                };
                (rank_points, remaining)
            })
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, meter)| meter)
}

#[must_use]
pub fn summary_bucket(view: &FocusedUsageView) -> Option<&QuotaBucketView> {
    jackin_usage_provider_core::summary_bucket(&view.buckets)
}

#[must_use]
pub(crate) fn surface_for_view(view: &FocusedUsageView) -> Option<HostSurfaceId> {
    HostSurfaceId::from_provider_alias(&view.account.provider_label)
}
