// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Per-container allowlisted relay to the host-only usage broker.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result};
use jackin_config::AppConfig;
use jackin_core::{JackinPaths, UsageCredentialEnvName, WorkspaceName};
use jackin_protocol::CapsuleConfig;
use jackin_protocol::usage_broker::{
    USAGE_BROKER_MAX_FRAME_BYTES, USAGE_BROKER_PROTOCOL_VERSION, UsageAccountCapability,
    UsageBrokerOperation, UsageBrokerResponse, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageRelayTunnelRequest, UsageRelayTunnelResponse,
};
use jackin_usage::coordinator::UsageCapabilitySet;
use jackin_usage::host::{
    CachedProviderCredentialResolver, ForwardedUsageSources, HostSurfaceId,
    ProviderCredentialSecretOutcome, ProviderCredentialSecretResolution,
    ProviderCredentialSecretSource, UsageBrokerClient, UsageBrokerConfig, discover_usage_sources,
    forwarded_usage_capabilities, usage_capability_for_selected_account, validate_usage_sources,
};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt as _, AsyncRead, AsyncReadExt as _, AsyncWrite,
    AsyncWriteExt as _, BufReader,
};
use tokio::sync::{mpsc, oneshot};

const TUNNEL_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

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

/// Resolved Capsule launch membership used by usage presentation.
///
/// This is derived only from the host-validated Capsule configuration. It is
/// intentionally a closed, deduplicated agent list: usage discovery may enrich
/// an agent with a forwarded canonical account, but global host discovery or a
/// capability alone cannot create a Capsule row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLaunchUsageInventory {
    /// Instance config IDs in deterministic launch-config order.
    pub instances: Vec<String>,
}

/// Project the resolved launch configuration into the Capsule usage boundary.
#[must_use]
pub fn resolved_launch_usage_inventory(config: &CapsuleConfig) -> ResolvedLaunchUsageInventory {
    let mut instances = config.instances.clone();
    instances.sort();
    instances.dedup();
    ResolvedLaunchUsageInventory { instances }
}

/// Session-lifetime relay ownership. Drop revokes the socket task.
pub struct UsageRelayGuard {
    task: Option<tokio::task::JoinHandle<()>>,
    shutdown: Option<oneshot::Sender<()>>,
}

struct AbortTaskOnDrop<T>(tokio::task::JoinHandle<T>);

impl<T> AbortTaskOnDrop<T> {
    fn is_finished(&self) -> bool {
        self.0.is_finished()
    }
}

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
    canonical_launch_usage_capabilities: CanonicalLaunchUsageCapabilities,
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
    state: &crate::instance::RoleState,
    resolved_env: &jackin_env::ResolvedEnv,
) -> ForwardedUsageSources {
    let profile_surface_ids = state
        .auth_outcomes
        .iter()
        .filter(|(_, outcome)| **outcome == crate::instance::AuthProvisionOutcome::Synced)
        .map(|(agent, _)| HostSurfaceId::from_agent(*agent).id().to_owned())
        .collect();
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
        profile_surface_ids,
        env_keys,
    }
}

/// Source proof for the serialized launch config. The account ids are the
/// exact configured selections and are used to filter host bindings before a
/// relay capability allowlist is created.
#[must_use]
pub fn forwarded_sources_from_launch_config(
    state: &crate::instance::RoleState,
    resolved_env: &jackin_env::ResolvedEnv,
    launch_config: &CapsuleConfig,
) -> ForwardedUsageSources {
    let mut sources = forwarded_sources_from_launch(state, resolved_env);
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
    let (broker, capabilities, canonical_launch_usage_capabilities) =
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
    Ok(PreparedUsageRelay {
        broker,
        capabilities,
        canonical_launch_usage_capabilities,
    })
}

impl PreparedUsageRelay {
    pub(crate) fn apply_to_launch_config(&self, launch_config: &mut CapsuleConfig) -> Result<()> {
        self.canonical_launch_usage_capabilities
            .apply_to_launch_config(launch_config)
    }
}

/// Start the production Docker stdio tunnel after the Capsule is running.
pub fn start_docker_tunnel(
    container_name: &str,
    prepared: PreparedUsageRelay,
) -> Result<UsageRelayGuard> {
    start_docker_tunnel_with_command(
        container_name,
        prepared.broker,
        prepared.capabilities,
        &[
            jackin_core::container_paths::CAPSULE_BIN.to_owned(),
            "usage-relay-proxy".to_owned(),
        ],
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
        "container",
        apple_tunnel_args(container_name),
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
    container_name: &str,
    broker: UsageBrokerClient,
    capabilities: Vec<UsageAccountCapability>,
    proxy_command: &[String],
) -> Result<UsageRelayGuard> {
    let mut args = vec!["exec".to_owned(), "-i".to_owned()];
    args.push(container_name.to_owned());
    args.extend_from_slice(proxy_command);
    start_tunnel_with_command(broker, capabilities, "docker", args)
}

fn start_tunnel_with_command(
    broker: UsageBrokerClient,
    capabilities: Vec<UsageAccountCapability>,
    program: &str,
    args: Vec<String>,
) -> Result<UsageRelayGuard> {
    if capabilities.is_empty() {
        return Ok(UsageRelayGuard {
            task: None,
            shutdown: None,
        });
    }
    let request = jackin_process::ExecRequest::new(program, args)
        .stdin_mode(jackin_process::StdioMode::Capture)
        .stdout_mode(jackin_process::StdioMode::Capture)
        .stderr_mode(jackin_process::StdioMode::Inherit);
    start_tunnel_process(request, broker, capabilities)
}

fn start_tunnel_process(
    request: jackin_process::ExecRequest,
    broker: UsageBrokerClient,
    capabilities: Vec<UsageAccountCapability>,
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
            result = serve_stdio_tunnel(reader, writer, broker, allowlist) => result,
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
)> {
    let broker_config = UsageBrokerConfig::for_data_dir(paths.data_dir.clone());
    let fallback = broker_config.client();
    if paths.test_layout {
        return Ok((
            fallback,
            Vec::new(),
            CanonicalLaunchUsageCapabilities::default(),
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
        ));
    }
    Ok((client, capabilities, canonical_launch_usage_capabilities))
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
                let capability =
                    usage_capability_for_selected_account(discovery, account_id, surface_id)?;
                allowed
                    .contains(&capability)
                    .then_some(((account_id.clone(), surface_id.clone()), capability))
            })
            .collect(),
    }
}

async fn dispatch(
    operation: UsageBrokerOperation,
    broker: UsageBrokerClient,
    allowlist: UsageCapabilitySet,
) -> UsageBrokerResponse {
    let authorized = match operation {
        UsageBrokerOperation::CurrentForCapability { capability } => allowlist
            .authorize(&capability)
            .map(|()| UsageBrokerOperation::Current { capability }),
        UsageBrokerOperation::RefreshForCapability {
            capability,
            observed_generation,
            force,
        } => allowlist
            .authorize(&capability)
            .map(|()| UsageBrokerOperation::Refresh {
                capability,
                observed_generation,
                force,
            }),
        UsageBrokerOperation::JoinForCapability {
            capability,
            generation,
            timeout_ms,
        } => allowlist
            .authorize(&capability)
            .map(|()| UsageBrokerOperation::Join {
                capability,
                generation,
                timeout_ms,
            }),
        UsageBrokerOperation::CurrentProjection
        | UsageBrokerOperation::RequestRefresh { .. }
        | UsageBrokerOperation::JoinPublication { .. }
        | UsageBrokerOperation::ReconcileCatalog { .. }
        | UsageBrokerOperation::CurrentProjectionForSurface
        | UsageBrokerOperation::RequestRefreshForSurface { .. }
        | UsageBrokerOperation::JoinPublicationForSurface { .. } => Err(UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unauthorized,
            message: "canonical projection requires a scoped relay operation".to_owned(),
        }),
        operation @ (UsageBrokerOperation::Current { .. }
        | UsageBrokerOperation::Refresh { .. }
        | UsageBrokerOperation::Join { .. }) => {
            let (UsageBrokerOperation::Current { capability }
            | UsageBrokerOperation::Refresh { capability, .. }
            | UsageBrokerOperation::Join { capability, .. }) = &operation
            else {
                unreachable!()
            };
            allowlist.authorize(capability).map(|()| operation)
        }
    };
    let operation = match authorized {
        Ok(operation) => operation,
        Err(error) => return UsageBrokerResponse::Error { error },
    };
    match jackin_telemetry::spawn::joined_blocking(move || broker.execute(operation)).await {
        Ok(Ok(state)) => UsageBrokerResponse::State {
            state: Box::new(state),
        },
        Ok(Err(error)) => UsageBrokerResponse::Error { error },
        Err(_) => error_response(UsageCoordinationErrorKind::Unavailable),
    }
}

async fn serve_stdio_tunnel<R, W>(
    reader: R,
    writer: W,
    broker: UsageBrokerClient,
    allowlist: UsageCapabilitySet,
) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (responses, mut response_rx) = mpsc::channel::<UsageRelayTunnelResponse>(128);
    let writer = AbortTaskOnDrop(jackin_telemetry::spawn::spawn_stream(
        "usage_relay.tunnel_writer",
        async move {
            let mut writer = writer;
            while let Some(response) = response_rx.recv().await {
                if write_async_frame(&mut writer, &response).await.is_err() {
                    return;
                }
            }
        },
    ));
    let mut reader = BufReader::new(reader);
    loop {
        let tunneled = read_async_frame::<_, UsageRelayTunnelRequest>(&mut reader).await?;
        let broker = broker.clone();
        let allowlist = allowlist.clone();
        let responses = responses.clone();
        drop(jackin_telemetry::spawn::spawn_stream(
            "usage_relay.tunnel_request",
            async move {
                let response = if tunneled.request.protocol_version != USAGE_BROKER_PROTOCOL_VERSION
                    || tunneled.request.build_id != env!("CARGO_PKG_VERSION")
                {
                    error_response(UsageCoordinationErrorKind::ProtocolMismatch)
                } else {
                    dispatch(tunneled.request.operation, broker, allowlist).await
                };
                drop(
                    responses
                        .send(UsageRelayTunnelResponse {
                            request_id: tunneled.request_id,
                            response,
                        })
                        .await,
                );
            },
        ));
        if writer.is_finished() {
            return Err(anyhow::anyhow!("usage relay tunnel writer exited"));
        }
    }
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
