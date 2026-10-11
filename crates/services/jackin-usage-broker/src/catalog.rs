// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker-owned refresh-time discovery.

use std::sync::Arc;
use std::sync::Mutex;

use jackin_protocol::usage_broker::{UsageCoordinationError, UsageCoordinationErrorKind};
use jackin_usage_discovery::{
    UsageDiscoveryScope, ValidatedUsageDiscovery, discover_usage_sources, validate_usage_sources,
};
use jackin_usage_host_credentials::ProviderCredentialEnvResolver;

/// Resolves the current host catalog inside the broker for an explicit refresh.
///
/// Consumers send only a refresh request. This object keeps config discovery,
/// credential resolution, and identity validation in the broker process.
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

    /// Serialize a catalog scan with its broker-owned catalog mutation. This
    /// keeps a slow launch scan from publishing over a newer explicit scan.
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

/// Confirm an empty catalog once before it can revoke previously known
/// capabilities. Every retry invokes a fresh broker-owned scan.
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

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, VecDeque};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn empty_discovery(revision: &str) -> ValidatedUsageDiscovery {
        ValidatedUsageDiscovery {
            config_generation: Some(revision.to_owned()),
            accounts: Vec::new(),
            diagnostics: Vec::new(),
            candidates: Vec::new(),
            bindings: Vec::new(),
        }
    }

    fn populated_discovery(revision: &str) -> ValidatedUsageDiscovery {
        use jackin_usage_discovery::{ValidatedCredentialBinding, ValidatedCredentialSource};
        use jackin_usage_host_accounts::{CanonicalAccountIdentity, CanonicalAccountSubject};
        use jackin_usage_host_presentation::HostSurfaceId;

        ValidatedUsageDiscovery {
            config_generation: Some(revision.to_owned()),
            accounts: Vec::new(),
            diagnostics: Vec::new(),
            candidates: Vec::new(),
            bindings: vec![ValidatedCredentialBinding {
                surface: HostSurfaceId::Claude,
                identity: Some(CanonicalAccountIdentity {
                    surface: HostSurfaceId::Claude,
                    subject: CanonicalAccountSubject::ProviderStableHandle(
                        "fixture-account".to_owned(),
                    ),
                }),
                source_id: "fixture-source".to_owned(),
                capability_id: "fixture-capability".to_owned(),
                credential_revision: "fixture-credential".to_owned(),
                provenance: BTreeSet::default(),
                source: ValidatedCredentialSource::Capability,
            }],
        }
    }

    #[test]
    fn transient_empty_scan_uses_a_fresh_confirmation_scan() {
        let scans = AtomicUsize::new(0);
        let mut results = VecDeque::from([
            empty_discovery("transient-empty"),
            populated_discovery("confirmed-populated"),
        ]);

        let discovery = discover_confirming_empty(
            || -> Result<ValidatedUsageDiscovery, UsageCoordinationError> {
                scans.fetch_add(1, Ordering::SeqCst);
                Ok(results.pop_front().expect("scripted scan"))
            },
        )
        .unwrap();

        assert_eq!(scans.load(Ordering::SeqCst), 2);
        assert_eq!(
            discovery.config_generation.as_deref(),
            Some("confirmed-populated")
        );
        assert_eq!(crate::usage_catalog_entries(&discovery).len(), 1);
    }

    #[test]
    fn guard_failure_prevents_catalog_scan() {
        let scans = AtomicUsize::new(0);
        let error = with_unattended_guard::<(), ()>(
            || Err(discovery_unavailable()),
            || {
                scans.fetch_add(1, Ordering::SeqCst);
                Err(discovery_unavailable())
            },
        )
        .unwrap_err();

        assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
        assert_eq!(scans.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn injected_guard_allows_catalog_scan_without_native_keychain_access() {
        struct FakeGuard;

        let scans = AtomicUsize::new(0);
        let _error = with_unattended_guard::<FakeGuard, ()>(
            || Ok(FakeGuard),
            || {
                scans.fetch_add(1, Ordering::SeqCst);
                Err(discovery_unavailable())
            },
        )
        .unwrap_err();

        assert_eq!(scans.load(Ordering::SeqCst), 1);
    }
}
