// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{PercentObservationOrigin, RawPercentPoints};

/// Preserve the monitor's historical source choice using unrounded percentage
/// points, then convert once at the existing basis-point boundary.
pub(super) fn monitor_window_used_points(
    window: &jackin_protocol::usage_broker::UsageLimitWindowV1,
) -> Option<RawPercentPoints> {
    monitor_source_used_points(
        window.used_percent,
        window.used_percent_origin,
        window.remaining_percent,
        window.remaining_percent_origin,
        window.money_used_percent,
    )
}

pub(super) fn monitor_source_used_points(
    used: Option<RawPercentPoints>,
    used_origin: Option<PercentObservationOrigin>,
    remaining: Option<RawPercentPoints>,
    remaining_origin: Option<PercentObservationOrigin>,
    money_used: Option<RawPercentPoints>,
) -> Option<RawPercentPoints> {
    if used_origin == Some(PercentObservationOrigin::ProviderReported) {
        return used;
    }

    // Preserve the monitor's longstanding overage behavior, which treats a
    // Money ratio above capacity as full usage even when another side exists.
    if money_used.is_some_and(|money| money.get() > 100.0) {
        return money_used;
    }

    if remaining_origin == Some(PercentObservationOrigin::ProviderReported) {
        return remaining.and_then(|remaining| RawPercentPoints::new(100.0 - remaining.get()).ok());
    }

    // Exact Money ratios take precedence over complementary geometry inferred
    // from derived or historical-unknown source percentages. In particular,
    // an old persisted remaining=0 must not hide an exact Money ratio of 1%.
    if money_used.is_some() {
        return money_used;
    }

    used.or_else(|| {
        remaining.and_then(|remaining| RawPercentPoints::new(100.0 - remaining.get()).ok())
    })
}

/// Round percentage points to the monitor's stable basis-point unit.
/// Negative percentages remain in the generic projection but are not monitor
/// evidence; overage retains the old clamp at 100 percent.
pub(super) fn monitor_basis_points(percent: RawPercentPoints) -> Option<i32> {
    let points = percent.get();
    if !points.is_finite() || points < 0.0 {
        return None;
    }
    Some((points.min(100.0) * 100.0).round() as i32)
}
