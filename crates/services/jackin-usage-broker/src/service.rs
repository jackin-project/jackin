// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker service entry points.

use std::collections::BTreeMap;
use std::fs::{self};

use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::net::UnixListener;

use std::sync::{Arc, Mutex};

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationError,
};

use jackin_usage_coordinator::{
    FileAccountStateStore, FileProjectionStateStore, UsageCoordinator, UsageProviderExecutor,
};

use crate::{
    BROKER_LEADER, BrokerStartupCleanup, DiscoveryProviderExecutor, LoadedProjection, ServeConfig,
    ServePolicy, UsageBrokerClient, UsageBrokerConfig, claim_leader, connect_probe,
    grouped_bindings, load_projection, publication_identity_metadata, publish,
    secure_run_directory, serve, unavailable, usage_catalog_entries, validate_owned_mode,
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
    let identity_metadata = publication_identity_metadata(&discovery);
    let catalog_revision = discovery
        .config_generation
        .clone()
        .unwrap_or_else(|| "empty".to_owned());
    let catalog = usage_catalog_entries(&discovery);
    let bindings = grouped_bindings(&discovery);
    let executor = Arc::new(DiscoveryProviderExecutor {
        bindings: Mutex::new(bindings),
        validated_catalog: Mutex::new(None),
        scope,
        resolver,
        probe_budget: config.coordinator.provider_timeout,
    });
    run_usage_broker_service_with_executor_and_metadata(
        config,
        executor,
        identity_metadata,
        Some((catalog_revision, catalog)),
    )
}

/// Process service seam used by the shipped broker binary and process tests.
pub fn run_usage_broker_service_with_executor(
    config: UsageBrokerConfig,
    executor: Arc<dyn UsageProviderExecutor>,
) -> Result<(), UsageCoordinationError> {
    run_usage_broker_service_with_executor_and_metadata(config, executor, BTreeMap::new(), None)
}

pub(crate) fn run_usage_broker_service_with_executor_and_metadata(
    config: UsageBrokerConfig,
    executor: Arc<dyn UsageProviderExecutor>,
    identity_metadata: BTreeMap<UsageAccountCapability, publish::AccountIdentityMetadata>,
    initial_catalog: Option<(String, Vec<UsageCatalogEntry>)>,
) -> Result<(), UsageCoordinationError> {
    let run_dir = secure_run_directory(&config.data_dir)?;
    let leader_path = run_dir.join(BROKER_LEADER);
    let Some(lease) = claim_leader(&leader_path, &config.build_id, config.lease_duration)? else {
        return Ok(());
    };
    let socket_path = config.socket_path();
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
    if let Some((catalog_revision, catalog)) = initial_catalog {
        publisher.reconcile_catalog(catalog_revision, catalog, chrono::Utc::now().timestamp())?;
    }
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
    });
    Ok(())
}

/// Test/runtime seam that preserves the same process election and transport.
#[doc(hidden)]
pub fn ensure_usage_broker_with_executor(
    config: UsageBrokerConfig,
    executor: Arc<dyn UsageProviderExecutor>,
) -> Result<UsageBrokerClient, UsageCoordinationError> {
    let socket_path = config.socket_path();
    let client = UsageBrokerClient::at(socket_path.clone(), config.build_id.clone());
    if connect_probe(&client) {
        return Ok(client);
    }

    let run_dir = secure_run_directory(&config.data_dir)?;
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
        });
    })
    .map_err(|_| unavailable())?;
    wait_for_leader(&client)?;
    Ok(client)
}
