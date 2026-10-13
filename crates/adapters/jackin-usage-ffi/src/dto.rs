// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! boltffi-safe mirrors of protocol usage views (string enums, no secrets).

use jackin_usage_provider_core::{PercentStyle, ResetStyle, UsageFormatPrefs};

/// Open configuration from Swift (paths only — no credentials).
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct OpenConfig {
    /// Optional data-dir override for hermetic tests/smoke only. Production is `None`.
    pub data_dir_override: Option<String>,
    /// Optional config-root override for hermetic tests/fixtures only. Production is `None`.
    pub config_root_override: Option<String>,
    /// Refresh floor seconds (clamped ≥ 60 in Rust).
    pub refresh_floor_secs: u64,
    /// Enabled surface ids; empty = all.
    pub enabled_surface_ids: Vec<String>,
    /// Whether the standard broker may dispatch live provider probes. `false`
    /// reads an existing broker publication without credential lookup or
    /// provider work. Not persisted.
    pub allow_live_probes: bool,
}

/// Surface row for Settings / list.
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct SurfaceDescriptorDto {
    pub id: String,
    pub label: String,
    pub agent: String,
    pub provider: Option<String>,
    pub enabled: bool,
}

/// Sanitized issue from the broker-owned usage publication.
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct DiscoveryDiagnosticDto {
    pub surface_id: Option<String>,
    pub scope_label: String,
    pub issue: String,
    pub message: String,
    pub display_label: String,
}

/// Monetary amount (minor units).
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct MoneyDto {
    pub amount_minor: i64,
    pub currency: String,
    pub exponent: u8,
}

/// One quota / spend bucket.
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct QuotaBucketDto {
    pub label: String,
    pub used_label: Option<String>,
    pub limit_label: Option<String>,
    pub remaining_percent: Option<u8>,
    pub reset_label: Option<String>,
    pub resets_at: Option<i64>,
    pub status_slot: Option<String>,
    pub pace_label: Option<String>,
    pub status: String,
    pub used_money: Option<MoneyDto>,
    pub limit_money: Option<MoneyDto>,
    pub severity: String,
    /// Rust-owned percentage segment text (segment 0), when present.
    pub remaining_label: Option<String>,
    /// Rust-owned complete semantic segments in display order.
    pub display_segments: Vec<String>,
    /// `display_segments` joined with the canonical `" · "` separator.
    pub display_label: String,
    /// Meter fill geometry only (remaining for normal/credits, used for Spend).
    pub meter_percent: Option<u8>,
}

/// One already-grouped visual line of a [`UsageDetailRowDto`] (1:1 mirror of the
/// Rust `UsagePresentationLine`). `leading`/`trailing` are finished strings.
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct UsagePresentationLineDto {
    pub leading: Option<String>,
    pub trailing: Option<String>,
}

/// One provider-detail row (1:1 mirror of the Rust `UsageDetailRow`). Every
/// visible string is Rust-owned; `kind`/`severity` are machine strings and
/// `meter_percent` is meter geometry only.
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct UsageDetailRowDto {
    pub row_id: String,
    /// `metadata` | `bucket` | `detail`
    pub kind: String,
    pub label: String,
    pub layout_lines: Vec<UsagePresentationLineDto>,
    pub display_label: String,
    pub meter_percent: Option<u8>,
    /// `normal` | `warn` | `danger`
    pub severity: String,
}

/// Rust-owned provider details assembled from broker-typed metric groups.
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct UsageDetailPresentationDto {
    pub rows: Vec<UsageDetailRowDto>,
}

/// Finished provider/account/activity identity copy.
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct UsageIdentityPresentationDto {
    pub provider_title: String,
    pub account_label: String,
    pub activity_label: String,
    /// `idle` | `updating` | `exceptional`
    pub activity_kind: String,
    pub accessibility_label: String,
}

/// One selected-account-aware provider glance row (1:1 mirror of the Rust
/// `HostProviderGlanceRow`). The Desktop status bar, popover, and Usage window
/// all consume this same Rust-owned row.
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct ProviderGlanceRowDto {
    pub surface_id: String,
    pub icon_key: String,
    pub fallback_glyph: String,
    pub usage_url: Option<String>,
    pub display_label: String,
    pub account_label: String,
    pub plan_label: Option<String>,
    pub glance_remaining_percent: Option<u8>,
    pub bar_label: String,
    pub headline: String,
    pub reset_label: Option<String>,
    pub compact_reset_label: Option<String>,
    pub exact_reset: Option<String>,
    pub status_word: String,
    pub is_refreshing: bool,
    pub status_label: String,
    pub severity: String,
    pub updated_label: String,
    pub activity_label: String,
    pub activity_kind: String,
    pub accessibility_label: String,
    pub last_error: Option<String>,
    pub dimmed: bool,
}

/// Full focused usage view for one surface.
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct UsageViewDto {
    pub identity: UsageIdentityPresentationDto,
    pub focused_agent: Option<String>,
    pub focused_provider: Option<String>,
    pub provider_label: String,
    pub account_label: String,
    pub username: Option<String>,
    pub plan_label: Option<String>,
    /// `None`: broker publications do not expose credential-origin paths.
    pub credential_origin: Option<String>,
    pub buckets: Vec<QuotaBucketDto>,
    pub status: String,
    /// Broker projection has no source-class field; this remains `none`.
    pub source: String,
    /// Broker projection has no confidence field; this remains `none`.
    pub confidence: String,
    pub fetched_at_epoch: i64,
    pub updated_label: String,
    pub status_bar_label: String,
    pub last_error: Option<String>,
    /// Honesty caption when estimated / local-log derived; `None` for authoritative.
    pub estimate_caption: Option<String>,
    /// Typed broker metric groups rendered as Rust-owned detail rows.
    pub detail_presentation: UsageDetailPresentationDto,
}

/// Presentation-time format prefs (string enums).
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct UsageFormatPrefsDto {
    /// `left` | `used`
    pub percent_style: String,
    /// `countdown` | `exact_clock`
    pub reset_style: String,
}

/// Overview row for glance popover / Usage-window sidebar.
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct OverviewRowDto {
    pub surface_id: String,
    pub display_label: String,
    pub headline: String,
    pub reset_label: Option<String>,
    pub exact_reset: Option<String>,
    pub status_word: String,
    pub severity: String,
}

/// One known account for a host surface (multi-account Desktop).
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct AccountDescriptorDto {
    pub surface_id: String,
    pub provider_column_label: String,
    pub account_key: String,
    pub account_label: String,
    pub plan_label: Option<String>,
    pub selected: bool,
    pub lifecycle: String,
    pub lifecycle_label: String,
    pub provenance: Vec<String>,
    pub provenance_label: String,
    pub plan_or_status_label: String,
    pub remaining_percent: Option<u8>,
    pub remaining_label: String,
    pub headline: String,
    pub reset_label: Option<String>,
    pub reset_display_label: String,
    pub exact_reset: Option<String>,
    pub status_word: String,
    pub status_label: String,
    pub severity: String,
    pub updated_label: String,
    pub last_error: Option<String>,
    pub dimmed: bool,
    pub accessibility_label: String,
}

/// Provider state when no stable account identity exists yet.
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct DesktopProviderStateDto {
    pub status_word: String,
    pub status_label: String,
    pub updated_label: String,
    pub last_error: Option<String>,
    pub is_refreshing: bool,
}

/// One Rust-ordered provider group in the atomic Desktop inventory.
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct DesktopProviderGroupDto {
    pub surface_id: String,
    pub display_label: String,
    pub icon_key: String,
    pub fallback_glyph: String,
    pub usage_url: Option<String>,
    pub account_column_label: String,
    pub plan_or_status_label: String,
    pub remaining_label: String,
    pub reset_display_label: String,
    pub accessibility_label: String,
    pub accounts: Vec<AccountDescriptorDto>,
    pub empty_state: Option<DesktopProviderStateDto>,
}

/// Atomic Rust-owned Desktop inventory.
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct DesktopInventoryDto {
    pub groups: Vec<DesktopProviderGroupDto>,
}

/// One grouped provider plus its exact selected account/detail snapshot.
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct DesktopProviderProjectionDto {
    pub group: DesktopProviderGroupDto,
    pub selected_account_route: SelectedAccountRouteDto,
    pub selected_usage: UsageViewDto,
}

/// Provider-scoped selected-account route with a validated status/key/notice shape.
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct SelectedAccountRouteDto {
    /// `unselected` | `resolving` | `available` | `unavailable`.
    pub status: String,
    pub account_key: Option<String>,
    /// Present only for `unavailable`; Rust-owned fixed notice copy.
    pub notice: Option<String>,
}

/// Complete immutable native Desktop state for one runtime generation.
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct DesktopProjectionDto {
    pub generation: u64,
    pub refresh_in_progress: bool,
    pub error_message: Option<String>,
    pub next_refresh_label: String,
    pub surfaces: Vec<SurfaceDescriptorDto>,
    pub providers: Vec<DesktopProviderProjectionDto>,
    pub glance_rows: Vec<ProviderGlanceRowDto>,
    pub status_bar_glance_rows: Vec<ProviderGlanceRowDto>,
    pub diagnostics: Vec<DiscoveryDiagnosticDto>,
}

/// One host event.
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct UsageEventDto {
    pub sequence: u64,
    pub kind: String,
    pub surface_id: Option<String>,
    pub detail: Option<String>,
}

/// Bounded event batch.
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct UsageEventBatchDto {
    pub next_cursor: u64,
    pub events: Vec<UsageEventDto>,
    pub resync_required: bool,
}

pub(crate) fn map_open_err(err: String) -> crate::error::UsageBridgeError {
    crate::error::UsageBridgeError::rejected("open", err)
}

pub(crate) fn map_runtime_err(err: String) -> crate::error::UsageBridgeError {
    if err == "runtime not open" {
        crate::error::UsageBridgeError::RuntimeUnavailable
    } else {
        crate::error::UsageBridgeError::rejected("runtime", err)
    }
}

pub(crate) fn parse_format_prefs(dto: UsageFormatPrefsDto) -> Result<UsageFormatPrefs, String> {
    let percent_style = match dto.percent_style.as_str() {
        "left" => PercentStyle::Left,
        "used" => PercentStyle::Used,
        other => return Err(format!("unknown percent_style: {other}")),
    };
    let reset_style = match dto.reset_style.as_str() {
        "countdown" => ResetStyle::Countdown,
        "exact_clock" => ResetStyle::ExactClock,
        other => return Err(format!("unknown reset_style: {other}")),
    };
    Ok(UsageFormatPrefs {
        percent_style,
        reset_style,
    })
}

/// Local projection-open settings and path-only broker activation scope.
#[derive(Debug, Clone)]
pub(crate) struct ProjectionOpenConfig {
    pub data_dir: std::path::PathBuf,
    pub config_root: std::path::PathBuf,
    pub operator_home: std::path::PathBuf,
    pub refresh_floor_secs: u64,
    pub enabled_surface_ids: Vec<String>,
    pub allow_live_probes: bool,
}

/// Build local presentation settings and broker path metadata from the FFI config.
pub(crate) fn to_projection_config(config: OpenConfig) -> Result<ProjectionOpenConfig, String> {
    let paths = jackin_core::JackinPaths::detect()
        .map_err(|_| "host path discovery unavailable".to_owned())?;
    let data_dir_override = config.data_dir_override.map(std::path::PathBuf::from);
    let operator_home = data_dir_override
        .clone()
        .unwrap_or_else(|| paths.home_dir.clone());
    let data_dir = data_dir_override.unwrap_or(paths.data_dir);
    let config_root = config
        .config_root_override
        .map(std::path::PathBuf::from)
        .unwrap_or(paths.config_dir);
    Ok(ProjectionOpenConfig {
        data_dir,
        config_root,
        operator_home,
        refresh_floor_secs: config.refresh_floor_secs,
        enabled_surface_ids: config.enabled_surface_ids,
        allow_live_probes: config.allow_live_probes,
    })
}
