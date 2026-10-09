// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker service entry points.

use std::collections::BTreeMap;
use std::fs::{self};

use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::net::UnixListener;

use std::sync::{Arc, Mutex};

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageCredentialScope,
};
use jackin_usage_coordinator::{
    FileAccountStateStore, FileProjectionStateStore, ProviderProbeOutcome, UsageCoordinator,
    UsageProviderExecutor,
};

use crate::{
    BROKER_LEADER, BrokerCatalogRefresh, BrokerStartupCleanup, DiscoveryProviderExecutor,
    LoadedProjection, MonitorStore, ServeConfig, ServePolicy, UsageBrokerClient, UsageBrokerConfig,
    claim_leader, connect_probe, grouped_bindings, load_projection, publication_identity_metadata,
    publish, secure_run_directory, serve, unavailable, usage_catalog_entries,
    validate_broker_data_ancestors, validate_broker_data_tree, validate_owned_mode,
    wait_for_leader,
};
use jackin_usage_discovery::{UsageDiscoveryScope, ValidatedUsageDiscovery};
use jackin_usage_host_credentials::ProviderCredentialEnvResolver;

/// Run the process-owned service until its idle lease expires.
pub fn run_usage_broker_service(
    config: UsageBrokerConfig,
    scope: UsageDiscoveryScope,
    discovery: ValidatedUsageDiscovery,
    resolver: Arc<dyn ProviderCredentialEnvResolver>,
) -> Result<(), UsageCoordinationError> {
    let _unattended_keychain_guard =
        jackin_usage_provider_claude::unattended_keychain_guard().map_err(|_| unavailable())?;
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

pub(crate) fn run_usage_broker_service_with_executor_and_metadata(
    config: UsageBrokerConfig,
    executor: Arc<dyn UsageProviderExecutor>,
    identity_metadata: BTreeMap<UsageAccountCapability, publish::AccountIdentityMetadata>,
    initial_catalog: Option<(String, Vec<UsageCatalogEntry>, publish::CatalogDiagnostics)>,
    catalog_refresh: Option<Arc<BrokerCatalogRefresh>>,
) -> Result<(), UsageCoordinationError> {
    validate_broker_data_ancestors(&config.data_dir)?;
    let run_dir = secure_run_directory(&config.data_dir)?;
    validate_broker_data_tree(&config.data_dir)?;
    let socket_path = config.prepare_socket_path()?;
    let leader_path = run_dir.join(BROKER_LEADER);
    let Some(lease) = claim_leader(&leader_path, &config.build_id, config.lease_duration)? else {
        return Ok(());
    };
    let cleanup = BrokerStartupCleanup::new(leader_path.clone(), socket_path.clone(), lease);
    if socket_path.exists() {
        fs::remove_file(&socket_path).map_err(|_| unavailable())?;
    }
    let listener = UnixListener::bind(&socket_path).map_err(|_| unavailable())?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
        .map_err(|_| unavailable())?;
    validate_owned_mode(&socket_path, 0o600)?;
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
    let monitor_store = Arc::new(MonitorStore::open(&config.data_dir).map_err(|_| unavailable())?);
    let initial_projection = publisher.current_projection().map_err(|_| unavailable())?;
    let monitor_now = chrono::Utc::now().timestamp();
    monitor_store
        .observe_projection(&initial_projection, monitor_now)
        .map_err(|_| unavailable())?;
    monitor_store.tick(monitor_now).map_err(|_| unavailable())?;
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

    validate_broker_data_ancestors(&config.data_dir)?;
    let run_dir = secure_run_directory(&config.data_dir)?;
    validate_broker_data_tree(&config.data_dir)?;
    let leader_path = run_dir.join(BROKER_LEADER);
    let Some(lease) = claim_leader(&leader_path, &config.build_id, config.lease_duration)? else {
        wait_for_leader(&client)?;
        return Ok(client);
    };

    let cleanup = BrokerStartupCleanup::new(leader_path, socket_path.clone(), lease);
    if socket_path.exists() {
        fs::remove_file(&socket_path).map_err(|_| unavailable())?;
    }
    let listener = UnixListener::bind(&socket_path).map_err(|_| unavailable())?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
        .map_err(|_| unavailable())?;
    validate_owned_mode(&socket_path, 0o600)?;
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
