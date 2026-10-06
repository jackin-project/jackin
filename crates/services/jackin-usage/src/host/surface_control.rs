// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `HostUsageRuntime` surface enablement and diagnostics.

use super::{
    HostSurfaceDescriptor, HostSurfaceId, HostUsageRuntime, UsageDiscoveryDiagnostic,
    UsageDiscoveryIssue, ValidatedUsageDiscovery,
};

use jackin_protocol::control::{FocusedUsageView, UsageSnapshotStatus};

impl HostUsageRuntime {
    /// Clone the current validated catalog for host broker attachment.
    #[must_use]
    pub fn validated_discovery(&self) -> Option<ValidatedUsageDiscovery> {
        self.discovery.clone()
    }

    /// Whether one provider surface is enabled for refresh.
    #[must_use]
    pub fn surface_enabled(&self, surface_id: &str) -> bool {
        self.enabled.contains(surface_id)
    }

    /// List surfaces with enable flags.
    pub fn list_surfaces(&self) -> Result<Vec<HostSurfaceDescriptor>, String> {
        self.require_open()?;
        Ok(HostSurfaceId::ALL
            .iter()
            .copied()
            .map(|surface| HostSurfaceDescriptor {
                id: surface.id().to_owned(),
                label: surface.label().to_owned(),
                agent: surface.agent_slug().to_owned(),
                provider: surface.provider_label().map(str::to_owned),
                enabled: self.enabled.contains(surface.id()),
            })
            .collect())
    }

    /// Sanitized discovery failures for the current completed catalog generation.
    pub fn discovery_diagnostics(&self) -> Result<Vec<UsageDiscoveryDiagnostic>, String> {
        self.require_open()?;
        Ok(self
            .discovery
            .as_ref()
            .map(|discovery| discovery.diagnostics.clone())
            .unwrap_or_default())
    }

    /// Enable or disable a surface for bar + refresh set.
    pub fn set_enabled(&mut self, surface_id: &str, enabled: bool) -> Result<(), String> {
        self.require_open()?;
        let surface = HostSurfaceId::from_id(surface_id)
            .ok_or_else(|| format!("unknown surface: {surface_id}"))?;
        if enabled {
            self.enabled.insert(surface.id().to_owned());
        } else {
            self.enabled.remove(surface.id());
        }
        self.push_event(
            "enabled_changed",
            Some(surface.id()),
            Some(if enabled { "enabled" } else { "disabled" }.to_owned()),
        );
        Ok(())
    }

    pub(crate) fn with_discovery_diagnostic(
        &self,
        surface: HostSurfaceId,
        view: FocusedUsageView,
    ) -> FocusedUsageView {
        // A cold placeholder must never mask a known discovery failure: when
        // no refresh is in flight and discovery already diagnosed this
        // surface, surface the honest needs-login/unavailable view instead.
        if view.is_refreshing_placeholder()
            && !self.surface_refresh_in_progress(surface.id())
            && let Some(honest) = self.diagnostic_view_for_surface(surface)
        {
            return honest;
        }
        view
    }

    /// Honest view for a surface whose discovery already failed.
    ///
    /// Returns `None` when the surface has no discovery diagnostic (a genuine
    /// cold start keeps the `refreshing` placeholder). Messages are sanitized:
    /// category + surface label only, never paths or secret coordinates.
    pub(crate) fn diagnostic_view_for_surface(
        &self,
        surface: HostSurfaceId,
    ) -> Option<FocusedUsageView> {
        let issue = self
            .discovery
            .as_ref()?
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.surface_id.as_deref() == Some(surface.id()))
            .map(|diagnostic| diagnostic.issue)?;
        let now = chrono::Utc::now().timestamp();
        let mut view = crate::usage::cached_unavailable_view(
            surface.agent_slug(),
            surface.provider_label(),
            now,
        );
        let label = surface.label();
        let (status, updated_label, status_bar_label, message) = match issue {
            UsageDiscoveryIssue::CredentialMissing => (
                UsageSnapshotStatus::NeedsLogin,
                "Needs login",
                "needs login",
                format!("credential missing: log in to {label} to enable usage"),
            ),
            UsageDiscoveryIssue::CredentialMalformed => (
                UsageSnapshotStatus::NeedsLogin,
                "Needs login",
                "needs login",
                format!("credential malformed: log in again to {label} to enable usage"),
            ),
            UsageDiscoveryIssue::CredentialDenied => (
                UsageSnapshotStatus::NeedsSecret,
                "Needs secret",
                "needs secret",
                format!("credential access denied for {label}"),
            ),
            UsageDiscoveryIssue::InteractionRequired => (
                UsageSnapshotStatus::NeedsSecret,
                "Needs secret",
                "needs secret",
                format!("credential access requires interaction for {label}"),
            ),
            UsageDiscoveryIssue::KeychainConsentRequired => (
                UsageSnapshotStatus::NeedsSecret,
                "Approve access",
                "approve access",
                format!("keychain access requires approval for {label}"),
            ),
            UsageDiscoveryIssue::ConfigUnreadable => (
                UsageSnapshotStatus::Unavailable,
                "Unavailable",
                "usage unavailable",
                format!("configuration unreadable for {label}"),
            ),
            UsageDiscoveryIssue::ConfigInvalid => (
                UsageSnapshotStatus::Unavailable,
                "Unavailable",
                "usage unavailable",
                format!("configuration invalid for {label}"),
            ),
            UsageDiscoveryIssue::ConfigVersionUnsupported => (
                UsageSnapshotStatus::Unavailable,
                "Unavailable",
                "usage unavailable",
                format!("configuration version unsupported for {label}"),
            ),
            UsageDiscoveryIssue::ConfigTransientConflict => (
                UsageSnapshotStatus::Unavailable,
                "Unavailable",
                "usage unavailable",
                format!("configuration changed during discovery for {label}"),
            ),
        };
        view.status = status;
        view.updated_label = updated_label.to_owned();
        view.status_bar_label = status_bar_label.to_owned();
        view.last_error = Some(message);
        Some(view)
    }
}
