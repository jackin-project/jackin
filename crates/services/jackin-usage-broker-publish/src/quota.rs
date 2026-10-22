// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    PercentObservationOrigin, ProviderQuotaState, QuotaBucketView, UsageSeverity,
    UsageSnapshotStatus,
};
use jackin_protocol::usage_broker::{UsageCoordinationErrorKind, UsageQuotaStateV1};
use jackin_usage_provider_core::{money_used_percent, quota_bucket_has_observation};

pub fn quota_state_for_bucket(bucket: &QuotaBucketView) -> UsageQuotaStateV1 {
    match bucket.status {
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale => {
            match bucket.provider_quota_state {
                Some(ProviderQuotaState::Exhausted) => return UsageQuotaStateV1::Exhausted,
                Some(ProviderQuotaState::Unlimited) => return UsageQuotaStateV1::NotApplicable,
                None => {}
            }

            // Exact count is the authority for a count quota, including when
            // independently reported percentage data conflicts with it.
            if let Some(count) = bucket.count_quota {
                return match count.remaining {
                    Some(0) => UsageQuotaStateV1::Exhausted,
                    Some(_) if bucket.severity == UsageSeverity::Warn => UsageQuotaStateV1::Warning,
                    Some(_) => UsageQuotaStateV1::Available,
                    None => UsageQuotaStateV1::Unknown,
                };
            }

            let has_valid_money = money_used_percent(bucket).is_some();
            let provider_reported_exhaustion = bucket
                .remaining_percent
                .is_some_and(|remaining| remaining.get() == 0.0)
                && bucket.remaining_percent_origin
                    == Some(PercentObservationOrigin::ProviderReported);
            let unverified_zero_without_money = bucket
                .remaining_percent
                .is_some_and(|remaining| remaining.get() == 0.0)
                && bucket.remaining_percent_origin
                    != Some(PercentObservationOrigin::ProviderReported)
                && !has_valid_money;
            if money_is_exhausted(bucket)
                || provider_reported_exhaustion
                || unverified_zero_without_money
            {
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

pub fn money_is_exhausted(bucket: &QuotaBucketView) -> bool {
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

pub fn bucket_has_quantity(bucket: &QuotaBucketView) -> bool {
    quota_bucket_has_observation(bucket)
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

#[cfg(test)]
mod tests;
