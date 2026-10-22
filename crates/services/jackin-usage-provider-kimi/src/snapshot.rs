// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    FocusedUsageView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use jackin_usage_provider_core::{
    ProviderPresence, UsageSurface, UsageViewInput, bucket, home_path, humanize_plan_label,
    provider_outcome, split_fetch, usage_view,
};

use super::{KimiUsageResponse, fetch_kimi_usage};

pub(crate) struct KimiQuotaObservation {
    pub(crate) buckets: Vec<jackin_protocol::control::QuotaBucketView>,
    pub(crate) status: UsageSnapshotStatus,
    pub(crate) error: Option<String>,
}

pub(crate) fn kimi_quota_observation(
    usage: Option<&KimiUsageResponse>,
    provider_status: UsageSnapshotStatus,
    provider_error: Option<&str>,
    now: i64,
) -> KimiQuotaObservation {
    let Some(usage) = usage else {
        let message = provider_error.unwrap_or("Kimi billing endpoint unavailable");
        return KimiQuotaObservation {
            buckets: kimi_fallback_buckets(message, provider_status),
            status: provider_status,
            error: None,
        };
    };

    let mut buckets = usage.buckets(now);
    if buckets.is_empty() {
        let error = "Kimi quota response contained no quantitative observation";
        return KimiQuotaObservation {
            buckets: kimi_fallback_buckets(error, UsageSnapshotStatus::Unavailable),
            status: UsageSnapshotStatus::Unavailable,
            error: Some(error.to_owned()),
        };
    }

    let has_observation = buckets.iter().any(kimi_bucket_has_quantitative_observation);
    let diagnostics = buckets
        .iter()
        .filter_map(|bucket| bucket.pace_label.as_deref())
        .filter(|pace| pace.starts_with("Kimi quota window "))
        .collect::<Vec<_>>();
    let error = if diagnostics.is_empty() {
        None
    } else {
        Some(diagnostics.join("; "))
    };
    let status = if has_observation {
        provider_status
    } else {
        UsageSnapshotStatus::Unavailable
    };
    let error = if status == UsageSnapshotStatus::Unavailable && error.is_none() {
        Some("Kimi quota response contained no quantitative observation".to_owned())
    } else {
        error
    };
    if !has_observation && error.is_some() {
        for bucket in &mut buckets {
            bucket.status = UsageSnapshotStatus::Unavailable;
        }
    }
    KimiQuotaObservation {
        buckets,
        status,
        error,
    }
}

fn kimi_bucket_has_quantitative_observation(
    bucket: &jackin_protocol::control::QuotaBucketView,
) -> bool {
    bucket.status == UsageSnapshotStatus::Fresh
        && (bucket.used_percent.is_some()
            || bucket.remaining_percent.is_some()
            || bucket.count_quota.is_some_and(|quota| {
                quota.used.is_some() || quota.limit.is_some() || quota.remaining.is_some()
            }))
}

fn kimi_fallback_buckets(
    message: &str,
    status: UsageSnapshotStatus,
) -> Vec<jackin_protocol::control::QuotaBucketView> {
    ["Weekly", "5-hour rate limit"]
        .into_iter()
        .map(|label| bucket(label, None, None, None, None, Some(message), status))
        .collect()
}

pub fn kimi_snapshot(agent: &str, token: Option<&str>, now: i64) -> FocusedUsageView {
    let has_local = home_path(".kimi-code").exists() || home_path(".kimi").exists();
    let has_token = token.is_some_and(|value| !value.is_empty());
    let (provider_usage, provider_error) = split_fetch(token.map(fetch_kimi_usage));
    kimi_snapshot_from_usage(
        agent,
        provider_usage.as_ref(),
        provider_error.as_deref(),
        has_token,
        has_local,
        now,
    )
}

pub(crate) fn kimi_snapshot_from_usage(
    agent: &str,
    provider_usage: Option<&KimiUsageResponse>,
    provider_error: Option<&str>,
    has_token: bool,
    has_local: bool,
    now: i64,
) -> FocusedUsageView {
    let (provider_status, source, confidence) = provider_outcome(ProviderPresence {
        has_data: provider_usage.is_some(),
        has_secret: has_token || has_local,
    });
    let observation = kimi_quota_observation(provider_usage, provider_status, provider_error, now);
    let status = observation.status;
    let (source, confidence) = if provider_usage.is_some() && status != UsageSnapshotStatus::Fresh {
        (UsageSource::None, UsageConfidence::None)
    } else {
        (source, confidence)
    };
    let (account_label, username, plan_label) = provider_usage
        .map(kimi_account_identity)
        .unwrap_or_default();
    usage_view(UsageViewInput {
        agent,
        provider: None,
        surface: UsageSurface::Kimi,
        account_label,
        username,
        plan_label,
        credential_origin: Some(
            if has_token {
                "API token · env KIMI_CODE_API_KEY"
            } else if has_local {
                "API key · ~/.kimi-code"
            } else {
                "needs Kimi auth"
            }
            .to_owned(),
        ),
        buckets: observation.buckets,
        status,
        source,
        confidence,
        now,
        last_error: observation.error.or_else(|| match status {
            UsageSnapshotStatus::NeedsSecret => {
                Some("Kimi auth not available to Capsule".to_owned())
            }
            UsageSnapshotStatus::Unsupported => Some(provider_error.map_or_else(
                || "Kimi billing endpoint unavailable; local presence only".to_owned(),
                str::to_owned,
            )),
            _ => None,
        }),
    })
}

pub(crate) fn kimi_account_identity(
    usage: &KimiUsageResponse,
) -> (String, Option<String>, Option<String>) {
    let user = usage.user.as_ref();
    let email = user
        .and_then(|user| user.email.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let name = user
        .and_then(|user| user.name.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let id = user
        .and_then(|user| user.id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let account_label = email.or(name).or(id).unwrap_or_default().to_owned();
    let username = email.or(name).map(str::to_owned);
    let level = user
        .and_then(|user| user.membership.as_ref())
        .and_then(|membership| {
            membership
                .level
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        });
    let plan_label = level.map(|level| kimi_membership_plan(level, usage.version.as_deref()));
    (account_label, username, plan_label)
}

pub(crate) fn kimi_membership_plan(level: &str, version: Option<&str>) -> String {
    if version.is_some_and(|version| version != "GOODS_VERSION_V1") {
        return humanize_plan_label(&level.to_ascii_lowercase());
    }
    match level {
        "LEVEL_FREE" => "Adagio",
        "LEVEL_TRIAL" => "Andante",
        "LEVEL_BASIC" => "Moderato",
        "LEVEL_INTERMEDIATE" => "Allegretto",
        "LEVEL_ADVANCED" => "Allegro",
        other => {
            return humanize_plan_label(
                &other
                    .strip_prefix("LEVEL_")
                    .unwrap_or(other)
                    .to_ascii_lowercase(),
            );
        }
    }
    .to_owned()
}
