//! jackin-usage-provider-claude: `Claude` / `Anthropic` usage snapshot.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`claude_snapshot`] — `Claude` usage snapshot.

mod cli;
mod credentials;
mod diagnostic;
mod keychain;
mod oauth_types;
mod refresh;
mod snapshot;
mod spend;
mod wave;
mod windows;

pub use self::diagnostic::parse_claude_usage_output;
pub use self::diagnostic::run_claude_usage_diagnostic;
pub use self::diagnostic::run_claude_usage_diagnostic_with;
pub use cli::ClaudeUsageDiagnostic;
pub use cli::{
    ClaudeCliUsage, claude_code_user_agent, claude_code_user_agent_with,
    claude_code_version_from_text, fetch_claude_cli_usage,
};
pub use credentials::{
    ClaudeOAuthCredentials, claude_email_from_value, claude_oauth_from_value,
    claude_organization_type_from_value, load_claude_account_email,
};
#[cfg(any(test, feature = "test-support"))]
pub use credentials::{load_claude_oauth_credentials, load_claude_organization_type};
#[cfg(any(target_os = "macos", test))]
pub use keychain::classify_claude_keychain_status;
pub(crate) use keychain::claude_keychain_state;
pub use keychain::read_claude_keychain_item;
pub use keychain::{ClaudeKeychainRead, ClaudeKeychainState};
pub use oauth_types::{
    ClaudeOAuthExtraUsage, ClaudeOAuthLimit, ClaudeOAuthLimitModel, ClaudeOAuthLimitScope,
    ClaudeOAuthMoney, ClaudeOAuthSpend, ClaudeOAuthUsageResponse, ClaudeOAuthUsageWindow,
};
pub use refresh::{
    ClaudeFileProbe, ClaudeOAuthEnvToken, ClaudeResolved, ClaudeWaveResolution,
    resolve_claude_refresh_wave_with,
};
pub use snapshot::{
    claude_account_identity, claude_api_key_snapshot, claude_oauth_candidates, claude_snapshot,
    resolve_claude_wave,
};
pub use spend::{
    ClaudeSpend, claude_spend_bucket, fetch_claude_oauth_usage, normalize_claude_spend,
    push_claude_dollar_windows,
};
#[cfg(test)]
pub(crate) use wave::claude_resolved_last_error;
pub(crate) use wave::claude_scope_file_probe;
pub use wave::{
    ClaudeWavePolicy, claude_error_is_scope_restriction, claude_provider_error_label,
    claude_view_from_wave_with_rate_limit, claude_wave_policy, read_claude_oauth_env_token,
};
pub use windows::ClaudeQuotaWindow;
pub use windows::{CLAUDE_SESSION_WINDOW_SECONDS, CLAUDE_WEEKLY_WINDOW_SECONDS};

#[cfg(test)]
mod tests;
