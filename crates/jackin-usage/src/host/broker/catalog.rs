// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker-owned refresh-time host discovery.

use std::sync::{Arc, Mutex};

use jackin_protocol::usage_broker::{UsageCoordinationError, UsageCoordinationErrorKind};

use super::super::{
    ProviderCredentialEnvResolver, UsageDiscoveryScope, ValidatedUsageDiscovery,
    discover_usage_sources, validate_usage_sources,
};

/// Resolves the current host catalog inside the broker for an explicit refresh.
/// Consumers send only a refresh request. Discovery and credential resolution
/// remain inside the broker process.
pub(super) struct BrokerCatalogRefresh {
    scope: UsageDiscoveryScope,
    resolver: Arc<dyn ProviderCredentialEnvResolver>,
    serial: Mutex<()>,
    #[cfg(test)]
    test_discovery: Option<ValidatedUsageDiscovery>,
}

impl BrokerCatalogRefresh {
    pub(super) fn new(
        scope: UsageDiscoveryScope,
        resolver: Arc<dyn ProviderCredentialEnvResolver>,
    ) -> Self {
        Self {
            scope,
            resolver,
            serial: Mutex::new(()),
            #[cfg(test)]
            test_discovery: None,
        }
    }

    #[cfg(test)]
    pub(super) fn with_test_discovery(mut self, discovery: ValidatedUsageDiscovery) -> Self {
        self.test_discovery = Some(discovery);
        self
    }

    /// Serialize catalog scans with their broker-owned catalog mutation.
    pub(super) fn with_discovery<T>(
        &self,
        action: impl FnOnce(ValidatedUsageDiscovery) -> Result<T, UsageCoordinationError>,
    ) -> Result<T, UsageCoordinationError> {
        let _serial = self.serial.lock().map_err(|_| discovery_unavailable())?;
        #[cfg(test)]
        if let Some(discovery) = &self.test_discovery {
            return action(discovery.clone());
        }
        let discovery = with_unattended_guard(crate::usage::unattended_keychain_guard, || {
            self.resolver.begin_manual_retry();
            discover_confirming_empty(|| self.discover_once())
        })?;
        action(discovery)
    }

    fn discover_once(&self) -> Result<ValidatedUsageDiscovery, UsageCoordinationError> {
        let catalog = discover_usage_sources(&self.scope, self.resolver.as_ref())
            .map_err(|_| discovery_unavailable())?;
        Ok(validate_usage_sources(catalog, self.resolver.as_ref()))
    }
}

/// Confirm an empty catalog once before it can revoke known capabilities.
pub(super) fn discover_confirming_empty(
    mut discover_once: impl FnMut() -> Result<ValidatedUsageDiscovery, UsageCoordinationError>,
) -> Result<ValidatedUsageDiscovery, UsageCoordinationError> {
    let first = discover_once()?;
    if !super::usage_catalog_entries(&first).is_empty() {
        return Ok(first);
    }
    discover_once()
}

fn discovery_unavailable() -> UsageCoordinationError {
    UsageCoordinationError {
        kind: UsageCoordinationErrorKind::Unavailable,
        message: "usage discovery unavailable".to_owned(),
    }
}

fn with_unattended_guard<G, T>(
    establish_guard: impl FnOnce() -> Result<G, crate::usage::ClaudeKeychainPolicyError>,
    action: impl FnOnce() -> Result<T, UsageCoordinationError>,
) -> Result<T, UsageCoordinationError> {
    let _guard = establish_guard().map_err(|_| discovery_unavailable())?;
    action()
}

#[cfg(test)]
mod tests;
