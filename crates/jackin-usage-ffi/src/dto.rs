// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! UniFFI-safe mirrors of protocol usage views (string enums, no secrets).

use jackin_protocol::control::{
    FocusedUsageView, Money, QuotaBucketView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use jackin_usage::host::{
    HostAccountDescriptor, HostEventBatch, HostOverviewRow, HostSurfaceDescriptor, HostUsageEvent,
};
use jackin_usage::usage::{PercentStyle, ResetStyle, UsageFormatPrefs, estimate_caption};

/// Open configuration from Swift (paths only — no credentials).
#[derive(Debug, Clone, uniffi::Record)]
pub struct OpenConfig {
    /// Absolute jackin data directory (`~/.jackin/data`).
    pub data_dir: String,
    /// Refresh floor seconds (clamped ≥ 60 in Rust).
    pub refresh_floor_secs: u64,
    /// Enabled surface ids; empty = all.
    pub enabled_surface_ids: Vec<String>,
}

/// Surface row for Settings / list.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SurfaceDescriptorDto {
    pub id: String,
    pub label: String,
    pub agent: String,
    pub provider: Option<String>,
    pub enabled: bool,
}

/// Monetary amount (minor units).
#[derive(Debug, Clone, uniffi::Record)]
pub struct MoneyDto {
    pub amount_minor: i64,
    pub currency: String,
    pub exponent: u8,
}

/// One quota / spend bucket.
#[derive(Debug, Clone, uniffi::Record)]
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
}

/// Full focused usage view for one surface.
#[derive(Debug, Clone, uniffi::Record)]
pub struct UsageViewDto {
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
    /// Honesty caption when estimated / local-log derived; `None` for authoritative.
    pub estimate_caption: Option<String>,
}

/// Presentation-time format prefs (string enums).
#[derive(Debug, Clone, uniffi::Record)]
pub struct UsageFormatPrefsDto {
    /// `left` | `used`
    pub percent_style: String,
    /// `countdown` | `exact_clock`
    pub reset_style: String,
}

/// Overview row for glance popover / Usage-window sidebar.
#[derive(Debug, Clone, uniffi::Record)]
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
#[derive(Debug, Clone, uniffi::Record)]
pub struct AccountDescriptorDto {
    pub surface_id: String,
    pub account_key: String,
    pub account_label: String,
    pub plan_label: Option<String>,
    pub selected: bool,
    pub remaining_percent: Option<u8>,
    pub status_word: String,
}

pub(crate) fn account_dto(row: HostAccountDescriptor) -> AccountDescriptorDto {
    AccountDescriptorDto {
        surface_id: row.surface_id,
        account_key: row.account_key,
        account_label: row.account_label,
        plan_label: row.plan_label,
        selected: row.selected,
        remaining_percent: row.remaining_percent,
        status_word: row.status_word,
    }
}

/// One host event.
#[derive(Debug, Clone, uniffi::Record)]
pub struct UsageEventDto {
    pub sequence: u64,
    pub kind: String,
    pub surface_id: Option<String>,
    pub detail: Option<String>,
}

/// Bounded event batch.
#[derive(Debug, Clone, uniffi::Record)]
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

pub(crate) fn surface_dto(row: HostSurfaceDescriptor) -> SurfaceDescriptorDto {
    SurfaceDescriptorDto {
        id: row.id,
        label: row.label,
        agent: row.agent,
        provider: row.provider,
        enabled: row.enabled,
    }
}

pub(crate) fn event_batch_dto(batch: HostEventBatch) -> UsageEventBatchDto {
    UsageEventBatchDto {
        next_cursor: batch.next_cursor,
        events: batch.events.into_iter().map(event_dto).collect(),
        resync_required: batch.resync_required,
    }
}

fn event_dto(event: HostUsageEvent) -> UsageEventDto {
    UsageEventDto {
        sequence: event.sequence,
        kind: event.kind,
        surface_id: event.surface_id,
        detail: event.detail,
    }
}

pub(crate) fn view_dto(view: FocusedUsageView) -> UsageViewDto {
    let caption = estimate_caption(&view);
    UsageViewDto {
        focused_agent: view.focused_agent,
        focused_provider: view.focused_provider,
        provider_label: view.account.provider_label,
        account_label: view.account.account_label,
        username: view.account.username,
        plan_label: view.account.plan_label,
        credential_origin: view.account.credential_origin,
        buckets: view.buckets.into_iter().map(bucket_dto).collect(),
        status: status_label(view.status).to_owned(),
        source: source_label(view.source).to_owned(),
        confidence: confidence_label(view.confidence).to_owned(),
        fetched_at_epoch: view.fetched_at_epoch,
        updated_label: view.updated_label,
        status_bar_label: view.status_bar_label,
        last_error: view.last_error,
        estimate_caption: caption,
    }
}

pub(crate) fn overview_row_dto(row: HostOverviewRow) -> OverviewRowDto {
    OverviewRowDto {
        surface_id: row.surface_id,
        display_label: row.display_label,
        headline: row.headline,
        reset_label: row.reset_label,
        exact_reset: row.exact_reset,
        status_word: row.status_word,
        severity: row.severity,
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

fn bucket_dto(bucket: QuotaBucketView) -> QuotaBucketDto {
    QuotaBucketDto {
        label: bucket.label,
        used_label: bucket.used_label,
        limit_label: bucket.limit_label,
        remaining_percent: bucket.remaining_percent,
        reset_label: bucket.reset_label,
        resets_at: bucket.resets_at,
        status_slot: bucket.status_slot.map(|slot| {
            match slot {
                jackin_protocol::control::StatusSlot::Session => "session",
                jackin_protocol::control::StatusSlot::Weekly => "weekly",
                jackin_protocol::control::StatusSlot::Spend => "spend",
            }
            .to_owned()
        }),
        pace_label: bucket.pace_label,
        status: status_label(bucket.status).to_owned(),
        used_money: bucket.used_money.map(money_dto),
        limit_money: bucket.limit_money.map(money_dto),
        severity: match bucket.severity {
            jackin_protocol::control::UsageSeverity::Normal => "normal",
            jackin_protocol::control::UsageSeverity::Warn => "warn",
            jackin_protocol::control::UsageSeverity::Danger => "danger",
        }
        .to_owned(),
    }
}

fn money_dto(money: Money) -> MoneyDto {
    MoneyDto {
        amount_minor: money.amount_minor,
        currency: money.currency,
        exponent: money.exponent,
    }
}

fn status_label(status: UsageSnapshotStatus) -> &'static str {
    jackin_usage::usage::usage_status_storage_label(status)
}

fn source_label(source: UsageSource) -> &'static str {
    jackin_usage::usage::usage_source_storage_label(source)
}

fn confidence_label(confidence: UsageConfidence) -> &'static str {
    jackin_usage::usage::usage_confidence_storage_label(confidence)
}

/// Build open config for the host runtime.
pub(crate) fn to_host_config(config: OpenConfig) -> jackin_usage::host::HostRuntimeConfig {
    jackin_usage::host::HostRuntimeConfig {
        data_dir: std::path::PathBuf::from(config.data_dir),
        refresh_floor_secs: config.refresh_floor_secs,
        enabled_surface_ids: config.enabled_surface_ids,
    }
}
