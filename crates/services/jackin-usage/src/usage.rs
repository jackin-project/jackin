// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Focused-agent usage snapshots for Capsule.
//!
//! The TUI reads normalized cached snapshots from this module. Provider-specific
//! details stay here so status chrome and dialogs render strings, not API
//! branches.

mod credential_snapshots;

pub use self::credential_snapshots::provider_credential_snapshot;
pub(crate) use self::credential_snapshots::provider_credential_snapshot_with_rate_limit;
#[cfg(test)]
use jackin_protocol::control::{UsageProviderTab, UsageSeverity};
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
#[cfg(any(target_os = "macos", test))]
pub(crate) use jackin_usage_provider_claude::classify_claude_keychain_status;
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use jackin_usage_provider_claude::{
    ClaudeCliUsage, ClaudeKeychainRead, ClaudeOAuthCredentials, ClaudeOAuthEnvToken,
    ClaudeOAuthExtraUsage, ClaudeOAuthLimit, ClaudeOAuthLimitModel, ClaudeOAuthLimitScope,
    ClaudeOAuthMoney, ClaudeOAuthSpend, ClaudeOAuthUsageResponse, ClaudeOAuthUsageWindow,
    ClaudeQuotaWindow, ClaudeResolved, ClaudeSpend, ClaudeWavePolicy, ClaudeWaveResolution,
    claude_account_identity, claude_api_key_snapshot, claude_code_user_agent,
    claude_code_user_agent_with, claude_code_version_from_text, claude_email_from_value,
    claude_error_is_scope_restriction, claude_oauth_candidates, claude_oauth_from_value,
    claude_organization_type_from_value, claude_provider_error_label, claude_snapshot,
    claude_spend_bucket, claude_view_from_wave_with_rate_limit, claude_wave_policy,
    fetch_claude_cli_usage, fetch_claude_oauth_usage, load_claude_account_email,
    normalize_claude_spend, parse_claude_usage_output, push_claude_dollar_windows,
    read_claude_keychain_item, resolve_claude_wave,
};
#[cfg(test)]
pub(crate) use jackin_usage_provider_claude::{
    ClaudeFileProbe, ClaudeKeychainState, load_claude_oauth_credentials,
    load_claude_organization_type, read_claude_oauth_env_token, resolve_claude_refresh_wave_with,
    run_claude_usage_diagnostic_with,
};
pub use jackin_usage_provider_claude::{ClaudeUsageDiagnostic, run_claude_usage_diagnostic};
#[cfg(test)]
pub(crate) use jackin_usage_provider_codex::load_codex_oauth_credentials;
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
pub(crate) use jackin_usage_provider_opencode::opencode_profile_snapshot;
#[cfg(test)]
pub(crate) use jackin_usage_provider_opencode::{load_opencode_api_key, parse_opencode_usage};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use jackin_usage_provider_openrouter::{
    OPENROUTER_DEFAULT_BASE_URL, OpenRouterCreditsOutcome, OpenRouterKeyQuota,
    OpenRouterModelCheck, check_openrouter_model_in_catalog, fetch_openrouter_credits,
    fetch_openrouter_key_usage, fetch_openrouter_model_check, openrouter_base_url,
    openrouter_base_url_from, openrouter_credits_bucket, openrouter_snapshot,
    openrouter_snapshot_with_base, openrouter_snapshot_with_rate_limit, parse_openrouter_credits,
    parse_openrouter_key_usage,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use jackin_usage_provider_zai::{
    ZaiLimitRaw, ZaiQuotaData, ZaiQuotaResponse, fetch_zai_usage, provider_key_snapshot,
    resolve_zai_quota_url, resolve_zai_quota_url_from, zai_bucket, zai_count_line, zai_quota_host,
};
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::time::Duration;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[expect(
    unused_imports,
    reason = "suite/vendor-test support shim; cases use overlapping subsets of the core API"
)]
pub(crate) use jackin_usage_provider_core::{
    AMP_HANDOFF_SECRETS_PATH, CLAUDE_CODE_USER_AGENT_FALLBACK, CLAUDE_HANDOFF_CREDENTIALS_PATH,
    CLAUDE_VERSION_TIMEOUT, CODEX_HANDOFF_AUTH_PATH, CODEX_OAUTH_CLIENT_ID, CODEX_OAUTH_TOKEN_URL,
    CODEX_RPC_INIT_TIMEOUT, CODEX_RPC_LAUNCH_COOLDOWN, CODEX_RPC_REQUEST_TIMEOUT, CachedUsage,
    ChildOperation, CliOutput, Fixed32Field, GROK_HANDOFF_AUTH_PATH, GROK_RPC_INIT_TIMEOUT,
    GROK_RPC_REQUEST_TIMEOUT, MATERIALIZED_TMP_COUNTER, MATERIALIZED_USAGE_ACCOUNTS_PATH,
    ManagedCliLaunchGate, MaterializedUsageAccounts, PROCESS_OUTPUT_MAX, PROVIDER_CLI_TIMEOUT,
    PROVIDER_HTTP_TIMEOUT, PercentStyle, ProtobufScan, ProviderError, ProviderHttpError,
    ProviderPresence, ProviderRateLimit, ResetStyle, USAGE_SNAPSHOT_STORE_PATH,
    UsageBucketPresentation, UsageCache, UsageFormatPrefs, UsageRefreshTarget, UsageSurface,
    UsageViewInput, VarintField, account_snapshot_views_from_cache, account_tab_label_for_parts,
    amp_status_bar_headline, atomic_write_usage_json, broker_surface_id, bucket,
    cached_refreshing_view, cached_unavailable_view, cached_usage_for_capability,
    cached_usage_for_target, cached_usage_key_for_target, canonical_usage_cache_key,
    capability_matches_surface, codex_account_from_value, codex_limit_label, collect_cli_output,
    compact_account_identity, compact_count, compact_duration_label, complete_external_rpc,
    decorate_surface_view, dollar_amounts, enrich_provider_tabs, env_dir_or_home, env_value,
    epoch_seconds_from_maybe_ms, estimate_caption, exact_reset_parenthetical, expiry_label,
    external_rpc_operation, first_credential, first_credential_with_path, first_string_key,
    format_amount_with_unit, format_cents, format_currency, get_json_bearer, home_path,
    humanize_plan_label, humanize_reason, humanize_window_label, humanize_words_with, json_number,
    local_timestamp_label, looks_like_protobuf_payload, mark_active_tab, normalize_url_or_host,
    now_epoch, oauth_origin, parse_chatgpt_base_url, parse_iso_epoch, percent_before_used,
    percent_headline, preserve_cached_quota_on_failed_refresh, provider_display_label,
    provider_http_client, provider_outcome, provider_request, provider_tabs,
    quota_amounts_for_account_snapshot, quota_pace_label, read_json_file, read_process_pipe,
    read_varint, refresh_cached_updated_label, relative_updated_label, remaining_from_fraction,
    reset_label, reset_label_with_prefs, resolve_identity, resolve_identity_with_extra,
    resolve_surface, resolved_usage_provider_label, retry_after_header_seconds,
    retry_after_header_value, run_cli_with_timeout, run_cli_with_timeout_full, severity_from_label,
    spend_headline_label, split_fetch, split_provider_fetch, stable_cache_account_label,
    status_bar_fresh_or_stale, status_bar_headline_for_surface, status_bar_label,
    status_bar_quota_labels, summary_bucket, timed_bucket, titlecase_ascii, unpollable_snapshot,
    unsupported_snapshot, usage_account_tab_id, usage_bucket_presentation,
    usage_cache_key_for_broker_account, usage_cache_key_for_view, usage_confidence_storage_label,
    usage_detail_presentation, usage_display_status_label, usage_error_is_rate_limited,
    usage_error_is_unauthorized, usage_identity_presentation, usage_source_storage_label,
    usage_status_storage_label, usage_tab_source_label, usage_tab_status_label, usage_view,
    used_percent_from_fraction, used_percent_label, window_minutes_label, with_status_slot,
    write_json_line, write_materialized_usage_accounts,
};
