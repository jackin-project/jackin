// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `Codex` / `OpenAI` usage snapshot.
//!
//! Carved out of `usage.rs` for the file-size ratchet. Items in this module
//! are `pub(crate)` so the coordinator (`usage.rs`) can re-export them.

mod credentials;
mod endpoints;
mod oauth;
mod rpc;
mod rpc_types;
mod snapshot;
mod types;
mod views;
mod windows;

#[cfg(test)]
use super::*;
#[cfg(test)]
use jackin_usage_provider_core::ProviderError;

#[cfg(test)]
pub(crate) use credentials::load_codex_oauth_credentials;
pub(crate) use credentials::{
    CodexOAuthCredentials, codex_account_label_from_id_token, codex_oauth_from_value,
};
pub(crate) use endpoints::{
    resolve_codex_base_url, resolve_codex_reset_credits_url, resolve_codex_usage_url,
};
pub(crate) use oauth::{
    codex_access_token_from_response, codex_refresh_request_body, fetch_codex_oauth_reset_credits,
    fetch_codex_oauth_usage, fetch_codex_oauth_usage_refreshing, refresh_codex_access_token,
};
pub(crate) use rpc::{
    codex_rpc_notification, codex_rpc_request, decode_codex_rpc_usage, fetch_codex_rpc_usage,
};
pub(crate) use rpc_types::{
    CodexAdditionalRateLimit, CodexRpcAccountDetails, CodexRpcAccountResponse, CodexRpcCredits,
    CodexRpcLimitEntry, CodexRpcRateLimitWindow, CodexRpcRateLimits, CodexRpcRateLimitsResponse,
    CodexRpcResetCredits, CodexRpcUsage,
};
pub(crate) use snapshot::{
    codex_account_identity, codex_auth_candidates, codex_plan_display_name,
    codex_plan_exact_display, codex_plan_word_display, codex_profile_snapshot,
    codex_profile_snapshot_with_rate_limit,
};
pub(crate) use types::{
    CodexCreditDetails, CodexIndividualLimit, CodexRateLimitDetails, CodexSpendControl,
    CodexUsageResponse, CodexWindowSnapshot,
};
pub(crate) use views::codex_snapshot;
pub(crate) use windows::{CodexResetCredit, CodexResetCredits, push_codex_window};

#[cfg(test)]
mod tests;
