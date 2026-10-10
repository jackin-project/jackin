// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Usage time and freshness labels.

use super::UsageMetricGroup;

use jackin_protocol::usage_broker::{
    UsageFreshnessPhaseV1, UsageIdentityKindV1, UsageLifecycleV1, UsageMetricGroupKindV1,
    UsageQuotaStateV1, UsageWindowCategoryV1,
};

/// Lifecycle word for one account. The shared vocabulary (`needs login`,
/// `needs secret`, `unsupported`, `unavailable`, `error`) matches Capsule
/// `usage_tab_status_label`; `available`/`not started` have no Capsule
/// snapshot-status counterpart (Capsule says `fresh` for the freshness axis,
/// a different concept) and stay console-owned. See the alignment table test.
pub(crate) fn lifecycle_label(lifecycle: UsageLifecycleV1) -> &'static str {
    match lifecycle {
        UsageLifecycleV1::Available => "available",
        UsageLifecycleV1::AgentUninitialized => "not started",
        UsageLifecycleV1::NeedsLogin => "needs login",
        UsageLifecycleV1::NeedsSecret => "needs secret",
        UsageLifecycleV1::Unsupported => "unsupported",
        UsageLifecycleV1::Unavailable => "unavailable",
        UsageLifecycleV1::Error => "error",
    }
}

/// Quota-state word for one window/group. The Capsule tab vocabulary has no
/// quota axis (`usage_tab_status_label` reports snapshot status plus the
/// `{n}% left` headline, which the console mirrors in its list summary), so
/// these words stay console-owned and are pinned by the alignment table test.
pub(crate) fn quota_state_label(state: UsageQuotaStateV1) -> &'static str {
    match state {
        UsageQuotaStateV1::Available => "available",
        UsageQuotaStateV1::NotStarted => "not started",
        UsageQuotaStateV1::Warning => "warning",
        UsageQuotaStateV1::Exhausted => "exhausted",
        UsageQuotaStateV1::Unsupported => "unsupported",
        UsageQuotaStateV1::Unavailable => "unavailable",
        UsageQuotaStateV1::NoPermission => "no permission",
        UsageQuotaStateV1::Unknown => "unknown",
        UsageQuotaStateV1::NotApplicable => "n/a",
        UsageQuotaStateV1::Error => "error",
    }
}

/// Rank of one window category in the settled Overview-summary order (D30:
/// long-range weekly/daily/monthly, model-specific, session, then other).
pub(crate) const fn summary_category_rank(category: UsageWindowCategoryV1) -> u8 {
    match category {
        UsageWindowCategoryV1::LongRange => 0,
        UsageWindowCategoryV1::Model => 1,
        UsageWindowCategoryV1::Session => 2,
        UsageWindowCategoryV1::Other => 3,
    }
}

pub(crate) fn metric_group_kind_label(kind: UsageMetricGroupKindV1) -> &'static str {
    match kind {
        UsageMetricGroupKindV1::Window => "window",
        UsageMetricGroupKindV1::Balance => "balance",
        UsageMetricGroupKindV1::SpendCap => "spend cap",
        UsageMetricGroupKindV1::TokenTotals => "token totals",
        UsageMetricGroupKindV1::RateLimit => "rate limit",
        UsageMetricGroupKindV1::Plan => "plan",
    }
}

pub(crate) fn identity_kind_label(kind: UsageIdentityKindV1) -> &'static str {
    match kind {
        UsageIdentityKindV1::ProviderAccountId => "provider account id",
        UsageIdentityKindV1::ProviderStableHandle => "provider handle",
        UsageIdentityKindV1::LocalSourceHandle => "local source handle",
        UsageIdentityKindV1::UnverifiedHandle => "unverified handle",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_source_identity_label_does_not_claim_provider_identity() {
        assert_eq!(
            identity_kind_label(UsageIdentityKindV1::LocalSourceHandle),
            "local source handle"
        );
    }

    #[test]
    fn unverified_identity_label_does_not_claim_provider_or_local_identity() {
        assert_eq!(
            identity_kind_label(UsageIdentityKindV1::UnverifiedHandle),
            "unverified handle"
        );
    }
}

/// Past-age bucket shared by account and group freshness labels.
pub(crate) fn past_age_label(age_secs: i64) -> String {
    if age_secs < 60 {
        "just now".to_owned()
    } else if age_secs < 3_600 {
        format!("{}m ago", age_secs / 60)
    } else if age_secs < 86_400 {
        format!("{}h ago", age_secs / 3_600)
    } else {
        format!("{}d ago", age_secs / 86_400)
    }
}

/// `updated …` age fragment for freshness labels. The sub-minute bucket reads
/// `updated now`, matching Capsule `relative_updated_label` ("Updated now")
/// modulo the console's lowercase row style.
pub(crate) fn updated_age_label(age_secs: i64) -> String {
    if age_secs < 60 {
        "updated now".to_owned()
    } else {
        format!("updated {}", past_age_label(age_secs))
    }
}

/// Relative time for a reset/renewal/expiry/retry epoch against an explicit
/// `now`. Pure so tests stay deterministic; render passes wall-clock time.
/// Past mirrors the freshness buckets; future uses `in …` buckets.
pub(crate) fn relative_time_label(now_epoch: i64, epoch: i64) -> String {
    if epoch >= now_epoch {
        let ahead = epoch.saturating_sub(now_epoch);
        if ahead < 60 {
            "in under a minute".to_owned()
        } else if ahead < 3_600 {
            format!("in {}m", ahead / 60)
        } else if ahead < 86_400 {
            format!("in {}h", ahead / 3_600)
        } else {
            format!("in {}d", ahead / 86_400)
        }
    } else {
        past_age_label(now_epoch.saturating_sub(epoch))
    }
}

/// Credential/auth-session expiry as one relative fact. Derived only from the
/// canonical expiry epoch: already-passed reads `expired …`, future reads
/// `expires …`. Never confused with a quota reset or subscription renewal.
pub(crate) fn credential_expiry_label(now_epoch: i64, expires_at_epoch: i64) -> String {
    if expires_at_epoch < now_epoch {
        format!(
            "expired {}",
            past_age_label(now_epoch.saturating_sub(expires_at_epoch))
        )
    } else {
        format!(
            "expires {}",
            relative_time_label(now_epoch, expires_at_epoch)
        )
    }
}

/// Operator-facing freshness age for one metric group. Staleness is per
/// group: a fresh sibling never makes retained old data fresh.
#[must_use]
pub fn group_freshness_label(now_epoch: i64, group: &UsageMetricGroup) -> String {
    if group.phase == UsageFreshnessPhaseV1::Refreshing {
        return "refreshing…".to_owned();
    }
    let Some(last_success) = group.last_success_at_epoch else {
        return "never updated".to_owned();
    };
    let updated = updated_age_label(now_epoch.saturating_sub(last_success).max(0));
    if group.is_stale
        || matches!(
            group.phase,
            UsageFreshnessPhaseV1::Stale | UsageFreshnessPhaseV1::Failed
        )
    {
        format!("stale · {updated}")
    } else {
        updated
    }
}
