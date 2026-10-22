// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{PercentStyle, ResetStyle, UsageFormatPrefs};

#[derive(Debug, Clone)]
#[boltffi::data]
pub struct OpenConfig {
    pub data_dir_override: Option<String>,
    pub config_root_override: Option<String>,
    pub refresh_floor_secs: u64,
    pub enabled_surface_ids: Vec<String>,
    pub allow_live_probes: bool,
}

#[derive(Debug, Clone)]
#[boltffi::data]
pub struct SurfaceDescriptorDto {
    pub id: String,
    pub label: String,
    pub agent: String,
    pub provider: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
#[boltffi::data]
pub struct DiscoveryDiagnosticDto {
    pub surface_id: Option<String>,
    pub scope_label: String,
    pub issue: String,
    pub message: String,
    pub display_label: String,
}

#[derive(Debug, Clone)]
#[boltffi::data]
pub struct MoneyDto {
    pub amount_minor: i64,
    pub currency: String,
    pub exponent: u8,
}

#[derive(Debug, Clone)]
#[boltffi::data]
pub struct CountQuotaDto {
    pub used: Option<u64>,
    pub limit: Option<u64>,
    pub remaining: Option<u64>,
    pub unit: String,
    pub period: String,
    pub provenance: String,
}

#[derive(Debug, Clone)]
#[boltffi::data]
pub struct QuotaBucketDto {
    pub label: String,
    pub used_label: Option<String>,
    pub limit_label: Option<String>,
    pub used_percent: Option<f64>,
    pub used_percent_origin: Option<String>,
    pub remaining_percent: Option<f64>,
    pub remaining_percent_origin: Option<String>,
    pub count_quota: Option<CountQuotaDto>,
    pub provider_quota_state: Option<String>,
    pub reset_label: Option<String>,
    pub resets_at: Option<i64>,
    pub status_slot: Option<String>,
    pub pace_label: Option<String>,
    pub status: String,
    pub used_money: Option<MoneyDto>,
    pub limit_money: Option<MoneyDto>,
    pub severity: String,
    pub remaining_label: Option<String>,
    pub display_segments: Vec<String>,
    pub display_label: String,
    pub meter_percent: Option<f64>,
    pub meter_side: Option<String>,
}

#[derive(Debug, Clone)]
#[boltffi::data]
pub struct UsagePresentationLineDto {
    pub leading: Option<String>,
    pub trailing: Option<String>,
}

#[derive(Debug, Clone)]
#[boltffi::data]
pub struct UsageDetailRowDto {
    pub row_id: String,
    pub kind: String,
    pub label: String,
    pub layout_lines: Vec<UsagePresentationLineDto>,
    pub display_label: String,
    pub meter_percent: Option<f64>,
    pub meter_side: Option<String>,
    pub severity: String,
}

#[derive(Debug, Clone)]
#[boltffi::data]
pub struct UsageDetailPresentationDto {
    pub rows: Vec<UsageDetailRowDto>,
}

#[derive(Debug, Clone)]
#[boltffi::data]
pub struct UsageIdentityPresentationDto {
    pub provider_title: String,
    pub account_label: String,
    pub activity_label: String,
    pub activity_kind: String,
    pub accessibility_label: String,
}

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
    pub glance_used_percent: Option<f64>,
    pub glance_used_percent_origin: Option<String>,
    pub glance_remaining_percent: Option<f64>,
    pub glance_remaining_percent_origin: Option<String>,
    pub glance_count_quota: Option<CountQuotaDto>,
    pub glance_provider_quota_state: Option<String>,
    pub glance_resets_at: Option<i64>,
    pub glance_meter_percent: Option<f64>,
    pub glance_meter_side: Option<String>,
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
    pub credential_origin: Option<String>,
    pub buckets: Vec<QuotaBucketDto>,
    pub status: String,
    pub source: String,
    pub confidence: String,
    pub fetched_at_epoch: i64,
    pub updated_label: String,
    pub status_bar_label: String,
    pub last_error: Option<String>,
    pub estimate_caption: Option<String>,
    pub detail_presentation: UsageDetailPresentationDto,
}

#[derive(Debug, Clone)]
#[boltffi::data]
pub struct UsageFormatPrefsDto {
    pub percent_style: String,
    pub reset_style: String,
}

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
    pub used_percent: Option<f64>,
    pub used_percent_origin: Option<String>,
    pub remaining_percent: Option<f64>,
    pub remaining_percent_origin: Option<String>,
    pub count_quota: Option<CountQuotaDto>,
    pub provider_quota_state: Option<String>,
    pub resets_at: Option<i64>,
    pub meter_percent: Option<f64>,
    pub meter_side: Option<String>,
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

#[derive(Debug, Clone)]
#[boltffi::data]
pub struct DesktopProviderStateDto {
    pub status_word: String,
    pub status_label: String,
    pub updated_label: String,
    pub last_error: Option<String>,
    pub is_refreshing: bool,
}

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

#[derive(Debug, Clone)]
#[boltffi::data]
pub struct DesktopInventoryDto {
    pub groups: Vec<DesktopProviderGroupDto>,
}

#[derive(Debug, Clone)]
#[boltffi::data]
pub struct DesktopProviderProjectionDto {
    pub group: DesktopProviderGroupDto,
    pub selected_account_route: SelectedAccountRouteDto,
    pub selected_usage: UsageViewDto,
}

#[derive(Debug, Clone)]
#[boltffi::data]
pub struct SelectedAccountRouteDto {
    pub status: String,
    pub account_key: Option<String>,
    pub notice: Option<String>,
}

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

#[derive(Debug, Clone)]
#[boltffi::data]
pub struct UsageEventDto {
    pub sequence: u64,
    pub kind: String,
    pub surface_id: Option<String>,
    pub detail: Option<String>,
}

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

#[derive(Debug, Clone)]
pub(crate) struct ProjectionOpenConfig {
    pub data_dir: std::path::PathBuf,
    pub config_root: std::path::PathBuf,
    pub operator_home: std::path::PathBuf,
    pub refresh_floor_secs: u64,
    pub enabled_surface_ids: Vec<String>,
    pub allow_live_probes: bool,
}

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
