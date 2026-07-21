// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Coarse synchronous facade matching the roadmap `UniFFI` surface.

use std::sync::{Arc, Mutex};

use jackin_usage::host::HostUsageRuntime;

use crate::dto::{
    OpenConfig, SurfaceDescriptorDto, UsageEventBatchDto, UsageViewDto, event_batch_dto,
    map_open_err, map_runtime_err, surface_dto, to_host_config, view_dto,
};
use crate::error::{UsageBridgeError, catch_entry};

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
    ///
    /// When `force` is false, respects the runtime refresh floor (poll-safe).
    /// When `force` is true, bypasses the floor (manual Refresh).
    pub fn refresh(&self, surface_id: Option<String>, force: bool) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let mut guard = self.lock()?;
            guard
                .refresh(surface_id.as_deref(), force)
                .map_err(map_runtime_err)
        })
    }

    /// Set refresh floor seconds (clamped ≥ 60 in Rust).
    pub fn set_refresh_floor_secs(&self, secs: u64) -> Result<(), UsageBridgeError> {
        catch_entry(|| {
            let mut guard = self.lock()?;
            guard.set_refresh_floor_secs(secs).map_err(map_runtime_err)
        })
    }

    /// Whether a non-forced refresh would probe the network.
    pub fn refresh_due(&self) -> Result<bool, UsageBridgeError> {
        catch_entry(|| {
            let guard = self.lock()?;
            Ok(guard.refresh_due())
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
    pub fn status_bar_label(&self, surface_id: String) -> Result<Option<String>, UsageBridgeError> {
        catch_entry(|| {
            let mut guard = self.lock()?;
            guard.status_bar_label(&surface_id).map_err(map_runtime_err)
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
            let mut guard = self.lock()?;
            let batch = guard.next_events(cursor, max).map_err(map_runtime_err)?;
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
            #[expect(
                clippy::panic,
                reason = "intentional containment probe for UniFFI gate"
            )]
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
mod tests;
