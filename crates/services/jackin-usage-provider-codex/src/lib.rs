//! jackin-usage-provider-codex: `Codex` / `OpenAI` usage snapshot collection.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`codex_snapshot`] — `Codex` usage snapshot.

mod credentials;
mod endpoints;
mod oauth;
mod rpc;
mod rpc_types;
mod snapshot;
mod types;
mod views;
mod windows;

#[cfg(any(test, feature = "test-support"))]
pub use credentials::load_codex_oauth_credentials;
pub use credentials::{
    CodexOAuthCredentials, codex_account_label_from_id_token, codex_oauth_from_value,
};
pub use endpoints::{
    resolve_codex_base_url, resolve_codex_reset_credits_url, resolve_codex_usage_url,
};
pub use oauth::{
    codex_access_token_from_response, codex_refresh_request_body, fetch_codex_oauth_reset_credits,
    fetch_codex_oauth_usage, fetch_codex_oauth_usage_refreshing, refresh_codex_access_token,
};
pub use rpc::{
    codex_rpc_notification, codex_rpc_request, decode_codex_rpc_usage, fetch_codex_rpc_usage,
};
pub use rpc_types::{
    CodexAdditionalRateLimit, CodexRpcAccountDetails, CodexRpcAccountResponse, CodexRpcCredits,
    CodexRpcLimitEntry, CodexRpcRateLimitWindow, CodexRpcRateLimits, CodexRpcRateLimitsResponse,
    CodexRpcResetCredits, CodexRpcUsage,
};
pub use snapshot::{
    codex_account_identity, codex_auth_candidates, codex_plan_display_name,
    codex_plan_exact_display, codex_plan_word_display, codex_profile_snapshot,
    codex_profile_snapshot_with_rate_limit,
};
pub use types::{
    CodexCreditDetails, CodexIndividualLimit, CodexRateLimitDetails, CodexSpendControl,
    CodexUsageResponse, CodexWindowSnapshot,
};
pub use views::codex_snapshot;
pub use windows::{CodexResetCredit, CodexResetCredits, push_codex_window};

#[cfg(test)]
mod tests;
