// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Native DTO presentation directly from broker-owned usage projections.

use std::time::{SystemTime, UNIX_EPOCH};

use jackin_protocol::control::{
    FocusedAccountHeader, FocusedUsageView, Money, QuotaBucketView, StatusSlot, UsageConfidence,
    UsageSeverity, UsageSnapshotStatus, UsageSource,
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
use jackin_usage_provider_core::{PercentStyle, UsageFormatPrefs};

use crate::dto::{
    AccountDescriptorDto, DesktopInventoryDto, DesktopProjectionDto, DesktopProviderGroupDto,
    DesktopProviderProjectionDto, DesktopProviderStateDto, DiscoveryDiagnosticDto, MoneyDto,
    OverviewRowDto, ProviderGlanceRowDto, QuotaBucketDto, SelectedAccountRouteDto,
    SurfaceDescriptorDto, UsageDetailPresentationDto, UsageDetailRowDto,
    UsageIdentityPresentationDto, UsagePresentationLineDto, UsageViewDto,
};

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
    let glance_rows = provider_glance_rows(runtime, enabled, format_prefs)?;
    let mut status_rows = glance_rows
        .iter()
        .filter(|row| row.glance_remaining_percent.is_some_and(|value| value > 0))
        .cloned()
        .collect::<Vec<_>>();
    status_rows.truncate(status_bar_max.clamp(1, 3) as usize);
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

pub(crate) fn provider_glance_rows(
    runtime: &HostUsageProjectionRuntime,
    enabled: &[String],
    format_prefs: UsageFormatPrefs,
) -> Result<Vec<ProviderGlanceRowDto>, String> {
    let mut rows = Vec::new();
    for surface in HostSurfaceId::DESKTOP_PROVIDER_ORDER.iter().copied() {
        if !is_surface_enabled(enabled, surface) {
            continue;
        }
        let presentation = runtime.provider_presentation(surface.id())?;
        if let HostUsageProjectionSelectedAccount::Available { account, .. } =
            presentation.selected_account
        {
            rows.push(glance_row(
                surface,
                presentation.provider,
                account,
                presentation.glance_metric_group,
                format_prefs,
            ));
        }
    }
    Ok(rows)
}

fn glance_row(
    surface: HostSurfaceId,
    provider: Option<&UsageProviderV1>,
    account: &UsageAccountV1,
    metric_window: Option<&UsageMetricGroupV1>,
    format_prefs: UsageFormatPrefs,
) -> ProviderGlanceRowDto {
    let window = account.windows.first();
    let remaining = metric_window.and_then(window_group_remaining).or_else(|| {
        window.and_then(|window| {
            window
                .remaining_percent
                .map(jackin_protocol::usage_broker::UsagePercent::get)
        })
    });
    let quota_state = metric_window
        .map(|group| group.quota_state)
        .or_else(|| window.map(|window| window.quota_state));
    let severity = quota_state.map_or("normal", severity_label);
    let status = metric_window.map_or_else(
        || account_status(account),
        |group| metric_status(group, account),
    );
    let activity_kind =
        if metric_window.is_some_and(|group| group.phase == UsageFreshnessPhaseV1::Refreshing) {
            "updating"
        } else {
            activity_kind(account)
        };
    let activity_label = if activity_kind == "updating" {
        "Updating".to_owned()
    } else {
        activity_label(account)
    };
    let account_label = account.display_label.clone();
    let last_error = metric_window
        .and_then(|group| group.issues.first())
        .or_else(|| account.issues.first())
        .or_else(|| provider.and_then(|provider| provider.issues.first()))
        .map(|issue| issue.message.clone());
    let label = metric_window
        .map(metric_group_summary)
        .or_else(|| window.map(|window| window.value_label.clone()))
        .unwrap_or_else(|| "Usage unavailable".to_owned());
    let headline = metric_window
        .and_then(|group| metric_window_headline(group, format_prefs))
        .or_else(|| window.map(|window| window_headline(window, format_prefs)))
        .unwrap_or_else(|| label.clone());
    let bar_label = match format_prefs.percent_style {
        PercentStyle::Left => metric_window
            .and_then(|group| metric_window_percent_label(group, PercentStyle::Left))
            .or_else(|| window.map(|window| window.value_label.clone()))
            .unwrap_or_else(|| "—".to_owned()),
        PercentStyle::Used => metric_window
            .and_then(|group| metric_window_percent_label(group, PercentStyle::Used))
            .or_else(|| window.and_then(window_used_label))
            .or_else(|| window.map(|window| window.value_label.clone()))
            .unwrap_or_else(|| "—".to_owned()),
    };
    let reset = metric_window
        .and_then(|group| formatted_epoch_reset(group.reset_at_epoch, format_prefs))
        .or_else(|| window.and_then(|window| formatted_reset(window, format_prefs)));
    let updated_label = updated_label(
        metric_window
            .and_then(|group| group.last_success_at_epoch)
            .or(account.freshness.last_good_at_epoch),
    );
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
        glance_remaining_percent: remaining,
        bar_label: bar_label.clone(),
        headline,
        reset_label: reset.clone(),
        compact_reset_label: reset.clone(),
        exact_reset: metric_window
            .and_then(|group| group.reset_at_epoch)
            .or_else(|| window.and_then(|window| window.reset_at_epoch))
            .map(jackin_usage_provider_core::local_timestamp_label),
        status_word: status.to_owned(),
        is_refreshing: metric_window
            .is_some_and(|group| group.phase == UsageFreshnessPhaseV1::Refreshing)
            || account.freshness.phase == UsageFreshnessPhaseV1::Refreshing,
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
    let percent = window.and_then(|window| {
        window
            .remaining_percent
            .map(jackin_protocol::usage_broker::UsagePercent::get)
    });
    let status = account_status(account);
    let severity = window.map_or("normal", |window| severity_label(window.quota_state));
    let remaining_label =
        window.map_or_else(|| "—".to_owned(), |window| window.value_label.clone());
    let reset_label = window.and_then(|window| formatted_reset(window, format_prefs));
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
        remaining_percent: percent,
        remaining_label,
        headline: window.map_or_else(|| status.to_owned(), |window| window.value_label.clone()),
        reset_label: reset_label.clone(),
        reset_display_label: reset_label.unwrap_or_else(|| "—".to_owned()),
        exact_reset: window
            .and_then(|window| window.reset_at_epoch)
            .map(jackin_usage_provider_core::local_timestamp_label),
        status_word: status.to_owned(),
        status_label: status.to_owned(),
        severity: severity.to_owned(),
        updated_label: updated_label(account.freshness.last_good_at_epoch),
        last_error: account
            .issues
            .first()
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
                let mut dto = bucket_dto(bucket);
                if let Some(window) = account.windows.get(index) {
                    dto.display_label = window.value_label.clone();
                    dto.display_segments = vec![window.value_label.clone()];
                    dto.remaining_label = window
                        .remaining_raw_percent
                        .map(|raw| format!("{raw}% left"))
                        .or_else(|| {
                            window
                                .remaining_percent
                                .map(|percent| format!("{}% left", percent.get()))
                        });
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
        .map(|window| match format_prefs.percent_style {
            PercentStyle::Left if window.remaining_percent.is_some() => window.value_label.clone(),
            PercentStyle::Used if window.used_percent.is_some() => window.value_label.clone(),
            _ => window.value_label.clone(),
        })
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
    QuotaBucketView {
        label: window.label.clone(),
        used_label: window.used_percent.map(|percent| {
            format!(
                "{}% used",
                window.used_raw_percent.unwrap_or(i32::from(percent.get()))
            )
        }),
        limit_label: None,
        remaining_percent: window
            .remaining_percent
            .map(jackin_protocol::usage_broker::UsagePercent::get),
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

fn bucket_dto(bucket: QuotaBucketView) -> QuotaBucketDto {
    let presentation = jackin_usage_provider_core::usage_bucket_presentation(&bucket);
    QuotaBucketDto {
        label: bucket.label,
        used_label: bucket.used_label,
        limit_label: bucket.limit_label,
        remaining_percent: bucket.remaining_percent,
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
        rows.push(UsageDetailRowDto {
            row_id: window.window_id.clone(),
            kind: "bucket".to_owned(),
            label: window.label.clone(),
            layout_lines: vec![UsagePresentationLineDto {
                leading: Some(window.value_label.clone()),
                trailing: formatted_reset(window, format_prefs),
            }],
            display_label: window.value_label.clone(),
            meter_percent: window
                .remaining_percent
                .map(jackin_protocol::usage_broker::UsagePercent::meter_fill),
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
                UsageMetricValueV1::Window {
                    remaining_percent, ..
                } => remaining_percent.map(jackin_protocol::usage_broker::UsagePercent::meter_fill),
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
            remaining_percent,
            remaining_raw_percent,
            used_percent,
            used_raw_percent,
            period,
            unit,
        } => {
            let value = if let Some(raw) = remaining_raw_percent {
                Some(format!("{raw}% left"))
            } else if let Some(percent) = remaining_percent {
                Some(format!("{}% left", percent.get()))
            } else if let Some(raw) = used_raw_percent {
                Some(format!("{raw}% used"))
            } else {
                used_percent.map(|percent| format!("{}% used", percent.get()))
            };
            let period = match period {
                UsageMetricPeriodV1::Rolling { window_secs } => format!("{window_secs}s"),
                UsageMetricPeriodV1::Calendar { granularity } => {
                    format!("{granularity:?}").to_lowercase()
                }
                UsageMetricPeriodV1::ProviderDefined => "provider-defined period".to_owned(),
                UsageMetricPeriodV1::Unknown => "period unknown".to_owned(),
            };
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

fn activity_label(account: &UsageAccountV1) -> String {
    match activity_kind(account) {
        "updating" => "Updating".to_owned(),
        "idle" => updated_label(account.freshness.last_good_at_epoch),
        _ => account.issues.first().map_or_else(
            || lifecycle_display(account.lifecycle).to_owned(),
            |issue| issue.message.clone(),
        ),
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

fn window_group_remaining(group: &UsageMetricGroupV1) -> Option<u8> {
    match &group.value {
        UsageMetricValueV1::Window {
            remaining_percent, ..
        } => remaining_percent.map(jackin_protocol::usage_broker::UsagePercent::get),
        _ => None,
    }
}

fn metric_window_percent_label(group: &UsageMetricGroupV1, style: PercentStyle) -> Option<String> {
    let UsageMetricValueV1::Window {
        remaining_percent,
        remaining_raw_percent,
        used_percent,
        used_raw_percent,
        ..
    } = &group.value
    else {
        return None;
    };
    match style {
        PercentStyle::Left => remaining_raw_percent
            .map(|raw| format!("{raw}%"))
            .or_else(|| remaining_percent.map(|value| format!("{}%", value.get()))),
        PercentStyle::Used => used_raw_percent
            .map(|raw| format!("{raw}%"))
            .or_else(|| used_percent.map(|value| format!("{}%", value.get())))
            .or_else(|| {
                remaining_percent.map(|value| format!("{}%", 100u8.saturating_sub(value.get())))
            }),
    }
}

fn metric_window_headline(
    group: &UsageMetricGroupV1,
    format_prefs: UsageFormatPrefs,
) -> Option<String> {
    let value = metric_window_percent_label(group, format_prefs.percent_style)?;
    let headline = match format_prefs.percent_style {
        PercentStyle::Left => format!("{value} left"),
        PercentStyle::Used => format!("{value} used"),
    };
    Some(
        group
            .scope
            .model
            .as_ref()
            .map_or_else(|| headline.clone(), |model| format!("{model} {headline}")),
    )
}

fn window_headline(window: &UsageLimitWindowV1, format_prefs: UsageFormatPrefs) -> String {
    match format_prefs.percent_style {
        PercentStyle::Left => window
            .remaining_raw_percent
            .map(|raw| format!("{raw}% left"))
            .or_else(|| {
                window
                    .remaining_percent
                    .map(|percent| format!("{}% left", percent.get()))
            })
            .unwrap_or_else(|| window.value_label.clone()),
        PercentStyle::Used => window_used_label(window)
            .map(|value| format!("{value} used"))
            .or_else(|| {
                window.remaining_percent.map(|remaining| {
                    jackin_usage_provider_core::percent_headline(remaining.get(), format_prefs)
                })
            })
            .unwrap_or_else(|| window.value_label.clone()),
    }
}

fn window_used_label(window: &UsageLimitWindowV1) -> Option<String> {
    window
        .used_raw_percent
        .map(|raw| format!("{raw}%"))
        .or_else(|| window.used_percent.map(|value| format!("{}%", value.get())))
        .or_else(|| {
            window
                .remaining_percent
                .map(|remaining| format!("{}%", 100u8.saturating_sub(remaining.get())))
        })
}

fn metric_status(group: &UsageMetricGroupV1, account: &UsageAccountV1) -> &'static str {
    let metric_status = if group.phase == UsageFreshnessPhaseV1::Failed && !group.is_stale {
        "error"
    } else if group.is_stale || group.phase == UsageFreshnessPhaseV1::Stale {
        "stale"
    } else {
        jackin_usage_provider_core::usage_status_storage_label(status_for_quota(group.quota_state))
    };
    if metric_status == "fresh" {
        account_status(account)
    } else {
        metric_status
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
