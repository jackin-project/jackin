// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `Claude` / `Anthropic` usage snapshot.
//!
//! Carved out of `usage.rs` for the file-size ratchet. Items in this module
//! are `pub(crate)` so the coordinator (`usage.rs`) can re-export them.

mod cli;
mod credentials;
mod keychain;
mod oauth_types;
mod refresh;
mod snapshot;
mod spend;
mod wave;
mod windows;

#[cfg(test)]
use super::refresh::ProviderError;
#[cfg(test)]
use super::*;

pub use cli::ClaudeUsageDiagnostic;
pub(crate) use cli::{
    ClaudeCliUsage, claude_code_user_agent, claude_code_user_agent_with,
    claude_code_version_from_text, fetch_claude_cli_usage,
};
pub(crate) use credentials::{
    ClaudeOAuthCredentials, claude_email_from_value, claude_oauth_from_value,
    claude_organization_type_from_value, load_claude_account_email,
};
#[cfg(test)]
pub(crate) use credentials::{load_claude_oauth_credentials, load_claude_organization_type};
#[cfg(any(target_os = "macos", test))]
pub(crate) use keychain::classify_claude_keychain_status;
#[cfg(not(target_os = "macos"))]
pub(crate) use keychain::read_claude_keychain_item;
#[cfg(target_os = "macos")]
pub(crate) use keychain::read_claude_keychain_item;
pub(crate) use keychain::{ClaudeKeychainRead, ClaudeKeychainState, claude_keychain_state};
pub(crate) use oauth_types::{
    ClaudeOAuthExtraUsage, ClaudeOAuthLimit, ClaudeOAuthLimitModel, ClaudeOAuthLimitScope,
    ClaudeOAuthMoney, ClaudeOAuthSpend, ClaudeOAuthUsageResponse, ClaudeOAuthUsageWindow,
};
pub(crate) use refresh::{
    ClaudeFileProbe, ClaudeOAuthEnvToken, ClaudeResolved, ClaudeWaveResolution,
    resolve_claude_refresh_wave_with,
};
pub(crate) use snapshot::{
    claude_account_identity, claude_api_key_snapshot, claude_oauth_candidates, claude_snapshot,
    resolve_claude_wave,
};
pub(crate) use spend::{
    ClaudeSpend, claude_spend_bucket, fetch_claude_oauth_usage, normalize_claude_spend,
    push_claude_dollar_windows,
};
#[cfg(test)]
pub(crate) use wave::claude_resolved_last_error;
pub(crate) use wave::{
    ClaudeWavePolicy, claude_error_is_scope_restriction, claude_provider_error_label,
    claude_scope_file_probe, claude_view_from_wave_with_rate_limit, claude_wave_policy,
    read_claude_oauth_env_token,
};
pub(crate) use windows::ClaudeQuotaWindow;
pub(crate) use windows::{CLAUDE_SESSION_WINDOW_SECONDS, CLAUDE_WEEKLY_WINDOW_SECONDS};

#[cfg(test)]
mod tests;
