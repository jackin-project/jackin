// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Codex` identity and profile snapshots.

use super::super::*;
use jackin_usage_provider_core::{
    CODEX_HANDOFF_AUTH_PATH, ProviderError, ProviderRateLimit, UsageSurface, UsageViewInput,
    bucket, env_dir_or_home, humanize_words_with, read_json_file, split_provider_fetch,
    titlecase_ascii, usage_error_is_unauthorized, usage_view,
};

use super::{
    CodexOAuthCredentials, codex_oauth_from_value, fetch_codex_oauth_reset_credits,
    fetch_codex_oauth_usage,
};

/// Codex auth credential candidates (home auth first, forwarded handoff last) —
/// shared by `codex_snapshot` and `codex_account_identity`.
pub(crate) fn codex_auth_candidates(codex_home: &Path) -> [PathBuf; 2] {
    [
        codex_home.join("auth.json"),
        PathBuf::from(CODEX_HANDOFF_AUTH_PATH),
    ]
}

/// Codex account identity (`account_id`, else the account label) from the same
/// auth candidates `codex_snapshot` uses, without fetching usage.
pub(crate) fn codex_account_identity() -> Option<String> {
    let codex_home = env_dir_or_home("CODEX_HOME", ".codex");
    codex_auth_candidates(&codex_home).iter().find_map(|path| {
        let creds = codex_oauth_from_value(&read_json_file(path)?)?;
        creds.account_id.or(creds.account_label)
    })
}

/// Map a Codex/`ChatGPT` `plan_type` to its display name, mirroring `CodexBar`'s
/// `CodexPlanFormatting.displayName` (F7a): `pro` → `Pro 20x`, the pro-lite
/// variants → `Pro 5x`, machine identifiers humanized (`enterprise_cbp_usage_based`
/// → `Enterprise CBP Usage Based`), already-readable text preserved. Returns
/// `None` for blank input so an unknown plan is omitted, never shown as `pro`.
pub(crate) fn codex_plan_display_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(exact) = codex_plan_exact_display(trimmed) {
        return Some(exact);
    }
    // Strip boilerplate words (claude/codex/account/plan) the way CodexBar's
    // `UsageFormatter.cleanPlanName` does, then re-check the exact map.
    let cleaned = trimmed
        .split_whitespace()
        .filter(|word| {
            !matches!(
                word.to_ascii_lowercase().as_str(),
                "claude" | "codex" | "account" | "plan"
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return Some(trimmed.to_owned());
    }
    if let Some(exact) = codex_plan_exact_display(cleaned) {
        return Some(exact);
    }
    let formatted = humanize_words_with(cleaned, codex_plan_word_display);
    if formatted.is_empty() {
        return Some(cleaned.to_owned());
    }
    Some(formatted)
}

pub(crate) fn codex_plan_exact_display(value: &str) -> Option<String> {
    match value.to_ascii_lowercase().as_str() {
        "pro" => Some("Pro 20x".to_owned()),
        "prolite" | "pro_lite" | "pro-lite" | "pro lite" => Some("Pro 5x".to_owned()),
        _ => None,
    }
}

pub(crate) fn codex_plan_word_display(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if matches!(lower.as_str(), "cbp" | "k12") {
        return lower.to_ascii_uppercase();
    }
    // Preserve existing acronyms (all-caps with a letter, e.g. "AI").
    if raw == raw.to_ascii_uppercase() && raw.chars().any(char::is_alphabetic) {
        return raw.to_owned();
    }
    titlecase_ascii(raw)
}

/// Read-only explicit-profile probe used by host discovery. It intentionally
/// does not refresh or rewrite `auth.json`; the agent remains credential owner.
pub(crate) fn codex_profile_snapshot(
    agent: &str,
    credentials: &CodexOAuthCredentials,
    codex_home: &Path,
    now: i64,
) -> FocusedUsageView {
    codex_profile_snapshot_with_rate_limit(agent, credentials, codex_home, now).0
}

pub(crate) fn codex_profile_snapshot_with_rate_limit(
    agent: &str,
    credentials: &CodexOAuthCredentials,
    codex_home: &Path,
    now: i64,
) -> (FocusedUsageView, Option<ProviderRateLimit>) {
    // Same reset-credits merge as the ambient lane: the read-only GET must not
    // gate the quota — a failure degrades to no "Limit Reset Credits" row.
    let (quota, error) = split_provider_fetch(Some(
        fetch_codex_oauth_usage(credentials, codex_home)
            .map_err(ProviderError::from)
            .map(|mut usage| {
                usage.reset_credits = fetch_codex_oauth_reset_credits(credentials, codex_home)
                    .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::HttpError)
                    .ok();
                usage
            }),
    ));
    let error_message = error.as_ref().map(|error| error.message().to_owned());
    let status = if quota.is_some() {
        UsageSnapshotStatus::Fresh
    } else if error.as_ref().is_some_and(usage_error_is_unauthorized) {
        UsageSnapshotStatus::NeedsLogin
    } else {
        UsageSnapshotStatus::Stale
    };
    let rate_limit = (quota.is_none())
        .then_some(error.as_ref())
        .flatten()
        .and_then(ProviderError::rate_limit);
    let buckets = quota
        .as_ref()
        .map(|usage| usage.buckets(now))
        .filter(|buckets| !buckets.is_empty())
        .unwrap_or_else(|| {
            vec![
                bucket(
                    "Session",
                    None,
                    None,
                    None,
                    None,
                    error_message.as_deref().or(Some("provider quota pending")),
                    status,
                ),
                bucket(
                    "Weekly",
                    None,
                    None,
                    None,
                    None,
                    error_message.as_deref().or(Some("provider quota pending")),
                    status,
                ),
            ]
        });
    let view = usage_view(UsageViewInput {
        agent,
        provider: Some("OpenAI"),
        surface: UsageSurface::Codex,
        account_label: credentials.account_label.clone().unwrap_or_default(),
        username: None,
        plan_label: quota
            .as_ref()
            .and_then(|usage| usage.plan_type.as_deref())
            .and_then(codex_plan_display_name),
        credential_origin: Some("OAuth · configured profile".to_owned()),
        buckets,
        status,
        source: if quota.is_some() {
            UsageSource::ProviderApi
        } else {
            UsageSource::None
        },
        confidence: if quota.is_some() {
            UsageConfidence::Authoritative
        } else {
            UsageConfidence::None
        },
        now,
        last_error: error_message,
    });
    (view, rate_limit)
}
