// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Discovery provider executor.

use std::collections::{BTreeMap, BTreeSet};

use std::sync::{Arc, Mutex};
use std::time::Duration;

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageCredentialScope,
};

use crate::coordinator::{ProviderProbeOutcome, UsageProviderExecutor};

use super::super::discovery::{ProviderCredentialEnvResolver, ValidatedCredentialBinding};
use super::super::{UsageDiscoveryScope, ValidatedUsageDiscovery};
use super::{
    authorize_credential_binding_group, catalog_discovery_mismatch, catalog_entry_map,
    credential_scope_mismatch, ensure_catalog_matches, grouped_bindings, probe,
    rediscover_all_bindings, rediscover_bindings, rediscover_discovery, refresh_binding_outcome,
    unavailable, unscoped_refresh_binding, usage_catalog_entries,
};

pub(crate) struct DiscoveryProviderExecutor {
    pub(crate) bindings: Mutex<BTreeMap<UsageAccountCapability, Vec<ValidatedCredentialBinding>>>,
    pub(crate) validated_catalog: Mutex<Option<StagedDiscoveryCatalog>>,
    pub(crate) scope: UsageDiscoveryScope,
    pub(crate) resolver: Arc<dyn ProviderCredentialEnvResolver>,
    pub(crate) probe_budget: Duration,
}

pub(crate) struct StagedDiscoveryCatalog {
    catalog_revision: String,
    entries: BTreeMap<UsageAccountCapability, String>,
    discovery: ValidatedUsageDiscovery,
}

pub(crate) fn probe_with_scope(
    executor: &DiscoveryProviderExecutor,
    capability: &UsageAccountCapability,
    launch_scope: Option<&UsageCredentialScope>,
) -> ProviderProbeOutcome {
    // The coordinator only classifies elapsed time after a probe returns, so
    // the blocking provider call (child CLI/RPC, secret resolution) runs under
    // an explicit broker-side budget. Expiry completes the generation through
    // the normal failure path: last-good quota is preserved and broker
    // ownership is unaffected.
    let cached = executor
        .bindings
        .lock()
        .ok()
        .and_then(|bindings| bindings.get(capability).cloned());
    let scope = executor.scope.clone();
    let resolver = Arc::clone(&executor.resolver);
    let task_capability = capability.clone();
    let launch_scope = launch_scope.cloned();
    let outcome = probe::run_probe_with_budget(executor.probe_budget, move || {
        let (bindings, refreshed) = match cached {
            Some(bindings) => (Some(bindings), None),
            None => rediscover_bindings(&scope, resolver.as_ref(), &task_capability),
        };
        let binding = bindings.as_deref().and_then(|bindings| {
            launch_scope.as_ref().map_or_else(
                || unscoped_refresh_binding(bindings),
                |scope| {
                    authorize_credential_binding_group(bindings, &task_capability.surface_id, scope)
                },
            )
        });
        let outcome = match binding {
            Some(binding) => refresh_binding_outcome(&binding, resolver.as_ref()),
            None => ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::Unauthorized,
                message: "usage account capability is not authorized".to_owned(),
                retry_at_epoch: None,
            },
        };
        (outcome, refreshed)
    });
    match outcome {
        Ok((outcome, refreshed)) => {
            if let Some(refreshed) = refreshed
                && let Ok(mut bindings) = executor.bindings.lock()
            {
                *bindings = refreshed;
            }
            outcome
        }
        Err(_) => probe::probe_timeout_outcome(),
    }
}

impl UsageProviderExecutor for DiscoveryProviderExecutor {
    fn authorize_credential_scope(
        &self,
        capability: &UsageAccountCapability,
        scope: &UsageCredentialScope,
    ) -> Result<(), UsageCoordinationError> {
        let bindings = self
            .bindings
            .lock()
            .map_err(|_| unavailable())?
            .get(capability)
            .cloned()
            .ok_or_else(credential_scope_mismatch)?;
        authorize_credential_binding_group(&bindings, &capability.surface_id, scope)
            .map(|_| ())
            .ok_or_else(credential_scope_mismatch)
    }

    fn probe(&self, capability: &UsageAccountCapability, _generation: u64) -> ProviderProbeOutcome {
        probe_with_scope(self, capability, None)
    }

    fn probe_scoped(
        &self,
        capability: &UsageAccountCapability,
        _generation: u64,
        scope: &UsageCredentialScope,
    ) -> ProviderProbeOutcome {
        probe_with_scope(self, capability, Some(scope))
    }

    fn reconcile_catalog(
        &self,
        entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        let admitted = entries
            .iter()
            .map(|entry| entry.capability.clone())
            .collect::<BTreeSet<_>>();
        // The caller's catalog is authoritative. A transient discovery failure
        // must clear old bindings rather than leave a revoked credential
        // usable; a later probe can rediscover one admitted capability.
        let mut bindings =
            rediscover_all_bindings(&self.scope, self.resolver.as_ref()).unwrap_or_default();
        bindings.retain(|capability, _| admitted.contains(capability));
        self.bindings
            .lock()
            .map_err(|_| unavailable())?
            .clone_from(&bindings);
        Ok(())
    }

    fn validate_catalog(
        &self,
        entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        let Some(discovery) = rediscover_discovery(&self.scope, self.resolver.as_ref()) else {
            return Err(unavailable());
        };
        let expected = usage_catalog_entries(&discovery)
            .into_iter()
            .map(|entry| (entry.capability, entry.revision))
            .collect::<BTreeMap<_, _>>();
        let observed = entries
            .iter()
            .map(|entry| (entry.capability.clone(), entry.revision.clone()))
            .collect::<BTreeMap<_, _>>();
        if expected == observed {
            Ok(())
        } else {
            Err(catalog_discovery_mismatch())
        }
    }

    fn validate_catalog_revision(
        &self,
        catalog_revision: &str,
        entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        let Some(discovery) = rediscover_discovery(&self.scope, self.resolver.as_ref()) else {
            return Err(unavailable());
        };
        ensure_catalog_matches(&discovery, catalog_revision, entries)?;
        self.validated_catalog
            .lock()
            .map_err(|_| unavailable())?
            .replace(StagedDiscoveryCatalog {
                catalog_revision: catalog_revision.to_owned(),
                entries: catalog_entry_map(entries),
                discovery,
            });
        Ok(())
    }

    fn reconcile_catalog_revision(
        &self,
        catalog_revision: &str,
        entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        let requested_entries = catalog_entry_map(entries);
        let staged = self
            .validated_catalog
            .lock()
            .map_err(|_| unavailable())?
            .take()
            .filter(|staged| {
                staged.catalog_revision == catalog_revision && staged.entries == requested_entries
            });
        let discovery = if let Some(staged) = staged {
            staged.discovery
        } else {
            let Some(discovery) = rediscover_discovery(&self.scope, self.resolver.as_ref()) else {
                return Err(unavailable());
            };
            ensure_catalog_matches(&discovery, catalog_revision, entries)?;
            discovery
        };
        let admitted = entries
            .iter()
            .map(|entry| entry.capability.clone())
            .collect::<BTreeSet<_>>();
        // Preserve every binding in a canonical capability group. Profile
        // sources remain first for refresh selection, while authorization
        // checks every relevant env proof against the complete group.
        let mut bindings = grouped_bindings(&discovery);
        bindings.retain(|capability, _| admitted.contains(capability));
        self.bindings
            .lock()
            .map_err(|_| unavailable())?
            .clone_from(&bindings);
        Ok(())
    }
}
