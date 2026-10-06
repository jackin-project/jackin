// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Kimi` snapshot entry point and identity.

use super::super::{
    FocusedUsageView, ProviderPresence, UsageSnapshotStatus, UsageSurface, UsageViewInput, bucket,
    home_path, humanize_plan_label, provider_outcome, split_fetch, usage_view,
};

use super::{KimiUsageResponse, fetch_kimi_usage};

pub(crate) fn kimi_snapshot(agent: &str, token: Option<&str>, now: i64) -> FocusedUsageView {
    let has_local = home_path(".kimi-code").exists() || home_path(".kimi").exists();
    let has_token = token.is_some_and(|value| !value.is_empty());
    let (provider_usage, provider_error) = split_fetch(token.map(fetch_kimi_usage));
    let (status, source, confidence) = provider_outcome(ProviderPresence {
        has_data: provider_usage.is_some(),
        has_secret: has_token || has_local,
    });
    let buckets = provider_usage
        .as_ref()
        .map(|usage| usage.buckets(now))
        .filter(|buckets| !buckets.is_empty())
        .unwrap_or_else(|| {
            vec![
                bucket(
                    "Weekly",
                    None,
                    None,
                    None,
                    None,
                    provider_error
                        .as_deref()
                        .or(Some("Kimi billing endpoint unavailable")),
                    status,
                ),
                bucket(
                    "5-hour rate limit",
                    None,
                    None,
                    None,
                    None,
                    provider_error
                        .as_deref()
                        .or(Some("Kimi billing endpoint unavailable")),
                    status,
                ),
            ]
        });
    // One Kimi billing identity can fund the Kimi, Claude and Codex clients;
    // surface the stable subject (email/id) plus membership plan so the broker
    // can dedup observations by (service + billing subject), never by client.
    let (account_label, username, plan_label) = provider_usage
        .as_ref()
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
        buckets,
        status,
        source,
        confidence,
        now,
        last_error: match status {
            UsageSnapshotStatus::NeedsSecret => {
                Some("Kimi auth not available to Capsule".to_owned())
            }
            UsageSnapshotStatus::Unsupported => Some(provider_error.unwrap_or_else(|| {
                "Kimi billing endpoint unavailable; local presence only".to_owned()
            })),
            _ => None,
        },
    })
}

/// Stable billing identity shared across every client one Kimi account funds:
/// `(account_label, username, plan_label)`.
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

/// Map a `user.membership.level` to its display plan. The `LEVEL_*` →
/// tempo-name mapping is only valid when `version` is absent or
/// `GOODS_VERSION_V1`; any other goods version passes the raw level through
/// humanized rather than risking a wrong plan name.
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
