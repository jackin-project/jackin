// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! boltffi-safe mirrors of protocol usage views (string enums, no secrets).

use jackin_protocol::control::{
    FocusedUsageView, Money, QuotaBucketView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use jackin_usage::host::{
    HostAccountDescriptor, HostDesktopInventory, HostDesktopProjection, HostDesktopProviderGroup,
    HostDesktopProviderProjection, HostDesktopProviderState, HostEventBatch, HostOverviewRow,
    HostSurfaceDescriptor, HostUsageEvent, UsageDiscoveryDiagnostic,
};
use jackin_usage::usage::{PercentStyle, ResetStyle, UsageFormatPrefs, estimate_caption};

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
    /// Whether live provider probes may dispatch. `false` = smoke/defense mode
    /// (no credential/file/env/CLI/network/Keychain resolution). Not persisted.
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

/// Sanitized config/credential discovery failure. Contains no source path or secret.
#[derive(Debug, Clone)]
#[boltffi::data]
pub struct DiscoveryDiagnosticDto {
    pub surface_id: Option<String>,
    pub scope_label: String,
    pub issue: String,
    pub message: String,
    pub display_label: String,
}

pub(crate) fn discovery_diagnostic_dto(
    diagnostic: UsageDiscoveryDiagnostic,
) -> DiscoveryDiagnosticDto {
    let message = diagnostic.issue.display_message().to_owned();
    let display_label = format!("{}: {message}", diagnostic.scope_label);
    DiscoveryDiagnosticDto {
        surface_id: diagnostic.surface_id,
        scope_label: diagnostic.scope_label,
        issue: diagnostic.issue.id().to_owned(),
        message,
        display_label,
    }
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

/// The complete Rust-owned provider-detail card (mirror of
/// `UsageDetailPresentation`). Rows are already in canonical order.
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

pub(crate) fn provider_glance_row_dto(
    row: jackin_usage::host::HostProviderGlanceRow,
) -> ProviderGlanceRowDto {
    ProviderGlanceRowDto {
        surface_id: row.surface_id,
        icon_key: row.icon_key,
        fallback_glyph: row.fallback_glyph,
        usage_url: row.usage_url,
        display_label: row.display_label,
        account_label: row.account_label,
        plan_label: row.plan_label,
        glance_remaining_percent: row.glance_remaining_percent,
        bar_label: row.bar_label,
        headline: row.headline,
        reset_label: row.reset_label,
        compact_reset_label: row.compact_reset_label,
        exact_reset: row.exact_reset,
        status_word: row.status_word,
        is_refreshing: row.is_refreshing,
        status_label: row.status_label,
        severity: row.severity,
        updated_label: row.updated_label,
        activity_label: row.activity_label,
        activity_kind: row.activity_kind,
        accessibility_label: row.accessibility_label,
        last_error: row.last_error,
        dimmed: row.dimmed,
    }
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
    /// Rust-owned Capsule-parity provider-detail card (same rows/strings/order
    /// as the Capsule usage dialog). The Usage window renders this verbatim.
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

pub(crate) fn account_dto(row: HostAccountDescriptor) -> AccountDescriptorDto {
    AccountDescriptorDto {
        surface_id: row.surface_id,
        provider_column_label: row.provider_column_label,
        account_key: row.account_key,
        account_label: row.account_label,
        plan_label: row.plan_label,
        selected: row.selected,
        lifecycle: row.lifecycle,
        lifecycle_label: row.lifecycle_label,
        provenance: row.provenance,
        provenance_label: row.provenance_label,
        plan_or_status_label: row.plan_or_status_label,
        remaining_percent: row.remaining_percent,
        remaining_label: row.remaining_label,
        headline: row.headline,
        reset_label: row.reset_label,
        reset_display_label: row.reset_display_label,
        exact_reset: row.exact_reset,
        status_word: row.status_word,
        status_label: row.status_label,
        severity: row.severity,
        updated_label: row.updated_label,
        last_error: row.last_error,
        dimmed: row.dimmed,
        accessibility_label: row.accessibility_label,
    }
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
    pub selected_account_key: Option<String>,
    pub selected_usage: UsageViewDto,
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

pub(crate) fn desktop_inventory_dto(inventory: HostDesktopInventory) -> DesktopInventoryDto {
    DesktopInventoryDto {
        groups: inventory
            .groups
            .into_iter()
            .map(desktop_provider_group_dto)
            .collect(),
    }
}

pub(crate) fn desktop_projection_dto(projection: HostDesktopProjection) -> DesktopProjectionDto {
    DesktopProjectionDto {
        generation: projection.generation,
        refresh_in_progress: projection.refresh_in_progress,
        error_message: projection.error_message,
        next_refresh_label: projection.next_refresh_label,
        surfaces: projection.surfaces.into_iter().map(surface_dto).collect(),
        providers: projection
            .providers
            .into_iter()
            .map(desktop_provider_projection_dto)
            .collect(),
        glance_rows: projection
            .glance_rows
            .into_iter()
            .map(provider_glance_row_dto)
            .collect(),
        status_bar_glance_rows: projection
            .status_bar_glance_rows
            .into_iter()
            .map(provider_glance_row_dto)
            .collect(),
        diagnostics: projection
            .diagnostics
            .into_iter()
            .map(discovery_diagnostic_dto)
            .collect(),
    }
}

fn desktop_provider_projection_dto(
    projection: HostDesktopProviderProjection,
) -> DesktopProviderProjectionDto {
    let account = projection.selected_account_key.as_deref().and_then(|key| {
        projection
            .group
            .accounts
            .iter()
            .find(|row| row.account_key == key)
    });
    DesktopProviderProjectionDto {
        group: desktop_provider_group_dto(projection.group.clone()),
        selected_account_key: projection.selected_account_key,
        selected_usage: view_dto_with_context(
            projection.selected_usage,
            projection.identity,
            account,
        ),
    }
}

fn desktop_provider_group_dto(group: HostDesktopProviderGroup) -> DesktopProviderGroupDto {
    DesktopProviderGroupDto {
        surface_id: group.surface_id,
        display_label: group.display_label,
        icon_key: group.icon_key,
        fallback_glyph: group.fallback_glyph,
        usage_url: group.usage_url,
        account_column_label: group.account_column_label,
        plan_or_status_label: group.plan_or_status_label,
        remaining_label: group.remaining_label,
        reset_display_label: group.reset_display_label,
        accessibility_label: group.accessibility_label,
        accounts: group.accounts.into_iter().map(account_dto).collect(),
        empty_state: group.empty_state.map(desktop_provider_state_dto),
    }
}

fn desktop_provider_state_dto(state: HostDesktopProviderState) -> DesktopProviderStateDto {
    DesktopProviderStateDto {
        status_word: state.status_word,
        status_label: state.status_label,
        updated_label: state.updated_label,
        last_error: state.last_error,
        is_refreshing: state.is_refreshing,
    }
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

fn detail_presentation_dto(
    view: &FocusedUsageView,
    account: Option<&HostAccountDescriptor>,
) -> UsageDetailPresentationDto {
    // Same Rust builder Capsule uses — one parity handoff, no second assembler.
    let presentation = jackin_usage::usage::usage_detail_presentation(view);
    let mut rows = presentation
        .rows
        .into_iter()
        .map(detail_row_dto)
        .collect::<Vec<_>>();
    if let Some(account) = account {
        let insert_at = rows
            .iter()
            .position(|row| row.kind == "bucket" || row.kind == "detail")
            .unwrap_or(rows.len());
        let mut context_rows = Vec::new();
        if !account.provenance_label.is_empty() {
            context_rows.push(metadata_detail_row_dto(
                "provenance",
                "Source",
                account.provenance_label.clone(),
            ));
        }
        context_rows.push(metadata_detail_row_dto(
            "lifecycle",
            "Availability",
            account.lifecycle_label.clone(),
        ));
        rows.splice(insert_at..insert_at, context_rows);
    }
    UsageDetailPresentationDto { rows }
}

pub(crate) fn view_dto(view: FocusedUsageView) -> UsageViewDto {
    let provider_title = jackin_usage::usage::provider_display_label(&view.account.provider_label);
    let identity = jackin_usage::usage::usage_identity_presentation(
        provider_title,
        &view,
        view.is_refreshing_placeholder(),
    );
    view_dto_with_context(view, identity, None)
}

fn view_dto_with_context(
    view: FocusedUsageView,
    identity: jackin_protocol::control::UsageIdentityPresentation,
    account: Option<&HostAccountDescriptor>,
) -> UsageViewDto {
    let caption = estimate_caption(&view);
    let detail_presentation = detail_presentation_dto(&view, account);
    UsageViewDto {
        identity: identity_dto(identity),
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
        detail_presentation,
    }
}

fn identity_dto(
    identity: jackin_protocol::control::UsageIdentityPresentation,
) -> UsageIdentityPresentationDto {
    UsageIdentityPresentationDto {
        provider_title: identity.provider_title,
        account_label: identity.account_label,
        activity_label: identity.activity_label,
        activity_kind: match identity.activity_kind {
            jackin_protocol::control::UsageActivityKind::Idle => "idle",
            jackin_protocol::control::UsageActivityKind::Updating => "updating",
            jackin_protocol::control::UsageActivityKind::Exceptional => "exceptional",
        }
        .to_owned(),
        accessibility_label: identity.accessibility_label,
    }
}

fn detail_row_dto(row: jackin_protocol::control::UsageDetailRow) -> UsageDetailRowDto {
    UsageDetailRowDto {
        row_id: row.row_id,
        kind: match row.kind {
            jackin_protocol::control::UsageDetailRowKind::Metadata => "metadata",
            jackin_protocol::control::UsageDetailRowKind::Bucket => "bucket",
            jackin_protocol::control::UsageDetailRowKind::Detail => "detail",
        }
        .to_owned(),
        label: row.label,
        layout_lines: row
            .layout_lines
            .into_iter()
            .map(|line| UsagePresentationLineDto {
                leading: line.leading,
                trailing: line.trailing,
            })
            .collect(),
        display_label: row.display_label,
        meter_percent: row.meter_percent,
        severity: match row.severity {
            jackin_protocol::control::UsageSeverity::Normal => "normal",
            jackin_protocol::control::UsageSeverity::Warn => "warn",
            jackin_protocol::control::UsageSeverity::Danger => "danger",
        }
        .to_owned(),
    }
}

fn metadata_detail_row_dto(row_id: &str, label: &str, value: String) -> UsageDetailRowDto {
    UsageDetailRowDto {
        row_id: row_id.to_owned(),
        kind: "metadata".to_owned(),
        label: label.to_owned(),
        layout_lines: vec![UsagePresentationLineDto {
            leading: Some(value.clone()),
            trailing: None,
        }],
        display_label: value,
        meter_percent: None,
        severity: "normal".to_owned(),
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
    // Rust owns the limits-only segment choice/order; Swift renders it verbatim.
    let presentation = jackin_usage::usage::usage_bucket_presentation(&bucket);
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
                jackin_protocol::control::StatusSlot::Daily => "daily",
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
        remaining_label: presentation.remaining_label,
        display_segments: presentation.display_segments,
        display_label: presentation.display_label,
        meter_percent: presentation.meter_percent,
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
pub(crate) fn to_host_config(
    config: OpenConfig,
) -> Result<jackin_usage::host::HostRuntimeConfig, String> {
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
    Ok(jackin_usage::host::HostRuntimeConfig {
        data_dir,
        refresh_floor_secs: config.refresh_floor_secs,
        enabled_surface_ids: config.enabled_surface_ids,
        probe_policy: if config.allow_live_probes {
            jackin_usage::host::HostProbePolicy::Live
        } else {
            jackin_usage::host::HostProbePolicy::Disabled
        },
        discovery_scope: jackin_usage::host::UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home,
        },
    })
}
