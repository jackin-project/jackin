// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `HostUsageRuntime` snapshot reads and refresh floor.

use super::{
    CanonicalAccountIdentity, DiscoveredAccountDescriptor, HostProbePolicy, HostSurfaceId,
    HostUsageRuntime,
};

use std::time::Duration;

use jackin_protocol::control::FocusedUsageView;

impl HostUsageRuntime {
    /// Seed a fixture view (tests / offline QA). Does not hit the network.
    pub fn inject_snapshot(
        &mut self,
        surface_id: &str,
        view: FocusedUsageView,
    ) -> Result<(), String> {
        self.require_open()?;
        let surface = HostSurfaceId::from_id(surface_id)
            .ok_or_else(|| format!("unknown surface: {surface_id}"))?;
        self.cache
            .insert_snapshot_for_test(surface.agent_slug(), surface.provider_label(), view);
        if let Some(discovery) = &mut self.discovery {
            let injected = self
                .cache
                .focused_snapshot(Some(surface.agent_slug()), surface.provider_label());
            if let Some(identity) = CanonicalAccountIdentity::from_view(surface, &injected) {
                let account_key = identity.account_key();
                if !discovery
                    .accounts
                    .iter()
                    .any(|account| account.account_key == account_key)
                {
                    discovery.accounts.push(DiscoveredAccountDescriptor {
                        surface_id: surface.id().to_owned(),
                        account_key,
                        account_label: injected.account.account_label.clone(),
                        provenance: Vec::new(),
                        source_ids: vec!["fixture".to_owned()],
                        identity,
                    });
                }
            }
        }
        self.push_event(
            "snapshot_updated",
            Some(surface.id()),
            Some("injected".to_owned()),
        );
        Ok(())
    }

    /// Update the refresh floor (seconds). Clamped to ≥ 60.
    pub fn set_refresh_floor_secs(&mut self, secs: u64) -> Result<(), String> {
        self.require_open()?;
        let clamped = secs.max(60);
        self.refresh_floor_secs = clamped;
        self.push_event(
            "config_changed",
            None,
            Some(format!("refresh_floor_secs={clamped}")),
        );
        Ok(())
    }

    /// Whether a non-forced refresh would hit the network (floor elapsed or never).
    #[must_use]
    pub fn refresh_due(&self) -> bool {
        if self.probe_policy == HostProbePolicy::Disabled {
            return false;
        }
        if self.broker_refresh_in_progress() {
            return true;
        }
        match self.last_refresh {
            None => true,
            Some(last) => last.elapsed() >= Duration::from_secs(self.refresh_floor_secs),
        }
    }

    /// Cached snapshot for one surface (honest refreshing/unavailable).
    ///
    /// When a non-live account is selected, returns that account's durable view
    /// (multi-account Desktop); otherwise the live host-login snapshot.
    pub fn snapshot(&mut self, surface_id: &str) -> Result<FocusedUsageView, String> {
        self.require_open()?;
        let surface = HostSurfaceId::from_id(surface_id)
            .ok_or_else(|| format!("unknown surface: {surface_id}"))?;
        if !self.enabled.contains(surface.id()) {
            return Err(format!("surface disabled: {surface_id}"));
        }
        let live = self
            .cache
            .focused_snapshot(Some(surface.agent_slug()), surface.provider_label());
        let catalog = self.materialize_account_catalog()?;
        self.reconcile_selected_accounts(&catalog, std::slice::from_ref(&surface))?;
        let view = self
            .selected_view_for_catalog(&catalog, surface)
            .unwrap_or(live);
        Ok(self.with_discovery_diagnostic(surface, view))
    }

    /// Refresh floor in seconds (clamped).
    #[must_use]
    pub fn refresh_floor_secs(&self) -> u64 {
        self.refresh_floor_secs
    }
}
