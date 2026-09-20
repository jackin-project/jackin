// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Coarse synchronous facade matching the roadmap `boltffi` surface.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageCoordinationErrorKind,
};
use jackin_usage::host::{
    HostSurfaceId, HostUsageRuntime, UsageBrokerClient, UsageBrokerConfig, UsageDiscoveryScope,
    ValidatedUsageDiscovery, discover_usage_sources, validate_usage_sources,
};

use crate::discovery::DesktopCredentialResolver;
use crate::dto::{
    AccountDescriptorDto, DesktopInventoryDto, DesktopProjectionDto, DiscoveryDiagnosticDto,
    OpenConfig, OverviewRowDto, ProviderGlanceRowDto, SurfaceDescriptorDto, UsageEventBatchDto,
    UsageFormatPrefsDto, UsageViewDto, account_dto, desktop_inventory_dto, desktop_projection_dto,
    discovery_diagnostic_dto, event_batch_dto, map_open_err, map_runtime_err, overview_row_dto,
    parse_format_prefs, provider_glance_row_dto, surface_dto, to_host_config, view_dto,
};
use crate::error::{UsageBridgeError, catch_entry};

/// Process-scoped `boltffi` facade over the host usage runtime.
pub struct UsageMenuBarBridge {
    inner: Arc<Mutex<HostUsageRuntime>>,
    credential_resolver: Arc<DesktopCredentialResolver>,
    broker: Arc<Mutex<Option<DesktopBroker>>>,
    broker_lifecycle: Arc<Mutex<()>>,
    joiners: Arc<Mutex<BTreeSet<(UsageAccountCapability, u64)>>>,
}

#[derive(Clone)]
struct DesktopBroker {
    client: UsageBrokerClient,
    capabilities: Vec<UsageAccountCapability>,
    catalog_lease: String,
    config: UsageBrokerConfig,
    scope: UsageDiscoveryScope,
}

#[boltffi::export]
impl UsageMenuBarBridge {
    /// Construct a closed bridge.
    #[must_use]
    pub fn create() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HostUsageRuntime::new())),
            credential_resolver: Arc::new(DesktopCredentialResolver::default()),
            broker: Arc::new(Mutex::new(None)),
            broker_lifecycle: Arc::new(Mutex::new(())),
            joiners: Arc::new(Mutex::new(BTreeSet::new())),
        }
    }

    /// Open the host runtime (paths + enable set). Idempotent replace.
    pub fn open_runtime(&self, config: OpenConfig) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let host_config = to_host_config(config).map_err(map_open_err)?;
            let broker_config = UsageBrokerConfig::for_data_dir(host_config.data_dir.clone());
            self.open_runtime_with_config(host_config, broker_config)
        })
    }

    /// List all host surfaces with enable flags.
    pub fn list_surfaces(&self) -> Result<Vec<SurfaceDescriptorDto>, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let guard = self.lock()?;
            Ok(guard
                .list_surfaces()
                .map_err(map_runtime_err)?
                .into_iter()
                .map(surface_dto)
                .collect())
        })
    }

    /// Sanitized discovery diagnostics for the current catalog generation.
    pub fn discovery_diagnostics(&self) -> Result<Vec<DiscoveryDiagnosticDto>, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let guard = self.lock()?;
            Ok(guard
                .discovery_diagnostics()
                .map_err(map_runtime_err)?
                .into_iter()
                .map(discovery_diagnostic_dto)
                .collect())
        })
    }

    /// Enable or disable a surface for bar + refresh.
    pub fn set_enabled(&self, surface_id: String, enabled: bool) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let mut guard = self.lock()?;
            guard
                .set_enabled(&surface_id, enabled)
                .map_err(map_runtime_err)
        })
    }

    /// Refresh one surface (`surface_id`) or all enabled (`None`).
    ///
    /// When `force` is false, respects the runtime refresh floor (poll-safe).
    /// When `force` is true, bypasses the floor (manual Refresh).
    pub fn refresh(&self, surface_id: Option<String>, force: bool) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            if force {
                self.reconcile_broker_catalog()?;
            }
            {
                let guard = self.lock()?;
                if !guard.live_probes_enabled() {
                    return Ok(());
                }
                if surface_id
                    .as_deref()
                    .is_some_and(|surface| !guard.surface_enabled(surface))
                {
                    return Err(UsageBridgeError::rejected("runtime", "surface is disabled"));
                }
            }
            let broker = self
                .broker_lock()?
                .clone()
                .ok_or(UsageBridgeError::RuntimeUnavailable)?;
            let capabilities = {
                let runtime = self.lock()?;
                broker
                    .capabilities
                    .iter()
                    .filter(|capability| {
                        surface_id
                            .as_deref()
                            .is_none_or(|surface| capability.surface_id == surface)
                    })
                    .filter(|capability| runtime.surface_enabled(&capability.surface_id))
                    .cloned()
                    .collect::<Vec<_>>()
            };
            let mut first_error = None;
            for capability in capabilities {
                let result = broker
                    .client
                    .current(capability.clone())
                    .and_then(|current| {
                        broker
                            .client
                            .refresh(capability.clone(), current.generation, force)
                    });
                let state = match result {
                    Ok(state) => state,
                    Err(error) => {
                        self.lock()?
                            .record_broker_error(&capability, &error)
                            .map_err(map_runtime_err)?;
                        first_error.get_or_insert(error);
                        continue;
                    }
                };
                self.lock()?
                    .apply_broker_generation(state.clone())
                    .map_err(map_runtime_err)?;
                if state.phase.is_active() {
                    self.schedule_join(
                        broker.client.clone(),
                        capability,
                        state.generation,
                        broker.catalog_lease.clone(),
                    );
                }
            }
            first_error.map_or(Ok(()), |error| Err(map_coordination_err(error)))
        })
    }

    /// True while at least one Rust broker generation is queued/updating.
    pub fn refresh_in_progress(&self) -> Result<bool, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            Ok(self.lock()?.broker_refresh_in_progress())
        })
    }

    /// Set refresh floor seconds (clamped ≥ 60 in Rust).
    pub fn set_refresh_floor_secs(&self, secs: u64) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let mut guard = self.lock()?;
            guard.set_refresh_floor_secs(secs).map_err(map_runtime_err)
        })
    }

    /// Whether a non-forced refresh would probe the network.
    pub fn refresh_due(&self) -> Result<bool, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let guard = self.lock()?;
            Ok(guard.refresh_due())
        })
    }

    /// Snapshot for one enabled surface (selected multi-account when set).
    pub fn snapshot(&self, surface_id: String) -> Result<UsageViewDto, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let mut guard = self.lock()?;
            guard
                .snapshot(&surface_id)
                .map(view_dto)
                .map_err(map_runtime_err)
        })
    }

    /// List known accounts for one surface (`Some`) or all surfaces (`None`).
    pub fn list_accounts(
        &self,
        surface_id: Option<String>,
    ) -> Result<Vec<AccountDescriptorDto>, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let mut guard = self.lock()?;
            Ok(guard
                .list_accounts(surface_id.as_deref())
                .map_err(map_runtime_err)?
                .into_iter()
                .map(account_dto)
                .collect())
        })
    }

    /// Atomic, Rust-ordered provider/account projection for jackin❯ desktop.
    pub fn desktop_inventory(&self) -> Result<DesktopInventoryDto, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let mut guard = self.lock()?;
            guard
                .desktop_inventory()
                .map(desktop_inventory_dto)
                .map_err(map_runtime_err)
        })
    }

    /// Complete native Desktop state produced beneath one runtime mutex hold.
    /// Any required nested projection failure fails the whole call; callers retain
    /// their last complete value rather than replacing it with partial arrays.
    pub fn desktop_projection(
        &self,
        status_bar_max: u32,
    ) -> Result<DesktopProjectionDto, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let mut guard = self.lock()?;
            guard
                .desktop_projection(status_bar_max)
                .map(desktop_projection_dto)
                .map_err(map_runtime_err)
        })
    }

    /// Select which account drives snapshot/detail for a surface (persisted).
    pub fn set_selected_account(
        &self,
        surface_id: String,
        account_key: String,
    ) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let mut guard = self.lock()?;
            guard
                .set_selected_account(&surface_id, &account_key)
                .map_err(map_runtime_err)
        })
    }

    /// Compact bar label for one surface.
    pub fn status_bar_label(&self, surface_id: String) -> Result<Option<String>, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let mut guard = self.lock()?;
            guard.status_bar_label(&surface_id).map_err(map_runtime_err)
        })
    }

    /// Merged menu-bar text for all enabled surfaces.
    pub fn merged_status_bar_label(&self) -> Result<String, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let mut guard = self.lock()?;
            guard.merged_status_bar_label().map_err(map_runtime_err)
        })
    }

    /// Short status-item label (worst enabled surface; remaining % by default).
    pub fn compact_status_bar_label(&self) -> Result<String, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let mut guard = self.lock()?;
            guard.compact_status_bar_label().map_err(map_runtime_err)
        })
    }

    /// Presentation-time format prefs (`left`/`used`, `countdown`/`exact_clock`).
    pub fn set_format_prefs(&self, prefs: UsageFormatPrefsDto) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let parsed = parse_format_prefs(prefs).map_err(map_runtime_err)?;
            let _lifecycle = self.lifecycle_lock()?;
            let mut guard = self.lock()?;
            guard.set_format_prefs(parsed).map_err(map_runtime_err)
        })
    }

    /// Pinned-surface compact status-item label.
    pub fn compact_status_bar_label_for(
        &self,
        surface_id: String,
    ) -> Result<Option<String>, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let mut guard = self.lock()?;
            guard
                .compact_status_bar_label_for(&surface_id)
                .map_err(map_runtime_err)
        })
    }

    /// Worst-first multi-surface compact strip (joined with ` · `).
    pub fn compact_status_bar_strip(&self, max: u32) -> Result<String, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let mut guard = self.lock()?;
            guard.compact_status_bar_strip(max).map_err(map_runtime_err)
        })
    }

    /// Overview rows for every enabled surface (popover + Usage window).
    pub fn overview_rows(&self) -> Result<Vec<OverviewRowDto>, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let mut guard = self.lock()?;
            Ok(guard
                .overview_rows()
                .map_err(map_runtime_err)?
                .into_iter()
                .map(overview_row_dto)
                .collect())
        })
    }

    /// Selected-account-aware provider glance rows in the canonical Desktop
    /// order (popover / Usage inventory). Full detected set — includes 0%.
    /// Rust owns detection, ordering, and every display string.
    pub fn provider_glance_rows(&self) -> Result<Vec<ProviderGlanceRowDto>, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let mut guard = self.lock()?;
            Ok(guard
                .provider_glance_rows()
                .map_err(map_runtime_err)?
                .into_iter()
                .map(provider_glance_row_dto)
                .collect())
        })
    }

    /// Burn-first **status bar** glance rows only (SB-3/14/17/19).
    ///
    /// Filters 0%, ranks soonest-then-remaining, hard-caps at 3. `max` is
    /// clamped into `1…3`. Popover/Usage keep using [`Self::provider_glance_rows`].
    pub fn status_bar_provider_glance_rows(
        &self,
        max: u32,
    ) -> Result<Vec<ProviderGlanceRowDto>, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let mut guard = self.lock()?;
            Ok(guard
                .status_bar_provider_glance_rows(max)
                .map_err(map_runtime_err)?
                .into_iter()
                .map(provider_glance_row_dto)
                .collect())
        })
    }

    /// Next network refresh label (`Next update in …` / `Next update due`).
    pub fn next_refresh_label(&self) -> Result<String, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let guard = self.lock()?;
            Ok(guard.next_refresh_label())
        })
    }

    /// Poll events after `cursor` (exclusive).
    ///
    /// Always returns `Ok` for a valid open runtime. When the client cursor is
    /// behind the retained log, `resync_required` is true on the batch (do not
    /// turn that into an error — presentation must reset the cursor).
    pub fn next_events(
        &self,
        cursor: u64,
        max: u32,
    ) -> Result<UsageEventBatchDto, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let mut guard = self.lock()?;
            let batch = guard.next_events(cursor, max).map_err(map_runtime_err)?;
            Ok(event_batch_dto(batch))
        })
    }

    /// Refresh floor seconds (clamped policy).
    pub fn refresh_floor_secs(&self) -> Result<u64, UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let guard = self.lock()?;
            Ok(guard.refresh_floor_secs())
        })
    }

    /// Shutdown; idempotent.
    pub fn shutdown(&self) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let _lifecycle = self.lifecycle_lock()?;
            let mut guard = self.lock()?;
            guard.shutdown();
            *self.broker_lock()? = None;
            if let Ok(mut joiners) = self.joiners.lock() {
                joiners.clear();
            }
            Ok(())
        })
    }

    /// Intentional panic probe for containment tests (never call from product UI).
    pub fn panic_probe(&self) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            #[expect(
                clippy::panic,
                reason = "intentional containment probe for boltffi gate"
            )]
            {
                panic!("usage-ffi intentional panic probe");
            }
        })
    }
}

impl UsageMenuBarBridge {
    fn open_runtime_with_config(
        &self,
        host_config: jackin_usage::host::HostRuntimeConfig,
        broker_config: UsageBrokerConfig,
    ) -> Result<(), UsageBridgeError> {
        let _lifecycle = self.lifecycle_lock()?;
        let discovery_scope = host_config.discovery_scope.clone();
        let unknown = host_config
            .enabled_surface_ids
            .iter()
            .filter(|id| HostSurfaceId::from_id(id).is_none())
            .cloned()
            .collect::<Vec<_>>();
        if !unknown.is_empty() {
            return Err(map_open_err(format!(
                "unknown enabled surface ids: {}",
                unknown.join(", ")
            )));
        }
        let live = host_config.probe_policy == jackin_usage::host::HostProbePolicy::Live;
        let catalog = discover_usage_sources(
            &host_config.discovery_scope,
            self.credential_resolver.as_ref(),
        )
        .map_err(map_open_err)?;
        let discovery = validate_usage_sources(catalog, self.credential_resolver.as_ref());
        let fallback = broker_config.client();
        let broker = if live {
            self.activate_broker(
                broker_config.clone(),
                discovery_scope.clone(),
                discovery.clone(),
            )
            .map_err(map_coordination_err)?
        } else {
            DesktopBroker {
                client: fallback,
                capabilities: Vec::new(),
                catalog_lease: "disabled".to_owned(),
                config: broker_config.clone(),
                scope: discovery_scope.clone(),
            }
        };
        let mut runtime = self.lock()?;
        runtime
            .open_with_validated_discovery(host_config, discovery)
            .map_err(map_open_err)?;
        *self.broker_lock()? = Some(broker);
        Ok(())
    }

    fn reconcile_broker_catalog(&self) -> Result<(), UsageBridgeError> {
        let mut broker_guard = self.broker_lock()?;
        let current = broker_guard
            .clone()
            .ok_or(UsageBridgeError::RuntimeUnavailable)?;
        let mut runtime = self.lock()?;
        if !runtime.live_probes_enabled() {
            return Ok(());
        }
        let Some(staged) = runtime
            .stage_discovery(self.credential_resolver.as_ref())
            .map_err(map_runtime_err)?
        else {
            return Ok(());
        };
        if !staged.changed {
            runtime
                .commit_staged_discovery(staged)
                .map_err(map_runtime_err)?;
            return Ok(());
        }
        let discovery = staged.discovery.clone();
        let refreshed = self
            .activate_broker(current.config, current.scope, discovery)
            .map_err(map_coordination_err)?;
        runtime
            .commit_staged_discovery(staged)
            .map_err(map_runtime_err)?;
        *broker_guard = Some(refreshed);
        Ok(())
    }

    fn activate_broker(
        &self,
        config: UsageBrokerConfig,
        scope: UsageDiscoveryScope,
        discovery: ValidatedUsageDiscovery,
    ) -> Result<DesktopBroker, UsageCoordinationError> {
        let handle = jackin_usage::host::ensure_usage_broker(
            config.clone(),
            scope.clone(),
            discovery,
            self.credential_resolver.clone(),
        )?;
        Ok(DesktopBroker {
            client: handle.client,
            capabilities: handle.capabilities,
            catalog_lease: handle.catalog_lease,
            config,
            scope,
        })
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, HostUsageRuntime>, UsageBridgeError> {
        self.inner
            .lock()
            .map_err(|_| UsageBridgeError::rejected("lock", "runtime mutex poisoned"))
    }

    fn broker_lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, Option<DesktopBroker>>, UsageBridgeError> {
        self.broker
            .lock()
            .map_err(|_| UsageBridgeError::rejected("lock", "broker mutex poisoned"))
    }

    fn lifecycle_lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, UsageBridgeError> {
        self.broker_lifecycle
            .lock()
            .map_err(|_| UsageBridgeError::rejected("lock", "broker lifecycle mutex poisoned"))
    }

    fn schedule_join(
        &self,
        client: UsageBrokerClient,
        capability: UsageAccountCapability,
        generation: u64,
        catalog_lease: String,
    ) {
        let key = (capability.clone(), generation);
        {
            let Ok(mut joiners) = self.joiners.lock() else {
                return;
            };
            if !joiners.insert(key.clone()) {
                return;
            }
        }
        let runtime = Arc::clone(&self.inner);
        let lifecycle = Arc::clone(&self.broker_lifecycle);
        let broker = self.broker.clone();
        let joiners = Arc::clone(&self.joiners);
        let name = format!("desktop-usage-join-{}", capability.surface_id);
        let worker_key = key.clone();
        let worker = jackin_telemetry::spawn::thread_joined_named(name, move || {
            let result = client.join(capability.clone(), generation, Duration::from_secs(30));
            let Ok(_lifecycle) = lifecycle.lock() else {
                if let Ok(mut joiners) = joiners.lock() {
                    joiners.remove(&worker_key);
                }
                return;
            };
            let Ok(mut runtime) = runtime.lock() else {
                if let Ok(mut joiners) = joiners.lock() {
                    joiners.remove(&worker_key);
                }
                return;
            };
            let stale = broker
                .lock()
                .ok()
                .and_then(|broker| {
                    broker
                        .as_ref()
                        .map(|broker| broker.catalog_lease != catalog_lease)
                })
                .unwrap_or(true);
            if stale {
                let error = UsageCoordinationError {
                    kind: UsageCoordinationErrorKind::CatalogRevoked,
                    message: "usage refresh generation was fenced by catalog rotation".to_owned(),
                };
                drop(runtime.record_broker_error(&capability, &error));
            } else {
                match result {
                    Ok(state) => drop(runtime.apply_broker_generation(state)),
                    Err(error) => {
                        drop(runtime.record_broker_error(&capability, &error));
                    }
                }
            }
            if let Ok(mut joiners) = joiners.lock() {
                joiners.remove(&worker_key);
            }
        });
        if worker.is_err()
            && let Ok(mut joiners) = self.joiners.lock()
        {
            joiners.remove(&key);
        }
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
