// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `omp` (oh-my-pi) attribution adapter.
//!
//! omp is a local aggregator CLI, not a billed provider: it has no single
//! identity or usage endpoint. Usage is attributed to underlying provider
//! accounts via per-provider collectors; native pool filters/counters are
//! routing state and must never become an independent subscription budget.
//! The auth-broker pool file (`OMP_AUTH_BROKER_ACCOUNT_POOL_FILE`) is
//! trusted-client routing, not server authorization, and must never gate
//! selected-accounts-only admission.

use super::{
    FocusedAccountHeader, FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSnapshotStatus,
    UsageSource, status_bar_quota_labels,
};

/// Usage attributed to one underlying provider account. The collector buckets
/// pass through unchanged — omp adds no budget of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OmpAttribution {
    pub(crate) provider: String,
    pub(crate) account_label: String,
    pub(crate) buckets: Vec<QuotaBucketView>,
}

pub(crate) fn omp_attribute(
    provider: &str,
    account_label: &str,
    buckets: Vec<QuotaBucketView>,
) -> OmpAttribution {
    OmpAttribution {
        provider: provider.to_owned(),
        account_label: account_label.to_owned(),
        buckets,
    }
}

/// Native pool routing state (filters/counters) is never quota: mapping it
/// always yields zero buckets, so no independent subscription budget can form
/// from omp-local counters.
pub(crate) fn omp_pool_routing_buckets(_routing: &serde_json::Value) -> Vec<QuotaBucketView> {
    Vec::new()
}

/// Broker pool-file environment variable: trusted-client routing only.
pub(crate) const OMP_AUTH_BROKER_ACCOUNT_POOL_FILE: &str = "OMP_AUTH_BROKER_ACCOUNT_POOL_FILE";

/// Always `false`: the broker pool file is routing, not authorization, and
/// must never gate selected-accounts-only admission.
pub(crate) fn omp_broker_pool_is_authorization() -> bool {
    false
}

pub(crate) fn omp_attributed_view(
    agent: &str,
    attribution: &OmpAttribution,
    now: i64,
) -> FocusedUsageView {
    let has_buckets = !attribution.buckets.is_empty();
    let status = if has_buckets {
        UsageSnapshotStatus::Fresh
    } else {
        UsageSnapshotStatus::Unavailable
    };
    let labels = status_bar_quota_labels(&attribution.buckets);
    let status_bar_label = if labels.is_empty() {
        if has_buckets {
            format!("{} via {agent}", attribution.provider)
        } else {
            "usage unavailable".to_owned()
        }
    } else {
        labels.join(" · ")
    };
    FocusedUsageView {
        focused_agent: Some(agent.to_owned()),
        focused_provider: Some(attribution.provider.clone()),
        account: FocusedAccountHeader {
            provider_label: attribution.provider.clone(),
            account_label: attribution.account_label.clone(),
            username: None,
            plan_label: None,
            credential_origin: Some("omp provider entry".to_owned()),
        },
        buckets: attribution.buckets.clone(),
        status,
        source: if has_buckets {
            UsageSource::ProviderApi
        } else {
            UsageSource::None
        },
        confidence: if has_buckets {
            UsageConfidence::Authoritative
        } else {
            UsageConfidence::None
        },
        fetched_at_epoch: now,
        updated_label: if status == UsageSnapshotStatus::Fresh {
            "Updated now"
        } else {
            "Unavailable"
        }
        .to_owned(),
        status_bar_label,
        tabs: Vec::new(),
        last_error: if has_buckets {
            None
        } else {
            Some("omp has no attributed provider usage".to_owned())
        },
    }
}

#[cfg(test)]
mod tests;
