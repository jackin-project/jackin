// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![expect(
    dead_code,
    reason = "provider-adapter fixtures remain testable while production dispatch is broker-only"
)]

//! Focused-agent usage snapshots for Capsule.
//!
//! The TUI reads normalized cached snapshots from this module. Provider-specific
//! details stay here so status chrome and dialogs render strings, not API
//! branches.

mod amp;
mod antigravity;
mod claude;
mod codex;
mod cursor;
mod format;
mod gemini;
mod grok;
mod hermes;
mod kimi;
mod minimax;
mod muse;
mod omp;
mod opencode;
mod openrouter;
pub(crate) mod process_telemetry;
mod refresh;
mod view;
mod zai;

mod cache;
mod cache_keys;
mod consts;
mod credentials;
mod diagnostic;
mod fallback;
mod http;
mod io;
mod labels;
mod outcome;
mod surface;
mod transport;

use jackin_core::account_key_hash;
use std::collections::{BTreeMap, HashMap};
use std::fs;

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use jackin_protocol::control::{
    AccountUsageSnapshotView, FocusedAccountHeader, FocusedUsageView, Money, QuotaBucketView,
    StatusSlot, UsageConfidence, UsageProviderTab, UsageSeverity, UsageSnapshotStatus, UsageSource,
};
use jackin_telemetry::ResultTelemetryExt as _;
use serde::Serialize;

use format::{
    CliOutput, codex_account_from_value, codex_limit_label, compact_count, dollar_amounts,
    env_value, expiry_label, first_string_key, format_amount_with_unit, format_cents,
    format_currency, home_path, humanize_plan_label, humanize_words_with, json_number,
    oauth_origin, parse_iso_epoch, quota_pace_label, remaining_from_fraction, reset_label,
    run_cli_with_timeout, run_cli_with_timeout_full, titlecase_ascii, used_percent_from_fraction,
    used_percent_label, window_minutes_label,
};

#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::amp::{
    AmpRenewal, AmpSubscription, AmpSubscriptionKind, AmpSuccessContext, AmpUsage,
    AmpWorkspaceBalance, amp_api_key_snapshot, amp_snapshot, amp_view_from_usage,
    fetch_amp_api_usage, fetch_amp_cli_usage, load_amp_api_key, parse_amp_usage_output,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::antigravity::{
    ANTIGRAVITY_KEYCHAIN_SERVICE, ANTIGRAVITY_MIN_JSON_VERSION, AntigravityCredits,
    AntigravityFamily, AntigravityPool, AntigravityUsage, AntigravityWindow,
    agy_version_supports_json, antigravity_buckets, antigravity_cli_version,
    antigravity_credits_bucket, antigravity_identity_from_value, antigravity_plan_from_value,
    antigravity_snapshot, fetch_antigravity_cli_credits, fetch_antigravity_cli_usage,
    parse_agy_version, parse_antigravity_credits_output, parse_antigravity_usage_output,
};
pub use self::claude::ClaudeUsageDiagnostic;
#[cfg(any(target_os = "macos", test))]
pub(crate) use self::claude::classify_claude_keychain_status;
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::claude::{
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
    normalize_claude_spend, push_claude_dollar_windows, read_claude_keychain_item,
    resolve_claude_wave,
};
#[cfg(test)]
pub(crate) use self::claude::{
    ClaudeFileProbe, ClaudeKeychainState, load_claude_oauth_credentials,
    load_claude_organization_type, read_claude_oauth_env_token, resolve_claude_refresh_wave_with,
};
#[cfg(test)]
pub(crate) use self::codex::load_codex_oauth_credentials;
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::codex::{
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
pub(crate) use self::cursor::{
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
pub(crate) use self::gemini::{
    GEMINI_CONSUMER_OAUTH_END, GeminiEntitlement, GeminiProjectQuota,
    gemini_consumer_oauth_retired, gemini_credential_origin, gemini_credential_presence,
    gemini_migration_action, gemini_oauth_creds_path, gemini_quota_buckets, gemini_snapshot,
    gemini_snapshot_with_presence, parse_gemini_entitlement, parse_gemini_project_quotas,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::grok::{
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
pub(crate) use self::kimi::{
    KimiRateLimit, KimiUsageDetail, KimiUsageItem, KimiUsageResponse, KimiWindow, fetch_kimi_usage,
    kimi_bucket, kimi_local_token_from_value, kimi_snapshot, kimi_window_seconds,
    load_kimi_local_token, load_kimi_local_token_from_home,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::minimax::{
    MiniMaxBaseResponse, MiniMaxComboCard, MiniMaxModelRemain, MiniMaxUsageData,
    MiniMaxUsageResponse, MiniMaxWindow, fetch_minimax_usage, first_minimax_usage, minimax_bucket,
    minimax_bucket_label, minimax_is_general_model, minimax_operation_path, minimax_remains_host,
    minimax_reset_epoch, minimax_snapshot, minimax_usage_count_line, resolve_minimax_remains_urls,
    resolve_minimax_remains_urls_from,
};
pub(crate) use self::opencode::opencode_profile_snapshot;
#[cfg(test)]
pub(crate) use self::opencode::{load_opencode_api_key, parse_opencode_usage};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::openrouter::{
    OPENROUTER_DEFAULT_BASE_URL, OpenRouterCreditsOutcome, OpenRouterKeyQuota,
    OpenRouterModelCheck, check_openrouter_model_in_catalog, fetch_openrouter_credits,
    fetch_openrouter_key_usage, fetch_openrouter_model_check, openrouter_base_url,
    openrouter_base_url_from, openrouter_credits_bucket, openrouter_snapshot,
    openrouter_snapshot_with_base, openrouter_snapshot_with_rate_limit, parse_openrouter_credits,
    parse_openrouter_key_usage,
};
#[cfg(test)]
pub(crate) use self::refresh::MaterializedUsageAccounts;
pub use self::refresh::ProviderRateLimit;
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::refresh::{
    MATERIALIZED_TMP_COUNTER, atomic_write_usage_json, usage_error_is_rate_limited,
    usage_error_is_unauthorized, write_materialized_usage_accounts,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::view::{
    UsageViewInput, account_snapshot_views_from_cache, account_tab_label_for_parts,
    amp_status_bar_headline, bucket, cached_refreshing_view, cached_unavailable_view,
    compact_account_identity, decorate_surface_view, enrich_provider_tabs, mark_active_tab,
    preserve_cached_quota_on_failed_refresh, provider_tabs, quota_amounts_for_account_snapshot,
    spend_headline_label, status_bar_fresh_or_stale, status_bar_headline_for_surface,
    status_bar_label, status_bar_quota_labels, summary_bucket, timed_bucket, usage_account_tab_id,
    usage_tab_source_label, usage_tab_status_label, usage_view, with_status_slot,
};
#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) use self::zai::{
    ZaiLimitRaw, ZaiQuotaData, ZaiQuotaResponse, fetch_zai_usage, json_epoch_seconds,
    provider_key_snapshot, resolve_zai_quota_url, resolve_zai_quota_url_from, zai_bucket,
    zai_count_line, zai_quota_host,
};
// Crate-visible re-exports for host overview/compact presentation (plan 008).
pub use self::format::{
    PercentStyle, ResetStyle, UsageBucketPresentation, UsageFormatPrefs, usage_bucket_presentation,
    usage_detail_presentation, usage_display_status_label, usage_identity_presentation,
};
pub(crate) use format::{
    compact_duration_label, exact_reset_parenthetical, percent_headline, reset_label_with_prefs,
};

pub(crate) use cache::CachedUsage;
pub use cache::{UsageCache, UsageRefreshTarget};
pub(crate) use cache_keys::{
    cached_usage_for_capability, cached_usage_for_target, cached_usage_key_for_target,
    canonical_usage_cache_key, stable_cache_account_label, usage_cache_key_for_broker_account,
    usage_cache_key_for_view,
};
pub use consts::USAGE_SNAPSHOT_STORE_PATH;
pub(crate) use consts::{
    AMP_HANDOFF_SECRETS_PATH, CLAUDE_CODE_USER_AGENT_FALLBACK, CLAUDE_HANDOFF_CREDENTIALS_PATH,
    CLAUDE_VERSION_TIMEOUT, CODEX_HANDOFF_AUTH_PATH, CODEX_OAUTH_CLIENT_ID, CODEX_OAUTH_TOKEN_URL,
    CODEX_RPC_INIT_TIMEOUT, CODEX_RPC_LAUNCH_COOLDOWN, CODEX_RPC_REQUEST_TIMEOUT,
    GROK_HANDOFF_AUTH_PATH, GROK_RPC_INIT_TIMEOUT, GROK_RPC_REQUEST_TIMEOUT,
    MATERIALIZED_USAGE_ACCOUNTS_PATH, PROVIDER_CLI_TIMEOUT, PROVIDER_HTTP_TIMEOUT,
};
#[cfg(test)]
pub(crate) use credentials::first_credential;
pub(crate) use credentials::{
    first_credential_with_path, read_json_file, resolve_identity, resolve_identity_with_extra,
};
#[cfg(test)]
pub(crate) use diagnostic::run_claude_usage_diagnostic_with;
pub(crate) use diagnostic::{now_epoch, parse_claude_usage_output, refresh_cached_updated_label};
pub use diagnostic::{relative_updated_label, run_claude_usage_diagnostic};
pub(crate) use fallback::{unpollable_snapshot, unsupported_snapshot};
#[cfg(test)]
pub(crate) use http::retry_after_header_value;
pub(crate) use http::{
    ProviderHttpError, epoch_seconds_from_maybe_ms, get_json_bearer, normalize_url_or_host,
    provider_request, retry_after_header_seconds,
};
pub(crate) use io::{
    Fixed32Field, ManagedCliLaunchGate, ProtobufScan, VarintField, looks_like_protobuf_payload,
    read_varint, write_json_line,
};
#[cfg(test)]
pub use labels::resolved_usage_provider_label;
pub use labels::{
    broker_surface_id, estimate_caption, provider_display_label, usage_confidence_storage_label,
    usage_source_storage_label, usage_status_storage_label,
};
pub(crate) use labels::{
    env_dir_or_home, humanize_reason, humanize_window_label, severity_from_label,
};
pub use outcome::provider_credential_snapshot;
pub(crate) use outcome::{
    ProviderPresence, capability_matches_surface, provider_credential_snapshot_with_rate_limit,
    provider_outcome, resolve_surface, split_fetch,
};
pub(crate) use surface::UsageSurface;
pub(crate) use transport::{parse_chatgpt_base_url, provider_http_client};

#[cfg(test)]
mod tests;
