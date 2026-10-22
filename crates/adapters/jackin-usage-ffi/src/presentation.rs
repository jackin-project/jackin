// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::{SystemTime, UNIX_EPOCH};

use jackin_protocol::control::{
    CanonicalRemainingPercent, CanonicalUsedPercent, CountQuota, FocusedAccountHeader,
    FocusedUsageView, MeterPercentPoints, Money, PercentObservationOrigin, ProviderQuotaState,
    QuotaBucketView, RawPercentPoints, StatusSlot, UsageConfidence, UsageMeterSide, UsageSeverity,
    UsageSnapshotStatus, UsageSource,
};
use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageFreshnessPhaseV1, UsageIssueScopeV1, UsageIssueV1, UsageLifecycleV1,
    UsageLimitWindowV1, UsageMetricGroupKindV1, UsageMetricGroupV1, UsageMetricPeriodV1,
    UsageMetricValueV1, UsageProjectionRefreshStateV1, UsageProjectionV1, UsageProviderV1,
    UsageQuotaStateV1, UsageWindowCategoryV1,
};
use jackin_usage::host::{
    HostSurfaceId, HostUsageProjectionRuntime, HostUsageProjectionSelectedAccount,
};
use jackin_usage_provider_core::{
    PercentStyle, UsageFormatPrefs, quota_source_remaining_rank_points,
};

use crate::dto::{
    AccountDescriptorDto, CountQuotaDto, DesktopInventoryDto, DesktopProjectionDto,
    DesktopProviderGroupDto, DesktopProviderProjectionDto, DesktopProviderStateDto,
    DiscoveryDiagnosticDto, MoneyDto, OverviewRowDto, ProviderGlanceRowDto, QuotaBucketDto,
    SelectedAccountRouteDto, SurfaceDescriptorDto, UsageDetailPresentationDto, UsageDetailRowDto,
    UsageIdentityPresentationDto, UsagePresentationLineDto, UsageViewDto,
};

mod quota_labels;
use quota_labels::{
    metric_window_bar_label, metric_window_headline, metric_window_percent_label,
    window_display_summary, window_headline, window_numeric_label,
};

#[derive(Clone)]
struct QuotaFields {
    used_percent: Option<f64>,
    used_percent_origin: Option<String>,
    remaining_percent: Option<f64>,
    remaining_percent_origin: Option<String>,
    count_quota: Option<CountQuotaDto>,
    provider_quota_state: Option<String>,
    meter_percent: Option<f64>,
    meter_side: Option<String>,
}

fn count_quota_dto(count: CountQuota) -> CountQuotaDto {
    CountQuotaDto {
        used: count.used,
        limit: count.limit,
        remaining: count.remaining,
        unit: count.unit.wire_label().to_owned(),
        period: count.period.wire_label().to_owned(),
        provenance: count.provenance.wire_label().to_owned(),
    }
}

struct QuotaFieldInputs {
    used_percent: Option<RawPercentPoints>,
    used_percent_origin: Option<PercentObservationOrigin>,
    remaining_percent: Option<RawPercentPoints>,
    remaining_percent_origin: Option<PercentObservationOrigin>,
    count_quota: Option<CountQuota>,
    meter_percent: Option<MeterPercentPoints>,
    meter_side: Option<UsageMeterSide>,
    provider_quota_state: Option<ProviderQuotaState>,
}

fn quota_fields(input: QuotaFieldInputs) -> QuotaFields {
    let QuotaFieldInputs {
        used_percent,
        used_percent_origin,
        remaining_percent,
        remaining_percent_origin,
        count_quota,
        meter_percent,
        meter_side,
        provider_quota_state,
    } = input;
    QuotaFields {
        used_percent: used_percent.map(RawPercentPoints::get),
        used_percent_origin: used_percent_origin.map(|origin| origin.wire_label().to_owned()),
        remaining_percent: remaining_percent.map(RawPercentPoints::get),
        remaining_percent_origin: remaining_percent_origin
            .map(|origin| origin.wire_label().to_owned()),
        count_quota: count_quota.map(count_quota_dto),
        provider_quota_state: provider_quota_state
            .map(|provider_state| provider_state.wire_label().to_owned()),
        meter_percent: meter_percent.map(MeterPercentPoints::get),
        meter_side: meter_side.map(|side| side.wire_label().to_owned()),
    }
}

fn window_quota_fields(window: &UsageLimitWindowV1) -> QuotaFields {
    quota_fields(QuotaFieldInputs {
        used_percent: window.used_percent,
        used_percent_origin: window.used_percent_origin,
        remaining_percent: window.remaining_percent,
        remaining_percent_origin: window.remaining_percent_origin,
        count_quota: window.count_quota,
        meter_percent: window.meter_percent,
        meter_side: window.meter_side,
        provider_quota_state: window.provider_quota_state,
    })
}

fn metric_window_quota_fields(group: &UsageMetricGroupV1) -> Option<QuotaFields> {
    let UsageMetricValueV1::Window {
        used_percent,
        used_percent_origin,
        remaining_percent,
        remaining_percent_origin,
        count_quota,
        provider_quota_state,
        meter_percent,
        meter_side,
        ..
    } = &group.value
    else {
        return None;
    };
    Some(quota_fields(QuotaFieldInputs {
        used_percent: *used_percent,
        used_percent_origin: *used_percent_origin,
        remaining_percent: *remaining_percent,
        remaining_percent_origin: *remaining_percent_origin,
        count_quota: *count_quota,
        meter_percent: *meter_percent,
        meter_side: *meter_side,
        provider_quota_state: *provider_quota_state,
    }))
}

pub(crate) fn surface_rows(enabled: &[String]) -> Vec<SurfaceDescriptorDto> {
    HostSurfaceId::ALL
        .iter()
        .copied()
        .map(|surface| SurfaceDescriptorDto {
            id: surface.id().to_owned(),
            label: surface.label().to_owned(),
            agent: surface.agent_slug().to_owned(),
            provider: surface.provider_label().map(str::to_owned),
            enabled: enabled.is_empty() || enabled.iter().any(|id| id == surface.id()),
        })
        .collect()
}

pub(crate) fn discovery_diagnostics(projection: &UsageProjectionV1) -> Vec<DiscoveryDiagnosticDto> {
    projection
        .issues
        .iter()
        .map(|issue| issue_dto(issue, None))
        .chain(projection.providers.iter().flat_map(|provider| {
            let surface = surface_for_provider_id(&provider.provider_id);
            provider
                .issues
                .iter()
                .map(move |issue| issue_dto(issue, surface))
                .chain(provider.accounts.iter().flat_map(move |account| {
                    account
                        .issues
                        .iter()
                        .map(move |issue| issue_dto(issue, surface))
                        .chain(account.metric_groups.iter().flat_map(move |group| {
                            group
                                .issues
                                .iter()
                                .map(move |issue| issue_dto(issue, surface))
                        }))
                }))
        }))
        .chain(projection.unresolved.iter().flat_map(|unresolved| {
            let surface = surface_for_provider_id(&unresolved.provider_id);
            unresolved
                .issues
                .iter()
                .map(move |issue| issue_dto(issue, surface))
        }))
        .collect()
}

fn issue_dto(issue: &UsageIssueV1, surface: Option<HostSurfaceId>) -> DiscoveryDiagnosticDto {
    let scope_label = match issue.scope {
        UsageIssueScopeV1::Projection => "usage projection".to_owned(),
        UsageIssueScopeV1::Provider => surface.map_or_else(
            || "usage provider".to_owned(),
            |surface| surface.label().to_owned(),
        ),
        UsageIssueScopeV1::Account => "provider account".to_owned(),
        UsageIssueScopeV1::Window => "quota window".to_owned(),
        UsageIssueScopeV1::Group => "usage metric".to_owned(),
    };
    DiscoveryDiagnosticDto {
        surface_id: surface.map(|surface| surface.id().to_owned()),
        display_label: format!("{scope_label}: {}", issue.message),
        scope_label,
        issue: issue.code.clone(),
        message: issue.message.clone(),
    }
}

pub(crate) fn desktop_inventory(
    runtime: &HostUsageProjectionRuntime,
    enabled: &[String],
    format_prefs: UsageFormatPrefs,
) -> Result<DesktopInventoryDto, String> {
    let mut groups = Vec::new();
    for surface in HostSurfaceId::ALL.iter().copied() {
        if !is_surface_enabled(enabled, surface) {
            continue;
        }
        let presentation = runtime.provider_presentation(surface.id())?;
        let Some(provider) = presentation.provider else {
            if presentation.unresolved_capabilities.is_empty() {
                continue;
            }
            groups.push(provider_group(
                surface,
                None,
                &[],
                Some(provider_state(
                    projection_issue_message(runtime.projection(), surface).or_else(|| {
                        presentation
                            .unresolved_capabilities
                            .first()
                            .map(|unresolved| lifecycle_display(unresolved.state).to_owned())
                    }),
                    runtime.projection().refresh_state == UsageProjectionRefreshStateV1::Refreshing,
                )),
            ));
            continue;
        };
        let accounts = provider
            .accounts
            .iter()
            .map(|account| {
                account_dto(
                    surface,
                    provider,
                    account,
                    format_prefs,
                    match presentation.selected_account {
                        HostUsageProjectionSelectedAccount::Available {
                            canonical_account_id,
                            ..
                        }
                        | HostUsageProjectionSelectedAccount::Unavailable {
                            canonical_account_id,
                        } => canonical_account_id == account.canonical_account_id.as_str(),
                        HostUsageProjectionSelectedAccount::Unselected => false,
                    },
                )
            })
            .collect::<Vec<_>>();
        let empty_state = accounts.is_empty().then(|| {
            provider_state(
                provider
                    .issues
                    .first()
                    .map(|issue| issue.message.clone())
                    .or_else(|| projection_issue_message(runtime.projection(), surface)),
                provider.freshness.phase == UsageFreshnessPhaseV1::Refreshing,
            )
        });
        groups.push(provider_group(
            surface,
            Some(provider),
            &accounts,
            empty_state,
        ));
    }
    Ok(DesktopInventoryDto { groups })
}

fn provider_group(
    surface: HostSurfaceId,
    provider: Option<&UsageProviderV1>,
    accounts: &[AccountDescriptorDto],
    empty_state: Option<DesktopProviderStateDto>,
) -> DesktopProviderGroupDto {
    let selected = accounts.iter().find(|account| account.selected);
    let account_column_label = selected.map_or_else(
        || "Account".to_owned(),
        |account| account.account_label.clone(),
    );
    let plan_or_status_label = selected
        .map(|account| account.plan_or_status_label.clone())
        .or_else(|| {
            provider.and_then(|provider| provider.issues.first().map(|issue| issue.message.clone()))
        })
        .unwrap_or_else(|| "—".to_owned());
    let remaining_label =
        selected.map_or_else(|| "—".to_owned(), |account| account.remaining_label.clone());
    let reset_display_label = selected.map_or_else(
        || "—".to_owned(),
        |account| account.reset_display_label.clone(),
    );
    DesktopProviderGroupDto {
        surface_id: surface.id().to_owned(),
        display_label: surface.label().to_owned(),
        icon_key: surface.id().to_owned(),
        fallback_glyph: surface.fallback_glyph().to_owned(),
        usage_url: surface.usage_url().map(str::to_owned),
        account_column_label,
        plan_or_status_label,
        remaining_label,
        reset_display_label,
        accessibility_label: format!("{} usage", surface.label()),
        accounts: accounts.to_vec(),
        empty_state,
    }
}

fn provider_state(message: Option<String>, refreshing: bool) -> DesktopProviderStateDto {
    DesktopProviderStateDto {
        status_word: if refreshing {
            "updating"
        } else {
            "unavailable"
        }
        .to_owned(),
        status_label: message.clone().unwrap_or_else(|| {
            if refreshing {
                "Updating"
            } else {
                "Unavailable"
            }
            .to_owned()
        }),
        updated_label: "No successful update".to_owned(),
        last_error: message,
        is_refreshing: refreshing,
    }
}

pub(crate) fn desktop_projection(
    runtime: &HostUsageProjectionRuntime,
    enabled: &[String],
    format_prefs: UsageFormatPrefs,
    status_bar_max: u32,
    refresh_floor_secs: u64,
    last_refresh_elapsed: Option<std::time::Duration>,
) -> Result<DesktopProjectionDto, String> {
    let inventory = desktop_inventory(runtime, enabled, format_prefs)?;
    let mut providers = Vec::new();
    for group in &inventory.groups {
        let surface = HostSurfaceId::from_id(&group.surface_id)
            .ok_or_else(|| format!("unknown surface: {}", group.surface_id))?;
        let presentation = runtime.provider_presentation(surface.id())?;
        let (route, account) = match presentation.selected_account {
            HostUsageProjectionSelectedAccount::Unselected => (
                SelectedAccountRouteDto {
                    status: "unselected".to_owned(),
                    account_key: None,
                    notice: None,
                },
                None,
            ),
            HostUsageProjectionSelectedAccount::Available {
                canonical_account_id,
                account,
            } => (
                SelectedAccountRouteDto {
                    status: "available".to_owned(),
                    account_key: Some(canonical_account_id.to_owned()),
                    notice: None,
                },
                Some(account),
            ),
            HostUsageProjectionSelectedAccount::Unavailable {
                canonical_account_id,
            } => (
                SelectedAccountRouteDto {
                    status: "unavailable".to_owned(),
                    account_key: Some(canonical_account_id.to_owned()),
                    notice: Some(
                        "The selected account is not in the current broker publication".to_owned(),
                    ),
                },
                None,
            ),
        };
        let selected_usage = account.map_or_else(
            || empty_view(surface, route.notice.clone()),
            |account| view_dto(surface, account, format_prefs),
        );
        providers.push(DesktopProviderProjectionDto {
            group: group.clone(),
            selected_account_route: route,
            selected_usage,
        });
    }
    let ranked_glance_rows = provider_glance_rows_with_reset_rank(runtime, enabled, format_prefs)?;
    let glance_rows = ranked_glance_rows
        .iter()
        .map(|(row, _, _, _)| row.clone())
        .collect::<Vec<_>>();
    let status_rows = select_status_bar_provider_glance_rows(ranked_glance_rows, status_bar_max);
    let projection = runtime.projection();
    let error_message = projection.issues.first().map(|issue| issue.message.clone());
    Ok(DesktopProjectionDto {
        generation: projection.broker_generation,
        refresh_in_progress: projection.refresh_state == UsageProjectionRefreshStateV1::Refreshing,
        error_message,
        next_refresh_label: next_refresh_label(refresh_floor_secs, last_refresh_elapsed),
        surfaces: surface_rows(enabled),
        providers,
        glance_rows,
        status_bar_glance_rows: status_rows,
        diagnostics: discovery_diagnostics(projection),
    })
}

fn glance_row_has_remaining_capacity(row: &ProviderGlanceRowDto) -> bool {
    glance_remaining_capacity(row).is_some_and(|remaining| remaining > 0.0)
}

fn glance_remaining_capacity(row: &ProviderGlanceRowDto) -> Option<f64> {
    match (row.glance_meter_percent, row.glance_meter_side.as_deref()) {
        (Some(percent), Some("remaining")) => Some(percent),
        (Some(percent), Some("used")) => Some(100.0 - percent),
        _ => None,
    }
}

pub(crate) fn provider_glance_rows(
    runtime: &HostUsageProjectionRuntime,
    enabled: &[String],
    format_prefs: UsageFormatPrefs,
) -> Result<Vec<ProviderGlanceRowDto>, String> {
    Ok(
        provider_glance_rows_with_reset_rank(runtime, enabled, format_prefs)?
            .into_iter()
            .map(|(row, _, _, _)| row)
            .collect(),
    )
}

pub(crate) fn status_bar_provider_glance_rows(
    runtime: &HostUsageProjectionRuntime,
    enabled: &[String],
    format_prefs: UsageFormatPrefs,
    status_bar_max: u32,
) -> Result<Vec<ProviderGlanceRowDto>, String> {
    let rows = provider_glance_rows_with_reset_rank(runtime, enabled, format_prefs)?;
    Ok(select_status_bar_provider_glance_rows(rows, status_bar_max))
}

fn select_status_bar_provider_glance_rows(
    rows: Vec<ProviderGlanceRowWithResetRank>,
    status_bar_max: u32,
) -> Vec<ProviderGlanceRowDto> {
    let mut status_rows = rows
        .into_iter()
        .filter(|(row, _, _, provider_state)| {
            provider_state.is_some() || glance_row_has_remaining_capacity(row)
        })
        .collect::<Vec<_>>();
    status_rows.sort_by(
        |(_, left_reset, left_rank, _), (_, right_reset, right_rank, _)| {
            left_reset
                .is_none()
                .cmp(&right_reset.is_none())
                .then_with(|| {
                    left_reset
                        .unwrap_or(i64::MAX)
                        .cmp(&right_reset.unwrap_or(i64::MAX))
                })
                .then_with(|| match (left_rank, right_rank) {
                    (Some(left), Some(right)) => left.total_cmp(right),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                })
        },
    );
    status_rows.truncate(status_bar_max.clamp(1, 3) as usize);
    status_rows.into_iter().map(|(row, _, _, _)| row).collect()
}

type ProviderGlanceRowWithResetRank = (
    ProviderGlanceRowDto,
    Option<i64>,
    Option<f64>,
    Option<ProviderQuotaState>,
);

fn provider_glance_rows_with_reset_rank(
    runtime: &HostUsageProjectionRuntime,
    enabled: &[String],
    format_prefs: UsageFormatPrefs,
) -> Result<Vec<ProviderGlanceRowWithResetRank>, String> {
    let mut rows = Vec::new();
    for surface in HostSurfaceId::DESKTOP_PROVIDER_ORDER.iter().copied() {
        if !is_surface_enabled(enabled, surface) {
            continue;
        }
        let presentation = runtime.provider_presentation(surface.id())?;
        if let HostUsageProjectionSelectedAccount::Available { account, .. } =
            presentation.selected_account
        {
            let metric_window = presentation.glance_metric_group;
            let reset_at_epoch = glance_reset_epoch(account, metric_window);
            let remaining_rank_points = if let Some(group) = metric_window {
                metric_window_remaining_rank_points(group)
            } else {
                account.windows.first().and_then(|window| {
                    quota_source_remaining_rank_points(
                        window.count_quota,
                        window.used_percent,
                        window.used_percent_origin,
                        window.remaining_percent,
                        window.remaining_percent_origin,
                        window.money_used_percent,
                        window.provider_quota_state,
                    )
                })
            };
            let provider_quota_state = metric_window
                .and_then(metric_window_provider_quota_state)
                .or_else(|| {
                    metric_window
                        .is_none()
                        .then(|| {
                            account
                                .windows
                                .first()
                                .and_then(|window| window.provider_quota_state)
                        })
                        .flatten()
                });
            rows.push((
                glance_row(
                    surface,
                    presentation.provider,
                    account,
                    metric_window,
                    format_prefs,
                ),
                reset_at_epoch,
                remaining_rank_points,
                provider_quota_state,
            ));
        }
    }
    Ok(rows)
}

fn glance_reset_epoch(
    account: &UsageAccountV1,
    metric_window: Option<&UsageMetricGroupV1>,
) -> Option<i64> {
    metric_window.map_or_else(
        || {
            account
                .windows
                .first()
                .and_then(|window| window.reset_at_epoch)
        },
        |group| group.reset_at_epoch,
    )
}

fn metric_window_remaining_rank_points(group: &UsageMetricGroupV1) -> Option<f64> {
    let UsageMetricValueV1::Window {
        used_percent,
        used_percent_origin,
        remaining_percent,
        remaining_percent_origin,
        count_quota,
        provider_quota_state,
        money_used_percent,
        ..
    } = &group.value
    else {
        return None;
    };
    quota_source_remaining_rank_points(
        *count_quota,
        *used_percent,
        *used_percent_origin,
        *remaining_percent,
        *remaining_percent_origin,
        *money_used_percent,
        *provider_quota_state,
    )
}

fn metric_window_provider_quota_state(group: &UsageMetricGroupV1) -> Option<ProviderQuotaState> {
    let UsageMetricValueV1::Window {
        provider_quota_state,
        ..
    } = &group.value
    else {
        return None;
    };
    *provider_quota_state
}

pub(crate) fn glance_row(
    surface: HostSurfaceId,
    provider: Option<&UsageProviderV1>,
    account: &UsageAccountV1,
    metric_window: Option<&UsageMetricGroupV1>,
    format_prefs: UsageFormatPrefs,
) -> ProviderGlanceRowDto {
    let window = account.windows.first();
    let quota_fields = match metric_window {
        Some(group) => metric_window_quota_fields(group),
        None => window.map(window_quota_fields),
    };
    let quota_state = metric_window
        .map(|group| group.quota_state)
        .or_else(|| window.map(|window| window.quota_state));
    let severity = quota_state.map_or("normal", severity_label);
    let status = metric_window.map_or_else(|| account_status(account), metric_status_for_glance);
    let activity_kind = metric_window.map_or_else(
        || activity_kind(account),
        |group| {
            if group.phase == UsageFreshnessPhaseV1::Refreshing {
                "updating"
            } else if status == "fresh" {
                "idle"
            } else {
                "exceptional"
            }
        },
    );
    let last_success_at = metric_window.map_or(account.freshness.last_good_at_epoch, |group| {
        group.last_success_at_epoch
    });
    let activity_label = match (metric_window, activity_kind) {
        (_, "updating") => "Updating".to_owned(),
        (_, "idle") => updated_label(last_success_at),
        (Some(group), _) => group
            .issues
            .first()
            .map_or_else(|| status.to_owned(), |issue| issue.message.clone()),
        (None, _) => account
            .issues
            .first()
            .map_or_else(|| status.to_owned(), |issue| issue.message.clone()),
    };
    let account_label = account.display_label.clone();
    let last_error = metric_window.map_or_else(
        || {
            account
                .issues
                .first()
                .or_else(|| provider.and_then(|provider| provider.issues.first()))
                .map(|issue| issue.message.clone())
        },
        |group| group.issues.first().map(|issue| issue.message.clone()),
    );
    let (label, headline, bar_label) = if let Some(group) = metric_window {
        let label = metric_group_summary(group);
        let headline = metric_window_headline(group, format_prefs).unwrap_or_else(|| label.clone());
        let bar_label = metric_window_bar_label(group, format_prefs.percent_style)
            .unwrap_or_else(|| label.clone());
        (label, headline, bar_label)
    } else {
        let label = window.map_or_else(
            || "Usage unavailable".to_owned(),
            |window| window_display_summary(window, format_prefs),
        );
        let headline = window.map_or_else(
            || label.clone(),
            |window| window_headline(window, format_prefs),
        );
        let bar_label = window
            .and_then(|window| window_numeric_label(window, format_prefs))
            .or_else(|| window.map(|window| window_display_summary(window, format_prefs)))
            .unwrap_or_else(|| "—".to_owned());
        (label, headline, bar_label)
    };
    let reset = metric_window.map_or_else(
        || window.and_then(|window| formatted_reset(window, format_prefs)),
        |group| formatted_epoch_reset(group.reset_at_epoch, format_prefs),
    );
    let updated_label = updated_label(last_success_at);
    let plan_label = account.plan_label.clone();
    let display_label = surface.label().to_owned();
    ProviderGlanceRowDto {
        surface_id: surface.id().to_owned(),
        icon_key: surface.id().to_owned(),
        fallback_glyph: surface.fallback_glyph().to_owned(),
        usage_url: surface.usage_url().map(str::to_owned),
        display_label,
        account_label: account_label.clone(),
        plan_label,
        glance_used_percent: quota_fields.as_ref().and_then(|fields| fields.used_percent),
        glance_used_percent_origin: quota_fields
            .as_ref()
            .and_then(|fields| fields.used_percent_origin.clone()),
        glance_remaining_percent: quota_fields
            .as_ref()
            .and_then(|fields| fields.remaining_percent),
        glance_remaining_percent_origin: quota_fields
            .as_ref()
            .and_then(|fields| fields.remaining_percent_origin.clone()),
        glance_count_quota: quota_fields
            .as_ref()
            .and_then(|fields| fields.count_quota.clone()),
        glance_provider_quota_state: quota_fields
            .as_ref()
            .and_then(|fields| fields.provider_quota_state.clone()),
        glance_resets_at: glance_reset_epoch(account, metric_window),
        glance_meter_percent: quota_fields
            .as_ref()
            .and_then(|fields| fields.meter_percent),
        glance_meter_side: quota_fields
            .as_ref()
            .and_then(|fields| fields.meter_side.clone()),
        bar_label: bar_label.clone(),
        headline,
        reset_label: reset.clone(),
        compact_reset_label: reset.clone(),
        exact_reset: glance_reset_epoch(account, metric_window)
            .map(jackin_usage_provider_core::local_timestamp_label),
        status_word: status.to_owned(),
        is_refreshing: metric_window.map_or(
            account.freshness.phase == UsageFreshnessPhaseV1::Refreshing,
            |group| group.phase == UsageFreshnessPhaseV1::Refreshing,
        ),
        status_label: status.to_owned(),
        severity: severity.to_owned(),
        updated_label,
        activity_label: activity_label.clone(),
        activity_kind: activity_kind.to_owned(),
        accessibility_label: format!(
            "{}: {}, {}, {}",
            surface.label(),
            account_label,
            label,
            activity_label
        ),
        last_error,
        dimmed: status != "fresh",
    }
}

pub(crate) fn account_dto(
    surface: HostSurfaceId,
    provider: &UsageProviderV1,
    account: &UsageAccountV1,
    format_prefs: UsageFormatPrefs,
    selected: bool,
) -> AccountDescriptorDto {
    let window = account.windows.first();
    let metric_window = account
        .metric_groups
        .iter()
        .filter(|group| {
            group.kind == UsageMetricGroupKindV1::Window
                && matches!(&group.value, UsageMetricValueV1::Window { .. })
        })
        .min_by_key(|group| group.rank);
    let quota_fields = window
        .map(window_quota_fields)
        .or_else(|| metric_window.and_then(metric_window_quota_fields));
    let status = if window.is_some() {
        account_status(account)
    } else {
        metric_window.map_or_else(
            || account_status(account),
            |group| metric_status(group, account),
        )
    };
    let severity = window
        .map(|window| severity_label(window.quota_state))
        .or_else(|| metric_window.map(|group| severity_label(group.quota_state)))
        .unwrap_or("normal");
    let remaining_label = window
        .map(|window| window_display_summary(window, format_prefs))
        .or_else(|| metric_window.map(metric_group_summary))
        .unwrap_or_else(|| "—".to_owned());
    let resets_at = window.map_or_else(
        || metric_window.and_then(|group| group.reset_at_epoch),
        |window| window.reset_at_epoch,
    );
    let reset_label = window.map_or_else(
        || {
            metric_window
                .and_then(|group| formatted_epoch_reset(group.reset_at_epoch, format_prefs))
        },
        |window| formatted_reset(window, format_prefs),
    );
    let headline = window
        .map(|window| window_headline(window, format_prefs))
        .or_else(|| metric_window.and_then(|group| metric_window_headline(group, format_prefs)))
        .unwrap_or_else(|| remaining_label.clone());
    let selected_group_error = window
        .is_none()
        .then(|| metric_window.and_then(|group| group.issues.first()))
        .flatten();
    let last_success_at_epoch = if window.is_some() {
        account.freshness.last_good_at_epoch
    } else {
        metric_window
            .and_then(|group| group.last_success_at_epoch)
            .or(account.freshness.last_good_at_epoch)
    };
    let lifecycle = lifecycle_label(account.lifecycle).to_owned();
    let lifecycle_label = lifecycle_display(account.lifecycle).to_owned();
    let provenance_label = format!(
        "{} configuration observation{}",
        account.provenance_count,
        if account.provenance_count == 1 {
            ""
        } else {
            "s"
        }
    );
    AccountDescriptorDto {
        surface_id: surface.id().to_owned(),
        provider_column_label: surface.label().to_owned(),
        account_key: account.canonical_account_id.clone(),
        account_label: account.display_label.clone(),
        plan_label: account.plan_label.clone(),
        selected,
        lifecycle,
        lifecycle_label,
        provenance: Vec::new(),
        provenance_label,
        plan_or_status_label: account
            .plan_label
            .clone()
            .or_else(|| account.status_label.clone())
            .unwrap_or_else(|| "—".to_owned()),
        used_percent: quota_fields.as_ref().and_then(|fields| fields.used_percent),
        used_percent_origin: quota_fields
            .as_ref()
            .and_then(|fields| fields.used_percent_origin.clone()),
        remaining_percent: quota_fields
            .as_ref()
            .and_then(|fields| fields.remaining_percent),
        remaining_percent_origin: quota_fields
            .as_ref()
            .and_then(|fields| fields.remaining_percent_origin.clone()),
        count_quota: quota_fields
            .as_ref()
            .and_then(|fields| fields.count_quota.clone()),
        provider_quota_state: quota_fields
            .as_ref()
            .and_then(|fields| fields.provider_quota_state.clone()),
        resets_at,
        meter_percent: quota_fields
            .as_ref()
            .and_then(|fields| fields.meter_percent),
        meter_side: quota_fields
            .as_ref()
            .and_then(|fields| fields.meter_side.clone()),
        remaining_label,
        headline,
        reset_label: reset_label.clone(),
        reset_display_label: reset_label.unwrap_or_else(|| "—".to_owned()),
        exact_reset: resets_at.map(jackin_usage_provider_core::local_timestamp_label),
        status_word: status.to_owned(),
        status_label: status.to_owned(),
        severity: severity.to_owned(),
        updated_label: updated_label(last_success_at_epoch),
        last_error: account
            .issues
            .first()
            .or(selected_group_error)
            .or_else(|| provider.issues.first())
            .map(|issue| issue.message.clone()),
        dimmed: status != "fresh",
        accessibility_label: format!("{} {} {}", surface.label(), account.display_label, status),
    }
}

pub(crate) fn view_dto(
    surface: HostSurfaceId,
    account: &UsageAccountV1,
    format_prefs: UsageFormatPrefs,
) -> UsageViewDto {
    let view = focused_view(surface, account, format_prefs);
    let title = surface.label().to_owned();
    let identity = jackin_usage_provider_core::usage_identity_presentation(
        &title,
        &view,
        account.freshness.phase == UsageFreshnessPhaseV1::Refreshing,
    );
    let detail_presentation = typed_detail_presentation(account, format_prefs);
    UsageViewDto {
        identity: UsageIdentityPresentationDto {
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
        },
        focused_agent: Some(surface.agent_slug().to_owned()),
        focused_provider: Some(surface.provider_id().to_owned()),
        provider_label: surface.label().to_owned(),
        account_label: account.display_label.clone(),
        username: None,
        plan_label: account.plan_label.clone(),
        credential_origin: None,
        buckets: view
            .buckets
            .into_iter()
            .enumerate()
            .map(|(index, bucket)| {
                let source = account.windows.get(index).map(window_quota_fields);
                let mut dto = bucket_dto(bucket, source);
                if let Some(window) = account.windows.get(index) {
                    if window.provider_quota_state.is_some()
                        || (window.money_used_percent.is_some()
                            && (window.used_percent.is_some()
                                || window.remaining_percent.is_some()))
                    {
                        let summary = window_display_summary(window, format_prefs);
                        dto.display_segments = summary.split(" · ").map(str::to_owned).collect();
                        dto.display_label = summary.clone();
                        dto.remaining_label = Some(summary);
                    } else if window.count_quota.is_none() {
                        dto.display_label = window.value_label.clone();
                        dto.display_segments = vec![window.value_label.clone()];
                        dto.remaining_label = window
                            .remaining_percent
                            .map(|raw| format!("{}% left", raw.display_value()));
                    }
                }
                dto
            })
            .collect(),
        status: jackin_usage_provider_core::usage_status_storage_label(view.status).to_owned(),
        source: jackin_usage_provider_core::usage_source_storage_label(view.source).to_owned(),
        confidence: jackin_usage_provider_core::usage_confidence_storage_label(view.confidence)
            .to_owned(),
        fetched_at_epoch: account.freshness.last_good_at_epoch.unwrap_or(0),
        updated_label: view.updated_label,
        status_bar_label: view.status_bar_label,
        last_error: view.last_error,
        estimate_caption: None,
        detail_presentation,
    }
}

fn focused_view(
    surface: HostSurfaceId,
    account: &UsageAccountV1,
    format_prefs: UsageFormatPrefs,
) -> FocusedUsageView {
    let status = account_status_enum(account);
    let mut buckets = account
        .windows
        .iter()
        .map(|window| window_bucket(window, format_prefs))
        .collect::<Vec<_>>();
    buckets.extend(account.metric_groups.iter().filter_map(spend_cap_bucket));
    let headline = account
        .windows
        .iter()
        .map(|window| window_display_summary(window, format_prefs))
        .collect::<Vec<_>>()
        .join(" · ");
    let last_error = account.issues.first().map(|issue| issue.message.clone());
    let updated_label = updated_label(account.freshness.last_good_at_epoch);
    FocusedUsageView {
        focused_agent: Some(surface.agent_slug().to_owned()),
        focused_provider: Some(surface.provider_id().to_owned()),
        account: FocusedAccountHeader {
            provider_label: surface.label().to_owned(),
            account_label: account.display_label.clone(),
            username: None,
            plan_label: account.plan_label.clone(),
            credential_origin: None,
        },
        buckets,
        status,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        fetched_at_epoch: account.freshness.last_good_at_epoch.unwrap_or(0),
        updated_label,
        status_bar_label: if headline.is_empty() {
            account_status(account).to_owned()
        } else {
            headline
        },
        tabs: Vec::new(),
        last_error,
    }
}

pub(crate) fn empty_view(surface: HostSurfaceId, reason: Option<String>) -> UsageViewDto {
    let mut view = FocusedUsageView::unavailable(
        reason.unwrap_or_else(|| "No broker account is currently available".to_owned()),
        now_epoch(),
    );
    view.focused_agent = Some(surface.agent_slug().to_owned());
    view.focused_provider = Some(surface.provider_id().to_owned());
    view.account.provider_label = surface.label().to_owned();
    view.account.account_label = "No account selected".to_owned();
    view_dto_from_view(view, surface)
}

fn view_dto_from_view(view: FocusedUsageView, surface: HostSurfaceId) -> UsageViewDto {
    let identity =
        jackin_usage_provider_core::usage_identity_presentation(surface.label(), &view, false);
    UsageViewDto {
        identity: UsageIdentityPresentationDto {
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
        },
        focused_agent: view.focused_agent,
        focused_provider: view.focused_provider,
        provider_label: view.account.provider_label,
        account_label: view.account.account_label,
        username: None,
        plan_label: None,
        credential_origin: None,
        buckets: Vec::new(),
        status: jackin_usage_provider_core::usage_status_storage_label(view.status).to_owned(),
        source: jackin_usage_provider_core::usage_source_storage_label(view.source).to_owned(),
        confidence: jackin_usage_provider_core::usage_confidence_storage_label(view.confidence)
            .to_owned(),
        fetched_at_epoch: view.fetched_at_epoch,
        updated_label: view.updated_label,
        status_bar_label: view.status_bar_label,
        last_error: view.last_error,
        estimate_caption: None,
        detail_presentation: UsageDetailPresentationDto { rows: Vec::new() },
    }
}

fn window_bucket(window: &UsageLimitWindowV1, format_prefs: UsageFormatPrefs) -> QuotaBucketView {
    let used_percent = window
        .used_percent
        .and_then(|percent| CanonicalUsedPercent::new(percent.get()).ok());
    let remaining_percent = window
        .remaining_percent
        .and_then(|percent| CanonicalRemainingPercent::new(percent.get()).ok());
    QuotaBucketView {
        label: window.label.clone(),
        used_label: window
            .used_percent
            .map(|percent| format!("{}% used", percent.display_value())),
        limit_label: None,
        used_percent,
        used_percent_origin: used_percent.and(window.used_percent_origin),
        remaining_percent,
        remaining_percent_origin: remaining_percent.and(window.remaining_percent_origin),
        count_quota: window.count_quota,
        provider_quota_state: window.provider_quota_state,
        reset_label: formatted_reset(window, format_prefs),
        resets_at: window.reset_at_epoch,
        status_slot: match window.category {
            UsageWindowCategoryV1::Session => Some(StatusSlot::Session),
            UsageWindowCategoryV1::LongRange
            | UsageWindowCategoryV1::Model
            | UsageWindowCategoryV1::Other => None,
        },
        pace_label: window.pace_label.clone(),
        status: status_for_quota(window.quota_state),
        used_money: None,
        limit_money: None,
        severity: severity_for_quota(window.quota_state),
    }
}

fn spend_cap_bucket(group: &UsageMetricGroupV1) -> Option<QuotaBucketView> {
    let UsageMetricValueV1::SpendCap { cap, spent, .. } = &group.value else {
        return None;
    };
    Some(QuotaBucketView {
        label: group.label.clone(),
        used_label: spent.as_ref().map(ToString::to_string),
        limit_label: cap.as_ref().map(ToString::to_string),
        remaining_percent: None,
        used_percent: None,
        used_percent_origin: None,
        remaining_percent_origin: None,
        count_quota: None,
        provider_quota_state: None,
        reset_label: None,
        resets_at: group.reset_at_epoch,
        status_slot: Some(StatusSlot::Spend),
        pace_label: None,
        status: status_for_quota(group.quota_state),
        used_money: spent.clone(),
        limit_money: cap.clone(),
        severity: severity_for_quota(group.quota_state),
    })
}

fn bucket_dto(bucket: QuotaBucketView, source: Option<QuotaFields>) -> QuotaBucketDto {
    let presentation = jackin_usage_provider_core::usage_bucket_presentation(&bucket);
    let fields = source.unwrap_or_else(|| QuotaFields {
        used_percent: bucket.used_percent.map(CanonicalUsedPercent::get),
        used_percent_origin: bucket
            .used_percent_origin
            .map(|origin| origin.wire_label().to_owned()),
        remaining_percent: bucket.remaining_percent.map(CanonicalRemainingPercent::get),
        remaining_percent_origin: bucket
            .remaining_percent_origin
            .map(|origin| origin.wire_label().to_owned()),
        count_quota: bucket.count_quota.map(count_quota_dto),
        provider_quota_state: bucket
            .provider_quota_state
            .map(|provider_state| provider_state.wire_label().to_owned()),
        meter_percent: presentation.meter_percent.map(MeterPercentPoints::get),
        meter_side: presentation
            .meter_side
            .map(|side| side.wire_label().to_owned()),
    });
    QuotaBucketDto {
        label: bucket.label,
        used_label: bucket.used_label,
        limit_label: bucket.limit_label,
        used_percent: fields.used_percent,
        used_percent_origin: fields.used_percent_origin,
        remaining_percent: fields.remaining_percent,
        remaining_percent_origin: fields.remaining_percent_origin,
        count_quota: fields.count_quota,
        provider_quota_state: fields.provider_quota_state,
        reset_label: bucket.reset_label,
        resets_at: bucket.resets_at,
        status_slot: bucket.status_slot.map(|slot| {
            match slot {
                StatusSlot::Session => "session",
                StatusSlot::Daily => "daily",
                StatusSlot::Weekly => "weekly",
                StatusSlot::Spend => "spend",
            }
            .to_owned()
        }),
        pace_label: bucket.pace_label,
        status: jackin_usage_provider_core::usage_status_storage_label(bucket.status).to_owned(),
        used_money: bucket.used_money.map(money_dto),
        limit_money: bucket.limit_money.map(money_dto),
        severity: severity_label_from_enum(bucket.severity).to_owned(),
        remaining_label: presentation.remaining_label,
        display_segments: presentation.display_segments,
        display_label: presentation.display_label,
        meter_percent: fields.meter_percent,
        meter_side: fields.meter_side,
    }
}

fn money_dto(money: Money) -> MoneyDto {
    MoneyDto {
        amount_minor: money.amount_minor,
        currency: money.currency,
        exponent: money.exponent,
    }
}

fn typed_detail_presentation(
    account: &UsageAccountV1,
    format_prefs: UsageFormatPrefs,
) -> UsageDetailPresentationDto {
    let mut rows = Vec::new();
    if let Some(plan) = &account.plan_label {
        rows.push(metadata_row("plan", "Plan", plan.clone()));
    }
    rows.push(metadata_row(
        "availability",
        "Availability",
        lifecycle_display(account.lifecycle).to_owned(),
    ));
    for window in &account.windows {
        let summary = window_display_summary(window, format_prefs);
        rows.push(UsageDetailRowDto {
            row_id: window.window_id.clone(),
            kind: "bucket".to_owned(),
            label: window.label.clone(),
            layout_lines: vec![UsagePresentationLineDto {
                leading: Some(summary.clone()),
                trailing: formatted_reset(window, format_prefs),
            }],
            display_label: summary,
            meter_percent: window.meter_percent.map(MeterPercentPoints::get),
            meter_side: window.meter_side.map(|side| side.wire_label().to_owned()),
            severity: severity_label(window.quota_state).to_owned(),
        });
    }
    for group in &account.metric_groups {
        let summary = metric_group_summary(group);
        let scope = metric_scope_label(group);
        rows.push(UsageDetailRowDto {
            row_id: group.group_id.clone(),
            kind: match group.kind {
                UsageMetricGroupKindV1::Window | UsageMetricGroupKindV1::SpendCap => "bucket",
                UsageMetricGroupKindV1::Balance
                | UsageMetricGroupKindV1::TokenTotals
                | UsageMetricGroupKindV1::RateLimit
                | UsageMetricGroupKindV1::Plan => "detail",
            }
            .to_owned(),
            label: group.label.clone(),
            layout_lines: vec![UsagePresentationLineDto {
                leading: Some(summary.clone()),
                trailing: scope,
            }],
            display_label: summary,
            meter_percent: match &group.value {
                UsageMetricValueV1::Window { meter_percent, .. } => {
                    meter_percent.map(MeterPercentPoints::get)
                }
                _ => None,
            },
            meter_side: match &group.value {
                UsageMetricValueV1::Window { meter_side, .. } => {
                    meter_side.map(|side| side.wire_label().to_owned())
                }
                _ => None,
            },
            severity: severity_label(group.quota_state).to_owned(),
        });
    }
    for issue in &account.issues {
        rows.push(UsageDetailRowDto {
            row_id: format!("issue-{}", issue.code),
            kind: "detail".to_owned(),
            label: issue.code.clone(),
            layout_lines: vec![UsagePresentationLineDto {
                leading: Some(issue.message.clone()),
                trailing: None,
            }],
            display_label: issue.message.clone(),
            meter_percent: None,
            meter_side: None,
            severity: "warn".to_owned(),
        });
    }
    UsageDetailPresentationDto { rows }
}

fn metadata_row(row_id: &str, label: &str, value: String) -> UsageDetailRowDto {
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
        meter_side: None,
        severity: "normal".to_owned(),
    }
}

fn metric_scope_label(group: &UsageMetricGroupV1) -> Option<String> {
    let mut values = Vec::new();
    for (name, value) in [
        ("service", group.scope.service.as_deref()),
        ("model", group.scope.model.as_deref()),
        ("pool", group.scope.pool.as_deref()),
        ("key", group.scope.key_id.as_deref()),
    ] {
        if let Some(value) = value {
            values.push(format!("{name}: {value}"));
        }
    }
    (!values.is_empty()).then(|| values.join(" · "))
}

fn metric_group_summary(group: &UsageMetricGroupV1) -> String {
    match &group.value {
        UsageMetricValueV1::Window {
            count_quota,
            period,
            unit,
            ..
        } => {
            let period = match period {
                UsageMetricPeriodV1::Rolling { window_secs } => format!("{window_secs}s"),
                UsageMetricPeriodV1::Calendar { granularity } => {
                    format!("{granularity:?}").to_lowercase()
                }
                UsageMetricPeriodV1::ProviderDefined => "provider-defined period".to_owned(),
                UsageMetricPeriodV1::Unknown => "period unknown".to_owned(),
            };
            if count_quota.is_some()
                && let Some(summary) = metric_window_percent_label(group, PercentStyle::Left)
            {
                return format!("{summary} · {period}");
            }
            let value = metric_window_percent_label(group, PercentStyle::Left);
            value.map_or_else(
                || format!("{period} · quota {}", quota_state_label(group.quota_state)),
                |value| {
                    format!(
                        "{value} · {period}{}",
                        unit.as_ref()
                            .map_or_else(String::new, |unit| format!(" · {unit}"))
                    )
                },
            )
        }
        UsageMetricValueV1::Balance {
            amount,
            expires_at_epoch,
        } => format!(
            "{}{}",
            amount,
            expires_at_epoch.map_or_else(String::new, |epoch| format!(" · expires {epoch}"))
        ),
        UsageMetricValueV1::SpendCap {
            cap,
            spent,
            remaining,
        } => {
            let mut parts = Vec::new();
            if let Some(spent) = spent {
                parts.push(format!("spent {spent}"));
            }
            if let Some(cap) = cap {
                parts.push(format!("cap {cap}"));
            }
            if let Some(remaining) = remaining {
                parts.push(format!("remaining {remaining}"));
            }
            if parts.is_empty() {
                quota_state_label(group.quota_state).to_owned()
            } else {
                parts.join(" · ")
            }
        }
        UsageMetricValueV1::TokenTotals {
            input,
            output,
            cached,
            reasoning,
            interval_label,
        } => {
            let mut parts = Vec::new();
            if let Some(value) = input {
                parts.push(format!("input {value}"));
            }
            if let Some(value) = output {
                parts.push(format!("output {value}"));
            }
            if let Some(value) = cached {
                parts.push(format!("cached {value}"));
            }
            if let Some(value) = reasoning {
                parts.push(format!("reasoning {value}"));
            }
            if let Some(value) = interval_label {
                parts.push(value.clone());
            }
            parts.join(" · ")
        }
        UsageMetricValueV1::RateLimit {
            limit,
            remaining,
            window_label,
        } => {
            let mut parts = Vec::new();
            if let Some(value) = remaining {
                parts.push(format!("{value} remaining"));
            }
            if let Some(value) = limit {
                parts.push(format!("limit {value}"));
            }
            if let Some(value) = window_label {
                parts.push(value.clone());
            }
            parts.join(" · ")
        }
        UsageMetricValueV1::Plan { plan_label, tier } => plan_label
            .as_ref()
            .or(tier.as_ref())
            .cloned()
            .unwrap_or_else(|| quota_state_label(group.quota_state).to_owned()),
    }
}

fn overview_row(row: &ProviderGlanceRowDto) -> OverviewRowDto {
    OverviewRowDto {
        surface_id: row.surface_id.clone(),
        display_label: row.display_label.clone(),
        headline: row.headline.clone(),
        reset_label: row.reset_label.clone(),
        exact_reset: row.exact_reset.clone(),
        status_word: row.status_word.clone(),
        severity: row.severity.clone(),
    }
}

pub(crate) fn overview_rows(
    runtime: &HostUsageProjectionRuntime,
    enabled: &[String],
    format_prefs: UsageFormatPrefs,
) -> Result<Vec<OverviewRowDto>, String> {
    Ok(provider_glance_rows(runtime, enabled, format_prefs)?
        .iter()
        .map(overview_row)
        .collect())
}

pub(crate) fn next_refresh_label(
    refresh_floor_secs: u64,
    last_refresh_elapsed: Option<std::time::Duration>,
) -> String {
    let Some(elapsed) = last_refresh_elapsed else {
        return "Next update due".to_owned();
    };
    let floor = std::time::Duration::from_secs(refresh_floor_secs.max(60));
    if elapsed >= floor {
        "Next update due".to_owned()
    } else {
        format!(
            "Next update in {}s",
            floor.saturating_sub(elapsed).as_secs().max(1)
        )
    }
}

fn account_status(account: &UsageAccountV1) -> &'static str {
    match account.lifecycle {
        UsageLifecycleV1::NeedsLogin => "needs_login",
        UsageLifecycleV1::NeedsSecret => "needs_secret",
        UsageLifecycleV1::Unsupported => "unsupported",
        UsageLifecycleV1::Error => "error",
        UsageLifecycleV1::Unavailable => "unavailable",
        UsageLifecycleV1::AgentUninitialized => "unavailable",
        UsageLifecycleV1::Available => match account.freshness.phase {
            UsageFreshnessPhaseV1::Current if !account.freshness.is_stale => "fresh",
            UsageFreshnessPhaseV1::Stale | UsageFreshnessPhaseV1::Refreshing => "stale",
            UsageFreshnessPhaseV1::Failed => "error",
            UsageFreshnessPhaseV1::Current => "stale",
        },
    }
}

fn account_status_enum(account: &UsageAccountV1) -> UsageSnapshotStatus {
    match account.lifecycle {
        UsageLifecycleV1::NeedsLogin => UsageSnapshotStatus::NeedsLogin,
        UsageLifecycleV1::NeedsSecret => UsageSnapshotStatus::NeedsSecret,
        UsageLifecycleV1::Unsupported => UsageSnapshotStatus::Unsupported,
        UsageLifecycleV1::Error => UsageSnapshotStatus::Error,
        UsageLifecycleV1::Unavailable | UsageLifecycleV1::AgentUninitialized => {
            UsageSnapshotStatus::Unavailable
        }
        UsageLifecycleV1::Available => match account.freshness.phase {
            UsageFreshnessPhaseV1::Current if !account.freshness.is_stale => {
                UsageSnapshotStatus::Fresh
            }
            UsageFreshnessPhaseV1::Stale | UsageFreshnessPhaseV1::Refreshing => {
                UsageSnapshotStatus::Stale
            }
            UsageFreshnessPhaseV1::Failed => UsageSnapshotStatus::Error,
            UsageFreshnessPhaseV1::Current => UsageSnapshotStatus::Stale,
        },
    }
}

fn activity_kind(account: &UsageAccountV1) -> &'static str {
    if account.freshness.phase == UsageFreshnessPhaseV1::Refreshing {
        "updating"
    } else if account_status(account) == "fresh" {
        "idle"
    } else {
        "exceptional"
    }
}

fn lifecycle_label(lifecycle: UsageLifecycleV1) -> &'static str {
    match lifecycle {
        UsageLifecycleV1::Available => "current",
        UsageLifecycleV1::AgentUninitialized => "uninitialized",
        UsageLifecycleV1::NeedsLogin => "needs_login",
        UsageLifecycleV1::NeedsSecret => "needs_secret",
        UsageLifecycleV1::Unsupported => "unsupported",
        UsageLifecycleV1::Unavailable => "unavailable",
        UsageLifecycleV1::Error => "error",
    }
}

fn lifecycle_display(lifecycle: UsageLifecycleV1) -> &'static str {
    match lifecycle {
        UsageLifecycleV1::Available => "Available",
        UsageLifecycleV1::AgentUninitialized => "Not initialized",
        UsageLifecycleV1::NeedsLogin => "Login required",
        UsageLifecycleV1::NeedsSecret => "Credential required",
        UsageLifecycleV1::Unsupported => "Unsupported",
        UsageLifecycleV1::Unavailable => "Unavailable",
        UsageLifecycleV1::Error => "Error",
    }
}

fn status_for_quota(quota: UsageQuotaStateV1) -> UsageSnapshotStatus {
    match quota {
        UsageQuotaStateV1::Available
        | UsageQuotaStateV1::NotStarted
        | UsageQuotaStateV1::Warning
        | UsageQuotaStateV1::Exhausted
        | UsageQuotaStateV1::NotApplicable => UsageSnapshotStatus::Fresh,
        UsageQuotaStateV1::Unsupported => UsageSnapshotStatus::Unsupported,
        UsageQuotaStateV1::Unavailable
        | UsageQuotaStateV1::NoPermission
        | UsageQuotaStateV1::Unknown => UsageSnapshotStatus::Unavailable,
        UsageQuotaStateV1::Error => UsageSnapshotStatus::Error,
    }
}

fn severity_for_quota(quota: UsageQuotaStateV1) -> UsageSeverity {
    match quota {
        UsageQuotaStateV1::Warning => UsageSeverity::Warn,
        UsageQuotaStateV1::Exhausted | UsageQuotaStateV1::Error => UsageSeverity::Danger,
        _ => UsageSeverity::Normal,
    }
}

fn severity_label(quota: UsageQuotaStateV1) -> &'static str {
    severity_label_from_enum(severity_for_quota(quota))
}

fn severity_label_from_enum(severity: UsageSeverity) -> &'static str {
    match severity {
        UsageSeverity::Normal => "normal",
        UsageSeverity::Warn => "warn",
        UsageSeverity::Danger => "danger",
    }
}

fn quota_state_label(quota: UsageQuotaStateV1) -> &'static str {
    match quota {
        UsageQuotaStateV1::Available => "available",
        UsageQuotaStateV1::NotStarted => "not started",
        UsageQuotaStateV1::Warning => "warning",
        UsageQuotaStateV1::Exhausted => "exhausted",
        UsageQuotaStateV1::Unsupported => "unsupported",
        UsageQuotaStateV1::Unavailable => "unavailable",
        UsageQuotaStateV1::NoPermission => "permission unavailable",
        UsageQuotaStateV1::Unknown => "unknown",
        UsageQuotaStateV1::NotApplicable => "not applicable",
        UsageQuotaStateV1::Error => "error",
    }
}

fn metric_status(group: &UsageMetricGroupV1, account: &UsageAccountV1) -> &'static str {
    let status = metric_group_status(group);
    if status == "fresh" {
        account_status(account)
    } else {
        status
    }
}

fn metric_status_for_glance(group: &UsageMetricGroupV1) -> &'static str {
    metric_group_status(group)
}

fn metric_group_status(group: &UsageMetricGroupV1) -> &'static str {
    if group.phase == UsageFreshnessPhaseV1::Failed && !group.is_stale {
        "error"
    } else if group.is_stale || group.phase == UsageFreshnessPhaseV1::Stale {
        "stale"
    } else {
        jackin_usage_provider_core::usage_status_storage_label(status_for_quota(group.quota_state))
    }
}

fn surface_for_provider_id(provider_id: &str) -> Option<HostSurfaceId> {
    HostSurfaceId::ALL
        .iter()
        .copied()
        .find(|surface| surface.provider_id() == provider_id)
}

fn is_surface_enabled(enabled: &[String], surface: HostSurfaceId) -> bool {
    enabled.is_empty() || enabled.iter().any(|id| id == surface.id())
}

fn projection_issue_message(
    projection: &UsageProjectionV1,
    surface: HostSurfaceId,
) -> Option<String> {
    projection
        .providers
        .iter()
        .find(|provider| provider.provider_id == surface.provider_id())
        .and_then(|provider| provider.issues.first().map(|issue| issue.message.clone()))
        .or_else(|| {
            projection
                .unresolved
                .iter()
                .find(|row| row.provider_id == surface.provider_id())
                .and_then(|row| row.issues.first().map(|issue| issue.message.clone()))
        })
}

fn nonempty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn formatted_reset(window: &UsageLimitWindowV1, format_prefs: UsageFormatPrefs) -> Option<String> {
    window.reset_at_epoch.map_or_else(
        || nonempty(window.reset_label.clone()),
        |reset_at| {
            Some(jackin_usage_provider_core::reset_label_with_prefs(
                reset_at,
                now_epoch(),
                format_prefs,
            ))
        },
    )
}

fn formatted_epoch_reset(
    reset_at_epoch: Option<i64>,
    format_prefs: UsageFormatPrefs,
) -> Option<String> {
    reset_at_epoch.map(|reset_at| {
        jackin_usage_provider_core::reset_label_with_prefs(reset_at, now_epoch(), format_prefs)
    })
}

fn updated_label(last_good_at_epoch: Option<i64>) -> String {
    let Some(last_good) = last_good_at_epoch else {
        return "No successful update".to_owned();
    };
    let elapsed = now_epoch().saturating_sub(last_good).max(0);
    match elapsed {
        0..=59 => "Updated just now".to_owned(),
        60..=3599 => format!("Updated {}m ago", elapsed / 60),
        3600..=86399 => format!("Updated {}h ago", elapsed / 3600),
        _ => format!("Updated {}d ago", elapsed / 86400),
    }
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or(0)
}
