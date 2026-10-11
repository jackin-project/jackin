// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Bucket quota states.

use jackin_protocol::control::{QuotaBucketView, UsageSeverity, UsageSnapshotStatus};
use jackin_protocol::usage_broker::{UsageCoordinationErrorKind, UsageQuotaStateV1};

/// Raw used percentage for money-backed quota windows. The broker publisher
/// must preserve overage just like the desktop projection; only bar geometry
/// is clamped.
pub fn money_used_raw_percent(bucket: &QuotaBucketView) -> Option<i32> {
    let used = bucket.used_money.as_ref()?;
    let limit = bucket.limit_money.as_ref()?;
    if used.currency != limit.currency || used.exponent != limit.exponent || limit.amount_minor <= 0
    {
        return None;
    }
    let scaled = used.amount_minor.saturating_mul(100);
    let raw = scaled.checked_div(limit.amount_minor)?;
    Some(i32::try_from(raw).unwrap_or(if raw < 0 { i32::MIN } else { i32::MAX }))
}

pub fn quota_state_for_bucket(bucket: &QuotaBucketView) -> UsageQuotaStateV1 {
    match bucket.status {
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale => {
            if bucket.remaining_percent == Some(0) || money_is_exhausted(bucket) {
                UsageQuotaStateV1::Exhausted
            } else {
                match bucket.severity {
                    UsageSeverity::Danger => UsageQuotaStateV1::Exhausted,
                    UsageSeverity::Warn => UsageQuotaStateV1::Warning,
                    UsageSeverity::Normal => {
                        if bucket_has_quantity(bucket) {
                            UsageQuotaStateV1::Available
                        } else {
                            UsageQuotaStateV1::Unknown
                        }
                    }
                }
            }
        }
        UsageSnapshotStatus::NeedsLogin | UsageSnapshotStatus::NeedsSecret => {
            UsageQuotaStateV1::NoPermission
        }
        UsageSnapshotStatus::Unsupported => UsageQuotaStateV1::Unsupported,
        UsageSnapshotStatus::Unavailable => UsageQuotaStateV1::Unavailable,
        UsageSnapshotStatus::Error => UsageQuotaStateV1::Error,
    }
}

pub(crate) fn money_is_exhausted(bucket: &QuotaBucketView) -> bool {
    match (bucket.used_money.as_ref(), bucket.limit_money.as_ref()) {
        (Some(used), Some(limit)) => {
            used.currency == limit.currency
                && used.exponent == limit.exponent
                && limit.amount_minor > 0
                && used.amount_minor >= limit.amount_minor
        }
        _ => false,
    }
}

pub(crate) fn bucket_has_quantity(bucket: &QuotaBucketView) -> bool {
    bucket.remaining_percent.is_some()
        || bucket.used_money.is_some()
        || bucket.limit_money.is_some()
        || bucket.used_label.is_some()
        || bucket.limit_label.is_some()
}

pub fn issue_code(kind: UsageCoordinationErrorKind) -> String {
    match kind {
        UsageCoordinationErrorKind::Unavailable => "unavailable",
        UsageCoordinationErrorKind::Unauthorized => "unauthorized",
        UsageCoordinationErrorKind::CatalogRevoked => "catalog_revoked",
        UsageCoordinationErrorKind::CatalogRevisionConflict => "catalog_revision_conflict",
        UsageCoordinationErrorKind::OwnerLost => "owner_lost",
        UsageCoordinationErrorKind::WaitTimeout => "wait_timeout",
        UsageCoordinationErrorKind::CorruptState => "corrupt_state",
        UsageCoordinationErrorKind::ProviderTimeout => "provider_timeout",
        UsageCoordinationErrorKind::ProviderUnavailable => "provider_unavailable",
        UsageCoordinationErrorKind::NeedsSecret => "needs_secret",
        UsageCoordinationErrorKind::RateLimited => "rate_limited",
        UsageCoordinationErrorKind::ProtocolMismatch => "protocol_mismatch",
        UsageCoordinationErrorKind::BrokerConflict => "broker_conflict",
    }
    .to_owned()
}
