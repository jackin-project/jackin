// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Coarse synchronous facade over broker-owned usage projections.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jackin_protocol::usage_broker::{
    UsageCoordinationError, UsageCoordinationErrorKind, UsageProjectionRefreshStateV1,
    UsageProjectionSchemaV1, UsageProjectionV1,
};
use jackin_usage::host::{
    HostSurfaceId, HostUsageProjectionConfig, HostUsageProjectionRuntime, UsageBrokerClient,
    UsageBrokerConfig,
};
use jackin_usage_provider_core::UsageFormatPrefs;

use crate::dto::{
    AccountDescriptorDto, DesktopInventoryDto, DesktopProjectionDto, DiscoveryDiagnosticDto,
    OpenConfig, OverviewRowDto, ProjectionOpenConfig, ProviderGlanceRowDto, SurfaceDescriptorDto,
    UsageEventBatchDto, UsageEventDto, UsageFormatPrefsDto, UsageViewDto, map_open_err,
    map_runtime_err, parse_format_prefs, to_projection_config,
};
use crate::error::{UsageBridgeError, catch_entry};

/// Process-scoped `boltffi` facade over broker-owned canonical publications.
pub struct UsageMenuBarBridge {
    inner: Arc<Mutex<Option<BridgeState>>>,
    lifecycle: Arc<Mutex<()>>,
}

impl std::fmt::Debug for UsageMenuBarBridge {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UsageMenuBarBridge")
            .finish_non_exhaustive()
    }
}

struct BridgeState {
    runtime: HostUsageProjectionRuntime,
    client: UsageBrokerClient,
    live_probes_enabled: bool,
    enabled_surface_ids: Vec<String>,
    refresh_floor_secs: u64,
    format_prefs: UsageFormatPrefs,
    last_refresh: Option<Instant>,
    event_sequence: u64,
    events: VecDeque<UsageEventDto>,
}

#[boltffi::export]
impl UsageMenuBarBridge {
    /// Construct a closed bridge.
    #[must_use]
    pub fn create() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            lifecycle: Arc::new(Mutex::new(())),
        }
    }

    /// Open the projection consumer. Live mode may activate the standard
    /// broker process; this client never discovers credentials or providers.
    pub fn open_runtime(&self, config: OpenConfig) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let host_config = to_projection_config(config).map_err(map_open_err)?;
            let broker_config = UsageBrokerConfig::for_data_dir(host_config.data_dir.clone());
            self.open_runtime_with_config(host_config, broker_config)
        })
    }

    /// List host surfaces and their current enable flags.
    pub fn list_surfaces(&self) -> Result<Vec<SurfaceDescriptorDto>, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.poll_latest_projection();
            self.with_state(|state| {
                Ok(crate::presentation::surface_rows(
                    &state.enabled_surface_ids,
                ))
            })
        })
    }

    /// Sanitized broker projection issues for the current publication.
    pub fn discovery_diagnostics(&self) -> Result<Vec<DiscoveryDiagnosticDto>, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.poll_latest_projection();
            self.with_state(|state| {
                Ok(crate::presentation::discovery_diagnostics(
                    state.runtime.projection(),
                ))
            })
        })
    }

    /// Enable or disable one native surface.
    pub fn set_enabled(&self, surface_id: String, enabled: bool) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let surface = HostSurfaceId::from_id(&surface_id)
                .ok_or_else(|| UsageBridgeError::rejected("runtime", "unknown surface"))?;
            self.with_state(|state| {
                if state.enabled_surface_ids.is_empty() {
                    state.enabled_surface_ids = HostSurfaceId::ALL
                        .iter()
                        .map(|surface| surface.id().to_owned())
                        .collect();
                }
                state.enabled_surface_ids.retain(|id| id != surface.id());
                if enabled {
                    state.enabled_surface_ids.push(surface.id().to_owned());
                }
                state
                    .runtime
                    .set_enabled_surface_ids(&state.enabled_surface_ids)
                    .map_err(map_runtime_err)?;
                push_event(
                    state,
                    "surface_enabled",
                    Some(surface.id().to_owned()),
                    None,
                );
                Ok(())
            })
        })
    }

    /// Read the latest projection or explicitly request a broker-owned refresh.
    /// Non-forced calls are cache reads; force refresh is account-wide because
    /// the broker owns a single canonical projection generation.
    pub fn refresh(&self, surface_id: Option<String>, force: bool) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let surface = surface_id
                .as_deref()
                .map(|surface_id| {
                    HostSurfaceId::from_id(surface_id)
                        .ok_or_else(|| UsageBridgeError::rejected("runtime", "unknown surface"))
                })
                .transpose()?;
            if let Some(surface) = surface {
                self.ensure_surface_enabled(surface)?;
            }

            let (client, live_probes_enabled, observed_projection_id) =
                self.with_state(|state| {
                    Ok((
                        state.client.clone(),
                        state.live_probes_enabled,
                        state.runtime.projection().projection_id.clone(),
                    ))
                })?;
            if !force || !live_probes_enabled {
                self.poll_latest_projection();
                self.with_state(|state| {
                    state.last_refresh = Some(Instant::now());
                    Ok(())
                })?;
                return Ok(());
            }

            let publication = client
                .request_refresh(Some(observed_projection_id), true)
                .map_err(map_coordination_err)?;
            self.apply_publication(publication)?;
            self.with_state(|state| {
                state.last_refresh = Some(Instant::now());
                Ok(())
            })
        })
    }

    /// True while the broker marks its current publication as refreshing.
    pub fn refresh_in_progress(&self) -> Result<bool, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.poll_latest_projection();
            self.with_state(|state| {
                Ok(state.runtime.projection().refresh_state
                    == UsageProjectionRefreshStateV1::Refreshing)
            })
        })
    }

    /// Set the local display refresh floor (clamped to at least 60 seconds).
    pub fn set_refresh_floor_secs(&self, secs: u64) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.with_state(|state| {
                state.refresh_floor_secs = secs.max(60);
                push_event(
                    state,
                    "refresh_floor_changed",
                    None,
                    Some(format!("refresh_floor_secs={}", state.refresh_floor_secs)),
                );
                Ok(())
            })
        })
    }

    /// Whether the local display floor elapsed since the last explicit refresh.
    /// This query never dispatches provider work.
    pub fn refresh_due(&self) -> Result<bool, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.with_state(|state| {
                Ok(state.last_refresh.is_none_or(|last| {
                    last.elapsed() >= Duration::from_secs(state.refresh_floor_secs)
                }))
            })
        })
    }

    /// Snapshot for one enabled surface and its exact selected account.
    pub fn snapshot(&self, surface_id: String) -> Result<UsageViewDto, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.poll_latest_projection();
            self.with_state(|state| {
                let surface = HostSurfaceId::from_id(&surface_id)
                    .ok_or_else(|| UsageBridgeError::rejected("runtime", "unknown surface"))?;
                if !is_enabled(&state.enabled_surface_ids, surface) {
                    return Err(UsageBridgeError::rejected("runtime", "surface is disabled"));
                }
                let presentation = state
                    .runtime
                    .provider_presentation(surface.id())
                    .map_err(map_runtime_err)?;
                match presentation.selected_account {
                    jackin_usage::host::HostUsageProjectionSelectedAccount::Available {
                        account,
                        ..
                    } => Ok(crate::presentation::view_dto(
                        surface,
                        account,
                        state.format_prefs,
                    )),
                    jackin_usage::host::HostUsageProjectionSelectedAccount::Unselected => {
                        Ok(crate::presentation::empty_view(surface, None))
                    }
                    jackin_usage::host::HostUsageProjectionSelectedAccount::Unavailable {
                        ..
                    } => Ok(crate::presentation::empty_view(
                        surface,
                        Some(
                            "The selected account is not in the current broker publication"
                                .to_owned(),
                        ),
                    )),
                }
            })
        })
    }

    /// List broker-canonical accounts. `account_key` carries the canonical id.
    pub fn list_accounts(
        &self,
        surface_id: Option<String>,
    ) -> Result<Vec<AccountDescriptorDto>, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.poll_latest_projection();
            self.with_state(|state| {
                let rows = state
                    .runtime
                    .account_inventory(surface_id.as_deref())
                    .map_err(map_runtime_err)?;
                rows.into_iter()
                    .map(|row| {
                        let surface = HostSurfaceId::from_id(row.surface_id)
                            .ok_or(UsageBridgeError::rejected("runtime", "unknown surface"))?;
                        Ok(crate::presentation::account_dto(
                            surface,
                            row.provider,
                            row.account,
                            state.format_prefs,
                            row.selected,
                        ))
                    })
                    .collect::<Result<Vec<_>, UsageBridgeError>>()
            })
        })
    }

    /// Atomic provider/account inventory from one broker publication.
    pub fn desktop_inventory(&self) -> Result<DesktopInventoryDto, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.poll_latest_projection();
            self.with_state(|state| {
                crate::presentation::desktop_inventory(
                    &state.runtime,
                    &state.enabled_surface_ids,
                    state.format_prefs,
                )
                .map_err(map_runtime_err)
            })
        })
    }

    /// Complete native Desktop state from one broker publication.
    pub fn desktop_projection(
        &self,
        status_bar_max: u32,
    ) -> Result<DesktopProjectionDto, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.poll_latest_projection();
            self.with_state(|state| {
                let elapsed = state.last_refresh.map(|last| last.elapsed());
                crate::presentation::desktop_projection(
                    &state.runtime,
                    &state.enabled_surface_ids,
                    state.format_prefs,
                    status_bar_max,
                    state.refresh_floor_secs,
                    elapsed,
                )
                .map_err(map_runtime_err)
            })
        })
    }

    /// Select one exact broker-canonical account id for the surface.
    pub fn set_selected_account(
        &self,
        surface_id: String,
        account_key: String,
    ) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.with_state(|state| {
                state
                    .runtime
                    .set_selected_account(&surface_id, Some(&account_key))
                    .map_err(map_runtime_err)?;
                push_event(
                    state,
                    "account_selected",
                    Some(surface_id),
                    Some(account_key),
                );
                Ok(())
            })
        })
    }

    pub fn status_bar_label(&self, surface_id: String) -> Result<Option<String>, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.poll_latest_projection();
            self.with_state(|state| {
                let rows = crate::presentation::provider_glance_rows(
                    &state.runtime,
                    &state.enabled_surface_ids,
                    state.format_prefs,
                )
                .map_err(map_runtime_err)?;
                Ok(rows
                    .into_iter()
                    .find(|row| row.surface_id == surface_id)
                    .map(|row| row.bar_label))
            })
        })
    }

    pub fn merged_status_bar_label(&self) -> Result<String, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.poll_latest_projection();
            self.with_state(|state| {
                Ok(crate::presentation::provider_glance_rows(
                    &state.runtime,
                    &state.enabled_surface_ids,
                    state.format_prefs,
                )
                .map_err(map_runtime_err)?
                .into_iter()
                .map(|row| row.bar_label)
                .collect::<Vec<_>>()
                .join(" · "))
            })
        })
    }

    pub fn compact_status_bar_label(&self) -> Result<String, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.poll_latest_projection();
            self.with_state(|state| {
                let rows = crate::presentation::provider_glance_rows(
                    &state.runtime,
                    &state.enabled_surface_ids,
                    state.format_prefs,
                )
                .map_err(map_runtime_err)?;
                Ok(rows
                    .iter()
                    .max_by_key(|row| severity_rank(&row.severity))
                    .map(|row| format!("{} {}", row.fallback_glyph, row.bar_label))
                    .unwrap_or_default())
            })
        })
    }

    pub fn set_format_prefs(&self, prefs: UsageFormatPrefsDto) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let parsed = parse_format_prefs(prefs).map_err(map_runtime_err)?;
            let _lifecycle = self.lifecycle_lock()?;
            self.with_state(|state| {
                state.format_prefs = parsed;
                push_event(state, "format_prefs_changed", None, None);
                Ok(())
            })
        })
    }

    pub fn compact_status_bar_label_for(
        &self,
        surface_id: String,
    ) -> Result<Option<String>, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.poll_latest_projection();
            self.with_state(|state| {
                let rows = crate::presentation::provider_glance_rows(
                    &state.runtime,
                    &state.enabled_surface_ids,
                    state.format_prefs,
                )
                .map_err(map_runtime_err)?;
                Ok(rows
                    .into_iter()
                    .find(|row| row.surface_id == surface_id)
                    .map(|row| format!("{} {}", row.fallback_glyph, row.bar_label)))
            })
        })
    }

    pub fn compact_status_bar_strip(&self, max: u32) -> Result<String, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.poll_latest_projection();
            self.with_state(|state| {
                let mut rows = crate::presentation::provider_glance_rows(
                    &state.runtime,
                    &state.enabled_surface_ids,
                    state.format_prefs,
                )
                .map_err(map_runtime_err)?;
                rows.sort_by_key(|row| std::cmp::Reverse(severity_rank(&row.severity)));
                rows.truncate(max.clamp(1, 3) as usize);
                Ok(rows
                    .into_iter()
                    .map(|row| format!("{} {}", row.fallback_glyph, row.bar_label))
                    .collect::<Vec<_>>()
                    .join(" · "))
            })
        })
    }

    pub fn overview_rows(&self) -> Result<Vec<OverviewRowDto>, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.poll_latest_projection();
            self.with_state(|state| {
                crate::presentation::overview_rows(
                    &state.runtime,
                    &state.enabled_surface_ids,
                    state.format_prefs,
                )
                .map_err(map_runtime_err)
            })
        })
    }

    pub fn provider_glance_rows(&self) -> Result<Vec<ProviderGlanceRowDto>, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.poll_latest_projection();
            self.with_state(|state| {
                crate::presentation::provider_glance_rows(
                    &state.runtime,
                    &state.enabled_surface_ids,
                    state.format_prefs,
                )
                .map_err(map_runtime_err)
            })
        })
    }

    pub fn status_bar_provider_glance_rows(
        &self,
        max: u32,
    ) -> Result<Vec<ProviderGlanceRowDto>, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.poll_latest_projection();
            self.with_state(|state| {
                let mut rows = crate::presentation::provider_glance_rows(
                    &state.runtime,
                    &state.enabled_surface_ids,
                    state.format_prefs,
                )
                .map_err(map_runtime_err)?
                .into_iter()
                .filter(|row| {
                    row.glance_remaining_percent
                        .is_some_and(|percent| percent > 0)
                })
                .collect::<Vec<_>>();
                rows.sort_by_key(|row| std::cmp::Reverse(severity_rank(&row.severity)));
                rows.truncate(max.clamp(1, 3) as usize);
                Ok(rows)
            })
        })
    }

    pub fn next_refresh_label(&self) -> Result<String, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.with_state(|state| {
                Ok(crate::presentation::next_refresh_label(
                    state.refresh_floor_secs,
                    state.last_refresh.map(|last| last.elapsed()),
                ))
            })
        })
    }

    pub fn next_events(
        &self,
        cursor: u64,
        max: u32,
    ) -> Result<UsageEventBatchDto, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.with_state(|state| {
                let oldest = state
                    .events
                    .front()
                    .map_or(state.event_sequence, |event| event.sequence);
                let resync_required = cursor.saturating_add(1) < oldest;
                let events = state
                    .events
                    .iter()
                    .filter(|event| resync_required || event.sequence > cursor)
                    .take(max.clamp(1, 256) as usize)
                    .cloned()
                    .collect::<Vec<_>>();
                let next_cursor = events
                    .last()
                    .map_or(state.event_sequence, |event| event.sequence);
                Ok(UsageEventBatchDto {
                    next_cursor,
                    events,
                    resync_required,
                })
            })
        })
    }

    pub fn refresh_floor_secs(&self) -> Result<u64, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            self.with_state(|state| Ok(state.refresh_floor_secs))
        })
    }

    pub fn shutdown(&self) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let mut guard = self.lock()?;
            *guard = None;
            Ok(())
        })
    }

    /// Intentional panic probe for containment tests.
    #[expect(
        clippy::panic,
        reason = "this exported test probe verifies panic containment at the FFI boundary"
    )]
    pub fn panic_probe(&self) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            panic!("usage-ffi intentional panic probe");
        })
    }
}

impl UsageMenuBarBridge {
    fn open_runtime_with_config(
        &self,
        host_config: ProjectionOpenConfig,
        broker_config: UsageBrokerConfig,
    ) -> Result<(), UsageBridgeError> {
        let _lifecycle = self.lifecycle_lock()?;
        let live = host_config.allow_live_probes;
        let enabled_surface_ids = host_config.enabled_surface_ids.clone();
        let scope = jackin_usage::host::UsageDiscoveryScope::HostDesktop {
            config_root: host_config.config_root.clone(),
            operator_home: host_config.operator_home.clone(),
        };
        let client = if live {
            jackin_usage::host::ensure_usage_broker_process(broker_config.clone(), &scope)
                .map_err(map_coordination_err)?
        } else {
            broker_config.client()
        };
        let projection = match client.current_projection() {
            Ok(projection) => projection,
            Err(_error) if !live => empty_projection(&broker_config.build_id),
            Err(error) => return Err(map_coordination_err(error)),
        };
        let mut projection_config = HostUsageProjectionConfig::under_data_dir(host_config.data_dir);
        projection_config.enabled_surface_ids = enabled_surface_ids.clone();
        let runtime = HostUsageProjectionRuntime::open(projection, projection_config)
            .map_err(map_open_err)?;
        let mut state = BridgeState {
            runtime,
            client,
            live_probes_enabled: live,
            enabled_surface_ids,
            refresh_floor_secs: host_config.refresh_floor_secs.max(60),
            format_prefs: UsageFormatPrefs::default(),
            last_refresh: None,
            event_sequence: 0,
            events: VecDeque::new(),
        };
        push_event(&mut state, "runtime_opened", None, None);
        *self.lock()? = Some(state);
        Ok(())
    }

    fn poll_latest_projection(&self) {
        let client = self
            .lock()
            .ok()
            .and_then(|state| state.as_ref().map(|state| state.client.clone()));
        let Some(client) = client else {
            return;
        };
        let Ok(publication) = client.current_projection() else {
            return;
        };
        drop(self.apply_publication(publication));
    }

    fn apply_publication(&self, publication: UsageProjectionV1) -> Result<(), UsageBridgeError> {
        self.with_state(|state| {
            let old_id = state.runtime.projection().projection_id.clone();
            state
                .runtime
                .apply_publication(publication)
                .map_err(map_runtime_err)?;
            let new_id = state.runtime.projection().projection_id.clone();
            if new_id != old_id {
                push_event(state, "projection_published", None, Some(new_id));
            }
            Ok(())
        })
    }

    fn with_state<R>(
        &self,
        operation: impl FnOnce(&mut BridgeState) -> Result<R, UsageBridgeError>,
    ) -> Result<R, UsageBridgeError> {
        let mut guard = self.lock()?;
        let state = guard.as_mut().ok_or(UsageBridgeError::RuntimeUnavailable)?;
        operation(state)
    }

    fn ensure_surface_enabled(&self, surface: HostSurfaceId) -> Result<(), UsageBridgeError> {
        self.with_state(|state| {
            if is_enabled(&state.enabled_surface_ids, surface) {
                Ok(())
            } else {
                Err(UsageBridgeError::rejected("runtime", "surface is disabled"))
            }
        })
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Option<BridgeState>>, UsageBridgeError> {
        self.inner
            .lock()
            .map_err(|_| UsageBridgeError::rejected("lock", "runtime mutex poisoned"))
    }

    fn lifecycle_lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, UsageBridgeError> {
        self.lifecycle
            .lock()
            .map_err(|_| UsageBridgeError::rejected("lock", "runtime lifecycle mutex poisoned"))
    }
}

fn push_event(
    state: &mut BridgeState,
    kind: &str,
    surface_id: Option<String>,
    detail: Option<String>,
) {
    state.event_sequence = state.event_sequence.saturating_add(1);
    state.events.push_back(UsageEventDto {
        sequence: state.event_sequence,
        kind: kind.to_owned(),
        surface_id,
        detail,
    });
    while state.events.len() > 256 {
        state.events.pop_front();
    }
}

fn is_enabled(ids: &[String], surface: HostSurfaceId) -> bool {
    ids.is_empty() || ids.iter().any(|id| id == surface.id())
}

fn severity_rank(severity: &str) -> u8 {
    match severity {
        "danger" => 2,
        "warn" => 1,
        _ => 0,
    }
}

fn empty_projection(build_id: &str) -> UsageProjectionV1 {
    UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: format!("offline-{build_id}"),
        generated_at_epoch: 0,
        discovery_revision: "offline".to_owned(),
        broker_instance_id: format!("offline-{build_id}"),
        broker_generation: 0,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: Vec::new(),
        unresolved: Vec::new(),
        issues: Vec::new(),
    }
}

fn map_coordination_err(error: UsageCoordinationError) -> UsageBridgeError {
    let code = match error.kind {
        UsageCoordinationErrorKind::Unavailable => "coordination_unavailable",
        UsageCoordinationErrorKind::Unauthorized => "coordination_unauthorized",
        UsageCoordinationErrorKind::OwnerLost => "coordination_owner_lost",
        UsageCoordinationErrorKind::WaitTimeout => "coordination_wait_timeout",
        UsageCoordinationErrorKind::CorruptState => "coordination_corrupt_state",
        UsageCoordinationErrorKind::ProviderTimeout => "coordination_provider_timeout",
        UsageCoordinationErrorKind::ProviderUnavailable => "coordination_provider_unavailable",
        UsageCoordinationErrorKind::NeedsSecret => "coordination_needs_secret",
        UsageCoordinationErrorKind::RateLimited => "coordination_rate_limited",
        UsageCoordinationErrorKind::ProtocolMismatch => "coordination_protocol_mismatch",
        UsageCoordinationErrorKind::CatalogRevoked => "coordination_catalog_revoked",
        UsageCoordinationErrorKind::CatalogRevisionConflict => {
            "coordination_catalog_revision_conflict"
        }
    };
    UsageBridgeError::rejected(code, error.message)
}

#[cfg(test)]
mod tests;
