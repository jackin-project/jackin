// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::sync::Mutex;

use jackin_protocol::usage_broker::{UsageCoordinationError, UsageCoordinationErrorKind};
use jackin_usage_discovery::{
    UsageDiscoveryScope, ValidatedUsageDiscovery, discover_usage_sources, validate_usage_sources,
};
use jackin_usage_host_credentials::ProviderCredentialEnvResolver;

pub(crate) struct BrokerCatalogRefresh {
    scope: UsageDiscoveryScope,
    resolver: Arc<dyn ProviderCredentialEnvResolver>,
    serial: Mutex<()>,
}

impl BrokerCatalogRefresh {
    pub(crate) fn new(
        scope: UsageDiscoveryScope,
        resolver: Arc<dyn ProviderCredentialEnvResolver>,
    ) -> Self {
        Self {
            scope,
            resolver,
            serial: Mutex::new(()),
        }
    }

    pub(crate) fn with_discovery<T>(
        &self,
        action: impl FnOnce(ValidatedUsageDiscovery) -> Result<T, UsageCoordinationError>,
    ) -> Result<T, UsageCoordinationError> {
        let _serial = self.serial.lock().map_err(|_| discovery_unavailable())?;
        let discovery = with_unattended_guard(
            || {
                jackin_usage_provider_claude::unattended_keychain_guard()
                    .map_err(|_| discovery_unavailable())
            },
            || {
                self.resolver.begin_manual_retry();
                discover_confirming_empty(|| self.discover_once())
            },
        )?;
        action(discovery)
    }

    fn discover_once(&self) -> Result<ValidatedUsageDiscovery, UsageCoordinationError> {
        let catalog = discover_usage_sources(&self.scope, self.resolver.as_ref())
            .map_err(|_| discovery_unavailable())?;
        Ok(validate_usage_sources(catalog, self.resolver.as_ref()))
    }
}

pub(crate) fn discover_confirming_empty(
    mut discover_once: impl FnMut() -> Result<ValidatedUsageDiscovery, UsageCoordinationError>,
) -> Result<ValidatedUsageDiscovery, UsageCoordinationError> {
    let first = discover_once()?;
    if !crate::usage_catalog_entries(&first).is_empty() {
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
    establish_guard: impl FnOnce() -> Result<G, UsageCoordinationError>,
    action: impl FnOnce() -> Result<T, UsageCoordinationError>,
) -> Result<T, UsageCoordinationError> {
    let _guard = establish_guard().map_err(|_| discovery_unavailable())?;
    action()
}
