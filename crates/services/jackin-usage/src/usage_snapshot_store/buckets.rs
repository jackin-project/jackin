// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(test)]
//! Bucket ordering and label mapping.

use std::collections::HashMap;

use jackin_protocol::control::{UsageConfidence, UsageSnapshotStatus, UsageSource};

use super::StoredAccountUsageSnapshot;

pub(crate) fn usage_bucket_order(provider: &str, label: &str) -> usize {
    let provider = normalize_provider_label(provider);
    let order: &[&str] = if provider_matches("openai", &provider)
        || provider_matches("codex", &provider)
    {
        &[
            "Session",
            "Weekly",
            "Codex Spark 5-hour",
            "Codex Spark Weekly",
            "Limit Reset Credits",
            "Credits",
        ]
    } else if provider_matches("anthropic", &provider) || provider_matches("claude", &provider) {
        &[
            "Session",
            "Weekly",
            "All models",
            "Sonnet",
            "Daily Routines",
        ]
    } else if provider_matches("amp", &provider) {
        &["Amp Free", "Credits", "Individual credits"]
    } else if provider_matches("zai", &provider) || provider_matches("glm", &provider) {
        // F9: short/active window first (operator override of CodexBar's
        // Tokens, MCP, 5-hour order).
        &["5-hour", "Tokens", "MCP"]
    } else if provider_matches("kimi", &provider) {
        // F10: rate (short/active) window on top, then Weekly (operator
        // override of CodexBar's Weekly, Rate Limit order).
        &["Rate Limit", "Weekly"]
    } else if provider_matches("minimax", &provider) {
        &["General · 5h", "General · Weekly", "Video"]
    } else {
        &[]
    };
    order
        .iter()
        .position(|entry| provider_matches(entry, label))
        .unwrap_or(order.len())
}

pub(crate) fn select_provider_rows(
    rows: Vec<StoredAccountUsageSnapshot>,
    focused_provider: Option<&str>,
) -> Option<(String, Vec<StoredAccountUsageSnapshot>)> {
    let focused = focused_provider.unwrap_or_default();
    let mut matches = rows
        .into_iter()
        .filter(|row| provider_matches(focused, &row.provider))
        .collect::<Vec<_>>();
    if matches.is_empty() {
        return None;
    }
    let latest = matches.iter().map(|row| row.fetched_at).max()?;
    matches.retain(|row| row.fetched_at == latest);
    let provider = matches.first()?.provider.clone();
    Some((provider, matches))
}

pub(crate) fn provider_matches(needle: &str, provider: &str) -> bool {
    if needle.trim().is_empty() {
        return false;
    }
    let needle = normalize_provider_label(needle);
    let provider = normalize_provider_label(provider);
    provider.contains(&needle)
        || needle.contains(&provider)
        || (needle.contains("openai") && provider.contains("codex"))
        || (needle.contains("codex") && provider.contains("openai"))
        || (needle.contains("anthropic") && provider.contains("claude"))
        || (needle.contains("claude") && provider.contains("anthropic"))
        || (needle.contains("xai") && provider.contains("grok"))
        || (needle.contains("grok") && provider.contains("xai"))
        || (needle.contains("zai") && provider.contains("glm"))
        || (needle.contains("glm") && provider.contains("zai"))
}

pub(crate) fn normalize_provider_label(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

pub(crate) fn usage_provider_tabs_from_rows(
    rows: &[StoredAccountUsageSnapshot],
) -> Vec<jackin_protocol::control::UsageProviderTab> {
    // One tab per distinct stored account, keyed by the stable
    // `account_key_hash`; the newest fetch wins per account. Same-provider
    // accounts never collapse, and an empty store stays empty.
    let mut latest: HashMap<&str, &StoredAccountUsageSnapshot> = HashMap::new();
    for row in rows {
        latest
            .entry(row.account_key_hash.as_str())
            .and_modify(|current| {
                if row.fetched_at > current.fetched_at {
                    *current = row;
                }
            })
            .or_insert(row);
    }
    let mut tabs: Vec<jackin_protocol::control::UsageProviderTab> = latest
        .values()
        .map(|row| jackin_protocol::control::UsageProviderTab {
            id: row.account_key_hash.clone(),
            label: crate::usage::account_tab_label_for_parts(
                &row.provider,
                &row.account_label,
                row.focused_provider.as_deref(),
            ),
            status_label: tab_status_label(row, rows),
            account_label: row.account_label.clone(),
            plan_label: row.plan_label.clone(),
            source_label: Some(format!("{} · {}", row.view_status, row.source)),
            active: false,
        })
        .collect();
    tabs.sort_by(|left, right| {
        left.label
            .cmp(&right.label)
            .then(left.account_label.cmp(&right.account_label))
            .then(left.id.cmp(&right.id))
    });
    tabs
}

pub(crate) fn tab_status_label(
    row: &StoredAccountUsageSnapshot,
    rows: &[StoredAccountUsageSnapshot],
) -> String {
    rows.iter()
        .filter(|candidate| {
            candidate.provider == row.provider
                && candidate.account_key_hash == row.account_key_hash
                && candidate.fetched_at == row.fetched_at
        })
        .find_map(|candidate| {
            candidate
                .remaining_percent
                .map(|remaining| format!("{} {remaining}% left", candidate.window_kind))
        })
        .unwrap_or_else(|| row.view_status.clone())
}

pub(crate) fn usage_status_from_label(label: &str) -> UsageSnapshotStatus {
    match label {
        "fresh" => UsageSnapshotStatus::Fresh,
        "stale" => UsageSnapshotStatus::Stale,
        "needs_login" => UsageSnapshotStatus::NeedsLogin,
        "needs_secret" => UsageSnapshotStatus::NeedsSecret,
        "unsupported" => UsageSnapshotStatus::Unsupported,
        "error" => UsageSnapshotStatus::Error,
        _ => UsageSnapshotStatus::Unavailable,
    }
}

pub(crate) fn usage_source_from_label(label: &str) -> UsageSource {
    match label {
        "provider_api" => UsageSource::ProviderApi,
        "cli" => UsageSource::Cli,
        "local_logs" => UsageSource::LocalLogs,
        "cache" => UsageSource::Cache,
        _ => UsageSource::None,
    }
}

pub(crate) fn usage_confidence_from_label(label: &str) -> UsageConfidence {
    match label {
        "authoritative" => UsageConfidence::Authoritative,
        "estimated" => UsageConfidence::Estimated,
        "presence_only" => UsageConfidence::PresenceOnly,
        _ => UsageConfidence::None,
    }
}

pub(crate) fn lifecycle_status_bar_label(status: UsageSnapshotStatus) -> String {
    match status {
        UsageSnapshotStatus::Fresh => "usage cached",
        UsageSnapshotStatus::Stale => "stale",
        UsageSnapshotStatus::NeedsLogin => "needs login",
        UsageSnapshotStatus::NeedsSecret => "needs secret",
        UsageSnapshotStatus::Unsupported => "unsupported",
        UsageSnapshotStatus::Unavailable => "usage unavailable",
        UsageSnapshotStatus::Error => "error",
    }
    .to_owned()
}
