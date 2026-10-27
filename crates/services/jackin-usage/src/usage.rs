// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub use jackin_usage_credential_resolver::provider_credential_snapshot;
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use jackin_usage_provider_amp::{
    AmpRenewal, AmpSubscription, AmpSubscriptionKind, AmpSuccessContext, AmpUsage,
    AmpWorkspaceBalance, amp_api_key_snapshot, amp_snapshot, amp_view_from_usage,
    fetch_amp_api_usage, fetch_amp_cli_usage, load_amp_api_key, parse_amp_usage_output,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use jackin_usage_provider_antigravity::{
    ANTIGRAVITY_KEYCHAIN_SERVICE, ANTIGRAVITY_MIN_JSON_VERSION, AntigravityCredits,
    AntigravityFamily, AntigravityPool, AntigravityUsage, AntigravityWindow,
    agy_version_supports_json, antigravity_buckets, antigravity_cli_version,
    antigravity_credits_bucket, antigravity_identity_from_value, antigravity_plan_from_value,
    antigravity_snapshot, fetch_antigravity_cli_credits, fetch_antigravity_cli_usage,
    parse_agy_version, parse_antigravity_credits_output, parse_antigravity_usage_output,
};

#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use jackin_usage_provider_codex::{
    CodexAdditionalRateLimit, CodexCreditDetails, CodexIndividualLimit, CodexOAuthCredentials,
    CodexRateLimitDetails, CodexResetCredit, CodexResetCredits, CodexRpcAccountDetails,
    CodexRpcAccountResponse, CodexRpcCredits, CodexRpcLimitEntry, CodexRpcRateLimitWindow,
    CodexRpcRateLimits, CodexRpcRateLimitsResponse, CodexRpcResetCredits, CodexRpcUsage,
    CodexSpendControl, CodexUsageResponse, CodexWindowSnapshot, codex_access_token_from_response,
    codex_account_identity, codex_account_label_from_id_token, codex_auth_candidates,
    codex_oauth_from_value, codex_plan_display_name, codex_plan_exact_display,
    codex_plan_word_display, codex_profile_snapshot, codex_profile_snapshot_with_rate_limit,
    codex_refresh_request_body, codex_rpc_notification, codex_rpc_request, codex_snapshot,
    decode_codex_rpc_usage, fetch_codex_oauth_reset_credits, fetch_codex_oauth_usage,
    fetch_codex_oauth_usage_refreshing, fetch_codex_rpc_usage, push_codex_window,
    refresh_codex_access_token, resolve_codex_base_url, resolve_codex_reset_credits_url,
    resolve_codex_usage_url,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use jackin_usage_provider_cursor::{
    CursorAuth, CursorEnterpriseScope, CursorMemberSpend, CursorPeriodUsage, CursorRequestUsage,
    CursorSandUsage, CursorTeamSpend, CursorUsageEvents, CursorUsageSummary,
    cursor_auth_from_value, cursor_auth_path, cursor_cli_identity_from_value,
    cursor_credits_bucket, cursor_dashboard_base, cursor_dashboard_post, cursor_default_base,
    cursor_enterprise_snapshot, cursor_events_buckets, cursor_identity_from_cli_config,
    cursor_needs_request_fallback, cursor_period_buckets, cursor_profile_snapshot,
    cursor_request_bucket, cursor_rest_get, cursor_sand_bucket, cursor_session_cookie,
    cursor_snapshot, cursor_snapshot_with_auth, cursor_summary_buckets, cursor_team_spend_buckets,
    cursor_teams_events_url, cursor_teams_spend_url, cursor_user_id_from_token,
    fetch_cursor_credit_grants, fetch_cursor_period_usage, fetch_cursor_plan_info,
    fetch_cursor_request_usage, fetch_cursor_sand_usage, fetch_cursor_stripe_balance,
    fetch_cursor_team_spend, fetch_cursor_usage_events, fetch_cursor_usage_summary,
    load_cursor_auth, load_cursor_cli_identity, parse_cursor_credit_grants,
    parse_cursor_period_usage, parse_cursor_plan_info, parse_cursor_request_usage,
    parse_cursor_sand_usage, parse_cursor_stripe_balance, parse_cursor_team_spend,
    parse_cursor_usage_events, parse_cursor_usage_summary,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use jackin_usage_provider_gemini::{
    GEMINI_CONSUMER_OAUTH_END, GeminiEntitlement, GeminiProjectQuota,
    gemini_consumer_oauth_retired, gemini_credential_origin, gemini_credential_presence,
    gemini_migration_action, gemini_oauth_creds_path, gemini_quota_buckets, gemini_snapshot,
    gemini_snapshot_with_presence, parse_gemini_entitlement, parse_gemini_project_quotas,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use jackin_usage_provider_grok::{
    GrokBillingConfig, GrokBillingResponse, GrokBillingSnapshot, GrokCent, GrokCurrentPeriod,
    GrokWebBillingSnapshot, fetch_grok_billing, fetch_grok_rest_billing, fetch_grok_rpc_billing,
    grok_account_label, grok_account_label_or_presence, grok_bearer_token,
    grok_bearer_token_from_entry, grok_binary_path, grok_cycle_label_from_minutes,
    grok_cycle_label_from_reset, grok_rpc_request, grok_rpc_request_payload, grok_snapshot,
    grok_snapshot_from_rpc_result, grok_snapshot_from_rpc_result_with_rate_limit,
    grpc_web_data_frames, parse_grok_rest_billing_response, parse_grok_web_billing_response,
    scan_protobuf,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use jackin_usage_provider_hermes::{
    HermesRuntime, HermesSubscription, hermes_auth_status, hermes_renews_label,
    hermes_subscription_bucket, hermes_tracker_counter_buckets, hermes_view,
    parse_hermes_subscription,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use jackin_usage_provider_kimi::{
    KimiRateLimit, KimiUsageDetail, KimiUsageItem, KimiUsageResponse, KimiWindow, fetch_kimi_usage,
    kimi_bucket, kimi_local_token_from_value, kimi_snapshot, kimi_window_seconds,
    load_kimi_local_token, load_kimi_local_token_from_home,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use jackin_usage_provider_minimax::{
    MiniMaxBaseResponse, MiniMaxComboCard, MiniMaxModelRemain, MiniMaxUsageData,
    MiniMaxUsageResponse, MiniMaxWindow, fetch_minimax_usage, first_minimax_usage, minimax_bucket,
    minimax_bucket_label, minimax_is_general_model, minimax_operation_path, minimax_remains_host,
    minimax_reset_epoch, minimax_snapshot, minimax_usage_count_line, resolve_minimax_remains_urls,
    resolve_minimax_remains_urls_from,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use jackin_usage_provider_muse::{
    MuseIdentity, MuseKeyExchangePolicy, MuseObservation, MuseWindow, muse_buckets,
    muse_freshness_epoch, muse_identity_from_value, muse_view, parse_muse_usage_read,
};

#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use jackin_usage_provider_openrouter::{
    OPENROUTER_DEFAULT_BASE_URL, OpenRouterCreditsOutcome, OpenRouterKeyQuota,
    OpenRouterModelCheck, check_openrouter_model_in_catalog, fetch_openrouter_credits,
    fetch_openrouter_key_usage_raw, fetch_openrouter_model_check, openrouter_base_url,
    openrouter_base_url_from, openrouter_credits_bucket, openrouter_snapshot,
    openrouter_snapshot_with_base, openrouter_snapshot_with_rate_limit, parse_openrouter_credits,
    parse_openrouter_key_usage_raw,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use jackin_usage_provider_zai::{
    ZaiLimitRaw, ZaiQuotaData, ZaiQuotaResponse, fetch_zai_usage, provider_key_snapshot,
    resolve_zai_quota_url, resolve_zai_quota_url_from, zai_bucket, zai_count_line, zai_quota_host,
};
