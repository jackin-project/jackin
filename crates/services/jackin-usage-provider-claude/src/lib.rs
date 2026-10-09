//! jackin-usage-provider-claude: `Claude` / `Anthropic` usage snapshot.
//!
//! **Architecture Invariant:** T3.
//! Broker-owned refreshes supply resolved OAuth material to the HTTP adapter.

mod credentials;
mod keychain;
mod oauth_types;
mod refresh;
mod snapshot;
mod spend;
mod wave;
mod windows;

pub use credentials::{
    ClaudeOAuthCredentials, claude_email_from_value, claude_oauth_from_value,
    claude_organization_type_from_value, load_claude_account_email,
};
#[cfg(any(test, feature = "test-support"))]
pub use credentials::{load_claude_oauth_credentials, load_claude_organization_type};
#[cfg(any(target_os = "macos", test))]
pub use keychain::classify_claude_keychain_status;
pub use keychain::{
    ClaudeKeychainInteractionPolicy, ClaudeKeychainPolicyError, ClaudeKeychainRead,
    ClaudeUnattendedKeychainGuard, read_claude_keychain_item, unattended_keychain_guard,
};
pub use oauth_types::{
    ClaudeOAuthExtraUsage, ClaudeOAuthLimit, ClaudeOAuthLimitModel, ClaudeOAuthLimitScope,
    ClaudeOAuthMoney, ClaudeOAuthSpend, ClaudeOAuthUsageResponse, ClaudeOAuthUsageWindow,
};
pub use refresh::{ClaudeResolved, ClaudeWaveResolution};
pub use snapshot::claude_api_key_snapshot;
pub use spend::{
    ClaudeSpend, claude_spend_bucket, fetch_claude_oauth_usage, normalize_claude_spend,
    push_claude_dollar_windows,
};
#[cfg(test)]
pub(crate) use wave::claude_resolved_view_with_fetch;
pub use wave::{
    ClaudeWavePolicy, claude_error_is_scope_restriction, claude_provider_error_label,
    claude_view_from_wave, claude_wave_policy,
};
pub use windows::ClaudeQuotaWindow;
pub use windows::{CLAUDE_SESSION_WINDOW_SECONDS, CLAUDE_WEEKLY_WINDOW_SECONDS};

#[cfg(test)]
mod tests;
