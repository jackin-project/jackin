// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker service entry points.

use std::collections::BTreeMap;
use std::fs::{self};

use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _, PermissionsExt as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageCredentialScope, UsageIdentityKindV1,
};
use jackin_usage_coordinator::{
    FileAccountStateStore, FileProjectionStateStore, ProviderProbeOutcome, UsageCoordinator,
    UsageProviderExecutor,
};

use crate::{
    BROKER_LEADER, BrokerCatalogRefresh, BrokerStartupCleanup, DiscoveryProviderExecutor,
    LoadedProjection, MonitorStore, ServeConfig, ServePolicy, UsageBrokerClient, UsageBrokerConfig,
    catalog_discovery_mismatch, claim_leader, connect_probe, grouped_bindings, load_projection,
    publication_identity_metadata, publish, secure_run_directory, serve, unavailable,
    usage_catalog_entries, validate_broker_data_ancestors, validate_broker_data_tree,
    validate_owned_mode, wait_for_leader,
};
use jackin_usage_discovery::{UsageDiscoveryScope, ValidatedUsageDiscovery};
use jackin_usage_host_accounts::CanonicalAccountIdentity;
use jackin_usage_host_credentials::{
    ProviderCredentialEnvResolution, ProviderCredentialEnvResolver,
};
use jackin_usage_host_presentation::HostSurfaceId;
use jackin_usage_provider_claude::{
    ClaudeCredentialBootstrapOutcome, bootstrap_claude_credential, unattended_keychain_guard,
};

const CLAUDE_KEYCHAIN_SOURCE_SCOPE: &str = "claude_keychain_service";

struct EmptyProviderCredentialResolver;

impl ProviderCredentialEnvResolver for EmptyProviderCredentialResolver {
    fn resolve_provider_credentials(
        &self,
        _config: &jackin_config::AppConfig,
        _workspace: Option<&jackin_core::WorkspaceName>,
        _role: Option<&str>,
        _keys: &[jackin_core::UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution> {
        Vec::new()
    }
}

/// Secret-free metadata emitted after the foreground broker has installed its
/// exact-source catalog and is ready to serve monitor requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageBrokerForegroundReady {
    pub capability: UsageAccountCapability,
    pub binding_scope: String,
}

/// Map the exact opaque source ID to the single broker capability used by the
/// foreground collector. Keep these identifier domains distinct.
pub(crate) fn claude_usage_capability_for_source_id(
    source_capability_id: &str,
) -> UsageAccountCapability {
    let identity =
        CanonicalAccountIdentity::source_capability(HostSurfaceId::Claude, source_capability_id);
    let subject = identity.account_key();
    let hashed = jackin_core::account_key_hash("claude", &subject);
    UsageAccountCapability {
        surface_id: HostSurfaceId::Claude.id().to_owned(),
        account_id: hashed.strip_prefix("sha256:").unwrap_or(&hashed).to_owned(),
    }
}

/// Bootstrap the exact selected Claude Keychain item, then hold its lease and
/// the broker lease for the foreground collector service lifetime.
pub fn run_usage_broker_foreground_bootstrap(
    config: UsageBrokerConfig,
    scope: UsageDiscoveryScope,
    service: &str,
    on_ready: impl FnOnce(UsageBrokerForegroundReady),
) -> Result<ClaudeCredentialBootstrapOutcome, UsageCoordinationError> {
    if !matches!(&scope, UsageDiscoveryScope::HostDesktop { .. }) {
        return Err(unavailable());
    }
    let cleanup = claim_foreground_startup_cleanup(&config)?;
    let outcome = bootstrap_claude_credential(service).map_err(|_| unavailable())?;
    let ClaudeCredentialBootstrapOutcome::Acquired(lease) = outcome else {
        return Ok(outcome);
    };
    let _credential_lease = &lease;
    let _unattended_keychain_guard = unattended_keychain_guard().map_err(|_| unavailable())?;
    let source_capability_id = lease.source_capability_id().to_owned();
    let capability = claude_usage_capability_for_source_id(&source_capability_id);
    let monitor_store = Arc::new(MonitorStore::open(&config.data_dir).map_err(|_| unavailable())?);
    monitor_store.set_experimental_collector_source(Some(source_capability_id));
    let catalog_entry = UsageCatalogEntry {
        revision: jackin_core::account_key_hash("usage-catalog-entry-v3", &capability.account_id),
        capability: capability.clone(),
    };
    let catalog = vec![catalog_entry];
    let catalog_revision = foreground_catalog_revision(lease.source_capability_id(), &catalog);
    let identity_metadata = BTreeMap::from([(
        capability.clone(),
        publish::AccountIdentityMetadata {
            identity_kind: UsageIdentityKindV1::ProviderStableHandle,
            provenance_count: 1,
        },
    )]);
    let resolver: Arc<dyn ProviderCredentialEnvResolver> =
        Arc::new(EmptyProviderCredentialResolver);
    let executor = Arc::new(DiscoveryProviderExecutor {
        bindings: Mutex::new(BTreeMap::new()),
        validated_catalog: Mutex::new(None),
        scope,
        resolver,
        probe_budget: config.coordinator.provider_timeout,
        collector_lease: Some(lease.clone()),
        monitor_store: Some(Arc::clone(&monitor_store)),
    });
    let ready = UsageBrokerForegroundReady {
        capability,
        binding_scope: CLAUDE_KEYCHAIN_SOURCE_SCOPE.to_owned(),
    };
    run_usage_broker_service_with_cleanup(
        config,
        executor,
        identity_metadata,
        Some((
            catalog_revision,
            catalog,
            publish::CatalogDiagnostics::default(),
        )),
        None,
        Some(monitor_store),
        cleanup,
        || on_ready(ready),
    )?;
    Ok(ClaudeCredentialBootstrapOutcome::Acquired(lease))
}

fn foreground_catalog_revision(
    source_capability_id: &str,
    entries: &[UsageCatalogEntry],
) -> String {
    let mut material = String::new();
    push_catalog_revision_component(&mut material, source_capability_id);
    material.push_str(&entries.len().to_string());
    material.push(':');
    let mut ordered_entries = entries.iter().collect::<Vec<_>>();
    ordered_entries.sort_by(|left, right| left.capability.cmp(&right.capability));
    for entry in ordered_entries {
        push_catalog_revision_component(&mut material, &entry.capability.surface_id);
        push_catalog_revision_component(&mut material, &entry.capability.account_id);
        push_catalog_revision_component(&mut material, &entry.revision);
    }
    jackin_core::account_key_hash("usage-foreground-claude-catalog-v2", &material)
}

fn push_catalog_revision_component(material: &mut String, value: &str) {
    material.push_str(&value.len().to_string());
    material.push(':');
    material.push_str(value);
}

pub(crate) fn validate_foreground_catalog_revision(
    source_capability_id: &str,
    catalog_revision: &str,
    entries: &[UsageCatalogEntry],
) -> Result<(), UsageCoordinationError> {
    if entries.len() != 1
        || entries[0].capability != claude_usage_capability_for_source_id(source_capability_id)
        || entries[0].revision.is_empty()
        || foreground_catalog_revision(source_capability_id, entries) != catalog_revision
    {
        return Err(catalog_discovery_mismatch());
    }
    Ok(())
}

/// Run the process-owned service until its idle lease expires.
pub fn run_usage_broker_service(
    config: UsageBrokerConfig,
    scope: UsageDiscoveryScope,
    discovery: ValidatedUsageDiscovery,
    resolver: Arc<dyn ProviderCredentialEnvResolver>,
) -> Result<(), UsageCoordinationError> {
    let _unattended_keychain_guard = unattended_keychain_guard().map_err(|_| unavailable())?;
    let identity_metadata = publication_identity_metadata(&discovery);
    let catalog_revision = discovery
        .config_generation
        .clone()
        .unwrap_or_else(|| "empty".to_owned());
    let catalog = usage_catalog_entries(&discovery);
    let diagnostics = crate::catalog_diagnostics::from_discovery(&discovery);
    let bindings = grouped_bindings(&discovery);
    let catalog_refresh = Arc::new(BrokerCatalogRefresh::new(
        scope.clone(),
        Arc::clone(&resolver),
    ));
    let executor = Arc::new(DiscoveryProviderExecutor {
        bindings: Mutex::new(bindings),
        validated_catalog: Mutex::new(None),
        scope,
        resolver: Arc::clone(&resolver),
        probe_budget: config.coordinator.provider_timeout,
        collector_lease: None,
        monitor_store: None,
    });
    run_usage_broker_service_with_executor_and_metadata(
        config,
        executor,
        identity_metadata,
        Some((catalog_revision, catalog, diagnostics)),
        Some(catalog_refresh),
    )
}

/// Process service seam used by the shipped broker binary and process tests.
pub fn run_usage_broker_service_with_executor(
    config: UsageBrokerConfig,
    executor: Arc<dyn UsageProviderExecutor>,
) -> Result<(), UsageCoordinationError> {
    run_usage_broker_service_with_executor_and_metadata(
        config,
        executor,
        BTreeMap::new(),
        None,
        None,
    )
}

/// Run the host broker without discovering accounts or resolving credentials.
///
/// This is the service entry point used by explicit monitor startup. Existing
/// usage reads can still use persisted projections, while provider refresh
/// requests fail closed without touching provider or credential APIs.
pub fn run_usage_monitor_service(config: UsageBrokerConfig) -> Result<(), UsageCoordinationError> {
    run_usage_broker_service_with_executor_and_metadata(
        config,
        Arc::new(LocalOnlyProviderExecutor),
        BTreeMap::new(),
        None,
        None,
    )
}

struct LocalOnlyProviderExecutor;

impl UsageProviderExecutor for LocalOnlyProviderExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        ProviderProbeOutcome::Failure {
            kind: UsageCoordinationErrorKind::ProviderUnavailable,
            message: "provider refresh is disabled in the local-only usage service".to_owned(),
            retry_at_epoch: None,
        }
    }

    fn probe_scoped(
        &self,
        capability: &UsageAccountCapability,
        generation: u64,
        _scope: &UsageCredentialScope,
    ) -> ProviderProbeOutcome {
        self.probe(capability, generation)
    }

    fn reconcile_catalog(
        &self,
        _entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        Ok(())
    }
}

fn broker_conflict() -> UsageCoordinationError {
    UsageCoordinationError {
        kind: UsageCoordinationErrorKind::BrokerConflict,
        message: "usage broker startup conflicts with an existing lease or socket".to_owned(),
    }
}

fn claim_startup_cleanup(
    config: &UsageBrokerConfig,
) -> Result<Option<BrokerStartupCleanup>, UsageCoordinationError> {
    validate_broker_data_ancestors(&config.data_dir)?;
    let run_dir = secure_run_directory(&config.data_dir)?;
    validate_broker_data_tree(&config.data_dir)?;
    let socket_path = config.prepare_socket_path()?;
    let leader_path = run_dir.join(BROKER_LEADER);
    let Some(lease) = claim_leader(&leader_path, &config.build_id, config.lease_duration)? else {
        return Ok(None);
    };
    let cleanup = BrokerStartupCleanup::new(leader_path, socket_path, lease);
    prepare_socket_for_startup(config, &cleanup)?;
    Ok(Some(cleanup))
}

fn claim_foreground_startup_cleanup(
    config: &UsageBrokerConfig,
) -> Result<BrokerStartupCleanup, UsageCoordinationError> {
    claim_startup_cleanup(config)?.ok_or_else(broker_conflict)
}

/// Reclaim only a dead socket paired with a lease that this process actually
/// replaced. A live endpoint, foreign path, or unproven stale socket is left
/// untouched.
pub(crate) fn prepare_socket_for_startup(
    config: &UsageBrokerConfig,
    cleanup: &BrokerStartupCleanup,
) -> Result<(), UsageCoordinationError> {
    let socket_path = config.prepare_socket_path()?;
    let metadata = match fs::symlink_metadata(&socket_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(broker_conflict()),
    };
    if !metadata.file_type().is_socket()
        || metadata.uid() != nix::unistd::geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o600
        || !cleanup.reclaimed_stale_lease()
    {
        return Err(broker_conflict());
    }
    let identity = (metadata.dev(), metadata.ino());
    match UnixStream::connect(&socket_path) {
        Ok(_stream) => return Err(broker_conflict()),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
            ) => {}
        Err(_) => return Err(broker_conflict()),
    }
    let current = match fs::symlink_metadata(&socket_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(broker_conflict()),
    };
    if !current.file_type().is_socket()
        || current.uid() != nix::unistd::geteuid().as_raw()
        || current.mode() & 0o777 != 0o600
        || (current.dev(), current.ino()) != identity
    {
        return Err(broker_conflict());
    }
    crate::leader::unlink_owned_path(&socket_path).map_err(|_| broker_conflict())
}

pub(crate) fn run_usage_broker_service_with_executor_and_metadata(
    config: UsageBrokerConfig,
    executor: Arc<dyn UsageProviderExecutor>,
    identity_metadata: BTreeMap<UsageAccountCapability, publish::AccountIdentityMetadata>,
    initial_catalog: Option<(String, Vec<UsageCatalogEntry>, publish::CatalogDiagnostics)>,
    catalog_refresh: Option<Arc<BrokerCatalogRefresh>>,
) -> Result<(), UsageCoordinationError> {
    let Some(cleanup) = claim_startup_cleanup(&config)? else {
        return Ok(());
    };
    run_usage_broker_service_with_cleanup(
        config,
        executor,
        identity_metadata,
        initial_catalog,
        catalog_refresh,
        None,
        cleanup,
        || {},
    )
}

fn run_usage_broker_service_with_cleanup(
    config: UsageBrokerConfig,
    executor: Arc<dyn UsageProviderExecutor>,
    identity_metadata: BTreeMap<UsageAccountCapability, publish::AccountIdentityMetadata>,
    initial_catalog: Option<(String, Vec<UsageCatalogEntry>, publish::CatalogDiagnostics)>,
    catalog_refresh: Option<Arc<BrokerCatalogRefresh>>,
    initial_monitor_store: Option<Arc<MonitorStore>>,
    cleanup: BrokerStartupCleanup,
    on_ready: impl FnOnce(),
) -> Result<(), UsageCoordinationError> {
    let socket_path = config.prepare_socket_path()?;
    let mut cleanup = cleanup;
    prepare_socket_for_startup(&config, &cleanup)?;
    let listener = UnixListener::bind(&socket_path).map_err(|_| broker_conflict())?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
        .map_err(|_| unavailable())?;
    validate_owned_mode(&socket_path, 0o600)?;
    cleanup
        .record_socket_identity()
        .map_err(|_| unavailable())?;
    let store = Arc::new(FileAccountStateStore::under_data_dir(&config.data_dir));
    let LoadedProjection {
        projection,
        catalog: persisted_catalog,
        catalog_revision,
    } = load_projection(&config)?;
    let previous_catalog = persisted_catalog.clone().unwrap_or_default();
    let coordinator = match (persisted_catalog.is_some(), catalog_revision) {
        (true, Some(catalog_revision)) => Arc::new(UsageCoordinator::with_catalog_revision(
            executor,
            store,
            config.coordinator,
            previous_catalog.clone(),
            catalog_revision,
        )),
        _ => Arc::new(UsageCoordinator::with_catalog(
            executor,
            store,
            config.coordinator,
            previous_catalog.clone(),
        )),
    };
    let publisher = publish::ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::clone(&projection),
        FileProjectionStateStore::under_data_dir(&config.data_dir),
    )
    .with_identity_metadata(identity_metadata);
    let publisher = publisher.with_catalog(previous_catalog);
    if let Some((catalog_revision, catalog, diagnostics)) = initial_catalog {
        publisher.reconcile_catalog_if_projection_with_diagnostics(
            None,
            catalog_revision,
            catalog,
            diagnostics,
            chrono::Utc::now().timestamp(),
        )?;
    }
    let monitor_store = match initial_monitor_store {
        Some(monitor_store) => monitor_store,
        None => Arc::new(MonitorStore::open(&config.data_dir).map_err(|_| unavailable())?),
    };
    let initial_projection = publisher.current_projection().map_err(|_| unavailable())?;
    let monitor_now = chrono::Utc::now().timestamp();
    monitor_store
        .observe_projection(&initial_projection, monitor_now)
        .map_err(|_| unavailable())?;
    monitor_store.tick(monitor_now).map_err(|_| unavailable())?;
    on_ready();
    serve(ServeConfig {
        listener,
        coordinator,
        build_id: config.build_id.clone(),
        cleanup,
        policy: ServePolicy {
            idle_exit: config.idle_exit,
            lease_duration: config.lease_duration,
            lease_renewal: config.lease_renewal,
        },
        publisher,
        monitor_store,
        catalog_refresh,
    });
    Ok(())
}

/// Test/runtime seam that preserves the same process election and transport.
#[doc(hidden)]
pub fn ensure_usage_broker_with_executor(
    config: UsageBrokerConfig,
    executor: Arc<dyn UsageProviderExecutor>,
) -> Result<UsageBrokerClient, UsageCoordinationError> {
    let socket_path = config.prepare_socket_path()?;
    let client = UsageBrokerClient::at(socket_path.clone(), config.build_id.clone());
    if connect_probe(&client) {
        return Ok(client);
    }

    let Some(mut cleanup) = claim_startup_cleanup(&config)? else {
        wait_for_leader(&client)?;
        return Ok(client);
    };

    prepare_socket_for_startup(&config, &cleanup)?;
    let listener = UnixListener::bind(&socket_path).map_err(|_| unavailable())?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
        .map_err(|_| unavailable())?;
    validate_owned_mode(&socket_path, 0o600)?;
    cleanup
        .record_socket_identity()
        .map_err(|_| unavailable())?;
    let store = Arc::new(FileAccountStateStore::under_data_dir(&config.data_dir));
    let LoadedProjection {
        projection,
        catalog: persisted_catalog,
        catalog_revision,
    } = load_projection(&config)?;
    let coordinator = match (persisted_catalog.clone(), catalog_revision) {
        (Some(catalog), Some(catalog_revision)) => {
            Arc::new(UsageCoordinator::with_catalog_revision(
                executor,
                store,
                config.coordinator,
                catalog,
                catalog_revision,
            ))
        }
        (Some(catalog), None) => Arc::new(UsageCoordinator::with_catalog(
            executor,
            store,
            config.coordinator,
            catalog,
        )),
        (None, _) => Arc::new(UsageCoordinator::new(executor, store, config.coordinator)),
    };
    let build_id = config.build_id.clone();
    let idle_exit = config.idle_exit;
    let lease_duration = config.lease_duration;
    let lease_renewal = config.lease_renewal;
    let publisher = publish::ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::clone(&projection),
        FileProjectionStateStore::under_data_dir(&config.data_dir),
    );
    let publisher = match persisted_catalog {
        Some(catalog) => publisher.with_catalog(catalog),
        None => publisher,
    };
    let monitor_store = Arc::new(MonitorStore::open(&config.data_dir).map_err(|_| unavailable())?);
    let initial_projection = publisher.current_projection().map_err(|_| unavailable())?;
    let monitor_now = chrono::Utc::now().timestamp();
    monitor_store
        .observe_projection(&initial_projection, monitor_now)
        .map_err(|_| unavailable())?;
    monitor_store.tick(monitor_now).map_err(|_| unavailable())?;
    jackin_telemetry::spawn::thread_joined_named("usage-broker".to_owned(), move || {
        serve(ServeConfig {
            listener,
            coordinator,
            build_id,
            cleanup,
            policy: ServePolicy {
                idle_exit,
                lease_duration,
                lease_renewal,
            },
            publisher,
            monitor_store,
            catalog_refresh: None,
        });
    })
    .map_err(|_| unavailable())?;
    wait_for_leader(&client)?;
    Ok(client)
}
