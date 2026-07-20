// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Coarse synchronous facade matching the roadmap `UniFFI` surface.

use std::sync::{Arc, Mutex};

use jackin_usage::host::HostUsageRuntime;

use crate::dto::{
    event_batch_dto, map_open_err, map_runtime_err, surface_dto, to_host_config, view_dto,
    OpenConfig, SurfaceDescriptorDto, UsageEventBatchDto, UsageViewDto,
};
use crate::error::{catch_entry, UsageBridgeError};

/// Process-scoped `UniFFI` facade over the host usage runtime.
#[derive(uniffi::Object)]
pub struct UsageMenuBarBridge {
    inner: Mutex<HostUsageRuntime>,
}

#[uniffi::export]
impl UsageMenuBarBridge {
    /// Construct a closed bridge.
    #[uniffi::constructor]
    #[must_use]
    pub fn create() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(HostUsageRuntime::new()),
        })
    }

    /// Open the host runtime (paths + enable set). Idempotent replace.
    pub fn open_runtime(&self, config: OpenConfig) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let mut guard = self.lock()?;
            guard.open(to_host_config(config)).map_err(map_open_err)
        })
    }

    /// List all host surfaces with enable flags.
    pub fn list_surfaces(&self) -> Result<Vec<SurfaceDescriptorDto>, UsageBridgeError> {
        catch_entry(|| {
            let guard = self.lock()?;
            Ok(guard
                .list_surfaces()
                .map_err(map_runtime_err)?
                .into_iter()
                .map(surface_dto)
                .collect())
        })
    }

    /// Enable or disable a surface for bar + refresh.
    pub fn set_enabled(&self, surface_id: String, enabled: bool) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let mut guard = self.lock()?;
            guard
                .set_enabled(&surface_id, enabled)
                .map_err(map_runtime_err)
        })
    }

    /// Refresh one surface (`surface_id`) or all enabled (`None`).
    pub fn refresh(&self, surface_id: Option<String>) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let mut guard = self.lock()?;
            guard
                .refresh(surface_id.as_deref())
                .map_err(map_runtime_err)
        })
    }

    /// Snapshot for one enabled surface.
    pub fn snapshot(&self, surface_id: String) -> Result<UsageViewDto, UsageBridgeError> {
        catch_entry(|| {
            let mut guard = self.lock()?;
            guard
                .snapshot(&surface_id)
                .map(view_dto)
                .map_err(map_runtime_err)
        })
    }

    /// Compact bar label for one surface.
    pub fn status_bar_label(
        &self,
        surface_id: String,
    ) -> Result<Option<String>, UsageBridgeError> {
        catch_entry(|| {
            let mut guard = self.lock()?;
            guard
                .status_bar_label(&surface_id)
                .map_err(map_runtime_err)
        })
    }

    /// Merged menu-bar text for all enabled surfaces.
    pub fn merged_status_bar_label(&self) -> Result<String, UsageBridgeError> {
        catch_entry(|| {
            let mut guard = self.lock()?;
            guard.merged_status_bar_label().map_err(map_runtime_err)
        })
    }

    /// Poll events after `cursor` (exclusive).
    pub fn next_events(
        &self,
        cursor: u64,
        max: u32,
    ) -> Result<UsageEventBatchDto, UsageBridgeError> {
        catch_entry(|| {
            let mut guard = self.lock()?;
            let batch = guard.next_events(cursor, max).map_err(map_runtime_err)?;
            if batch.resync_required {
                return Err(UsageBridgeError::ResyncRequired);
            }
            Ok(event_batch_dto(batch))
        })
    }

    /// Refresh floor seconds (clamped policy).
    pub fn refresh_floor_secs(&self) -> Result<u64, UsageBridgeError> {
        catch_entry(|| {
            let guard = self.lock()?;
            Ok(guard.refresh_floor_secs())
        })
    }

    /// Shutdown; idempotent.
    pub fn shutdown(&self) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let mut guard = self.lock()?;
            guard.shutdown();
            Ok(())
        })
    }

    /// Intentional panic probe for containment tests (never call from product UI).
    pub fn panic_probe(&self) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            #[expect(clippy::panic, reason = "intentional containment probe for UniFFI gate")]
            {
                panic!("usage-ffi intentional panic probe");
            }
        })
    }
}

impl UsageMenuBarBridge {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, HostUsageRuntime>, UsageBridgeError> {
        self.inner
            .lock()
            .map_err(|_| UsageBridgeError::rejected("lock", "runtime mutex poisoned"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jackin_protocol::control::{
        FocusedAccountHeader, FocusedUsageView, QuotaBucketView, StatusSlot, UsageConfidence,
        UsageSeverity, UsageSnapshotStatus, UsageSource,
    };
    use jackin_usage::host::HostUsageRuntime;

    fn open_bridge(dir: &std::path::Path) -> Arc<UsageMenuBarBridge> {
        let bridge = UsageMenuBarBridge::create();
        bridge
            .open_runtime(OpenConfig {
                data_dir: dir.display().to_string(),
                refresh_floor_secs: 120,
                enabled_surface_ids: vec!["codex".to_owned(), "claude".to_owned()],
            })
            .expect("open");
        bridge
    }

    #[test]
    fn panic_probe_is_contained() {
        let bridge = UsageMenuBarBridge::create();
        let err = bridge.panic_probe().expect_err("panic");
        assert!(matches!(err, UsageBridgeError::ContainedPanic { .. }));
        // Still usable after containment.
        bridge.shutdown().expect("shutdown");
    }

    #[test]
    fn fixture_snapshot_round_trip_via_bridge() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bridge = open_bridge(dir.path());
        // Inject through host runtime underneath for offline proof.
        {
            let mut guard = bridge.inner.lock().expect("lock");
            let view = FocusedUsageView {
                focused_agent: Some("codex".to_owned()),
                focused_provider: Some("Codex".to_owned()),
                account: FocusedAccountHeader {
                    provider_label: "OpenAI / Codex".to_owned(),
                    account_label: "codex@example.com".to_owned(),
                    username: None,
                    plan_label: Some("Pro 20x".to_owned()),
                    credential_origin: None,
                },
                buckets: vec![QuotaBucketView {
                    label: "Session".to_owned(),
                    used_label: Some("63% used".to_owned()),
                    limit_label: Some("100%".to_owned()),
                    remaining_percent: Some(37),
                    reset_label: Some("Resets in 2h".to_owned()),
                    resets_at: Some(99),
                    status_slot: Some(StatusSlot::Session),
                    pace_label: None,
                    status: UsageSnapshotStatus::Fresh,
                    used_money: None,
                    limit_money: None,
                    severity: UsageSeverity::Normal,
                }],
                status: UsageSnapshotStatus::Fresh,
                source: UsageSource::ProviderApi,
                confidence: UsageConfidence::Authoritative,
                fetched_at_epoch: 1,
                updated_label: "just now".to_owned(),
                status_bar_label: "Codex Session: 63% used · 37% left".to_owned(),
                tabs: Vec::new(),
                last_error: None,
            };
            guard
                .inject_snapshot("codex", view)
                .expect("inject");
        }
        let dto = bridge.snapshot("codex".to_owned()).expect("snapshot");
        assert_eq!(dto.status_bar_label, "Codex Session: 63% used · 37% left");
        assert_eq!(dto.buckets.len(), 1);
        assert_eq!(dto.buckets[0].remaining_percent, Some(37));
        assert_eq!(dto.buckets[0].resets_at, Some(99));
        assert_eq!(dto.status, "fresh");
        let merged = bridge.merged_status_bar_label().expect("merged");
        assert!(merged.contains("63%"));
        let surfaces = bridge.list_surfaces().expect("list");
        assert!(surfaces.iter().any(|s| s.id == "codex" && s.enabled));
        assert!(surfaces.iter().any(|s| s.id == "amp" && !s.enabled));
        bridge
            .set_enabled("codex".to_owned(), false)
            .expect("disable");
        assert!(bridge.snapshot("codex".to_owned()).is_err());
        bridge.shutdown().expect("shutdown");
        assert!(matches!(
            bridge.list_surfaces().expect_err("closed"),
            UsageBridgeError::RuntimeUnavailable
                | UsageBridgeError::Rejected { .. }
        ));
        // silence unused import warning if any
        let _ = HostUsageRuntime::new();
    }

    #[test]
    fn bounded_events_and_refresh_floor() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bridge = open_bridge(dir.path());
        assert_eq!(bridge.refresh_floor_secs().expect("floor"), 120);
        let batch = bridge.next_events(0, 50).expect("events");
        assert!(batch.next_cursor >= 1);
        assert!(batch.events.len() <= 256);
    }
}
