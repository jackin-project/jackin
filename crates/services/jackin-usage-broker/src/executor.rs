// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Discovery provider executor.

use std::collections::{BTreeMap, BTreeSet};

use std::sync::{Arc, Mutex};
use std::time::Duration;

use jackin_protocol::control::UsageSnapshotStatus;
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageCredentialScope,
};

use jackin_usage_coordinator::{ProviderProbeOutcome, UsageProviderExecutor};

use crate::{
    authorize_credential_binding_group, catalog_discovery_mismatch, catalog_entry_map,
    credential_scope_mismatch, ensure_catalog_matches, grouped_bindings, probe,
    rediscover_all_bindings, rediscover_bindings, rediscover_discovery, refresh_binding_outcome,
    unavailable, unscoped_refresh_binding, usage_catalog_entries,
};
use jackin_usage_discovery::{
    UsageDiscoveryScope, ValidatedCredentialBinding, ValidatedUsageDiscovery,
};
use jackin_usage_host_credentials::ProviderCredentialEnvResolver;
use jackin_usage_host_presentation::HostSurfaceId;
use jackin_usage_provider_claude::{
    ClaudeCollectionError, ClaudeCredentialLease, experimental_claude_usage_snapshot_for_lease,
};
use jackin_usage_provider_core::{ProviderErrorKind, ProviderFailureMetadata};

const INDEPENDENT_CLAUDE_REFRESH_DISABLED: &str =
    "independent Claude OAuth refresh is disabled; use statusline monitor";
const COLLECTOR_AUTH_REQUIRED: &str =
    "Claude collection requires foreground authentication preparation";
const COLLECTOR_NOT_AUTHORIZED: &str = "Claude collection is not authorized for this source";

pub(crate) struct DiscoveryProviderExecutor {
    pub(crate) bindings: Mutex<BTreeMap<UsageAccountCapability, Vec<ValidatedCredentialBinding>>>,
    pub(crate) validated_catalog: Mutex<Option<StagedDiscoveryCatalog>>,
    pub(crate) scope: UsageDiscoveryScope,
    pub(crate) resolver: Arc<dyn ProviderCredentialEnvResolver>,
    pub(crate) probe_budget: Duration,
    pub(crate) collector_lease: Option<ClaudeCredentialLease>,
    pub(crate) monitor_store: Option<Arc<crate::MonitorStore>>,
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
    if executor.collector_lease.is_some() {
        if capability.surface_id != HostSurfaceId::Claude.id() {
            return collector_not_authorized();
        }
        return probe_foreground_claude(executor, capability);
    }
    probe_with_scope_using(executor, capability, launch_scope, refresh_binding_outcome)
}

fn probe_foreground_claude(
    executor: &DiscoveryProviderExecutor,
    capability: &UsageAccountCapability,
) -> ProviderProbeOutcome {
    let (Some(lease), Some(monitor_store)) = (
        executor.collector_lease.as_ref(),
        executor.monitor_store.as_ref(),
    ) else {
        return collector_auth_required();
    };
    if !foreground_source_is_authorized(lease, monitor_store, capability) {
        return collector_not_authorized();
    }

    let lease = lease.clone();
    let monitor_store = Arc::clone(monitor_store);
    let capability = capability.clone();
    match probe::run_probe_with_budget(executor.probe_budget, move || {
        let consent_is_current =
            || foreground_source_is_authorized(&lease, &monitor_store, &capability);
        match experimental_claude_usage_snapshot_for_lease(
            "claude",
            Some("Claude"),
            chrono::Utc::now().timestamp(),
            &lease,
            consent_is_current,
        ) {
            Ok(Some((view, rate_limit, failure_metadata))) => {
                crate::rediscover::provider_probe_outcome_with_metadata(
                    view,
                    rate_limit,
                    failure_metadata,
                )
            }
            Ok(None) => collector_auth_required(),
            Err(ClaudeCollectionError::ConsentRevoked {
                provider_http_status: None,
            }) => collector_not_authorized(),
            Err(ClaudeCollectionError::ConsentRevoked {
                provider_http_status: Some(http_status),
            }) => {
                let mut view = jackin_protocol::control::FocusedUsageView::unavailable(
                    "claude",
                    chrono::Utc::now().timestamp(),
                );
                view.status = if http_status == 401 {
                    UsageSnapshotStatus::NeedsSecret
                } else {
                    UsageSnapshotStatus::Error
                };
                view.last_error = Some(format!(
                    "Claude collection consent was revoked after HTTP {http_status}; the provider result was discarded"
                ));
                crate::rediscover::provider_probe_outcome_with_metadata(
                    view,
                    None,
                    Some(ProviderFailureMetadata {
                        kind: ProviderErrorKind::HttpStatus,
                        http_status: Some(http_status),
                    }),
                )
            }
        }
    }) {
        Ok(outcome) => outcome,
        Err(_) => probe::probe_timeout_outcome(),
    }
}

fn foreground_source_is_authorized(
    lease: &ClaudeCredentialLease,
    monitor_store: &crate::MonitorStore,
    capability: &UsageAccountCapability,
) -> bool {
    let source_capability_id = lease.source_capability_id();
    collection_source_authorizes_broker_capability(
        source_capability_id,
        &monitor_store.collection_accounts(),
        capability,
    )
}

fn collection_source_authorizes_broker_capability(
    source_capability_id: &str,
    authorized_source_ids: &[String],
    capability: &UsageAccountCapability,
) -> bool {
    capability
        == &crate::source_identity::claude_usage_capability_for_source_id(source_capability_id)
        && authorized_source_ids
            .iter()
            .any(|authorized| authorized == source_capability_id)
}

fn collector_auth_required() -> ProviderProbeOutcome {
    ProviderProbeOutcome::Failure {
        kind: UsageCoordinationErrorKind::NeedsSecret,
        message: COLLECTOR_AUTH_REQUIRED.to_owned(),
        retry_at_epoch: None,
    }
}

fn collector_not_authorized() -> ProviderProbeOutcome {
    ProviderProbeOutcome::Failure {
        kind: UsageCoordinationErrorKind::Unauthorized,
        message: COLLECTOR_NOT_AUTHORIZED.to_owned(),
        retry_at_epoch: None,
    }
}

fn probe_with_scope_using<F>(
    executor: &DiscoveryProviderExecutor,
    capability: &UsageAccountCapability,
    launch_scope: Option<&UsageCredentialScope>,
    refresh: F,
) -> ProviderProbeOutcome
where
    F: FnOnce(
            &ValidatedCredentialBinding,
            &dyn ProviderCredentialEnvResolver,
        ) -> ProviderProbeOutcome
        + Send
        + 'static,
{
    // Independent Claude usage HTTP is disabled for the normal broker too.
    // Gate before cache lookup and fallback rediscovery so selected, cached,
    // and newly discovered Claude sources all stay local-only.
    if capability.surface_id == HostSurfaceId::Claude.id() {
        return ProviderProbeOutcome::Failure {
            kind: UsageCoordinationErrorKind::ProviderUnavailable,
            message: INDEPENDENT_CLAUDE_REFRESH_DISABLED.to_owned(),
            retry_at_epoch: None,
        };
    }

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
            Some(binding) => refresh(&binding, resolver.as_ref()),
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
        if self.collector_lease.is_some() {
            self.bindings.lock().map_err(|_| unavailable())?.clear();
            return Ok(());
        }
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
        if self.collector_lease.is_some() {
            return Err(catalog_discovery_mismatch());
        }
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
        if let Some(lease) = self.collector_lease.as_ref() {
            return crate::service::validate_foreground_catalog_revision(
                lease.source_capability_id(),
                catalog_revision,
                entries,
            );
        }
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
        if self.collector_lease.is_some() {
            self.bindings.lock().map_err(|_| unavailable())?.clear();
            return Ok(());
        }
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

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use jackin_config::AppConfig;
    use jackin_core::{UsageCredentialEnvName, WorkspaceName};
    use jackin_protocol::usage_broker::{
        UsageAccountCapability, UsageCoordinationErrorKind, UsageCredentialSourceIdentity,
    };
    use jackin_usage_discovery::{
        ProfileCredentialMaterial, UsageDiscoveryScope, ValidatedCredentialBinding,
        ValidatedCredentialSource,
    };
    use jackin_usage_host_credentials::{
        OpaqueCredentialHandle, ProviderCredentialEnvResolution, ProviderCredentialEnvResolver,
        ProviderCredentialRefreshOutcome, ProviderCredentialSourceMaterial,
    };
    use jackin_usage_provider_claude::ClaudeResolved;

    use super::*;

    #[derive(Default)]
    struct CountingResolver {
        resolve_calls: AtomicUsize,
        refresh_calls: AtomicUsize,
    }

    impl ProviderCredentialEnvResolver for CountingResolver {
        fn resolve_provider_credentials(
            &self,
            _config: &AppConfig,
            _workspace: Option<&WorkspaceName>,
            _role: Option<&str>,
            _keys: &[UsageCredentialEnvName],
        ) -> Vec<ProviderCredentialEnvResolution> {
            self.resolve_calls.fetch_add(1, Ordering::SeqCst);
            Vec::new()
        }

        fn refresh_provider_credential(
            &self,
            _surface: HostSurfaceId,
            _key: &str,
            _handle: &OpaqueCredentialHandle,
        ) -> ProviderCredentialRefreshOutcome {
            self.refresh_calls.fetch_add(1, Ordering::SeqCst);
            ProviderCredentialRefreshOutcome::Malformed
        }
    }

    fn capability(surface: HostSurfaceId, account_id: &str) -> UsageAccountCapability {
        UsageAccountCapability {
            account_id: account_id.to_owned(),
            surface_id: surface.id().to_owned(),
        }
    }

    fn binding(
        surface: HostSurfaceId,
        capability_id: &str,
        source: ValidatedCredentialSource,
    ) -> ValidatedCredentialBinding {
        ValidatedCredentialBinding {
            surface,
            identity: None,
            source_id: format!("source-{capability_id}"),
            capability_id: capability_id.to_owned(),
            credential_revision: format!("revision-{capability_id}"),
            provenance: BTreeSet::new(),
            source,
        }
    }

    fn profile_claude_binding(capability_id: &str) -> ValidatedCredentialBinding {
        binding(
            HostSurfaceId::Claude,
            capability_id,
            ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Claude(
                ClaudeResolved::from_token(
                    "fixture-token".to_owned(),
                    None,
                    Some(format!("{capability_id}@example.test")),
                    None,
                    "OAuth · configured profile".to_owned(),
                    false,
                ),
            )),
        )
    }

    fn env_claude_binding(capability_id: &str) -> ValidatedCredentialBinding {
        binding(
            HostSurfaceId::Claude,
            capability_id,
            ValidatedCredentialSource::Env {
                handle: OpaqueCredentialHandle::new(format!("handle-{capability_id}")),
                key: "ANTHROPIC_AUTH_TOKEN".to_owned(),
                dispatch_key: "ANTHROPIC_AUTH_TOKEN".to_owned(),
                launch_keys: BTreeSet::from(["ANTHROPIC_AUTH_TOKEN".to_owned()]),
                material: Some(env_material("ANTHROPIC_AUTH_TOKEN")),
            },
        )
    }

    fn env_binding(
        surface: HostSurfaceId,
        capability_id: &str,
        key: &str,
    ) -> ValidatedCredentialBinding {
        ValidatedCredentialBinding {
            surface,
            identity: None,
            source_id: format!("source-{capability_id}"),
            capability_id: capability_id.to_owned(),
            credential_revision: format!("revision-{capability_id}"),
            provenance: BTreeSet::new(),
            source: ValidatedCredentialSource::Env {
                handle: OpaqueCredentialHandle::new(format!("handle-{capability_id}")),
                key: key.to_owned(),
                dispatch_key: key.to_owned(),
                launch_keys: BTreeSet::from([key.to_owned()]),
                material: Some(env_material(key)),
            },
        }
    }

    fn env_material(name: &str) -> ProviderCredentialSourceMaterial {
        ProviderCredentialSourceMaterial {
            source: UsageCredentialSourceIdentity::HostEnv {
                name: name.to_owned(),
            },
            material_fingerprint: format!("fixture-{name}"),
        }
    }

    fn executor(
        bindings: BTreeMap<UsageAccountCapability, Vec<ValidatedCredentialBinding>>,
        scope: UsageDiscoveryScope,
        resolver: Arc<CountingResolver>,
    ) -> DiscoveryProviderExecutor {
        DiscoveryProviderExecutor {
            bindings: Mutex::new(bindings),
            validated_catalog: Mutex::new(None),
            scope,
            resolver,
            probe_budget: Duration::from_secs(1),
            collector_lease: None,
            monitor_store: None,
        }
    }

    #[test]
    fn projection_probes_cannot_refresh_selected_or_unselected_claude_accounts() {
        let resolver = Arc::new(CountingResolver::default());
        let profile_capability = capability(HostSurfaceId::Claude, "selected-profile");
        let env_capability = capability(HostSurfaceId::Claude, "unselected-env");
        let uncached_capability = capability(HostSurfaceId::Claude, "uncached-profile");
        let fixture = tempfile::tempdir().expect("empty discovery fixture");
        let scope = UsageDiscoveryScope::HostDesktop {
            config_root: fixture.path().join("config"),
            operator_home: fixture.path().join("home"),
        };

        let claude_executor = executor(
            BTreeMap::from([
                (
                    profile_capability.clone(),
                    vec![profile_claude_binding("selected-profile")],
                ),
                (
                    env_capability.clone(),
                    vec![env_claude_binding("unselected-env")],
                ),
            ]),
            scope.clone(),
            Arc::clone(&resolver),
        );
        let provider_refresh_calls = Arc::new(AtomicUsize::new(0));
        for capability in [profile_capability, env_capability, uncached_capability] {
            let provider_refresh_calls = Arc::clone(&provider_refresh_calls);
            let outcome = probe_with_scope_using(
                &claude_executor,
                &capability,
                None,
                move |binding, resolver| {
                    provider_refresh_calls.fetch_add(1, Ordering::SeqCst);
                    if let ValidatedCredentialSource::Env {
                        handle,
                        dispatch_key,
                        ..
                    } = &binding.source
                    {
                        resolver.refresh_provider_credential(binding.surface, dispatch_key, handle);
                    }
                    ProviderProbeOutcome::Failure {
                        kind: UsageCoordinationErrorKind::ProviderUnavailable,
                        message: "fake provider refresh reached".to_owned(),
                        retry_at_epoch: None,
                    }
                },
            );
            match outcome {
                ProviderProbeOutcome::Failure {
                    kind,
                    message,
                    retry_at_epoch,
                } => {
                    assert_eq!(kind, UsageCoordinationErrorKind::ProviderUnavailable);
                    assert_eq!(message, INDEPENDENT_CLAUDE_REFRESH_DISABLED);
                    assert_eq!(retry_at_epoch, None);
                }
                ProviderProbeOutcome::Success(_) => panic!("Claude refresh unexpectedly succeeded"),
            }
        }

        assert_eq!(provider_refresh_calls.load(Ordering::SeqCst), 0);
        assert_eq!(resolver.resolve_calls.load(Ordering::SeqCst), 0);
        assert_eq!(resolver.refresh_calls.load(Ordering::SeqCst), 0);

        let codex_capability = capability(HostSurfaceId::Codex, "codex-account");
        let codex_executor = executor(
            BTreeMap::from([(
                codex_capability.clone(),
                vec![env_binding(
                    HostSurfaceId::Codex,
                    "codex-account",
                    "OPENAI_API_KEY",
                )],
            )]),
            scope,
            Arc::clone(&resolver),
        );
        let outcome = probe_with_scope_using(
            &codex_executor,
            &codex_capability,
            None,
            move |_, resolver| {
                resolver.refresh_provider_credential(
                    HostSurfaceId::Codex,
                    "OPENAI_API_KEY",
                    &OpaqueCredentialHandle::new("codex-handle"),
                );
                ProviderProbeOutcome::Failure {
                    kind: UsageCoordinationErrorKind::ProviderUnavailable,
                    message: "fake provider failure".to_owned(),
                    retry_at_epoch: None,
                }
            },
        );

        assert!(matches!(
            outcome,
            ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::ProviderUnavailable,
                message,
                retry_at_epoch: None,
            } if message == "fake provider failure"
        ));
        assert_eq!(resolver.refresh_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn collector_source_id_and_broker_capability_cannot_cross_authorize() {
        let source_id = "a".repeat(64);
        let capability = crate::source_identity::claude_usage_capability_for_source_id(&source_id);

        assert!(collection_source_authorizes_broker_capability(
            &source_id,
            std::slice::from_ref(&source_id),
            &capability,
        ));

        assert!(!collection_source_authorizes_broker_capability(
            &source_id,
            std::slice::from_ref(&capability.account_id),
            &capability,
        ));

        let confused_capability = UsageAccountCapability {
            surface_id: capability.surface_id.clone(),
            account_id: source_id.clone(),
        };
        assert!(!collection_source_authorizes_broker_capability(
            &source_id,
            std::slice::from_ref(&source_id),
            &confused_capability,
        ));
    }
}
