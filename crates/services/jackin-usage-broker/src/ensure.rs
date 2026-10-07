// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker activation and leader lock.

use std::collections::BTreeMap;
use std::fs::File;

use std::os::unix::fs::MetadataExt as _;

use std::path::Path;
use std::process::{Command, Stdio};

use std::sync::Arc;

use jackin_protocol::usage_broker::{
    UsageCatalogEntry, UsageCoordinationError, UsageCoordinationErrorKind, UsageProjectionV1,
};
use nix::fcntl::{Flock, FlockArg, OFlag, open};

use nix::sys::stat::{Mode, fchmod};
use nix::unistd::geteuid;

use crate::{
    BROKER_ACTIVATE_LOCK, BROKER_ACTIVATION_ATTEMPTS, BROKER_DIR, ScopedCapability,
    UsageBrokerClient, UsageBrokerConfig, UsageBrokerHandle, capability_for_binding, connect_probe,
    forwarding_requirement, secure_run_directory, unavailable, usage_broker_capabilities,
    usage_catalog_entries, wait_for_leader,
};
use jackin_usage_discovery::{
    UsageDiscoveryScope, ValidatedUsageDiscovery, discover_usage_sources, validate_usage_sources,
};
use jackin_usage_host_credentials::ProviderCredentialEnvResolver;

/// Hold the inter-process activation lock across one
/// discover→read-lease→reconcile sequence.
///
/// The lock serializes concurrent activators, so every activation observes a
/// post-lease discovery generation: a slow activator can no longer pair its
/// stale caller-side catalog with a freshly read publication lease and win
/// over a newer winner. The lock releases on drop (and on process death via
/// the kernel), so a crashed activator never wedges later activations.
pub(crate) fn lock_activation(data_dir: &Path) -> Result<Flock<File>, UsageCoordinationError> {
    secure_run_directory(data_dir)?;
    let path = data_dir.join(BROKER_DIR).join(BROKER_ACTIVATE_LOCK);
    let fd = open(
        &path,
        OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_NOFOLLOW,
        Mode::from_bits_truncate(0o600),
    )
    .map_err(|_| unavailable())?;
    let file = File::from(fd);
    fchmod(&file, Mode::from_bits_truncate(0o600)).map_err(|_| unavailable())?;
    let metadata = file.metadata().map_err(|_| unavailable())?;
    if metadata.uid() != geteuid().as_raw() {
        return Err(unavailable());
    }
    Flock::lock(file, FlockArg::LockExclusive).map_err(|_| unavailable())
}

/// Ensure one host broker backed by a post-lease discovery generation.
///
/// The caller-supplied `discovery` is a fallback only. Every activation
/// re-discovers under the inter-process activation lock and publishes the
/// fresh generation, so a slow activator's stale caller-side catalog can
/// never win over a newer winner: the CAS lease alone cannot reject it,
/// because the stale caller would otherwise read the lease fresh. A failed
/// fresh scan falls back to the caller generation (documented degradation);
/// a CAS conflict retries with re-discovery up to
/// `BROKER_ACTIVATION_ATTEMPTS`, then fails closed.
pub fn ensure_usage_broker(
    config: UsageBrokerConfig,
    scope: UsageDiscoveryScope,
    discovery: ValidatedUsageDiscovery,
    resolver: Arc<dyn ProviderCredentialEnvResolver>,
) -> Result<UsageBrokerHandle, UsageCoordinationError> {
    let mut discover = || {
        resolver.begin_manual_retry();
        discover_usage_sources(&scope, resolver.as_ref())
            .map(|catalog| validate_usage_sources(catalog, resolver.as_ref()))
            .map_err(|_| unavailable())
    };
    let mut reconcile = |client: &UsageBrokerClient,
                         expected_projection_id: Option<String>,
                         catalog_revision: String,
                         entries: Vec<UsageCatalogEntry>| {
        client.reconcile_catalog_if_projection(expected_projection_id, catalog_revision, entries)
    };
    ensure_usage_broker_with_hooks(&config, &scope, discovery, &mut discover, &mut reconcile)
}

/// Activation core with injectable discovery/reconcile seams.
///
/// Production passes live discovery and the broker CAS reconcile; tests drive
/// scripted generations and interleavings through the same path. The
/// activation lock is held across every attempt's discover→read-lease→
/// reconcile sequence.
pub(crate) fn ensure_usage_broker_with_hooks(
    config: &UsageBrokerConfig,
    scope: &UsageDiscoveryScope,
    fallback: ValidatedUsageDiscovery,
    discover: &mut impl FnMut() -> Result<ValidatedUsageDiscovery, UsageCoordinationError>,
    reconcile: &mut impl FnMut(
        &UsageBrokerClient,
        Option<String>,
        String,
        Vec<UsageCatalogEntry>,
    ) -> Result<UsageProjectionV1, UsageCoordinationError>,
) -> Result<UsageBrokerHandle, UsageCoordinationError> {
    let _activation = lock_activation(&config.data_dir)?;
    let mut scan = || discover().unwrap_or_else(|_| fallback.clone());
    let mut attempts: u32 = 0;
    loop {
        attempts = attempts.saturating_add(1);
        // Post-lease discovery: resolve the current generation only after
        // holding the activation lock, so a slow activator can never pair a
        // stale caller-side catalog with a fresh publication lease.
        let mut discovery = scan();
        if usage_catalog_entries(&discovery).is_empty() {
            // Empty scans are confirmed by a second post-lease scan before
            // acting: a transient empty scan can neither wipe a live catalog
            // nor suppress broker activation for present accounts, while a
            // confirmed empty scan still revokes.
            discovery = scan();
        }
        let catalog = usage_catalog_entries(&discovery);
        let catalog_revision = discovery
            .config_generation
            .clone()
            .unwrap_or_else(|| "empty".to_owned());
        let client = if catalog.is_empty() {
            let probe_client = config.client();
            if !connect_probe(&probe_client) {
                return Ok(usage_broker_handle_for(
                    &discovery,
                    probe_client,
                    "no-catalog".to_owned(),
                ));
            }
            probe_client
        } else {
            ensure_usage_broker_process(config.clone(), scope)?
        };
        let expected_projection_id = client.current_projection()?.projection_id;
        match reconcile(
            &client,
            Some(expected_projection_id),
            catalog_revision,
            catalog,
        ) {
            Ok(projection) => {
                return Ok(usage_broker_handle_for(
                    &discovery,
                    client,
                    projection.projection_id,
                ));
            }
            Err(error)
                if error.kind == UsageCoordinationErrorKind::CatalogRevisionConflict
                    && attempts < BROKER_ACTIVATION_ATTEMPTS =>
            {
                // Retry with re-discovery; the loop re-scans above.
            }
            Err(error) => return Err(error),
        }
    }
}

/// Build the activation handle from the generation actually published.
pub(crate) fn usage_broker_handle_for(
    discovery: &ValidatedUsageDiscovery,
    client: UsageBrokerClient,
    catalog_lease: String,
) -> UsageBrokerHandle {
    let mut scoped_capabilities = BTreeMap::<String, Vec<ScopedCapability>>::new();
    for binding in &discovery.bindings {
        let capability = capability_for_binding(binding, discovery.config_generation.as_deref());
        let requirement = forwarding_requirement(binding);
        for provenance in &binding.provenance {
            let scoped = scoped_capabilities.entry(provenance.clone()).or_default();
            scoped.push(ScopedCapability {
                capability: capability.clone(),
                requirement: requirement.clone(),
            });
        }
    }
    let capabilities = usage_broker_capabilities(discovery);
    UsageBrokerHandle {
        client,
        capabilities,
        catalog_lease,
        scoped_capabilities,
    }
}

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
