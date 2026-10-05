// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Per-container allowlisted relay to the host-only usage broker.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result};
use jackin_config::AppConfig;
use jackin_core::{ContainerHandle, JackinPaths, UsageCredentialEnvName, WorkspaceName};
use jackin_protocol::CapsuleConfig;
use jackin_protocol::usage_broker::{
    USAGE_BROKER_MAX_FRAME_BYTES, USAGE_BROKER_PROTOCOL_VERSION, UsageAccountCapability,
    UsageBrokerOperation, UsageBrokerResponse, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageCredentialScope, UsageCredentialSourceIdentity, UsageCredentialSourceProof,
    UsageProfileSourceProof,
    UsageRelayTunnelMessage, UsageRelayTunnelResponse,
    usage_credential_material_fingerprint,
};
use jackin_usage::coordinator::UsageCapabilitySet;
use jackin_usage::host::{
    CachedProviderCredentialResolver, ForwardedUsageSources, HostSurfaceId,
    ProviderCredentialSecretOutcome, ProviderCredentialSecretResolution,
    ProviderCredentialSecretSource, UsageBrokerClient, UsageBrokerConfig, discover_usage_sources,
    forwarded_usage_capabilities, usage_capability_for_selected_account_with_sources,
    validate_usage_sources,
};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt as _, AsyncRead, AsyncReadExt as _, AsyncWrite,
    AsyncWriteExt as _, BufReader,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};
use tokio::task::JoinSet;

mod inventory;
mod instance_scope;
mod persistence;
use inventory::RelayUsageInventory;

const TUNNEL_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
const TUNNEL_REQUEST_CAPACITY: usize = 128;
const TUNNEL_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(35);

pub(crate) fn docker_runtime_mount(socket_dir: &Path) -> Result<String> {
    let source = socket_dir.to_str().ok_or_else(|| {
        anyhow::anyhow!(
            "socket dir {} contains non-UTF-8 bytes; cannot pass to docker -v",
            socket_dir.display(),
        )
    })?;
    Ok(format!(
        "{source}:{}",
        jackin_core::container_paths::RUN_DIR
    ))
}

pub(crate) fn apple_runtime_mount(
    socket_dir: PathBuf,
) -> crate::apple_container_client::AppleContainerMount {
    crate::apple_container_client::AppleContainerMount::new(
        socket_dir.join(jackin_protocol::CAPSULE_CONFIG_FILENAME),
        jackin_protocol::CAPSULE_CONFIG_PATH,
        true,
    )
}

#[derive(Default)]
struct RuntimeSecretSource;

impl ProviderCredentialSecretSource for RuntimeSecretSource {
    fn lookup_declaration(
        &self,
        config: &AppConfig,
        workspace: Option<&WorkspaceName>,
        role: Option<&str>,
        entry: UsageCredentialEnvName,
    ) -> Option<jackin_config::EnvValue> {
        jackin_env::lookup_operator_env_declaration(config, role, workspace, entry.name)
    }

    fn resolve_secret(
        &self,
        config: &AppConfig,
        workspace: Option<&WorkspaceName>,
        role: Option<&str>,
        entry: UsageCredentialEnvName,
    ) -> Option<ProviderCredentialSecretResolution> {
        let declaration =
            jackin_env::lookup_operator_env_declaration(config, role, workspace, entry.name)?;
        let resolved =
            jackin_env::resolve_operator_env_per_key_matching(config, role, workspace, |key| {
                key == entry.name
            })
            .into_iter()
            .next();
        let outcome = match resolved {
            Some(result)
                if result.status() == jackin_env::OperatorEnvKeyStatus::Resolved
                    && result.resolved_value().is_some() =>
            {
                ProviderCredentialSecretOutcome::Resolved(
                    result.resolved_value().unwrap_or_default().to_owned(),
                )
            }
            Some(result) => match result.status() {
                jackin_env::OperatorEnvKeyStatus::Resolved => {
                    ProviderCredentialSecretOutcome::Malformed
                }
                jackin_env::OperatorEnvKeyStatus::Missing => {
                    ProviderCredentialSecretOutcome::Missing
                }
                jackin_env::OperatorEnvKeyStatus::DeniedOrUnavailable => {
                    ProviderCredentialSecretOutcome::Denied
                }
                jackin_env::OperatorEnvKeyStatus::Malformed => {
                    ProviderCredentialSecretOutcome::Malformed
                }
                jackin_env::OperatorEnvKeyStatus::InteractionRequired => {
                    ProviderCredentialSecretOutcome::InteractionRequired
                }
            },
            None => return None,
        };
        Some(ProviderCredentialSecretResolution {
            declaration,
            outcome,
        })
    }
}

/// Host launch facts needed to construct one scoped usage relay.
#[derive(Debug)]
pub struct UsageRelayLaunch<'a> {
    /// Host paths for config, data, and private socket directories.
    pub paths: &'a JackinPaths,
    /// Effective workspace, if this is not an ad-hoc launch.
    pub workspace_name: Option<&'a str>,
    /// Effective role key.
    pub role_key: &'a str,
    /// Host-validated launch config carrying per-session Unix identities.
    pub launch_config: &'a CapsuleConfig,
    /// Exact credential sources proven to enter this Capsule.
    pub forwarded_sources: ForwardedUsageSources,
}

/// Session-lifetime relay ownership. Drop revokes the socket task.
pub struct UsageRelayGuard {
    task: Option<tokio::task::JoinHandle<()>>,
    shutdown: Option<oneshot::Sender<()>>,
}

struct AbortTaskOnDrop<T>(tokio::task::JoinHandle<T>);

impl<T> Drop for AbortTaskOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl std::fmt::Debug for UsageRelayGuard {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UsageRelayGuard")
            .finish_non_exhaustive()
    }
}

impl UsageRelayGuard {
    /// Revoke and await the transport before another attachment acquires its socket.
    pub(crate) async fn shutdown(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _sent = shutdown.send(());
        }
        if let Some(task) = self.task.take() {
            let _result = task.await;
        }
    }
}

impl Drop for UsageRelayGuard {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _sent = shutdown.send(());
        } else if let Some(task) = &self.task {
            task.abort();
        }
    }
}

/// Host broker and exact launch-derived capabilities awaiting a backend transport.
#[derive(Debug)]
pub struct PreparedUsageRelay {
    broker: UsageBrokerClient,
    capabilities: Vec<UsageAccountCapability>,
    instance_capabilities: BTreeMap<String, UsageAccountCapability>,
    credential_scope: UsageCredentialScope,
    inventory: Option<RelayUsageInventory>,
    persistence_context: Option<RelayPersistenceContext>,
}

#[derive(Debug, Clone)]
struct RelayPersistenceContext {
    paths: JackinPaths,
    workspace: Option<String>,
    role_key: String,
    config_generation: String,
}

/// Canonical host capabilities resolved for the configured account selections
/// in one Capsule launch. The launch config starts with config-account aliases
/// so discovery can select the right binding; this map replaces those aliases
/// with the exact opaque authorities accepted by the relay.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CanonicalLaunchUsageCapabilities {
    by_account_surface: BTreeMap<(String, String), UsageAccountCapability>,
}

impl CanonicalLaunchUsageCapabilities {
    fn for_instances(
        &self,
        launch_config: &CapsuleConfig,
        allowed: &BTreeSet<UsageAccountCapability>,
    ) -> BTreeMap<String, UsageAccountCapability> {
        launch_config
            .instances
            .iter()
            .filter_map(|instance_id| {
                let account_id = launch_config.accounts.get(instance_id)?;
                let alias = launch_config.usage_capabilities.get(instance_id)?;
                let capability = self
                    .by_account_surface
                    .get(&(account_id.clone(), alias.surface_id.clone()))?;
                allowed
                    .contains(capability)
                    .then_some((instance_id.clone(), capability.clone()))
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn apply_to_launch_config(&self, launch_config: &mut CapsuleConfig) -> Result<()> {
        let replacements = launch_config
            .instances
            .iter()
            .filter_map(|instance_id| {
                let account_id = launch_config.accounts.get(instance_id)?;
                let capability = launch_config.usage_capabilities.get(instance_id)?;
                Some((
                    instance_id.clone(),
                    account_id.clone(),
                    capability.surface_id.clone(),
                ))
            })
            .collect::<Vec<_>>();
        for (instance_id, account_id, surface_id) in replacements {
            let key = (account_id, surface_id);
            if let Some(capability) = self.by_account_surface.get(&key) {
                launch_config
                    .usage_capabilities
                    .insert(instance_id, capability.clone());
            } else {
                // Never leave a pre-discovery config alias in the Capsule when
                // host discovery did not prove that exact identity.
                launch_config.usage_capabilities.remove(&instance_id);
            }
        }
        ensure_distinct_usage_unix_identities(launch_config)
    }
}

/// Fail the launch closed when two instances carrying usage capabilities
/// share one Unix identity. The Capsule proxy authorizes peers by
/// `(uid, gid)`, so a collision would make two instances'
/// capabilities indistinguishable.
fn ensure_distinct_usage_unix_identities(launch_config: &CapsuleConfig) -> Result<()> {
    let mut seen = BTreeSet::new();
    for instance_id in &launch_config.instances {
        let Some(identity) = launch_config.identity_for_instance(instance_id) else {
            continue;
        };
        if !launch_config.usage_capabilities.contains_key(instance_id) {
            continue;
        }
        anyhow::ensure!(
            seen.insert((identity.uid, identity.gid)),
            "multiple usage instances share Unix identity {identity:?}"
        );
    }
    Ok(())
}

/// Derive source proof from credentials actually provisioned for this launch.
#[must_use]
pub fn forwarded_sources_from_launch(
    _state: &crate::instance::RoleState,
    resolved_env: &jackin_env::ResolvedEnv,
) -> ForwardedUsageSources {
    let env_names = resolved_env
        .vars
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<BTreeSet<_>>();
    let env_keys = jackin_core::USAGE_CREDENTIAL_ENV_REGISTRY
        .iter()
        .filter(|entry| env_names.contains(entry.name))
        .map(|entry| entry.name.to_owned())
        .collect();
    ForwardedUsageSources {
        selected_account_ids: BTreeSet::new(),
        selected_account_surfaces: BTreeMap::new(),
        env_keys,
        credential_scope: UsageCredentialScope::default(),
    }
}

/// Build the immutable, secret-free source fence immediately after launch
/// staging. The fingerprint is computed from the exact value written to the
/// per-instance credential file; no later config/env/op lookup participates.
pub fn usage_credential_scope_for_staged_launch(
    config: &AppConfig,
    instances: &[jackin_config::ResolvedInstance],
    credentials: &jackin_protocol::AgentCredentialEnv,
) -> Result<UsageCredentialScope> {
    let mut sources = BTreeSet::new();
    for instance in instances {
        let account = config
            .accounts
            .get(&instance.account_id)
            .ok_or_else(|| anyhow::anyhow!("unknown account {:?}", instance.account_id))?;
        let Some(surface) = HostSurfaceId::from_provider_alias(account.provider.slug()) else {
            continue;
        };
        let declarations = jackin_env::credential_env_declarations_for_instance(config, instance)?;
        let has_governed_credentials = jackin_core::USAGE_CREDENTIAL_ENV_REGISTRY
            .iter()
            .any(|entry| declarations.contains_key(entry.name));
        if !has_governed_credentials {
            continue;
        }
        let staged = credentials.instance(&instance.config_id).ok_or_else(|| {
            anyhow::anyhow!(
                "staged credentials missing for launch instance {:?}",
                instance.config_id
            )
        })?;
        for entry in jackin_core::USAGE_CREDENTIAL_ENV_REGISTRY {
            let Some(declaration) = declarations.get(entry.name) else {
                continue;
            };
            let value = staged.env.get(entry.name).ok_or_else(|| {
                anyhow::anyhow!(
                    "staged credential {:?} missing for launch instance {:?}",
                    entry.name,
                    instance.config_id
                )
            })?;
            sources.insert(UsageCredentialSourceProof {
                instance_id: instance.config_id.clone(),
                account_id: instance.account_id.clone(),
                surface_id: surface.id().to_owned(),
                key: entry.name.to_owned(),
                source: UsageCredentialSourceIdentity::from_declaration(declaration),
                material_fingerprint: usage_credential_material_fingerprint(value),
            });
        }
    }
    Ok(UsageCredentialScope { sources, profiles: BTreeSet::new() })
}

/// Merge only profile material captured for the exact admitted auth slots.
/// No host discovery or credential source reads participate in this fence.
pub(crate) fn merge_usage_profile_scope_for_prepared_launch(
    config: &AppConfig,
    instances: &[jackin_config::ResolvedInstance],
    state: &crate::instance::RoleState,
    credential_scope: &mut UsageCredentialScope,
) -> Result<()> {
    let profiles = usage_profile_scope_for_slots(config, instances, &state.auth.slots)?;
    credential_scope.profiles.extend(profiles);
    Ok(())
}

fn usage_profile_scope_for_slots(
    config: &AppConfig,
    instances: &[jackin_config::ResolvedInstance],
    slots: &BTreeMap<String, crate::instance::ProvisionedInstanceAuth>,
) -> Result<BTreeSet<UsageProfileSourceProof>> {
    let mut profiles = BTreeSet::new();
    for instance in instances {
        let account = config
            .accounts
            .get(&instance.account_id)
            .ok_or_else(|| anyhow::anyhow!("unknown account {:?}", instance.account_id))?;
        let jackin_config::AccountCredential::Profile { agent, .. } = &account.credential else {
            continue;
        };
        anyhow::ensure!(
            *agent == instance.agent && account.supports_agent(instance.agent),
            "profile account does not match launch instance {:?}",
            instance.config_id,
        );
        let slot = slots.get(&instance.config_id).ok_or_else(|| {
            anyhow::anyhow!(
                "prepared profile slot missing for launch instance {:?}",
                instance.config_id
            )
        })?;
        anyhow::ensure!(
            slot.agent == instance.agent && slot.account_id == instance.account_id,
            "prepared profile slot does not match launch instance {:?}",
            instance.config_id,
        );
        if slot.mode != jackin_config::AuthForwardMode::Sync || !slot.forward_auth {
            continue;
        }
        // The host CLI grant has no transferable profile credential payload.
        // Its presence must never authorize a surface-only profile capability.
        if instance.agent == jackin_core::Agent::Antigravity {
            continue;
        }
        let material = slot.profile_material.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "prepared profile proof missing for launch instance {:?}",
                instance.config_id
            )
        })?;
        anyhow::ensure!(
            material.source.agent == slot.agent
                && is_profile_fingerprint(&material.source.descriptor_fingerprint)
                && is_profile_fingerprint(&material.material_revision),
            "prepared profile proof does not match launch instance {:?}",
            instance.config_id,
        );
        let Some(surface) = HostSurfaceId::from_provider_alias(account.provider.slug()) else {
            continue;
        };
        // Descriptor/provider/selector identity is established by the selected
        // immutable snapshot binding. This opaque digest is never reconstructed
        // from the worker destination or an ambient host path.
        profiles.insert(UsageProfileSourceProof {
            instance_id: instance.config_id.clone(),
            account_id: instance.account_id.clone(),
            surface_id: surface.id().to_owned(),
            source: material.source.clone(),
            material_revision: material.material_revision.clone(),
        });
    }
    Ok(profiles)
}

fn is_profile_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Source proof for the serialized launch config. The account ids are the
/// exact configured selections and are used to filter host bindings before a
/// relay capability allowlist is created.
#[must_use]
pub fn forwarded_sources_from_launch_config(
    state: &crate::instance::RoleState,
    resolved_env: &jackin_env::ResolvedEnv,
    launch_config: &CapsuleConfig,
    credential_scope: &UsageCredentialScope,
) -> ForwardedUsageSources {
    let mut sources = forwarded_sources_from_launch(state, resolved_env);
    sources.credential_scope = credential_scope.clone();
    sources.selected_account_ids = launch_config.accounts.values().cloned().collect();
    for (instance_id, account_id) in &launch_config.accounts {
        if let Some(capability) = launch_config.usage_capabilities.get(instance_id) {
            sources
                .selected_account_surfaces
                .entry(account_id.clone())
                .or_insert_with(|| capability.surface_id.clone());
        }
    }
    sources
}

/// Populate the Capsule launch contract with canonical usage authorities for
/// every admitted instance. An unknown account or provider leaves that entry
/// absent; the Capsule then fails closed for usage refresh instead of falling
/// back to a same-surface account.
pub fn populate_launch_usage_capabilities(config: &AppConfig, launch_config: &mut CapsuleConfig) {
    for instance_id in &launch_config.instances {
        let Some(account_id) = launch_config.accounts.get(instance_id) else {
            continue;
        };
        let Some(account) = config.accounts.get(account_id) else {
            continue;
        };
        // The selected account's configured provider is the canonical usage
        // surface. Do not rediscover profiles here: launch materialization is
        // on the critical path, and profile identity readers may block on a
        // protected keychain. Broker discovery still validates credentials
        // when the relay starts; this contract only carries the already
        // authorized account/provider identity into Capsule.
        let Some(surface) = HostSurfaceId::from_provider_alias(account.provider.slug()) else {
            continue;
        };
        launch_config
            .credential_provider_surfaces
            .insert(instance_id.clone(), surface.id().to_owned());
        launch_config.usage_capabilities.insert(
            instance_id.clone(),
            UsageAccountCapability {
                account_id: account_id.clone(),
                surface_id: surface.id().to_owned(),
            },
        );
    }
}

/// Resolve global discovery and ensure the host broker for one stdio relay.
/// Broker activation failure is returned; a dead fallback client is not a
/// valid production relay authority.
pub async fn prepare_for_stdio_tunnel(launch: UsageRelayLaunch<'_>) -> Result<PreparedUsageRelay> {
    let paths = launch.paths.clone();
    let workspace_name = launch.workspace_name.map(str::to_owned);
    let role_key = launch.role_key.to_owned();
    let forwarded_sources = launch.forwarded_sources;
    let credential_scope = forwarded_sources.credential_scope.clone();
    let context_paths = paths.clone();
    let context_workspace = workspace_name.clone();
    let context_role = role_key.clone();
    let (broker, capabilities, canonical_launch_usage_capabilities, inventory) =
        jackin_telemetry::spawn::joined_blocking(move || {
            prepare_broker_client(
                &paths,
                workspace_name.as_deref(),
                &role_key,
                &forwarded_sources,
            )
        })
        .await
        .context("usage broker preparation task panicked")??;
    let allowed = capabilities.iter().cloned().collect::<BTreeSet<_>>();
    let instance_capabilities =
        canonical_launch_usage_capabilities.for_instances(launch.launch_config, &allowed);
    let persistence_context = inventory.as_ref().map(|inventory| RelayPersistenceContext {
        paths: context_paths,
        workspace: context_workspace,
        role_key: context_role,
        config_generation: inventory.config_generation().to_owned(),
    });
    Ok(PreparedUsageRelay {
        broker,
        capabilities,
        instance_capabilities,
        credential_scope,
        inventory,
        persistence_context,
    })
}

/// Validate the host-only current inventory scope for an immutable container.
/// Callers compare this proof before and after reading its admitted transport.
/// Missing or obsolete launch proof cannot authorize cache membership.
pub fn validated_usage_inventory_config_generation(
    paths: &JackinPaths,
    container: &ContainerHandle,
) -> Result<String> {
    restored_usage_inventory_scope(paths, container).map(|(_, generation)| generation)
}

/// Admit canonical membership only when every row, route and report section
/// equals the immutable host inventory's projection of that publication.
pub fn validate_usage_inventory_projection(
    paths: &JackinPaths,
    container: &ContainerHandle,
    projection: &jackin_protocol::usage_broker::UsageProjectionV2,
) -> Result<String> {
    jackin_protocol::control::UsageAccountMembershipV1::validate_current_projection(projection)
        .map_err(|_| anyhow::anyhow!("usage membership publication invalid"))?;
    let (inventory, generation) = restored_usage_inventory_scope(paths, container)?;
    anyhow::ensure!(
        inventory.projection_is_complete_and_scoped(projection),
        "usage membership publication is incomplete or exceeds admitted inventory scope"
    );
    // Read the existing issuer only. Catalog replacement and this read share
    // its lifecycle boundary; even same-config material replacement advances
    // publication identity. Never activate discovery or request provider work.
    let issuer = UsageBrokerConfig::for_data_dir(paths.data_dir.clone())
        .client().current_projection()
        .map_err(|_| anyhow::anyhow!("current usage membership issuer unavailable"))?;
    anyhow::ensure!(
        inventory.projection_matches_current_issuer(projection, &issuer),
        "usage membership publication is no longer current"
    );
    let (after, after_generation) = restored_usage_inventory_scope(paths, container)?;
    anyhow::ensure!(
        generation == after_generation
            && inventory.authority() == after.authority()
            && inventory.unresolved_grants() == after.unresolved_grants(),
        "usage membership scope changed during issuer admission"
    );
    Ok(generation)
}

fn restored_usage_inventory_scope(
    paths: &JackinPaths,
    container: &ContainerHandle,
) -> Result<(RelayUsageInventory, String)> {
    let name = container.name();
    let manifest = jackin_instance::manifest::InstanceManifest::read(&paths.data_dir.join(name))?;
    anyhow::ensure!(manifest.container_base == name, "usage inventory manifest identity mismatch");
    let saved = persistence::load(paths, name)?
        .ok_or_else(|| anyhow::anyhow!("usage inventory launch proof unavailable; rebuild this Capsule"))?;
    anyhow::ensure!(
        saved.container_id == container.id() && saved.container_name == name,
        "usage inventory immutable container authority mismatch"
    );
    anyhow::ensure!(
        saved.workspace == manifest.workspace_name && saved.role_key == manifest.role_key,
        "usage inventory workspace authority mismatch"
    );
    let selected_account_ids = manifest.admitted_instances.iter()
        .map(|instance| instance.account_id.clone()).collect::<BTreeSet<_>>();
    let inventory = RelayUsageInventory::restore(
        paths, manifest.workspace_name.as_deref(), &selected_account_ids,
        &saved.config_generation, &saved.inventory_accounts, &saved.unresolved_grants,
    )?;
    Ok((inventory, saved.config_generation))
}

/// Rebuild read authority and restore only the immutable proof bound to this container.
pub(crate) async fn prepare_for_reconnect(
    paths: &JackinPaths,
    container_name: &str,
    container: &ContainerHandle,
) -> Result<PreparedUsageRelay> {
    if paths.test_layout {
        return Ok(PreparedUsageRelay {
            broker: UsageBrokerConfig::for_data_dir(paths.data_dir.clone()).client(),
            capabilities: Vec::new(),
            instance_capabilities: BTreeMap::new(),
            credential_scope: UsageCredentialScope::default(),
            inventory: None,
            persistence_context: None,
        });
    }
    let paths = paths.clone();
    let container_name = container_name.to_owned();
    let container_id = container.id().to_owned();
    jackin_telemetry::spawn::joined_blocking(move || {
        let manifest = jackin_instance::manifest::InstanceManifest::read(&paths.data_dir.join(&container_name))?;
        anyhow::ensure!(manifest.container_base == container_name, "usage relay manifest container identity mismatch");
        let selected_account_ids = manifest.admitted_instances.iter().map(|instance| instance.account_id.clone()).collect::<BTreeSet<_>>();
        let saved = persistence::load(&paths, &container_name)?;
        let broker_config = UsageBrokerConfig::for_data_dir(paths.data_dir.clone());
        let broker = broker_config.client();
        if let Some(saved) = saved {
            anyhow::ensure!(saved.container_id == container_id && saved.container_name == container_name, "usage relay container authority mismatch");
            anyhow::ensure!(saved.workspace == manifest.workspace_name && saved.role_key == manifest.role_key, "usage relay workspace authority mismatch");
            let inventory = RelayUsageInventory::restore(&paths, manifest.workspace_name.as_deref(), &selected_account_ids, &saved.config_generation, &saved.inventory_accounts, &saved.unresolved_grants)?;
            let broker = match broker.current_projection() {
                Ok(_) => broker,
                Err(error) if error.kind == UsageCoordinationErrorKind::Unavailable => {
                    // Only broker activation may discover current host sources. It
                    // cannot replace the saved launch proof or grant map.
                    let resolver = Arc::new(CachedProviderCredentialResolver::new(RuntimeSecretSource));
                    let scope = jackin_usage::host::UsageDiscoveryScope::HostDesktop { config_root: paths.config_dir.clone(), operator_home: paths.home_dir.clone() };
                    let catalog = discover_usage_sources(&scope, resolver.as_ref()).map_err(|_| anyhow::anyhow!("usage broker discovery unavailable"))?;
                    let discovery = validate_usage_sources(catalog, resolver.as_ref());
                    anyhow::ensure!(discovery.config_generation.as_deref() == Some(saved.config_generation.as_str()), "usage relay launch authority changed; rebuild this Capsule");
                    jackin_usage::host::ensure_usage_broker(broker_config, scope, discovery, resolver).map_err(|_| anyhow::anyhow!("usage broker activation unavailable"))?.client
                }
                Err(_) => anyhow::bail!("usage broker publication unavailable"),
            };
            return Ok(PreparedUsageRelay { broker, capabilities: saved.capabilities, instance_capabilities: saved.instance_capabilities, credential_scope: saved.credential_scope, inventory: Some(inventory), persistence_context: None });
        }
        jackin_diagnostics::emit_operator_notice("Capsule usage refresh proof is missing; inventory is read-only. Rebuild this Capsule to restore account refresh.");
        let resolver = Arc::new(CachedProviderCredentialResolver::new(RuntimeSecretSource));
        let scope = jackin_usage::host::UsageDiscoveryScope::HostDesktop { config_root: paths.config_dir.clone(), operator_home: paths.home_dir.clone() };
        let catalog = discover_usage_sources(&scope, resolver.as_ref()).map_err(|_| anyhow::anyhow!("usage inventory discovery unavailable"))?;
        let discovery = validate_usage_sources(catalog, resolver.as_ref());
        let inventory = RelayUsageInventory::prepare(&paths, manifest.workspace_name.as_deref(), &selected_account_ids, &discovery)?;
        let broker = jackin_usage::host::ensure_usage_broker(broker_config, scope, discovery, resolver).map_err(|_| anyhow::anyhow!("usage broker activation unavailable"))?.client;
        Ok(PreparedUsageRelay { broker, capabilities: Vec::new(), instance_capabilities: BTreeMap::new(), credential_scope: UsageCredentialScope::default(), inventory: Some(inventory), persistence_context: None })
    }).await.context("reconnect usage preparation task panicked")?
}

impl PreparedUsageRelay {
    pub(crate) fn persist_for_container(&self, container: &ContainerHandle) -> Result<()> {
        let Some(context) = &self.persistence_context else {
            return Ok(());
        };
        let inventory_accounts = self
            .inventory
            .as_ref()
            .map(RelayUsageInventory::authority)
            .unwrap_or_default();
        persistence::save(
            &context.paths,
            container.name(),
            container.id(),
            context.workspace.as_deref(),
            &context.role_key,
            &context.config_generation,
            &self.capabilities,
            &self.credential_scope,
            &self.instance_capabilities,
            &inventory_accounts,
            self.inventory
                .as_ref()
                .map(RelayUsageInventory::unresolved_grants)
                .unwrap_or_default(),
        )
    }

    pub(crate) fn apply_to_launch_config(&self, launch_config: &mut CapsuleConfig) -> Result<()> {
        for instance_id in &launch_config.instances {
            if let Some(capability) = self.instance_capabilities.get(instance_id) {
                launch_config
                    .usage_capabilities
                    .insert(instance_id.clone(), capability.clone());
            } else {
                launch_config.usage_capabilities.remove(instance_id);
            }
        }
        ensure_distinct_usage_unix_identities(launch_config)
    }
}

/// Start the production Docker stdio tunnel after the Capsule is running.
pub fn start_docker_tunnel(
    container: &ContainerHandle,
    prepared: PreparedUsageRelay,
) -> Result<UsageRelayGuard> {
    start_docker_tunnel_with_inventory(
        container,
        prepared.broker,
        prepared.capabilities,
        prepared.credential_scope,
        prepared.inventory,
        &[
            jackin_core::container_paths::CAPSULE_BIN.to_owned(),
            "usage-relay-proxy".to_owned(),
        ],
        prepared.instance_capabilities,
    )
}

const CAPSULE_SUPERVISOR_USER: &str = "0:0";

/// Start the Apple Container stdio tunnel after the Capsule is running.
pub fn start_apple_tunnel(
    container_name: &str,
    prepared: PreparedUsageRelay,
) -> Result<UsageRelayGuard> {
    start_tunnel_with_command(
        prepared.broker,
        prepared.capabilities,
        prepared.credential_scope,
        prepared.inventory,
        "container",
        apple_tunnel_args(container_name),
        prepared.instance_capabilities,
    )
}

fn apple_tunnel_args(container_name: &str) -> Vec<String> {
    vec![
        "exec".to_owned(),
        "-i".to_owned(),
        "--user".to_owned(),
        CAPSULE_SUPERVISOR_USER.to_owned(),
        container_name.to_owned(),
        jackin_core::container_paths::CAPSULE_BIN.to_owned(),
        "usage-relay-proxy".to_owned(),
    ]
}

/// Test seam for a real container proxy command using production tunnel framing.
#[doc(hidden)]
pub fn start_docker_tunnel_with_command(
    container: &ContainerHandle,
    broker: UsageBrokerClient,
    capabilities: Vec<UsageAccountCapability>,
    credential_scope: UsageCredentialScope,
    proxy_command: &[String],
    instance_capabilities: BTreeMap<String, UsageAccountCapability>,
) -> Result<UsageRelayGuard> {
    start_docker_tunnel_with_inventory(
        container,
        broker,
        capabilities,
        credential_scope,
        None,
        proxy_command,
        instance_capabilities,
    )
}

fn start_docker_tunnel_with_inventory(
    container: &ContainerHandle,
    broker: UsageBrokerClient,
    capabilities: Vec<UsageAccountCapability>,
    credential_scope: UsageCredentialScope,
    inventory: Option<RelayUsageInventory>,
    proxy_command: &[String],
    instance_capabilities: BTreeMap<String, UsageAccountCapability>,
) -> Result<UsageRelayGuard> {
    let args = docker_tunnel_args(container, proxy_command);
    start_tunnel_with_command(
        broker,
        capabilities,
        credential_scope,
        inventory,
        "docker",
        args,
        instance_capabilities,
    )
}

fn docker_tunnel_args(container: &ContainerHandle, proxy_command: &[String]) -> Vec<String> {
    let mut args = vec!["exec".to_owned(), "-i".to_owned()];
    args.push(container.id().to_owned());
    args.extend_from_slice(proxy_command);
    args
}

fn start_tunnel_with_command(
    broker: UsageBrokerClient,
    capabilities: Vec<UsageAccountCapability>,
    credential_scope: UsageCredentialScope,
    inventory: Option<RelayUsageInventory>,
    program: &str,
    args: Vec<String>,
    instance_capabilities: BTreeMap<String, UsageAccountCapability>,
) -> Result<UsageRelayGuard> {
    if capabilities.is_empty() && inventory.is_none() {
        return Ok(UsageRelayGuard {
            task: None,
            shutdown: None,
        });
    }
    let request = jackin_process::ExecRequest::new(program, args)
        .stdin_mode(jackin_process::StdioMode::Capture)
        .stdout_mode(jackin_process::StdioMode::Capture)
        .stderr_mode(jackin_process::StdioMode::Inherit);
    start_tunnel_process(request, broker, capabilities, credential_scope, inventory, instance_capabilities)
}

fn start_tunnel_process(
    request: jackin_process::ExecRequest,
    broker: UsageBrokerClient,
    capabilities: Vec<UsageAccountCapability>,
    credential_scope: UsageCredentialScope,
    inventory: Option<RelayUsageInventory>,
    instance_capabilities: BTreeMap<String, UsageAccountCapability>,
) -> Result<UsageRelayGuard> {
    let (operation, mut child) = crate::process_telemetry::spawn_async(&request)
        .context("starting scoped usage stdio tunnel")?;
    let reader = child
        .stdout
        .take()
        .context("usage relay stdout was unavailable")?;
    let writer = child
        .stdin
        .take()
        .context("usage relay stdin was unavailable")?;
    let allowlist = UsageCapabilitySet::new(capabilities);
    let (shutdown, mut shutdown_rx) = oneshot::channel();
    let task = jackin_telemetry::spawn::spawn_stream("usage_relay.tunnel", async move {
        let relay_result = tokio::select! {
            result = serve_stdio_tunnel(reader, writer, broker, allowlist, credential_scope, inventory, instance_capabilities) => result,
            _ = &mut shutdown_rx => Ok(()),
        };
        let status =
            if let Ok(status) = tokio::time::timeout(TUNNEL_SHUTDOWN_TIMEOUT, child.wait()).await {
                status
            } else {
                drop(child.start_kill());
                child.wait().await
            };
        match status {
            Ok(status) => operation.complete_status(status),
            Err(_) => {
                operation.complete_failure(jackin_telemetry::schema::enums::ErrorType::IoError);
            }
        }
        if relay_result.is_err() {
            let _recorded = jackin_telemetry::record_error(
                jackin_telemetry::schema::enums::ErrorType::RpcError,
            );
        }
    });
    Ok(UsageRelayGuard {
        task: Some(task),
        shutdown: Some(shutdown),
    })
}

fn prepare_broker_client(
    paths: &JackinPaths,
    workspace_name: Option<&str>,
    role_key: &str,
    forwarded_sources: &ForwardedUsageSources,
) -> Result<(
    UsageBrokerClient,
    Vec<UsageAccountCapability>,
    CanonicalLaunchUsageCapabilities,
    Option<RelayUsageInventory>,
)> {
    let broker_config = UsageBrokerConfig::for_data_dir(paths.data_dir.clone());
    let fallback = broker_config.client();
    if paths.test_layout {
        return Ok((
            fallback,
            Vec::new(),
            CanonicalLaunchUsageCapabilities::default(),
            None,
        ));
    }
    let resolver = Arc::new(CachedProviderCredentialResolver::new(RuntimeSecretSource));
    let scope = jackin_usage::host::UsageDiscoveryScope::HostDesktop {
        config_root: paths.config_dir.clone(),
        operator_home: paths.home_dir.clone(),
    };
    let catalog = discover_usage_sources(&scope, resolver.as_ref())
        .map_err(|error| anyhow::anyhow!("usage account discovery failed: {error}"))?;
    let discovery = validate_usage_sources(catalog, resolver.as_ref());
    let inventory = RelayUsageInventory::prepare(
        paths,
        workspace_name,
        &forwarded_sources.selected_account_ids,
        &discovery,
    )?;
    let scope_label = workspace_name.map_or_else(
        || format!("role {role_key}"),
        |workspace| format!("workspace {workspace} role {role_key}"),
    );
    let capabilities = forwarded_usage_capabilities(&discovery, &scope_label, forwarded_sources);
    let allowed = capabilities.iter().cloned().collect::<BTreeSet<_>>();
    let canonical_launch_usage_capabilities =
        canonical_capabilities_for_launch(&discovery, forwarded_sources, &allowed);
    let client = jackin_usage::host::ensure_usage_broker(broker_config, scope, discovery, resolver)
        .map(|handle| handle.client)
        .map_err(|error| anyhow::anyhow!("usage broker activation failed: {}", error.message))?;
    if capabilities.is_empty() {
        return Ok((
            client,
            capabilities,
            CanonicalLaunchUsageCapabilities::default(),
            Some(inventory),
        ));
    }
    Ok((
        client,
        capabilities,
        canonical_launch_usage_capabilities,
        Some(inventory),
    ))
}

fn canonical_capabilities_for_launch(
    discovery: &jackin_usage::host::ValidatedUsageDiscovery,
    forwarded_sources: &ForwardedUsageSources,
    allowed: &BTreeSet<UsageAccountCapability>,
) -> CanonicalLaunchUsageCapabilities {
    CanonicalLaunchUsageCapabilities {
        by_account_surface: forwarded_sources
            .selected_account_surfaces
            .iter()
            .filter_map(|(account_id, surface_id)| {
                let capability = usage_capability_for_selected_account_with_sources(
                    discovery,
                    account_id,
                    surface_id,
                    Some(forwarded_sources),
                )?;
                allowed
                    .contains(&capability)
                    .then_some(((account_id.clone(), surface_id.clone()), capability))
            })
            .collect(),
    }
}

#[cfg(test)]
async fn dispatch(
    operation: UsageBrokerOperation,
    broker: UsageBrokerClient,
    allowlist: UsageCapabilitySet,
    credential_scope: UsageCredentialScope,
    inventory: Option<RelayUsageInventory>,
    instance_capabilities: BTreeMap<String, UsageAccountCapability>,
    instance_id: Option<String>,
) -> UsageBrokerResponse {
    dispatch_with_admission(
        operation,
        broker,
        allowlist,
        credential_scope,
        inventory,
        None,
        Some(tokio::time::Instant::now() + TUNNEL_REQUEST_TIMEOUT),
        instance_capabilities,
        instance_id,
    )
    .await
}

/// Dispatch one admitted request. Inventory validation moves the permit into
/// each blocking filesystem job, while account IPC remains cancellation-safe
/// async socket work.
#[expect(clippy::too_many_arguments, reason = "relay dispatch binds transport admission and independent immutable instance authority")]
async fn dispatch_with_admission(
    operation: UsageBrokerOperation,
    broker: UsageBrokerClient,
    allowlist: UsageCapabilitySet,
    credential_scope: UsageCredentialScope,
    inventory: Option<RelayUsageInventory>,
    permit: Option<OwnedSemaphorePermit>,
    deadline: Option<tokio::time::Instant>,
    instance_capabilities: BTreeMap<String, UsageAccountCapability>,
    instance_id: Option<String>,
) -> UsageBrokerResponse {
    if deadline.is_none_or(|deadline| tokio::time::Instant::now() >= deadline) {
        return error_response(UsageCoordinationErrorKind::Unavailable);
    }
    if matches!(operation, UsageBrokerOperation::CurrentProjectionForSurface) {
        if instance_id.is_some() {
            return error_response(UsageCoordinationErrorKind::Unauthorized);
        }
        let Some(inventory) = inventory else {
            return error_response(UsageCoordinationErrorKind::Unauthorized);
        };
        let result = match deadline {
            Some(deadline) => match tokio::time::timeout_at(
                deadline,
                inventory.read_async(&broker, permit, Some(deadline)),
            )
            .await
            {
                Ok(result) => result,
                Err(_) => Err(UsageCoordinationError {
                    kind: UsageCoordinationErrorKind::Unavailable,
                    message: "usage inventory request expired or was cancelled".to_owned(),
                }),
            },
            None => inventory.read_async(&broker, permit, None).await,
        };
        return match result {
            Ok(projection) => UsageBrokerResponse::Projection {
                projection: Box::new(projection),
            },
            Err(error) => UsageBrokerResponse::Error { error },
        };
    }
    let (capability, operation_instance) = match &operation {
        UsageBrokerOperation::CurrentForCapability { capability, instance_id }
        | UsageBrokerOperation::RefreshForCapability { capability, instance_id, .. }
        | UsageBrokerOperation::JoinForCapability { capability, instance_id, .. } =>
            (capability, Some(instance_id.as_str())),
        UsageBrokerOperation::Current { capability }
        | UsageBrokerOperation::Refresh { capability, .. }
        | UsageBrokerOperation::Join { capability, .. } => (capability, None),
        _ => return error_response(UsageCoordinationErrorKind::Unauthorized),
    };
    let Some(instance_id) = instance_id.as_deref() else {
        return error_response(UsageCoordinationErrorKind::Unauthorized);
    };
    if operation_instance.is_some_and(|requested| requested != instance_id)
        || allowlist.authorize(capability).is_err()
    {
        return error_response(UsageCoordinationErrorKind::Unauthorized);
    }
    let credential_scope = match instance_scope::for_instance(
        instance_id, capability, &instance_capabilities, &credential_scope,
    ) {
        Ok(scope) => scope,
        Err(error) => return UsageBrokerResponse::Error { error },
    };
    let operation = match operation {
        UsageBrokerOperation::CurrentForCapability { capability, .. } =>
            UsageBrokerOperation::Current { capability },
        UsageBrokerOperation::RefreshForCapability { capability, observed_generation, force, .. } =>
            UsageBrokerOperation::Refresh { capability, observed_generation, force },
        UsageBrokerOperation::JoinForCapability { capability, generation, timeout_ms, .. } =>
            UsageBrokerOperation::Join { capability, generation, timeout_ms },
        operation => operation,
    };
    let Some(deadline) = deadline else {
        drop(permit);
        return error_response(UsageCoordinationErrorKind::Unavailable);
    };
    // The broker's account exchange is async and cancellation-safe. The
    // request owner can therefore close its Unix socket when the tunnel
    // expires or loses its writer. Keep the admission permit owned by this
    // request until that cancellable exchange completes; aborting the task
    // then releases capacity at the same point its socket is closed.
    let _permit = permit;
    if tokio::time::Instant::now() >= deadline {
        return error_response(UsageCoordinationErrorKind::Unavailable);
    }
    match tokio::time::timeout_at(
        deadline,
        broker.execute_scoped_async(operation, credential_scope, deadline),
    )
    .await
    {
        Ok(Ok(state)) => UsageBrokerResponse::State {
            state: Box::new(state),
        },
        Ok(Err(error)) => UsageBrokerResponse::Error { error },
        Err(_) => error_response(UsageCoordinationErrorKind::Unavailable),
    }
}

#[expect(clippy::too_many_arguments, reason = "relay dispatch binds transport admission and independent immutable instance authority")]
async fn serve_stdio_tunnel<R, W>(
    reader: R,
    writer: W,
    broker: UsageBrokerClient,
    allowlist: UsageCapabilitySet,
    credential_scope: UsageCredentialScope,
    inventory: Option<RelayUsageInventory>,
    instance_capabilities: BTreeMap<String, UsageAccountCapability>,
) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (responses, mut response_rx) =
        mpsc::channel::<(tokio::time::Instant, UsageRelayTunnelResponse)>(TUNNEL_REQUEST_CAPACITY);
    let mut writer = AbortTaskOnDrop(jackin_telemetry::spawn::spawn_stream(
        "usage_relay.tunnel_writer",
        async move {
            let mut writer = writer;
            while let Some((deadline, response)) = response_rx.recv().await {
                if tokio::time::Instant::now() >= deadline {
                    continue;
                }
                tokio::time::timeout_at(deadline, write_async_frame(&mut writer, &response))
                    .await
                    .context("usage relay tunnel response deadline expired")??;
            }
            Ok::<(), anyhow::Error>(())
        },
    ));
    let admission = Arc::new(Semaphore::new(TUNNEL_REQUEST_CAPACITY));
    let mut requests = JoinSet::<u64>::new();
    let mut active_requests = BTreeMap::<u64, tokio::task::AbortHandle>::new();
    let mut reader = BufReader::new(reader);
    // Keep one frame future pinned across request-task completions. A
    // newline frame read consumes partial bytes before it yields; recreating
    // that future when another select branch wins would discard the prefix.
    let mut next_frame =
        Box::pin(read_async_frame_with_deadline::<_, UsageRelayTunnelMessage>(&mut reader));
    loop {
        while let Some(joined) = requests.try_join_next() {
            match joined {
                Ok(request_id) => {
                    active_requests.remove(&request_id);
                }
                Err(error) if error.is_cancelled() => {}
                Err(error) => {
                    requests.shutdown().await;
                    return Err(anyhow::anyhow!("usage relay request task failed: {error}"));
                }
            }
        }
        tokio::select! {
            joined = requests.join_next(), if !requests.is_empty() => {
                if let Some(joined) = joined {
                    match joined {
                        Ok(request_id) => {
                            active_requests.remove(&request_id);
                        }
                        Err(error) if error.is_cancelled() => {}
                        Err(error) => {
                            requests.shutdown().await;
                            return Err(anyhow::anyhow!("usage relay request task failed: {error}"));
                        }
                    }
                }
            }
            writer_result = &mut writer.0 => {
                requests.shutdown().await;
                active_requests.clear();
                return match writer_result {
                    Ok(result) => result.context("usage relay tunnel writer exited"),
                    Err(error) => Err(anyhow::anyhow!("usage relay tunnel writer task failed: {error}")),
                };
            }
            frame = &mut next_frame => {
                let (message, frame_deadline) = match frame {
                    Ok(frame) => frame,
                    Err(error) => {
                        requests.shutdown().await;
                        return Err(error);
                    }
                };
                drop(next_frame);
                next_frame =
                    Box::pin(read_async_frame_with_deadline::<_, UsageRelayTunnelMessage>(&mut reader));
                let tunneled = match message {
                    UsageRelayTunnelMessage::Request { request } => *request,
                    UsageRelayTunnelMessage::Cancel { request_id } => {
                        if let Some(task) = active_requests.remove(&request_id) {
                            task.abort();
                        }
                        continue;
                    }
                };
                let Some(wire_deadline) = tunnel_request_deadline(tunneled.expires_at_unix_ms) else {
                    // The guest already timed out this request. It must not
                    // reach authorization, broker IPC, or provider work.
                    continue;
                };
                let deadline = wire_deadline.min(frame_deadline);
                if active_requests.contains_key(&tunneled.request_id) {
                    // A duplicate live identifier cannot be routed safely:
                    // emitting a second response would race the original
                    // guest waiter. Preserve the original owner and discard
                    // the duplicate before admission or registry mutation.
                    continue;
                }
                if requests.len() >= TUNNEL_REQUEST_CAPACITY {
                    let response = UsageRelayTunnelResponse {
                        request_id: tunneled.request_id,
                        response: error_response(UsageCoordinationErrorKind::Unavailable),
                    };
                    if responses.try_send((deadline, response)).is_err() && responses.is_closed() {
                        requests.shutdown().await;
                        return Err(anyhow::anyhow!("usage relay response writer closed"));
                    }
                    continue;
                }
                let permit = match Arc::clone(&admission).try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => {
                        let response = UsageRelayTunnelResponse {
                            request_id: tunneled.request_id,
                            response: error_response(UsageCoordinationErrorKind::Unavailable),
                        };
                        if responses.try_send((deadline, response)).is_err() && responses.is_closed() {
                            requests.shutdown().await;
                            return Err(anyhow::anyhow!("usage relay response writer closed"));
                        }
                        continue;
                    }
                };
                if tokio::time::Instant::now() >= deadline {
                    drop(permit);
                    continue;
                }
                let broker = broker.clone();
                let allowlist = allowlist.clone();
                let credential_scope = credential_scope.clone();
                let inventory = inventory.clone();
                let instance_capabilities = instance_capabilities.clone();
                let responses = responses.clone();
                let request_id = tunneled.request_id;
                let task = requests.spawn(async move {
                    let response = if tunneled.request.protocol_version != USAGE_BROKER_PROTOCOL_VERSION
                        || tunneled.request.build_id != env!("CARGO_PKG_VERSION")
                    {
                        drop(permit);
                        error_response(UsageCoordinationErrorKind::ProtocolMismatch)
                    } else {
                        match tokio::time::timeout_at(
                            deadline,
                            dispatch_with_admission(
                                tunneled.request.operation,
                                broker,
                                allowlist,
                                credential_scope,
                                inventory,
                                Some(permit),
                                Some(deadline),
                                instance_capabilities,
                                tunneled.instance_id,
                            ),
                        )
                        .await
                        {
                            Ok(response) => response,
                            Err(_) => error_response(UsageCoordinationErrorKind::Unavailable),
                        }
                    };
                    drop(
                        tokio::time::timeout_at(
                            deadline,
                            responses.send((
                                deadline,
                                UsageRelayTunnelResponse {
                                    request_id: tunneled.request_id,
                                    response,
                                },
                            )),
                        )
                        .await,
                    );
                    request_id
                });
                active_requests.insert(request_id, task);
            }
        }
    }
}

/// Wait for the first byte so an idle, healthy tunnel can remain open. Once a
/// frame starts, bound the rest of its newline-delimited body. The absolute
/// request expiry inside the decoded envelope is applied separately to broker
/// admission and response delivery.
async fn read_async_frame_with_deadline<R, T>(reader: &mut R) -> Result<(T, tokio::time::Instant)>
where
    R: AsyncBufRead + Unpin,
    T: serde::de::DeserializeOwned,
{
    anyhow::ensure!(
        !reader.fill_buf().await?.is_empty(),
        "usage relay tunnel reached EOF"
    );
    let deadline = tokio::time::Instant::now() + TUNNEL_REQUEST_TIMEOUT;
    let value = tokio::time::timeout_at(deadline, read_async_frame(reader))
        .await
        .context("usage relay tunnel frame read deadline expired")??;
    Ok((value, deadline))
}

fn tunnel_request_deadline(expires_at_unix_ms: u64) -> Option<tokio::time::Instant> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    let expires = std::time::Duration::from_millis(expires_at_unix_ms);
    let remaining = expires.checked_sub(now)?;
    Some(tokio::time::Instant::now() + remaining.min(TUNNEL_REQUEST_TIMEOUT))
}

async fn read_async_frame<R, T>(reader: &mut R) -> Result<T>
where
    R: AsyncBufRead + Unpin,
    T: serde::de::DeserializeOwned,
{
    let mut bytes = Vec::new();
    let read = reader
        .take(u64::try_from(USAGE_BROKER_MAX_FRAME_BYTES).unwrap_or(u64::MAX) + 1)
        .read_until(b'\n', &mut bytes)
        .await?;
    anyhow::ensure!(
        read > 0 && read <= USAGE_BROKER_MAX_FRAME_BYTES && bytes.last() == Some(&b'\n'),
        "usage relay tunnel frame is invalid"
    );
    bytes.pop();
    serde_json::from_slice(&bytes).context("decoding usage relay tunnel frame")
}

async fn write_async_frame<W, T>(writer: &mut W, value: &T) -> Result<()>
where
    W: AsyncWrite + Unpin,
    T: serde::Serialize,
{
    let mut bytes = serde_json::to_vec(value)?;
    anyhow::ensure!(
        bytes.len() < USAGE_BROKER_MAX_FRAME_BYTES,
        "usage relay tunnel frame is too large"
    );
    bytes.push(b'\n');
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

fn error_response(kind: UsageCoordinationErrorKind) -> UsageBrokerResponse {
    let message = match kind {
        UsageCoordinationErrorKind::Unauthorized => "usage account capability is not authorized",
        UsageCoordinationErrorKind::ProtocolMismatch => "usage relay protocol mismatch",
        _ => "usage broker is unavailable",
    };
    UsageBrokerResponse::Error {
        error: UsageCoordinationError {
            kind,
            message: message.to_owned(),
        },
    }
}

#[cfg(test)]
mod tests;
