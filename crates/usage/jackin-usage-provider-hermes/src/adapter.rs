// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    FocusedAccountHeader, FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSnapshotStatus,
    UsageSource,
};
use jackin_usage_provider_core::{parse_iso_epoch, status_bar_quota_labels, timed_bucket};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HermesRuntime {
    pub profile: String,
    pub exclusive: bool,
}

impl HermesRuntime {
    pub fn for_profile(profile: &str) -> Self {
        Self {
            profile: profile.to_owned(),
            exclusive: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct HermesSubscriptionCurrentRaw {
    #[serde(default)]
    tier_name: Option<String>,
    #[serde(default)]
    monthly_credits: Option<String>,
    #[serde(default)]
    credits_remaining: Option<String>,
    #[serde(default)]
    cycle_ends_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct HermesSubscriptionRaw {
    #[serde(default)]
    current: Option<HermesSubscriptionCurrentRaw>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HermesSubscription {
    pub tier_name: Option<String>,
    pub monthly_credits: Option<String>,
    pub credits_remaining: Option<String>,
    pub cycle_ends_at: Option<i64>,
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

pub fn parse_hermes_subscription(
    value: serde_json::Value,
) -> Result<Option<HermesSubscription>, String> {
    let raw: HermesSubscriptionRaw = serde_json::from_value(value)
        .map_err(|_| "Hermes subscription response is malformed".to_owned())?;
    raw.current
        .map(|current| {
            let cycle_ends_at = current
                .cycle_ends_at
                .map(|iso| {
                    parse_iso_epoch(&iso)
                        .ok_or_else(|| "Hermes subscription cycle timestamp is invalid".to_owned())
                })
                .transpose()?;
            Ok(HermesSubscription {
                tier_name: non_empty(current.tier_name),
                monthly_credits: non_empty(current.monthly_credits),
                credits_remaining: non_empty(current.credits_remaining),
                cycle_ends_at,
            })
        })
        .transpose()
}

pub fn hermes_renews_label(cycle_ends_at: i64) -> Option<String> {
    let date = chrono::DateTime::from_timestamp(cycle_ends_at, 0)?;
    Some(format!("renews {}", date.format("%Y-%m-%d")))
}

pub fn hermes_subscription_bucket(subscription: &HermesSubscription, now: i64) -> QuotaBucketView {
    timed_bucket(
        "Credits",
        subscription
            .credits_remaining
            .as_ref()
            .map(|remaining| format!("{remaining} left")),
        subscription
            .monthly_credits
            .as_ref()
            .map(|monthly| format!("{monthly} total")),
        None,
        None,
        now,
        subscription
            .cycle_ends_at
            .and_then(hermes_renews_label)
            .as_deref(),
        UsageSnapshotStatus::Fresh,
    )
}

pub fn hermes_tracker_counter_buckets(_counters: &serde_json::Value) -> Vec<QuotaBucketView> {
    Vec::new()
}

pub fn hermes_auth_status(status_code: u16) -> UsageSnapshotStatus {
    match status_code {
        401 | 403 => UsageSnapshotStatus::NeedsLogin,
        _ => UsageSnapshotStatus::Error,
    }
}

pub fn hermes_view(
    agent: &str,
    runtime: &HermesRuntime,
    provider: &str,
    account_label: &str,
    underlying_buckets: &[QuotaBucketView],
    subscription: Option<&HermesSubscription>,
    now: i64,
) -> FocusedUsageView {
    let mut buckets = underlying_buckets.to_vec();
    if let Some(subscription) = subscription {
        buckets.push(hermes_subscription_bucket(subscription, now));
    }
    let has_data = !buckets.is_empty();
    let status = if has_data {
        UsageSnapshotStatus::Fresh
    } else {
        UsageSnapshotStatus::Unavailable
    };
    let labels = status_bar_quota_labels(&buckets);
    let status_bar_label = if labels.is_empty() {
        if has_data {
            format!("{provider} via {agent}")
        } else {
            "usage unavailable".to_owned()
        }
    } else {
        labels.join(" · ")
    };
    FocusedUsageView {
        focused_agent: Some(agent.to_owned()),
        focused_provider: Some(provider.to_owned()),
        account: FocusedAccountHeader {
            provider_label: provider.to_owned(),
            account_label: account_label.to_owned(),
            username: None,
            plan_label: subscription.and_then(|subscription| subscription.tier_name.clone()),
            credential_origin: Some(format!("Hermes profile '{}'", runtime.profile)),
            account_capability: None,
        },
        buckets,
        status,
        source: if has_data {
            UsageSource::ProviderApi
        } else {
            UsageSource::None
        },
        confidence: if has_data {
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
        last_error: if has_data {
            None
        } else {
            Some("Hermes has no attributed provider usage".to_owned())
        },
    }
}
