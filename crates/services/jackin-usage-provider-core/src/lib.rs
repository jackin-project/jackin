//! jackin-usage-provider-core: provider snapshot collection, caching, and view formatting.
//!
//! **Architecture Invariant:** T2.
//! Entry point: [`UsageCache`] — snapshot cache; [`usage_view`] — view composition.
//!
//! Shared substrate for the per-vendor provider crates (`jackin-usage-provider-*`):
//! snapshot cache, fetch transport, outcome mapping, labels, and usage views.

mod cache;
mod cache_keys;
mod consts;
mod credentials;
mod diagnostic;
mod fallback;
mod format;
mod http;
mod io;
mod labels;
mod omp;
mod outcome;
mod process_telemetry;
mod refresh;
mod surface;
mod transport;
mod view;

use jackin_core::account_key_hash;
use std::collections::HashMap;
use std::fs;

use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use jackin_protocol::control::{
    AccountUsageSnapshotView, FocusedAccountHeader, FocusedUsageView, QuotaBucketView, StatusSlot,
    UsageConfidence, UsageProviderTab, UsageSeverity, UsageSnapshotStatus, UsageSource,
};
use serde::Serialize;

pub use self::format::{
    PROCESS_OUTPUT_MAX, codex_account_from_value, codex_limit_label, collect_cli_output,
    compact_count, dollar_amounts, env_value, expiry_label, first_string_key,
    format_amount_with_unit, format_cents, format_currency, home_path, humanize_plan_label,
    humanize_words_with, json_epoch_seconds, json_number, local_timestamp_label, oauth_origin,
    parse_iso_epoch, quota_pace_label, read_process_pipe, remaining_from_fraction, reset_label,
    run_cli_with_timeout, titlecase_ascii, used_percent_from_fraction, used_percent_label,
    window_minutes_label,
};

pub use self::refresh::MaterializedUsageAccounts;
pub use self::refresh::ProviderError;
pub use self::refresh::ProviderRateLimit;
pub use self::refresh::{
    MATERIALIZED_TMP_COUNTER, atomic_write_usage_json, split_provider_fetch,
    usage_error_is_rate_limited, usage_error_is_unauthorized, write_materialized_usage_accounts,
};
pub use self::view::{UsageViewInput, usage_view};
pub use self::view::{
    account_snapshot_views_from_cache, account_tab_label_for_parts, amp_status_bar_headline,
    bucket, cached_refreshing_view, cached_unavailable_view, compact_account_identity,
    decorate_surface_view, enrich_provider_tabs, mark_active_tab,
    preserve_cached_quota_on_failed_refresh, provider_tabs, quota_amounts_for_account_snapshot,
    spend_headline_label, status_bar_fresh_or_stale, status_bar_headline_for_surface,
    status_bar_label, status_bar_quota_labels, summary_bucket, timed_bucket, usage_account_tab_id,
    usage_tab_source_label, usage_tab_status_label, with_status_slot,
};
// Crate-visible re-exports for host overview/compact presentation (plan 008).
pub use self::format::{CliOutput, percent_before_used, run_cli_with_timeout_full};
pub use self::format::{
    PercentStyle, ResetStyle, UsageBucketPresentation, UsageFormatPrefs, usage_bucket_presentation,
    usage_detail_presentation, usage_display_status_label, usage_identity_presentation,
};
pub use self::format::{
    compact_duration_label, exact_reset_parenthetical, percent_headline, reset_label_with_prefs,
};

pub use cache::CachedUsage;
pub use cache::{UsageCache, UsageRefreshTarget};
pub use cache_keys::{
    cached_usage_for_capability, cached_usage_for_target, cached_usage_key_for_target,
    canonical_usage_cache_key, stable_cache_account_label, usage_cache_key_for_broker_account,
    usage_cache_key_for_view,
};
pub use consts::{
    AMP_HANDOFF_SECRETS_PATH, CLAUDE_CODE_USER_AGENT_FALLBACK, CLAUDE_HANDOFF_CREDENTIALS_PATH,
    CLAUDE_VERSION_TIMEOUT, CODEX_HANDOFF_AUTH_PATH, CODEX_OAUTH_CLIENT_ID, CODEX_OAUTH_TOKEN_URL,
    CODEX_RPC_INIT_TIMEOUT, CODEX_RPC_LAUNCH_COOLDOWN, CODEX_RPC_REQUEST_TIMEOUT,
    GROK_RPC_INIT_TIMEOUT, GROK_RPC_REQUEST_TIMEOUT, MATERIALIZED_USAGE_ACCOUNTS_PATH,
    PROVIDER_HTTP_TIMEOUT,
};
pub use consts::{GROK_HANDOFF_AUTH_PATH, PROVIDER_CLI_TIMEOUT, USAGE_SNAPSHOT_STORE_PATH};
pub use credentials::first_credential;
pub use credentials::{
    first_credential_with_path, read_json_file, resolve_identity, resolve_identity_with_extra,
};
pub use diagnostic::refresh_cached_updated_label;
pub use diagnostic::{now_epoch, relative_updated_label};
pub use fallback::unpollable_snapshot;
pub use fallback::unsupported_snapshot;
pub use http::retry_after_header_value;
pub use http::{
    ProviderHttpError, epoch_seconds_from_maybe_ms, get_json_bearer, normalize_url_or_host,
    provider_request, retry_after_header_seconds,
};
pub use io::{
    Fixed32Field, ManagedCliLaunchGate, ProtobufScan, VarintField, looks_like_protobuf_payload,
    read_varint, write_json_line,
};
pub use labels::resolved_usage_provider_label;
pub use labels::{
    broker_surface_id, estimate_caption, provider_display_label, usage_confidence_storage_label,
    usage_source_storage_label, usage_status_storage_label,
};
pub use labels::{env_dir_or_home, humanize_reason, humanize_window_label, severity_from_label};
pub use outcome::{
    ProviderPresence, capability_matches_surface, provider_outcome, resolve_surface, split_fetch,
};
pub use process_telemetry::{ChildOperation, complete_external_rpc, external_rpc_operation};
pub use surface::UsageSurface;
pub use transport::{parse_chatgpt_base_url, provider_http_client};
