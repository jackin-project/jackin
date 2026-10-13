// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Claude` snapshot entry points and identity.

use super::super::{
    CLAUDE_HANDOFF_CREDENTIALS_PATH, FocusedUsageView, Path, PathBuf, UsageConfidence,
    UsageSnapshotStatus, UsageSource, UsageSurface, UsageViewInput, bucket, env_dir_or_home,
    home_path, usage_view,
};

use super::{
    ClaudeWaveResolution, claude_keychain_state, claude_scope_file_probe,
    claude_view_from_wave_with_rate_limit, load_claude_account_email, read_claude_keychain_item,
    read_claude_oauth_env_token, resolve_claude_refresh_wave_with,
};

/// Claude OAuth credential candidates, home-first — the single source of truth
/// for the path precedence, shared by `claude_snapshot` (token + identity) and
/// `claude_account_identity` (the shared-cache key) so the list can't drift.
pub(crate) fn claude_oauth_candidates(config: &Path) -> [PathBuf; 4] {
    [
        config.join(".credentials.json"),
        home_path(".claude/.credentials.json"),
        home_path(".claude.json"),
        PathBuf::from(CLAUDE_HANDOFF_CREDENTIALS_PATH),
    ]
}

/// Claude account identity (the `oauthAccount` email) from the same credential
/// candidates `claude_snapshot` uses, without fetching usage.
pub(crate) fn claude_account_identity() -> Option<String> {
    let config = env_dir_or_home("CLAUDE_CONFIG_DIR", ".claude");
    claude_oauth_candidates(&config)
        .iter()
        .find_map(|path| load_claude_account_email(path))
}

pub(crate) fn claude_snapshot(agent: &str, provider: Option<&str>, now: i64) -> FocusedUsageView {
    claude_view_from_wave_with_rate_limit(agent, provider, now, resolve_claude_wave()).0
}

/// Claude API keys do not authenticate the OAuth quota endpoint. Keep this
/// route explicit and unsupported rather than feeding an API key into the
/// OAuth adapter and reporting a misleading login/error state.
pub(crate) fn claude_api_key_snapshot(
    agent: &str,
    provider: Option<&str>,
    key_name: &str,
    secret: &str,
    now: i64,
) -> FocusedUsageView {
    let has_secret = !secret.trim().is_empty();
    let status = if has_secret {
        UsageSnapshotStatus::Unsupported
    } else {
        UsageSnapshotStatus::NeedsSecret
    };
    let message = if has_secret {
        "Claude API-key quota is unavailable; OAuth usage requires CLAUDE_CODE_OAUTH_TOKEN"
    } else {
        "Claude API key is missing"
    };
    usage_view(UsageViewInput {
        agent,
        provider: provider.or(Some("Claude")),
        surface: UsageSurface::Claude,
        account_label: "Claude API key".to_owned(),
        username: None,
        plan_label: None,
        credential_origin: Some(format!("API key · env {key_name}")),
        buckets: vec![bucket(
            "Usage",
            None,
            None,
            None,
            None,
            Some(message),
            status,
        )],
        status,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        now,
        last_error: Some(message.to_owned()),
    })
}

/// Production Claude wave resolution: derive the Keychain scope from the
/// effective `CLAUDE_CONFIG_DIR`, then resolve Keychain-first with
/// scope-appropriate file/env fallback.
pub(crate) fn resolve_claude_wave() -> ClaudeWaveResolution {
    let config = env_dir_or_home("CLAUDE_CONFIG_DIR", ".claude");
    let home = home_path("");
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let Some(scope) = jackin_core::claude_keychain_scope(&config, &home, &current_dir) else {
        // Non-UTF-8 config path: the service is unknowable, so treat as absence.
        return ClaudeWaveResolution::Missing;
    };
    resolve_claude_refresh_wave_with(
        &scope,
        claude_keychain_state(),
        read_claude_keychain_item,
        || claude_scope_file_probe(&scope, &config),
        || read_claude_oauth_env_token(|name| std::env::var(name)),
    )
}
