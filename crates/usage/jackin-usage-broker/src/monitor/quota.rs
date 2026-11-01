// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{PercentObservationOrigin, RawPercentPoints};

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

    if money_used.is_some_and(|money| money.get() > 100.0) {
        return money_used;
    }

    if remaining_origin == Some(PercentObservationOrigin::ProviderReported) {
        return remaining.and_then(|remaining| RawPercentPoints::new(100.0 - remaining.get()).ok());
    }

    if money_used.is_some() {
        return money_used;
    }

    used.or_else(|| {
        remaining.and_then(|remaining| RawPercentPoints::new(100.0 - remaining.get()).ok())
    })
}

pub(super) fn monitor_basis_points(percent: RawPercentPoints) -> Option<i32> {
    let points = percent.get();
    if !points.is_finite() || points < 0.0 {
        return None;
    }
    Some((points.min(100.0) * 100.0).round() as i32)
}
