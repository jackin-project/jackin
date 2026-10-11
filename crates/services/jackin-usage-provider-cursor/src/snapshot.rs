// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Cursor` snapshot entry points.

use super::{
    CursorAuth, CursorEnterpriseScope, cursor_auth_from_value, cursor_cli_identity_from_value,
    cursor_credits_bucket, cursor_dashboard_base, cursor_default_base,
    cursor_needs_request_fallback, cursor_period_buckets, cursor_request_bucket,
    cursor_sand_bucket, cursor_summary_buckets, cursor_team_spend_buckets,
    fetch_cursor_credit_grants, fetch_cursor_period_usage, fetch_cursor_plan_info,
    fetch_cursor_request_usage, fetch_cursor_sand_usage, fetch_cursor_stripe_balance,
    fetch_cursor_team_spend, fetch_cursor_usage_summary, load_cursor_auth,
    load_cursor_cli_identity,
};
use jackin_protocol::control::{
    FocusedUsageView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use jackin_usage_provider_core::{
    UsageSurface, UsageViewInput, bucket, read_json_file, split_fetch, usage_view,
};
use std::path::Path;

pub fn cursor_snapshot(agent: &str, provider: Option<&str>, now: i64) -> FocusedUsageView {
    let auth = match load_cursor_auth() {
        Ok(auth) => auth,
        Err(error) => {
            return cursor_status_view(
                agent,
                provider,
                now,
                UsageSnapshotStatus::NeedsSecret,
                &error,
            );
        }
    };
    cursor_snapshot_with_auth(
        agent,
        provider,
        &auth,
        load_cursor_cli_identity().as_deref(),
        "OAuth · ~/.cursor/auth.json",
        &cursor_dashboard_base(),
        now,
    )
}

/// Broker-refresh entry: `auth.json` at a registered profile root plus the
/// sibling `cli-config.json` identity. Never touches the default home.
pub fn cursor_profile_snapshot(agent: &str, auth_path: &Path, now: i64) -> FocusedUsageView {
    let auth = match read_json_file(auth_path)
        .ok_or_else(|| "Cursor auth.json is missing or unreadable".to_owned())
        .and_then(|value| {
            cursor_auth_from_value(&value)
                .ok_or_else(|| "Cursor access token is missing".to_owned())
        }) {
        Ok(auth) => auth,
        Err(error) => {
            return cursor_status_view(agent, None, now, UsageSnapshotStatus::NeedsSecret, &error);
        }
    };
    let identity = auth_path
        .parent()
        .and_then(|root| read_json_file(&root.join("cli-config.json")))
        .and_then(|value| cursor_cli_identity_from_value(&value));
    cursor_snapshot_with_auth(
        agent,
        None,
        &auth,
        identity.as_deref(),
        "OAuth · configured profile",
        &cursor_dashboard_base(),
        now,
    )
}

/// Personal snapshot from broker-minted material: the selected profile's
/// token, identity, and origin — never ambient files. The dashboard base is
/// explicit so hermetic tests can point the RPC at a dead port.
pub fn cursor_snapshot_with_auth(
    agent: &str,
    provider: Option<&str>,
    auth: &CursorAuth,
    identity: Option<&str>,
    credential_origin: &str,
    dashboard_base: &str,
    now: i64,
) -> FocusedUsageView {
    let token = auth.access_token.as_str();
    let (period, period_error) =
        split_fetch(Some(fetch_cursor_period_usage(dashboard_base, token)));
    let (plan, plan_error) = split_fetch(Some(fetch_cursor_plan_info(dashboard_base, token)));
    let (grants, grants_error) =
        split_fetch(Some(fetch_cursor_credit_grants(dashboard_base, token)));
    let (sand, sand_error) = split_fetch(Some(fetch_cursor_sand_usage(dashboard_base, token)));
    // Session-REST enrichment only for OAuth-file auth against the default base.
    let rest = auth.user_id.as_deref().filter(|_| cursor_default_base());
    let (summary, summary_error) =
        split_fetch(rest.map(|user| fetch_cursor_usage_summary(user, token)));
    let needs_requests = period.as_ref().is_none_or(cursor_needs_request_fallback);
    let (requests, requests_error) = split_fetch(
        rest.filter(|_| needs_requests)
            .map(|user| fetch_cursor_request_usage(user, token)),
    );
    let (stripe, stripe_error) =
        split_fetch(rest.map(|user| fetch_cursor_stripe_balance(user, token)));

    // Primary quota first (RPC wins; summary fills gaps, never duplicates the
    // same cycle meter), then requests, credits, and the Grok Bot meter.
    let mut buckets = Vec::new();
    if let Some(usage) = &period {
        buckets.extend(cursor_period_buckets(
            usage,
            summary.as_ref().and_then(|summary| summary.cycle_end),
            now,
        ));
    }
    if let Some(summary) = &summary {
        let has_cycle = buckets.iter().any(|bucket| bucket.label == "Billing cycle");
        buckets.extend(
            cursor_summary_buckets(summary, now)
                .into_iter()
                .filter(|bucket| bucket.label != "Billing cycle" || !has_cycle),
        );
    }
    if let Some(requests) = &requests {
        buckets.push(cursor_request_bucket(requests));
    }
    if grants.is_some() || stripe.is_some() {
        buckets.push(cursor_credits_bucket(
            grants.unwrap_or(0),
            stripe.unwrap_or(0),
        ));
    }
    if let Some(sand) = sand.as_ref().and_then(|sand| sand.as_ref()) {
        buckets.push(cursor_sand_bucket(sand, now));
    }
    if buckets.is_empty() {
        buckets.push(bucket(
            "Billing cycle",
            None,
            None,
            None,
            None,
            period_error
                .as_deref()
                .or(Some("Cursor dashboard unavailable")),
            UsageSnapshotStatus::Stale,
        ));
    }
    let status = if period.is_some() || summary.is_some() || requests.is_some() {
        UsageSnapshotStatus::Fresh
    } else {
        UsageSnapshotStatus::Stale
    };
    // Partial enrichment failures stay visible without erasing good quota.
    let mut failures = Vec::new();
    failures.extend(
        [
            period_error,
            plan_error,
            grants_error,
            sand_error,
            summary_error,
            requests_error,
            stripe_error,
        ]
        .into_iter()
        .flatten(),
    );
    let plan_label = plan.flatten().map(|plan| {
        if period.as_ref().is_some_and(|usage| usage.is_team) {
            format!("{plan} · Team")
        } else {
            plan
        }
    });
    let identity = identity.unwrap_or_default();
    usage_view(UsageViewInput {
        agent,
        provider: provider.or(Some("Cursor")),
        surface: UsageSurface::Cursor,
        account_label: identity.to_owned(),
        username: (!identity.is_empty()).then(|| identity.to_owned()),
        plan_label,
        credential_origin: Some(credential_origin.to_owned()),
        buckets,
        status,
        source: if status == UsageSnapshotStatus::Fresh {
            UsageSource::ProviderApi
        } else {
            UsageSource::None
        },
        confidence: if status == UsageSnapshotStatus::Fresh {
            UsageConfidence::Authoritative
        } else {
            UsageConfidence::None
        },
        now,
        last_error: (!failures.is_empty()).then(|| failures.join("; ")),
    })
}

pub fn cursor_enterprise_snapshot(
    agent: &str,
    provider: Option<&str>,
    scope: &CursorEnterpriseScope,
    now: i64,
) -> FocusedUsageView {
    // Usage events are hourly aggregated: the overview reads team spend only,
    // never the events endpoint.
    let (spend, spend_error) = split_fetch(Some(fetch_cursor_team_spend(scope)));
    let mut buckets = spend
        .as_ref()
        .map(|spend| cursor_team_spend_buckets(spend, now))
        .unwrap_or_default();
    if buckets.is_empty() {
        buckets.push(bucket(
            "Team spend (actual)",
            None,
            None,
            None,
            None,
            spend_error
                .as_deref()
                .or(Some("Cursor Admin API unavailable")),
            UsageSnapshotStatus::Stale,
        ));
    }
    let status = if spend.is_some() {
        UsageSnapshotStatus::Fresh
    } else {
        UsageSnapshotStatus::Stale
    };
    usage_view(UsageViewInput {
        agent,
        provider: provider.or(Some("Cursor")),
        surface: UsageSurface::Cursor,
        account_label: String::new(),
        username: None,
        plan_label: Some("Cursor Enterprise".to_owned()),
        credential_origin: Some("Admin API · explicit team scope".to_owned()),
        buckets,
        status,
        source: if status == UsageSnapshotStatus::Fresh {
            UsageSource::ProviderApi
        } else {
            UsageSource::None
        },
        confidence: if status == UsageSnapshotStatus::Fresh {
            UsageConfidence::Authoritative
        } else {
            UsageConfidence::None
        },
        now,
        last_error: match status {
            UsageSnapshotStatus::Fresh => None,
            _ => spend_error,
        },
    })
}

pub(crate) fn cursor_status_view(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    status: UsageSnapshotStatus,
    error: &str,
) -> FocusedUsageView {
    usage_view(UsageViewInput {
        agent,
        provider: provider.or(Some("Cursor")),
        surface: UsageSurface::Cursor,
        account_label: String::new(),
        username: None,
        plan_label: None,
        credential_origin: Some("needs ~/.cursor/auth.json".to_owned()),
        buckets: vec![bucket(
            "Billing cycle",
            None,
            None,
            None,
            None,
            Some(error),
            status,
        )],
        status,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        now,
        last_error: Some(error.to_owned()),
    })
}
