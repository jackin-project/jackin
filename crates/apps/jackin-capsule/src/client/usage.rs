// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Usage account listing and verification.

use super::request_control;
use anyhow::Result;

use crate::protocol::control::{AccountUsageSnapshotView, ClientMsg, ServerMsg};

/// # Errors
///
/// Returns an error when the usage broker request or JSON serialization fails.
pub async fn run_usage_accounts() -> Result<()> {
    let accounts = usage_accounts().await?;
    crate::output::stdout_line(format_args!("{}", serde_json::to_string_pretty(&accounts)?));
    Ok(())
}

/// # Errors
///
/// Returns an error when the usage broker request fails or any account check
/// reports a failure.
pub async fn run_usage_verify() -> Result<()> {
    let accounts = usage_accounts().await?;
    let checks = verify_usage_accounts(&accounts);
    for check in &checks {
        crate::output::stdout_line(format_args!(
            "{:<9} {}",
            check.label,
            check.detail.as_deref().unwrap_or(check.status)
        ));
    }
    let failures = checks
        .iter()
        .filter(|check| check.status != "ok")
        .map(|check| format!("{}: {}", check.label, check.status))
        .collect::<Vec<_>>();
    if !failures.is_empty() {
        anyhow::bail!("usage verification failed: {}", failures.join(", "));
    }
    crate::output::stdout_line(format_args!("usage verification passed"));
    Ok(())
}

pub(crate) async fn usage_accounts() -> Result<Vec<AccountUsageSnapshotView>> {
    let msg = request_control(&ClientMsg::UsageAccountList).await?;
    match msg {
        ServerMsg::UsageAccounts { accounts } => Ok(accounts),
        other => anyhow::bail!(
            "daemon replied with {} for UsageAccountList request",
            other.kind()
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UsageVerifyCheck {
    pub(crate) label: &'static str,
    pub(crate) status: &'static str,
    pub(crate) detail: Option<String>,
}

pub(crate) fn verify_usage_accounts(
    accounts: &[AccountUsageSnapshotView],
) -> Vec<UsageVerifyCheck> {
    usage_verify_provider_aliases()
        .iter()
        .map(|(label, aliases)| verify_usage_provider(label, aliases, accounts))
        .collect()
}

pub(crate) fn usage_verify_provider_aliases() -> &'static [(&'static str, &'static [&'static str])]
{
    &[
        ("OpenAI", &["Codex", "OpenAI / Codex"]),
        ("Anthropic", &["Claude", "Anthropic / Claude"]),
        ("Amp", &["Amp"]),
        ("xAI", &["Grok Build", "xAI / Grok"]),
        ("Z.AI", &["GLM / Z.AI"]),
        ("Kimi", &["Kimi"]),
        ("MiniMax", &["MiniMax"]),
    ]
}

pub(crate) fn verify_usage_provider(
    label: &'static str,
    aliases: &[&str],
    accounts: &[AccountUsageSnapshotView],
) -> UsageVerifyCheck {
    let rows = accounts
        .iter()
        .filter(|account| {
            aliases
                .iter()
                .any(|alias| usage_provider_matches(alias, &account.provider))
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return UsageVerifyCheck {
            label,
            status: "missing",
            detail: None,
        };
    }
    let ok = rows.iter().any(|row| usage_row_proves_live_quota(row));
    if ok {
        let Some(latest) = rows.iter().max_by_key(|row| row.fetched_at) else {
            return UsageVerifyCheck {
                label,
                status: "missing",
                detail: None,
            };
        };
        return UsageVerifyCheck {
            label,
            status: "ok",
            detail: Some(format!(
                "ok: {} {} {} {} row(s)",
                latest.status,
                latest.source,
                latest.confidence,
                rows.len()
            )),
        };
    }
    let Some(latest) = rows.iter().max_by_key(|row| row.fetched_at) else {
        return UsageVerifyCheck {
            label,
            status: "missing",
            detail: None,
        };
    };
    UsageVerifyCheck {
        label,
        status: "untrusted",
        detail: Some(format!(
            "untrusted: latest status={} source={} confidence={} error={}",
            latest.status,
            latest.source,
            latest.confidence,
            latest.last_error.as_deref().unwrap_or("none")
        )),
    }
}

pub(crate) fn usage_row_proves_live_quota(row: &AccountUsageSnapshotView) -> bool {
    row.status == "fresh"
        && row.confidence == "authoritative"
        && matches!(row.source.as_str(), "provider_api" | "cli")
        && !row.window_kind.trim().is_empty()
        && !row.account_label.trim().is_empty()
        && !row.account_label.to_ascii_lowercase().contains("needs")
}

pub(crate) fn usage_provider_matches(needle: &str, provider: &str) -> bool {
    // Interchangeable provider/agent labels: a match needs one member of a group
    // on each side. Bidirectional and extensible — add a group, not two arms.
    pub(crate) const SYNONYMS: &[&[&str]] = &[
        &["openai", "codex"],
        &["anthropic", "claude"],
        &["xai", "grok"],
        &["zai", "glm"],
    ];
    let needle = normalize_usage_provider_label(needle);
    let provider = normalize_usage_provider_label(provider);
    provider.contains(&needle)
        || needle.contains(&provider)
        || SYNONYMS.iter().any(|group| {
            group.iter().any(|m| needle.contains(m)) && group.iter().any(|m| provider.contains(m))
        })
}

pub(crate) fn normalize_usage_provider_label(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}
