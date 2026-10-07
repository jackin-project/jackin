// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Host runtime broker glue methods.

use std::time::Instant;

use jackin_protocol::control::UsageSnapshotStatus;
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageGenerationView, UsageRefreshPhase,
};

use super::super::HostUsageRuntime;
use super::super::discovery::ValidatedCredentialBinding;
use jackin_usage_discovery::capability_for_binding;

impl HostUsageRuntime {
    /// Whether this runtime permits host broker provider work.
    #[must_use]
    pub fn live_probes_enabled(&self) -> bool {
        self.probe_policy == super::super::HostProbePolicy::Live
    }

    /// Whether any host broker generation remains active.
    #[must_use]
    pub fn broker_refresh_in_progress(&self) -> bool {
        self.broker_phases.values().any(|phase| phase.is_active())
    }

    /// Whether any active broker generation belongs to `surface_id`.
    #[must_use]
    pub fn surface_refresh_in_progress(&self, surface_id: &str) -> bool {
        self.broker_phases
            .iter()
            .any(|(capability, phase)| capability.surface_id == surface_id && phase.is_active())
    }

    /// Adopt one host-broker projection and never execute provider work here.
    pub fn apply_broker_generation(&mut self, state: UsageGenerationView) -> Result<(), String> {
        self.require_open()?;
        let capability = state.capability.clone();
        self.broker_phases.insert(capability.clone(), state.phase);
        let binding = self.discovery.as_ref().and_then(|discovery| {
            discovery
                .bindings
                .iter()
                .find(|binding| {
                    capability_for_binding(binding, discovery.config_generation.as_deref())
                        == capability
                })
                .cloned()
        });
        if binding.is_some() {
            self.broker_generations
                .insert(capability.clone(), state.clone());
        }
        if let Some(mut view) = state.snapshot {
            if let Some(error) = &state.error {
                view.last_error = Some(error.message.clone());
                view.status = if view.buckets.is_empty() {
                    UsageSnapshotStatus::Error
                } else {
                    UsageSnapshotStatus::Stale
                };
            }
            if let Some(binding) = &binding {
                self.record_discovered_snapshot(binding, view);
            }
        } else if let Some(error) = &state.error {
            // A failure without a snapshot means the broker holds no
            // last-good quota for this capability, so recording an honest
            // error view cannot clobber good data. Without it the snapshot
            // surface would keep showing a stale placeholder forever.
            if let Some(binding) = &binding {
                self.record_broker_error_view(binding, error);
            }
            self.push_event(
                "probe_failed",
                Some(&capability.surface_id),
                Some(error.message.clone()),
            );
        }
        if state.phase.is_terminal() {
            self.broker_phases.remove(&capability);
            self.last_refresh = Some(Instant::now());
        }
        self.push_event(
            "broker_phase_changed",
            Some(&capability.surface_id),
            Some(
                match state.phase {
                    UsageRefreshPhase::Idle => "idle",
                    UsageRefreshPhase::Queued => "queued",
                    UsageRefreshPhase::Updating => "updating",
                    UsageRefreshPhase::Completed => "completed",
                    UsageRefreshPhase::Failed => "failed",
                }
                .to_owned(),
            ),
        );
        Ok(())
    }

    /// Record one broker failure as an honest snapshot-surface view.
    ///
    /// Identity bindings resolve to their canonical account row;
    /// identity-less bindings stay surface-scoped so anonymous sources never
    /// mint rows. The broker error message carries the collector's specific
    /// gap reason.
    fn record_broker_error_view(
        &mut self,
        binding: &ValidatedCredentialBinding,
        error: &UsageCoordinationError,
    ) {
        let status = match error.kind {
            UsageCoordinationErrorKind::NeedsSecret => UsageSnapshotStatus::NeedsSecret,
            _ => UsageSnapshotStatus::Unavailable,
        };
        let (updated_label, status_bar_label) = match status {
            UsageSnapshotStatus::NeedsSecret => ("Needs secret", "secret"),
            _ => ("Unavailable", "usage unavailable"),
        };
        let mut view = jackin_protocol::control::FocusedUsageView::refreshing(
            binding.surface.provider_label(),
            chrono::Utc::now().timestamp(),
        );
        view.focused_agent = Some(binding.surface.agent_slug().to_owned());
        view.status = status;
        view.updated_label = updated_label.to_owned();
        view.status_bar_label = status_bar_label.to_owned();
        view.last_error = Some(error.message.clone());
        if let Some(identity) = binding.identity.clone() {
            let account_key = identity.account_key();
            view.account.account_label = self
                .discovered_views
                .get(&(binding.surface, account_key.clone()))
                .map(|view| view.account.account_label.clone())
                .filter(|label| !label.trim().is_empty())
                .or_else(|| {
                    self.discovery.as_ref().and_then(|discovery| {
                        discovery
                            .accounts
                            .iter()
                            .find(|account| account.identity == identity)
                            .map(|account| account.account_label.clone())
                    })
                })
                .unwrap_or_default();
            self.discovered_views
                .insert((binding.surface, account_key), view);
        } else {
            // Surface-scoped honest error: never overwrite a recorded view
            // from a sibling binding, and never mint an account row.
            self.discovered_provider_views
                .entry(binding.surface)
                .or_insert(view);
        }
        self.push_event("snapshot_updated", Some(binding.surface.id()), None);
    }

    /// Surface one coordination failure without discarding last-good quota.
    pub fn record_broker_error(
        &mut self,
        capability: &UsageAccountCapability,
        error: &UsageCoordinationError,
    ) -> Result<(), String> {
        self.require_open()?;
        if error.kind == UsageCoordinationErrorKind::CatalogRevoked
            && self.broker_phases.remove(capability).is_some()
        {
            self.push_event(
                "broker_phase_changed",
                Some(&capability.surface_id),
                Some("failed".to_owned()),
            );
        }
        // A failed client request still affects the rendered account. Keep
        // the last broker snapshot and reported retry deadline; a transport
        // failure supplies neither a new quota observation nor retry policy.
        let binding = self.discovery.as_ref().and_then(|discovery| {
            discovery.bindings.iter().find(|binding| {
                capability_for_binding(binding, discovery.config_generation.as_deref())
                    == *capability
            })
        });
        if let Some(binding) = binding {
            let mut state = self
                .broker_generations
                .get(capability)
                .cloned()
                .unwrap_or_else(|| UsageGenerationView {
                    capability: capability.clone(),
                    generation: 0,
                    phase: UsageRefreshPhase::Failed,
                    snapshot: binding.identity.as_ref().and_then(|identity| {
                        self.discovered_views
                            .get(&(binding.surface, identity.account_key()))
                            .cloned()
                    }),
                    error: None,
                    retry_at_epoch: None,
                });
            // Request failure does not cancel work already running at the
            // broker. Keep its active phase until the next received state.
            if !state.phase.is_active() || error.kind == UsageCoordinationErrorKind::CatalogRevoked
            {
                state.phase = UsageRefreshPhase::Failed;
            }
            state.error = Some(error.clone());
            self.apply_broker_generation(state)?;
        }
        self.push_event(
            "probe_failed",
            Some(&capability.surface_id),
            Some(error.message.clone()),
        );
        Ok(())
    }
}
