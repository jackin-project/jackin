// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `HostUsageRuntime` discovery staging.

use jackin_usage_discovery::{
    DiscoveredAccountDescriptor, ValidatedCredentialBinding, discover_usage_sources,
    validate_usage_sources,
};
use jackin_usage_host_credentials::ProviderCredentialEnvResolver;

use jackin_protocol::control::FocusedUsageView;

use super::super::CanonicalAccountIdentity;

use super::super::HostUsageRuntime;
use super::super::StagedUsageDiscovery;
use super::super::discovered_account_keys;

impl HostUsageRuntime {
    /// Run one fresh, read-only discovery scan. A failed scan returns `None`
    /// and never hands stale credentials back to broker rotation.
    pub fn stage_discovery(
        &mut self,
        resolver: &dyn ProviderCredentialEnvResolver,
    ) -> Result<Option<StagedUsageDiscovery>, String> {
        self.require_open()?;
        let Some(scope) = self.discovery_scope.clone() else {
            return Ok(None);
        };
        resolver.begin_manual_retry();
        let Ok(catalog) = discover_usage_sources(&scope, resolver) else {
            self.push_event(
                "discovery_failed",
                None,
                Some("current account discovery unavailable".to_owned()),
            );
            return Ok(None);
        };
        let discovered = validate_usage_sources(catalog, resolver);
        let changed = self.discovery.as_ref().is_none_or(|current| {
            jackin_usage_discovery::usage_catalog_entries(current)
                != jackin_usage_discovery::usage_catalog_entries(&discovered)
        });
        Ok(Some(StagedUsageDiscovery {
            base_generation: self.discovery_generation,
            changed,
            discovery: discovered,
        }))
    }

    /// Commit a successful discovery stage after broker activation. The local
    /// generation fence rejects an older scan even when its catalog revision
    /// string happens to match the newer scan.
    pub fn commit_staged_discovery(
        &mut self,
        staged: StagedUsageDiscovery,
    ) -> Result<bool, String> {
        self.require_open()?;
        if staged.base_generation != self.discovery_generation {
            return Err("stale usage discovery stage".to_owned());
        }
        if !staged.changed {
            self.push_event("discovery_reconciled", None, Some("unchanged".to_owned()));
            return Ok(false);
        }
        let current = discovered_account_keys(Some(&staged.discovery));
        self.discovery = Some(staged.discovery);
        self.discovery_generation = self.discovery_generation.saturating_add(1);
        self.discovered_views.retain(|key, _| current.contains(key));
        let active = self
            .broker_phases
            .iter()
            .filter(|(_, phase)| phase.is_active())
            .map(|(capability, _)| capability.clone())
            .collect::<Vec<_>>();
        self.broker_phases.clear();
        self.broker_generations.clear();
        for capability in active {
            self.push_event(
                "broker_phase_changed",
                Some(&capability.surface_id),
                Some("failed".to_owned()),
            );
        }
        self.push_event("discovery_reconciled", None, Some("changed".to_owned()));
        Ok(true)
    }

    /// Rescan the retained Rust discovery scope without dispatching provider probes.
    ///
    /// This is the manual-refresh reconciliation boundary used before broker
    /// capabilities are rebuilt. The prior validated generation remains usable
    /// when the read-only config scan is unavailable.
    pub fn reconcile_discovery(
        &mut self,
        resolver: &dyn ProviderCredentialEnvResolver,
    ) -> Result<bool, String> {
        let Some(staged) = self.stage_discovery(resolver)? else {
            return Ok(false);
        };
        self.commit_staged_discovery(staged)
    }

    pub(crate) fn record_discovered_snapshot(
        &mut self,
        binding: &ValidatedCredentialBinding,
        mut view: FocusedUsageView,
    ) {
        let identity = binding.identity.clone().or_else(|| {
            CanonicalAccountIdentity::from_view(binding.surface, &view).map(|_| {
                CanonicalAccountIdentity::source_capability(binding.surface, &binding.capability_id)
            })
        });
        let Some(identity) = identity else {
            let error = view.last_error.clone();
            let kind = if error.is_some() {
                "probe_failed"
            } else {
                "snapshot_updated"
            };
            self.discovered_provider_views.insert(binding.surface, view);
            self.push_event(kind, Some(binding.surface.id()), error);
            return;
        };
        if view.account.account_label.trim().is_empty()
            && let Some(account) = self.discovery.as_ref().and_then(|discovery| {
                discovery
                    .accounts
                    .iter()
                    .find(|account| account.identity == identity)
            })
        {
            view.account.account_label = account.account_label.clone();
        }
        let account_key = identity.account_key();
        self.discovered_views
            .insert((binding.surface, account_key.clone()), view);
        self.discovered_provider_views.remove(&binding.surface);
        self.ensure_discovered_account(identity, account_key, binding);
        self.push_event("snapshot_updated", Some(binding.surface.id()), None);
    }

    pub(crate) fn ensure_discovered_account(
        &mut self,
        identity: CanonicalAccountIdentity,
        account_key: String,
        binding: &ValidatedCredentialBinding,
    ) {
        let Some(discovery) = &mut self.discovery else {
            return;
        };
        if let Some(account) = discovery
            .accounts
            .iter_mut()
            .find(|account| account.identity == identity)
        {
            account
                .provenance
                .extend(binding.provenance.iter().cloned());
            account.provenance.sort();
            account.provenance.dedup();
            if !account.source_ids.contains(&binding.source_id) {
                account.source_ids.push(binding.source_id.clone());
                account.source_ids.sort();
            }
            return;
        }
        let Some(view) = self
            .discovered_views
            .get(&(binding.surface, account_key.clone()))
        else {
            return;
        };
        discovery.accounts.push(DiscoveredAccountDescriptor {
            surface_id: binding.surface.id().to_owned(),
            account_key,
            account_label: view.account.account_label.clone(),
            provenance: binding.provenance.iter().cloned().collect(),
            source_ids: vec![binding.source_id.clone()],
            identity,
        });
    }
}
