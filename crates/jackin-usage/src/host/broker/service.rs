// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Broker process startup and service lifecycle.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _, PermissionsExt as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageIdentityKindV1,
};

use crate::coordinator::{
    FileAccountStateStore, FileProjectionStateStore, UsageCoordinator, UsageProviderExecutor,
};
use crate::host::discovery::ProviderCredentialEnvResolver;

use super::client_security;
use super::{
    BROKER_LEADER, BrokerSocketIdentity, BrokerStartupCleanup, DiscoveryProviderExecutor,
    EmptyProviderCredentialResolver, LoadedProjection, ServePolicy, UsageBrokerClient,
    UsageBrokerConfig, UsageDiscoveryScope, broker_conflict, claim_leader,
    claude_usage_capability_for_service, connect_probe, load_projection, monitor,
    projection_identity_metadata, secure_run_directory, unavailable, validate_owned_mode,
    wait_for_leader,
};
use super::{catalog, publish, serve_loop};

const CLAUDE_KEYCHAIN_SOURCE_SCOPE: &str = "claude_keychain_service";

/// Activate the independent broker executable and attach a client.
///
/// The caller never supplies an executor to this path. The sibling service
/// performs discovery and provider work in its own process, then survives the
/// activating client. The installed `jackin` package owns this sibling binary.
pub fn ensure_usage_broker_process(
    config: UsageBrokerConfig,
    scope: &UsageDiscoveryScope,
) -> Result<UsageBrokerClient, UsageCoordinationError> {
    let client = config.client();
    if connect_probe(&client) {
        return Ok(client);
    }
    let executable = config
        .service_executable
        .as_ref()
        .ok_or_else(|| UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unavailable,
            message:
                "usage broker executable cannot be located; reinstall the complete jackin package"
                    .to_owned(),
        })?;
    let mut command = Command::new(executable);
    command
        .arg("--data-dir")
        .arg(&config.data_dir)
        .arg("--build-id")
        .arg(&config.build_id)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match scope {
        UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home,
        } => {
            command
                .arg("--config-root")
                .arg(config_root)
                .arg("--operator-home")
                .arg(operator_home);
        }
        UsageDiscoveryScope::Capsule { .. } => return Err(unavailable()),
    }
    command.spawn().map_err(|error| UsageCoordinationError {
        kind: UsageCoordinationErrorKind::Unavailable,
        message: format!(
            "cannot start usage broker executable {}: {error}; reinstall the complete jackin package",
            executable.display(),
        ),
    })?;
    wait_for_leader(&client)?;
    Ok(client)
}

/// Run the process-owned broker until its idle lease expires.
///
/// Startup uses only the persisted catalog and projection. Discovery is
/// broker-owned but occurs only in response to an explicit relay request;
/// account and credential discovery never runs while the service attaches.
pub fn run_usage_broker_service(
    config: UsageBrokerConfig,
    scope: UsageDiscoveryScope,
    resolver: Arc<dyn ProviderCredentialEnvResolver>,
) -> Result<(), UsageCoordinationError> {
    let Some(cleanup) = claim_startup_cleanup(&config)? else {
        return Ok(());
    };
    let monitor_store =
        Arc::new(monitor::MonitorStore::open(&config.data_dir).map_err(|_| unavailable())?);
    let catalog_refresh = Arc::new(catalog::BrokerCatalogRefresh::new(
        scope.clone(),
        Arc::clone(&resolver),
    ));
    let executor = Arc::new(DiscoveryProviderExecutor {
        bindings: Mutex::new(BTreeMap::new()),
        validated_catalog: Mutex::new(None),
        scope,
        resolver,
        monitor_store: Some(Arc::clone(&monitor_store)),
        collector_service: None,
        #[cfg(test)]
        claude_collector: None,
        probe_budget: config.coordinator.provider_timeout,
    });
    run_usage_broker_service_with_cleanup(
        config,
        executor,
        BTreeMap::new(),
        None,
        Some(catalog_refresh),
        Some(monitor_store),
        cleanup,
        || {},
    )
}

/// Secret-free metadata emitted once a foreground collector has bound its
/// exact source catalog and is ready to serve requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageBrokerForegroundReady {
    /// Stable local source account selected for this foreground process.
    pub capability: UsageAccountCapability,
    /// Non-secret source category. The raw operator-selected service is never
    /// included in readiness metadata.
    pub binding_scope: String,
}

/// Claim the broker lease before preparing the exact Claude Keychain service,
/// then keep the selected credential lease and broker lease for one foreground
/// service lifetime. No credential material crosses IPC or the ready callback.
pub fn run_usage_broker_foreground_bootstrap(
    config: UsageBrokerConfig,
    scope: UsageDiscoveryScope,
    service: &str,
    on_ready: impl FnOnce(UsageBrokerForegroundReady),
) -> Result<crate::usage::ClaudeCredentialBootstrapOutcome, UsageCoordinationError> {
    let outcome = run_usage_broker_foreground_bootstrap_core(
        config,
        scope,
        service,
        |service| {
            crate::usage::bootstrap_claude_credential(service)
                .map(foreground_bootstrap_outcome)
                .map_err(|_| unavailable())
        },
        || crate::usage::unattended_keychain_guard().map_err(|_| unavailable()),
        on_ready,
    )?;
    Ok(match outcome {
        ForegroundBootstrapOutcome::Acquired(credential_lease) => {
            crate::usage::ClaudeCredentialBootstrapOutcome::Acquired(credential_lease)
        }
        ForegroundBootstrapOutcome::Missing => {
            crate::usage::ClaudeCredentialBootstrapOutcome::Missing
        }
        ForegroundBootstrapOutcome::Denied => {
            crate::usage::ClaudeCredentialBootstrapOutcome::Denied
        }
        ForegroundBootstrapOutcome::InteractionRequired => {
            crate::usage::ClaudeCredentialBootstrapOutcome::InteractionRequired
        }
        ForegroundBootstrapOutcome::Malformed => {
            crate::usage::ClaudeCredentialBootstrapOutcome::Malformed
        }
    })
}

/// Secret-free bootstrap outcome shared by the real startup and the fake
/// credential/guard test seam. The lease value remains owned by the core
/// through the complete service loop.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ForegroundBootstrapOutcome<L> {
    Acquired(L),
    Missing,
    Denied,
    InteractionRequired,
    Malformed,
}

fn foreground_bootstrap_outcome(
    outcome: crate::usage::ClaudeCredentialBootstrapOutcome,
) -> ForegroundBootstrapOutcome<crate::usage::ClaudeCredentialLease> {
    match outcome {
        crate::usage::ClaudeCredentialBootstrapOutcome::Acquired(lease) => {
            ForegroundBootstrapOutcome::Acquired(lease)
        }
        crate::usage::ClaudeCredentialBootstrapOutcome::Missing => {
            ForegroundBootstrapOutcome::Missing
        }
        crate::usage::ClaudeCredentialBootstrapOutcome::Denied => {
            ForegroundBootstrapOutcome::Denied
        }
        crate::usage::ClaudeCredentialBootstrapOutcome::InteractionRequired => {
            ForegroundBootstrapOutcome::InteractionRequired
        }
        crate::usage::ClaudeCredentialBootstrapOutcome::Malformed => {
            ForegroundBootstrapOutcome::Malformed
        }
    }
}

fn run_usage_broker_foreground_bootstrap_core<L, G>(
    config: UsageBrokerConfig,
    scope: UsageDiscoveryScope,
    service: &str,
    bootstrap: impl FnOnce(&str) -> Result<ForegroundBootstrapOutcome<L>, UsageCoordinationError>,
    establish_guard: impl FnOnce() -> Result<G, UsageCoordinationError>,
    on_ready: impl FnOnce(UsageBrokerForegroundReady),
) -> Result<ForegroundBootstrapOutcome<L>, UsageCoordinationError> {
    if !matches!(&scope, UsageDiscoveryScope::HostDesktop { .. }) {
        return Err(unavailable());
    }
    let cleanup = claim_foreground_startup_cleanup(&config)?;
    let outcome = bootstrap(service)?;
    let ForegroundBootstrapOutcome::Acquired(credential_lease) = outcome else {
        return Ok(outcome);
    };
    let _credential_lease = &credential_lease;
    let _unattended_keychain_guard = establish_guard()?;
    let monitor_store =
        Arc::new(monitor::MonitorStore::open(&config.data_dir).map_err(|_| unavailable())?);
    let capability = claude_usage_capability_for_service(service);
    monitor_store.set_experimental_collector_source(Some(capability.account_id.clone()));
    let catalog_entry = UsageCatalogEntry {
        revision: jackin_core::account_key_hash("usage-catalog-entry-v3", &capability.account_id),
        capability: capability.clone(),
    };
    let identity_metadata = BTreeMap::from([(
        capability.clone(),
        publish::AccountIdentityMetadata {
            identity_kind: UsageIdentityKindV1::LocalSourceHandle,
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
        monitor_store: Some(Arc::clone(&monitor_store)),
        collector_service: Some(service.to_owned()),
        #[cfg(test)]
        claude_collector: None,
        probe_budget: config.coordinator.provider_timeout,
    });
    let ready = UsageBrokerForegroundReady {
        capability,
        binding_scope: CLAUDE_KEYCHAIN_SOURCE_SCOPE.to_owned(),
    };
    run_usage_broker_service_with_cleanup(
        config,
        executor,
        identity_metadata,
        Some(ForegroundCatalogSeed {
            service: service.to_owned(),
            entry: catalog_entry,
            diagnostics: publish::CatalogDiagnostics::default(),
        }),
        None,
        Some(monitor_store),
        cleanup,
        || on_ready(ready),
    )?;
    Ok(ForegroundBootstrapOutcome::Acquired(credential_lease))
}

#[cfg(test)]
pub(super) fn run_usage_broker_foreground_bootstrap_with_for_test<L, G>(
    config: UsageBrokerConfig,
    scope: UsageDiscoveryScope,
    service: &str,
    bootstrap: impl FnOnce(&str) -> Result<ForegroundBootstrapOutcome<L>, UsageCoordinationError>,
    establish_guard: impl FnOnce() -> Result<G, UsageCoordinationError>,
    on_ready: impl FnOnce(UsageBrokerForegroundReady),
) -> Result<ForegroundBootstrapOutcome<L>, UsageCoordinationError> {
    run_usage_broker_foreground_bootstrap_core(
        config,
        scope,
        service,
        bootstrap,
        establish_guard,
        on_ready,
    )
}

struct ForegroundCatalogSeed {
    service: String,
    entry: UsageCatalogEntry,
    diagnostics: publish::CatalogDiagnostics,
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

fn run_usage_broker_service_with_executor_and_metadata(
    config: UsageBrokerConfig,
    executor: Arc<dyn UsageProviderExecutor>,
    identity_metadata: BTreeMap<UsageAccountCapability, publish::AccountIdentityMetadata>,
    initial_catalog: Option<ForegroundCatalogSeed>,
    catalog_refresh: Option<Arc<catalog::BrokerCatalogRefresh>>,
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

fn claim_startup_cleanup(
    config: &UsageBrokerConfig,
) -> Result<Option<BrokerStartupCleanup>, UsageCoordinationError> {
    let (leader_path, socket_path) = startup_lease_paths(config)?;
    let Some(lease) = claim_leader(&leader_path, &config.build_id, config.lease_duration)? else {
        return Ok(None);
    };
    Ok(Some(BrokerStartupCleanup::new(
        leader_path,
        socket_path,
        lease,
    )))
}

fn claim_foreground_startup_cleanup(
    config: &UsageBrokerConfig,
) -> Result<BrokerStartupCleanup, UsageCoordinationError> {
    let (leader_path, socket_path) = startup_lease_paths(config)?;
    let lease = claim_leader(&leader_path, &config.build_id, config.lease_duration)
        .map_err(|_| broker_conflict())?
        .ok_or_else(broker_conflict)?;
    let cleanup = BrokerStartupCleanup::new(leader_path, socket_path, lease);
    prepare_socket_for_startup(config, &cleanup)?;
    Ok(cleanup)
}

/// Reclaim a socket only when a previously valid broker lease was expired and
/// a raw connection is refused or the path has disappeared. Protocol or build
/// mismatches still count as a live, unrecognized endpoint and are left alone.
fn prepare_socket_for_startup(
    config: &UsageBrokerConfig,
    cleanup: &BrokerStartupCleanup,
) -> Result<(), UsageCoordinationError> {
    let socket_path = config.socket_path();
    let metadata = match fs::symlink_metadata(&socket_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(broker_conflict()),
    };
    if !metadata.file_type().is_socket() || !cleanup.reclaimed_stale_lease() {
        return Err(broker_conflict());
    }
    let expected = BrokerSocketIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    };
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
        || current.dev() != expected.device
        || current.ino() != expected.inode
    {
        return Err(broker_conflict());
    }
    fs::remove_file(&socket_path).map_err(|_| broker_conflict())
}

fn startup_lease_paths(
    config: &UsageBrokerConfig,
) -> Result<(PathBuf, PathBuf), UsageCoordinationError> {
    client_security::validate_broker_data_ancestors(&config.data_dir)?;
    let run_dir = secure_run_directory(&config.data_dir)?;
    client_security::validate_broker_data_tree(&config.data_dir)?;
    let leader_path = run_dir.join(BROKER_LEADER);
    let socket_path = config.socket_path();
    Ok((leader_path, socket_path))
}

fn run_usage_broker_service_with_cleanup(
    config: UsageBrokerConfig,
    executor: Arc<dyn UsageProviderExecutor>,
    identity_metadata: BTreeMap<UsageAccountCapability, publish::AccountIdentityMetadata>,
    initial_catalog: Option<ForegroundCatalogSeed>,
    catalog_refresh: Option<Arc<catalog::BrokerCatalogRefresh>>,
    monitor_store: Option<Arc<monitor::MonitorStore>>,
    cleanup: BrokerStartupCleanup,
    on_ready: impl FnOnce(),
) -> Result<(), UsageCoordinationError> {
    let socket_path = config.socket_path();
    let mut cleanup = cleanup;
    prepare_socket_for_startup(&config, &cleanup)?;
    let listener = UnixListener::bind(&socket_path).map_err(|_| broker_conflict())?;
    cleanup
        .record_socket_identity()
        .map_err(|_| unavailable())?;
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
    let projection_guard = projection.lock().map_err(|_| unavailable())?;
    let mut complete_identity_metadata = projection_identity_metadata(&projection_guard);
    drop(projection_guard);
    complete_identity_metadata.extend(identity_metadata);
    let publisher = publish::ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::clone(&projection),
        FileProjectionStateStore::under_data_dir(&config.data_dir),
    )
    .with_identity_metadata(complete_identity_metadata);
    let publisher = publisher.with_catalog(previous_catalog.clone());
    if let Some(seed) = initial_catalog {
        // Retain this account's exact durable revision when it was already
        // known, then replace membership with only the operator-selected
        // Claude service. The coordinator revokes sibling rows and preserves
        // their cooldown tombstones without letting them run in this process.
        let entry = previous_catalog
            .iter()
            .find(|entry| entry.capability == seed.entry.capability)
            .cloned()
            .unwrap_or(seed.entry);
        let catalog = vec![entry];
        let catalog_revision = super::foreground_catalog_revision(&seed.service, &catalog);
        publisher.reconcile_catalog_if_projection(
            None,
            publish::CatalogReconciliation {
                catalog_revision,
                entries: catalog,
                diagnostics: seed.diagnostics,
                identity_metadata: None,
            },
            chrono::Utc::now().timestamp(),
        )?;
    }
    let monitor_store = match monitor_store {
        Some(monitor_store) => monitor_store,
        None => Arc::new(monitor::MonitorStore::open(&config.data_dir).map_err(|_| unavailable())?),
    };
    let initial_projection = publisher.current_projection().map_err(|_| unavailable())?;
    let monitor_now = chrono::Utc::now().timestamp();
    monitor_store
        .observe_projection(&initial_projection, monitor_now)
        .map_err(|_| unavailable())?;
    monitor_store.tick(monitor_now).map_err(|_| unavailable())?;
    on_ready();
    serve_loop::serve(serve_loop::ServeConfig {
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

    let mut cleanup = BrokerStartupCleanup::new(leader_path, socket_path.clone(), lease);
    prepare_socket_for_startup(&config, &cleanup)?;
    let listener = UnixListener::bind(&socket_path).map_err(|_| broker_conflict())?;
    cleanup
        .record_socket_identity()
        .map_err(|_| unavailable())?;
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
    let monitor_store =
        Arc::new(monitor::MonitorStore::open(&config.data_dir).map_err(|_| unavailable())?);
    let initial_projection = publisher.current_projection().map_err(|_| unavailable())?;
    let monitor_now = chrono::Utc::now().timestamp();
    monitor_store
        .observe_projection(&initial_projection, monitor_now)
        .map_err(|_| unavailable())?;
    monitor_store.tick(monitor_now).map_err(|_| unavailable())?;
    jackin_telemetry::spawn::thread_joined_named("usage-broker".to_owned(), move || {
        serve_loop::serve(serve_loop::ServeConfig {
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
