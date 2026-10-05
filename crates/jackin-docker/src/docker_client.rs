// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `BollardDockerClient`: concrete async Docker daemon implementation.
//!
//! The `DockerApi` trait, pure data types (`ContainerState`, `ContainerRow`,
//! etc.) are re-exported from `jackin-core` so all consumer crates depend on
//! the trait, not the bollard implementation.
//!
//! Not responsible for: subprocess-level `docker` CLI invocations
//! (`shell_runner.rs`), or the launch pipeline orchestration.

use std::{collections::HashMap, ffi::OsStr, future::Future, path::PathBuf, sync::OnceLock};

use crate::DockerError;
use anyhow::Context;
use bollard::Docker;
use bollard::container::LogOutput;
use bollard::exec::{CreateExecOptions, StartExecOptions, StartExecResults};
use bollard::models::{
    ContainerCreateBody, ContainerInspectResponse, ContainerStateStatusEnum, HostConfig,
    NetworkCreateRequest, ResourcesUlimits, VolumeCreateRequest,
};
use bollard::query_parameters::{
    CreateContainerOptions, InspectContainerOptions, ListContainersOptions, ListImagesOptions,
    ListNetworksOptions, RemoveContainerOptions, RemoveImageOptions, RemoveVolumeOptions,
    StartContainerOptions,
};
use futures_util::StreamExt;

pub use jackin_core::{
    ContainerHandle, ContainerInspection, ContainerRow, ContainerSpec, ContainerState, DaemonServerId, DockerApi,
    ControllerEndpoint, ControllerTlsFiles,
    NetworkId, NetworkRow, RemoveImageOutcome, VolumeRow,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DockerRoute {
    method: &'static str,
    template: &'static str,
}

impl DockerRoute {
    const fn new(method: &'static str, template: &'static str) -> Self {
        Self { method, template }
    }
}

const PING: DockerRoute = DockerRoute::new("GET", "/_ping");
const INFO: DockerRoute = DockerRoute::new("GET", "/info");
const CONTAINER_INSPECT: DockerRoute = DockerRoute::new("GET", "/containers/{id}/json");
const CONTAINER_REMOVE: DockerRoute = DockerRoute::new("DELETE", "/containers/{id}");
const CONTAINER_LIST: DockerRoute = DockerRoute::new("GET", "/containers/json");
const CONTAINER_CREATE: DockerRoute = DockerRoute::new("POST", "/containers/create");
const CONTAINER_START: DockerRoute = DockerRoute::new("POST", "/containers/{id}/start");
const VOLUME_REMOVE: DockerRoute = DockerRoute::new("DELETE", "/volumes/{name}");
const VOLUME_CREATE: DockerRoute = DockerRoute::new("POST", "/volumes/create");
const VOLUME_INSPECT: DockerRoute = DockerRoute::new("GET", "/volumes/{name}");
const NETWORK_CREATE: DockerRoute = DockerRoute::new("POST", "/networks/create");
const NETWORK_REMOVE: DockerRoute = DockerRoute::new("DELETE", "/networks/{id}");
const NETWORK_LIST: DockerRoute = DockerRoute::new("GET", "/networks");
const NETWORK_INSPECT: DockerRoute = DockerRoute::new("GET", "/networks/{id}");
const IMAGE_LIST: DockerRoute = DockerRoute::new("GET", "/images/json");
const IMAGE_REMOVE: DockerRoute = DockerRoute::new("DELETE", "/images/{name}");
const IMAGE_INSPECT: DockerRoute = DockerRoute::new("GET", "/images/{name}/json");
const IMAGE_PULL: DockerRoute = DockerRoute::new("POST", "/images/create");
const EXEC_CREATE: DockerRoute = DockerRoute::new("POST", "/containers/{id}/exec");
const EXEC_START: DockerRoute = DockerRoute::new("POST", "/exec/{id}/start");
const EXEC_INSPECT: DockerRoute = DockerRoute::new("GET", "/exec/{id}/json");

fn begin_docker_http(route: DockerRoute) -> jackin_telemetry::OperationGuard {
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::HTTP_REQUEST_METHOD,
            value: jackin_telemetry::Value::Str(route.method),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::URL_TEMPLATE,
            value: jackin_telemetry::Value::Str(route.template),
        },
    ];
    jackin_telemetry::operation_or_disabled(&jackin_telemetry::operation::HTTP_CLIENT, &attrs)
}

async fn docker_http<T>(
    route: DockerRoute,
    future: impl Future<Output = anyhow::Result<T>>,
) -> anyhow::Result<T> {
    let operation = begin_docker_http(route);
    let result = future.await;
    operation.complete(
        if result.is_ok() {
            jackin_telemetry::schema::enums::OutcomeValue::Success
        } else {
            jackin_telemetry::schema::enums::OutcomeValue::Error
        },
        result
            .as_ref()
            .err()
            .map(|_| jackin_telemetry::schema::enums::ErrorType::HttpError),
    );
    result
}

async fn consume_exec_start(container: &str, start: StartExecResults) -> anyhow::Result<String> {
    let StartExecResults::Attached { mut output, .. } = start else {
        return Err(DockerError::ExecDetached {
            container: container.to_owned(),
        }
        .into());
    };
    let mut output_buf = String::new();
    while let Some(chunk) = output.next().await {
        if let LogOutput::StdOut { message } | LogOutput::StdErr { message } =
            chunk.with_context(|| format!("reading exec output from {container}"))?
        {
            output_buf.push_str(&String::from_utf8_lossy(&message));
        }
    }
    Ok(output_buf)
}

#[derive(Debug)]
pub struct BollardDockerClient {
    inner: Docker,
    endpoint: ControllerEndpoint,
}

impl BollardDockerClient {
    pub fn connect() -> anyhow::Result<Self> {
        let (inner, endpoint) =
            connect_to_cli_docker_context().context("failed to connect to Docker daemon")?;
        Ok(Self { inner, endpoint })
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ConnectionChoice {
    Defaults,
    Host(String),
    Unsupported {
        reason: UnsupportedReason,
        host: String,
    },
}

#[derive(Debug, PartialEq, Eq)]
enum UnsupportedReason {
    SshTransport,
    TlsTransport,
    ContextTlsMaterial,
    UnsupportedUri,
}

impl ConnectionChoice {
    fn unsupported(reason: UnsupportedReason, host: impl Into<String>) -> Self {
        Self::Unsupported {
            reason,
            host: host.into(),
        }
    }

    fn unsupported_message(reason: &UnsupportedReason, host: &str) -> String {
        let detail = match reason {
            UnsupportedReason::SshTransport => format!(
                "active Docker context uses SSH transport ({host}); this jackin build cannot mirror SSH Docker contexts for Bollard API calls"
            ),
            UnsupportedReason::TlsTransport => format!(
                "active Docker context uses TLS transport ({host}); jackin reads TLS material from DOCKER_TLS_VERIFY and DOCKER_CERT_PATH, not from a Docker context"
            ),
            UnsupportedReason::ContextTlsMaterial => format!(
                "active Docker context for {host} includes TLS settings; jackin reads TLS material from DOCKER_TLS_VERIFY and DOCKER_CERT_PATH, not from a Docker context"
            ),
            UnsupportedReason::UnsupportedUri => {
                format!("active Docker context uses unsupported Docker host URI {host}")
            }
        };
        format!("{detail}. {OVERRIDE_HINT}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DockerContextEndpoint {
    host: String,
    skip_tls_verify: bool,
    has_tls_material: bool,
}

impl DockerContextEndpoint {
    fn new(host: impl Into<String>, skip_tls_verify: bool, has_tls_material: bool) -> Self {
        Self {
            host: host.into().trim().to_owned(),
            skip_tls_verify,
            has_tls_material,
        }
    }

    fn connection_choice(self) -> ConnectionChoice {
        let host = self.host.as_str();
        if host.is_empty() {
            return ConnectionChoice::Defaults;
        }

        if host.starts_with("ssh://") {
            return ConnectionChoice::unsupported(UnsupportedReason::SshTransport, host);
        }
        if host.starts_with("https://") {
            return ConnectionChoice::unsupported(UnsupportedReason::TlsTransport, host);
        }
        if self.skip_tls_verify || self.has_tls_material {
            return ConnectionChoice::unsupported(UnsupportedReason::ContextTlsMaterial, host);
        }

        if context_host_supported_without_extra_settings(host) {
            ConnectionChoice::Host(self.host)
        } else {
            ConnectionChoice::unsupported(UnsupportedReason::UnsupportedUri, host)
        }
    }
}

const OVERRIDE_HINT: &str = "Set DOCKER_HOST to a unix:// socket, a plain tcp:// endpoint, or a TLS tcp:// endpoint with DOCKER_TLS_VERIFY and DOCKER_CERT_PATH set, to override.";

fn context_host_supported_without_extra_settings(host: &str) -> bool {
    host.starts_with("unix://")
        || host.starts_with("tcp://")
        || host.starts_with("http://")
        || (cfg!(windows) && host.starts_with("npipe://"))
}

#[derive(serde::Deserialize)]
struct DockerContextInspect {
    #[serde(rename = "Endpoints")]
    endpoints: DockerContextEndpoints,
    #[serde(rename = "TLSMaterial", default)]
    tls_material: serde_json::Value,
}

#[derive(serde::Deserialize)]
struct DockerContextEndpoints {
    #[serde(rename = "docker")]
    docker: Option<DockerContextEndpointInspect>,
}

#[derive(serde::Deserialize)]
struct DockerContextEndpointInspect {
    #[serde(rename = "Host", default)]
    host: String,
    #[serde(rename = "SkipTLSVerify", default)]
    skip_tls_verify: bool,
}

/// Deliberately uses `std::process::Command` instead of `ShellRunner::capture`:
/// `connect()` is sync and called from `std::thread::scope` before any tokio
/// runtime exists, while `ShellRunner` wraps `tokio::process::Command`.
fn connect_to_cli_docker_context() -> anyhow::Result<(Docker, ControllerEndpoint)> {
    let host_env = std::env::var_os("DOCKER_HOST");
    let env_set = docker_host_env_is_set_from(host_env.as_deref());
    // Skip the subprocess when DOCKER_HOST already wins per Docker CLI precedence.
    let ctx_endpoint = if env_set {
        None
    } else {
        cached_context_endpoint()
    };
    let host = match choose_connection(env_set, ctx_endpoint) {
        ConnectionChoice::Defaults if env_set => host_env
            .and_then(|host| host.into_string().ok())
            .context("DOCKER_HOST contains non-UTF-8 bytes")?,
        ConnectionChoice::Defaults => default_controller_host().to_owned(),
        ConnectionChoice::Host(host) => host,
        ConnectionChoice::Unsupported { reason, host } => {
            return Err(DockerError::Message(ConnectionChoice::unsupported_message(&reason, &host)).into());
        }
    };
    let tls_requested = std::env::var("DOCKER_TLS_VERIFY").is_ok() || host.starts_with("https://");
    let cert_directory = if tls_requested && !host.starts_with("unix://") {
        Some(captured_docker_cert_directory()?)
    } else {
        None
    };
    let endpoint = capture_controller_endpoint(&host, tls_requested, cert_directory)?;
    let inner = connect_captured_endpoint(&endpoint)?;
    Ok((inner, endpoint))
}

fn captured_docker_cert_directory() -> anyhow::Result<PathBuf> {
    let directory = match std::env::var("DOCKER_CERT_PATH")
        .or_else(|_| std::env::var("DOCKER_CONFIG"))
    {
        Ok(path) => PathBuf::from(path),
        Err(_) => jackin_core::JackinPaths::detect()?.home_dir.join(".docker"),
    };
    if directory.is_absolute() {
        Ok(directory)
    } else {
        Ok(std::env::current_dir()?.join(directory))
    }
}

const fn default_controller_host() -> &'static str {
    #[cfg(windows)]
    { "npipe:////./pipe/docker_engine" }
    #[cfg(not(windows))]
    { "unix:///var/run/docker.sock" }
}

fn capture_controller_endpoint(
    host: &str,
    tls_requested: bool,
    cert_directory: Option<PathBuf>,
) -> anyhow::Result<ControllerEndpoint> {
    let tls_requested = tls_requested || host.starts_with("https://");
    if let Some(socket) = host.strip_prefix("unix://") {
        let socket = PathBuf::from(socket);
        anyhow::ensure!(socket.is_absolute(), "Docker Unix socket path must be absolute");
        return Ok(ControllerEndpoint::Unix { socket });
    }
    #[cfg(windows)]
    if host.starts_with("npipe://") {
        return Ok(ControllerEndpoint::NamedPipe { pipe: host.to_owned() });
    }
    anyhow::ensure!(
        host.starts_with("tcp://") || host.starts_with("http://") || host.starts_with("https://"),
        "unsupported Docker controller URI: {host}"
    );
    let url = reqwest::Url::parse(&host.replacen("tcp://", "http://", 1))?;
    anyhow::ensure!(
        url.host_str().is_some() && url.username().is_empty() && url.password().is_none()
            && url.query().is_none() && url.fragment().is_none() && matches!(url.path(), "" | "/"),
        "Docker controller URI must contain only a network authority"
    );
    let tls = if tls_requested {
        let directory = cert_directory.context("Docker TLS certificate directory was not captured")?;
        anyhow::ensure!(directory.is_absolute(), "Docker TLS certificate directory must be absolute");
        Some(ControllerTlsFiles {
            key: directory.join("key.pem"),
            cert: directory.join("cert.pem"),
            ca: directory.join("ca.pem"),
        })
    } else {
        None
    };
    let authority = host.replacen("http://", "tcp://", 1).replacen("https://", "tcp://", 1);
    Ok(ControllerEndpoint::Tcp { authority, tls })
}

#[cfg(unix)]
fn captured_unix_connector_address(socket: &std::path::Path) -> anyhow::Result<String> {
    // Bollard removes the first `unix://` occurrence, even inside a raw path.
    // Supply our own prefix so the captured path remains byte-for-byte intact.
    Ok(format!(
        "unix://{}",
        socket.to_str().context("Docker socket contains non-UTF-8 bytes")?
    ))
}

fn connect_captured_endpoint(endpoint: &ControllerEndpoint) -> anyhow::Result<Docker> {
    const TIMEOUT: u64 = 120;
    let version = bollard::API_DEFAULT_VERSION;
    let inner = match endpoint {
        #[cfg(unix)]
        ControllerEndpoint::Unix { socket } => Docker::connect_with_unix(
            &captured_unix_connector_address(socket)?, TIMEOUT, version
        ),
        #[cfg(not(unix))]
        ControllerEndpoint::Unix { .. } => anyhow::bail!("Unix Docker sockets are unsupported on this host"),
        ControllerEndpoint::Tcp { authority, tls: None } => Docker::connect_with_http(authority, TIMEOUT, version),
        ControllerEndpoint::Tcp { authority, tls: Some(tls) } => Docker::connect_with_ssl(
            authority, &tls.key, &tls.cert, &tls.ca, TIMEOUT, version
        ),
        #[cfg(windows)]
        ControllerEndpoint::NamedPipe { pipe } => Docker::connect_with_named_pipe(pipe, TIMEOUT, version),
    };
    inner.context("connect to captured Docker controller endpoint")
}

fn choose_connection(
    docker_host_env_set: bool,
    ctx_endpoint: Option<DockerContextEndpoint>,
) -> ConnectionChoice {
    if docker_host_env_set {
        return ConnectionChoice::Defaults;
    }
    ctx_endpoint.map_or(
        ConnectionChoice::Defaults,
        DockerContextEndpoint::connection_choice,
    )
}

/// Docker CLI treats an empty `DOCKER_HOST=` as unset and falls through to the
/// active context. Match that here so an empty value still consults `docker context inspect`.
fn docker_host_env_is_set_from(value: Option<&OsStr>) -> bool {
    value.is_some_and(|v| !v.is_empty())
}

/// Active Docker CLI context cannot change mid-process (`DOCKER_CONTEXT` and
/// `currentContext` are both read once at startup), so cache the
/// `docker context inspect` result across repeated `connect()` calls. Only
/// successful lookups are cached — a transient subprocess failure (docker
/// missing from PATH at first connect, slow daemon during boot) re-probes on
/// the next call instead of locking in `None` for the process lifetime.
fn cached_context_endpoint() -> Option<DockerContextEndpoint> {
    static CACHE: OnceLock<DockerContextEndpoint> = OnceLock::new();
    if let Some(cached) = CACHE.get() {
        return Some(cached.clone());
    }
    let endpoint = active_docker_context_endpoint()?;
    drop(CACHE.set(endpoint.clone()));
    Some(endpoint)
}

fn active_docker_context_endpoint() -> Option<DockerContextEndpoint> {
    let mut request = jackin_process::ExecRequest::new(
        "docker",
        ["context", "inspect", "--format", "{{json .}}"],
    );
    // Pin `DOCKER_CONTEXT` so resolution survives any future Docker CLI drift,
    // even though `docker context inspect` already honors it today.
    if let Some(ctx) = std::env::var("DOCKER_CONTEXT")
        .ok()
        .filter(|v| !v.is_empty())
    {
        request.args.push(ctx.into());
    }
    let Ok(output) = crate::process_telemetry::exec_sync(&request) else {
        return None;
    };
    if !output.success {
        return None;
    }
    parse_docker_context_endpoint(&output.stdout)
}

fn parse_docker_context_endpoint(stdout: &[u8]) -> Option<DockerContextEndpoint> {
    let context: DockerContextInspect = match serde_json::from_slice(stdout) {
        Ok(context) => context,
        Err(_) => return None,
    };
    let endpoint = context.endpoints.docker?;
    let has_tls_material = context
        .tls_material
        .get("docker")
        .is_some_and(tls_material_present);
    Some(DockerContextEndpoint::new(
        endpoint.host,
        endpoint.skip_tls_verify,
        has_tls_material,
    ))
}

fn tls_material_present(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::Array(items) => !items.is_empty(),
        serde_json::Value::Object(entries) => !entries.is_empty(),
        serde_json::Value::String(value) => !value.trim().is_empty(),
        serde_json::Value::Bool(value) => *value,
        serde_json::Value::Number(_) => true,
    }
}

const fn is_http_status(err: &bollard::errors::Error, code: u16) -> bool {
    matches!(
        err,
        bollard::errors::Error::DockerResponseServerError { status_code, .. }
        if *status_code == code
    )
}

fn build_label_filter(label_filters: &[&str]) -> Option<HashMap<String, Vec<String>>> {
    if label_filters.is_empty() {
        return None;
    }
    let mut map = HashMap::new();
    map.insert(
        "label".to_owned(),
        label_filters.iter().map(ToString::to_string).collect(),
    );
    Some(map)
}

fn authenticated_container_init_pid(
    info: &ContainerInspectResponse,
    expected: &ContainerHandle,
) -> anyhow::Result<u32> {
    ensure_container_identity(info, expected)?;
    let state = info.state.as_ref().context("runtime container state is unavailable")?;
    anyhow::ensure!(state.running == Some(true)
        && state.status == Some(ContainerStateStatusEnum::RUNNING),
        "credential relay requires a running container");
    let pid = state.pid.context("runtime container init PID is unavailable")?;
    let pid = u32::try_from(pid).context("runtime container init PID is invalid")?;
    anyhow::ensure!(pid > 0, "runtime container init PID is unavailable");
    Ok(pid)
}

fn ensure_container_identity(
    info: &ContainerInspectResponse,
    expected: &ContainerHandle,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        info.id.as_deref() == Some(expected.id()),
        "Docker returned a different container ID for {}",
        expected.name()
    );
    let actual_name = info.name.as_deref().unwrap_or_default().trim_start_matches('/');
    anyhow::ensure!(
        actual_name == expected.name(),
        "Docker returned a different container name for {}",
        expected.name()
    );
    Ok(())
}

fn container_state_from_inspect(info: &ContainerInspectResponse) -> ContainerState {
    let Some(state) = info.state.as_ref() else {
        return ContainerState::InspectUnavailable("no state field".to_owned());
    };
    match state.status {
        Some(ContainerStateStatusEnum::RUNNING) => ContainerState::Running,
        Some(ContainerStateStatusEnum::PAUSED) => ContainerState::Paused,
        Some(ContainerStateStatusEnum::RESTARTING) => ContainerState::Restarting,
        Some(ContainerStateStatusEnum::REMOVING) => ContainerState::Removing,
        Some(ContainerStateStatusEnum::CREATED) => ContainerState::Created,
        Some(ContainerStateStatusEnum::DEAD) => ContainerState::Dead,
        Some(ContainerStateStatusEnum::EXITED) | None => {
            let exit_code = state.exit_code.unwrap_or(0) as i32;
            let oom_killed = state.oom_killed.unwrap_or(false);
            ContainerState::Stopped {
                exit_code,
                oom_killed,
            }
        }
        Some(ContainerStateStatusEnum::EMPTY | ContainerStateStatusEnum::STOPPING) => {
            ContainerState::InspectUnavailable(format!(
                "unexpected container status: {:?}",
                state.status
            ))
        }
    }
}

impl DockerApi for BollardDockerClient {
    fn controller_endpoint(&self) -> &ControllerEndpoint {
        &self.endpoint
    }
    async fn daemon_server_id(&self) -> anyhow::Result<DaemonServerId> {
        docker_http(INFO, async {
            let info = self.inner.info().await.context("reading Docker daemon server identity")?;
            let id = info.id.context("Docker /info response is missing the server ID")?;
            DaemonServerId::parse(&id).context("validating Docker daemon server identity")
        }).await
    }

    async fn ping(&self) -> anyhow::Result<()> {
        let connection_attrs = [jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::CONNECTION_PEER_TYPE,
            value: jackin_telemetry::Value::Str(
                jackin_telemetry::schema::enums::ConnectionPeerType::Docker.as_str(),
            ),
        }];
        let connection = jackin_telemetry::operation_or_disabled(
            &jackin_telemetry::operation::CONNECTION_ATTEMPT,
            &connection_attrs,
        );
        let result = docker_http(PING, async {
            self.inner
                .ping()
                .await
                .map(|_| ())
                .context("pinging Docker daemon")
        })
        .await;
        connection.complete(
            if result.is_ok() {
                jackin_telemetry::schema::enums::OutcomeValue::Success
            } else {
                jackin_telemetry::schema::enums::OutcomeValue::Error
            },
            result
                .as_ref()
                .err()
                .map(|_| jackin_telemetry::schema::enums::ErrorType::DockerDaemonUnreachable),
        );
        result
    }

    async fn inspect_container_by_name(&self, name: &str) -> ContainerInspection {
        let operation = begin_docker_http(CONTAINER_INSPECT);
        let result = self
            .inner
            .inspect_container(name, None::<InspectContainerOptions>)
            .await;

        let inspection = match result {
            Err(ref e) if is_http_status(e, 404) => ContainerInspection {
                handle: None,
                state: ContainerState::NotFound,
            },
            Err(e) => ContainerInspection {
                handle: None,
                state: ContainerState::InspectUnavailable(e.to_string()),
            },
            Ok(info) => {
                let state = container_state_from_inspect(&info);
                let id = info.id.as_deref();
                let actual_name = info.name.as_deref().unwrap_or_default().trim_start_matches('/');
                let handle = id
                    .filter(|_| actual_name == name)
                    .and_then(|id| ContainerHandle::new(name, id).ok());
                let state = if handle.is_none() {
                    ContainerState::InspectUnavailable(
                        "container name or full ID did not match the requested identity".to_owned(),
                    )
                } else {
                    state
                };
                ContainerInspection { handle, state }
            }
        };
        let failed = matches!(inspection.state, ContainerState::InspectUnavailable(_));
        operation.complete(
            if failed {
                jackin_telemetry::schema::enums::OutcomeValue::Failure
            } else {
                jackin_telemetry::schema::enums::OutcomeValue::Success
            },
            failed.then_some(jackin_telemetry::schema::enums::ErrorType::HttpError),
        );
        inspection
    }

    async fn inspect_container_by_id(&self, container: &ContainerHandle) -> ContainerState {
        let operation = begin_docker_http(CONTAINER_INSPECT);
        let result = self
            .inner
            .inspect_container(container.id(), None::<InspectContainerOptions>)
            .await;
        let state = match result {
            Err(ref e) if is_http_status(e, 404) => ContainerState::NotFound,
            Err(e) => ContainerState::InspectUnavailable(e.to_string()),
            Ok(info) => match ensure_container_identity(&info, container) {
                Ok(()) => container_state_from_inspect(&info),
                Err(error) => ContainerState::InspectUnavailable(error.to_string()),
            },
        };
        let failed = matches!(state, ContainerState::InspectUnavailable(_));
        operation.complete(
            if failed {
                jackin_telemetry::schema::enums::OutcomeValue::Failure
            } else {
                jackin_telemetry::schema::enums::OutcomeValue::Success
            },
            failed.then_some(jackin_telemetry::schema::enums::ErrorType::HttpError),
        );
        state
    }

    async fn container_init_pid_by_id(&self, container: &ContainerHandle) -> anyhow::Result<u32> {
        docker_http(CONTAINER_INSPECT, async {
            let info = self.inner
                .inspect_container(container.id(), None::<InspectContainerOptions>)
                .await?;
            authenticated_container_init_pid(&info, container)
        }).await
    }

    async fn remove_container_by_id(&self, container: &ContainerHandle) -> anyhow::Result<()> {
        docker_http(CONTAINER_REMOVE, async {
            match self.inner.inspect_container(container.id(), None::<InspectContainerOptions>).await {
                Ok(info) => ensure_container_identity(&info, container)?,
                Err(error) if is_http_status(&error, 404) => return Ok(()),
                Err(error) => return Err(anyhow::Error::from(error).context(format!(
                    "verifying container {} ({}) before removal",
                    container.name(), container.id()
                ))),
            }
            match self
                .inner
                .remove_container(
                    container.id(),
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await
            {
                Ok(()) => {}
                Err(e) if is_http_status(&e, 404) => {}
                Err(e) => Err(anyhow::Error::from(e).context(format!(
                    "removing container {} ({})",
                    container.name(),
                    container.id()
                )))?,
            }
            match self.inspect_container_by_id(container).await {
                ContainerState::NotFound => Ok(()),
                state => anyhow::bail!(
                    "Docker container {} ({}) remains or became ambiguous after removal: {}",
                    container.name(), container.id(), state.inspect_label()
                ),
            }
        })
        .await
    }

    async fn list_containers(
        &self,
        label_filters: &[&str],
        all: bool,
    ) -> anyhow::Result<Vec<ContainerRow>> {
        docker_http(CONTAINER_LIST, async {
            let filters = build_label_filter(label_filters);
            let summaries = self
                .inner
                .list_containers(Some(ListContainersOptions {
                    all,
                    filters,
                    ..Default::default()
                }))
                .await
                .context("listing containers")?;

            summaries
                .into_iter()
                .map(|s| {
                    let raw_name = s
                        .names
                        .unwrap_or_default()
                        .into_iter()
                        .next()
                        .unwrap_or_default();
                    let name = raw_name.trim_start_matches('/').to_owned();
                    let id = s.id.filter(|id| !id.is_empty()).ok_or_else(|| {
                        anyhow::anyhow!("Docker returned a container without an ID")
                    })?;
                    let labels = s.labels.unwrap_or_default();
                    Ok(ContainerRow { name, id, labels })
                })
                .collect()
        })
        .await
    }

    async fn create_container(
        &self,
        name: &str,
        spec: ContainerSpec,
    ) -> anyhow::Result<ContainerHandle> {
        docker_http(CONTAINER_CREATE, async {
            let memory = spec
                .memory_bytes
                .map(i64::try_from)
                .transpose()
                .context("converting container memory limit")?;
            let memory_reservation = spec
                .memory_reservation_bytes
                .map(i64::try_from)
                .transpose()
                .context("converting container memory reservation")?;
            let nofile = spec
                .nofile
                .map(i64::try_from)
                .transpose()
                .context("converting container nofile limit")?;
            let ulimits = nofile.map(|limit| {
                vec![ResourcesUlimits {
                    name: Some("nofile".to_owned()),
                    soft: Some(limit),
                    hard: Some(limit),
                }]
            });
            let tmpfs = (!spec.tmpfs.is_empty()).then(|| {
                spec.tmpfs
                    .iter()
                    .filter_map(|mount| {
                        let (path, options) = mount
                            .split_once(':')
                            .map_or((mount.as_str(), ""), |(path, options)| (path, options));
                        (!path.is_empty()).then(|| (path.to_owned(), options.to_owned()))
                    })
                    .collect()
            });
            let response = self
                .inner
                .create_container(
                    Some(CreateContainerOptions {
                        name: Some(name.to_owned()),
                        ..Default::default()
                    }),
                    ContainerCreateBody {
                        image: Some(spec.image),
                        hostname: spec.hostname,
                        user: spec.user,
                        cmd: spec.command,
                        env: Some(spec.env),
                        labels: Some(spec.labels),
                        host_config: Some(HostConfig {
                            network_mode: Some(spec.network),
                            binds: Some(spec.binds),
                            privileged: Some(spec.privileged),
                            cap_add: (!spec.cap_add.is_empty()).then_some(spec.cap_add),
                            cap_drop: (!spec.cap_drop.is_empty()).then_some(spec.cap_drop),
                            readonly_rootfs: Some(spec.readonly_rootfs),
                            security_opt: (!spec.security_opt.is_empty())
                                .then_some(spec.security_opt),
                            tmpfs,
                            extra_hosts: (!spec.extra_hosts.is_empty()).then_some(spec.extra_hosts),
                            memory,
                            memory_reservation,
                            nano_cpus: spec.nano_cpus,
                            pids_limit: spec.pids_limit,
                            ulimits,
                            ..Default::default()
                        }),
                        entrypoint: spec.entrypoint,
                        working_dir: spec.workdir,
                        ..Default::default()
                    },
                )
                .await
                .with_context(|| format!("creating container {name}"))?;
            ContainerHandle::new(name, response.id)
        })
        .await
    }

    async fn start_container_by_id(&self, container: &ContainerHandle) -> anyhow::Result<()> {
        docker_http(CONTAINER_START, async {
            let info = self.inner
                .inspect_container(container.id(), None::<InspectContainerOptions>)
                .await
                .with_context(|| format!("verifying container {} ({}) before start", container.name(), container.id()))?;
            ensure_container_identity(&info, container)?;
            self.inner
                .start_container(container.id(), None::<StartContainerOptions>)
                .await
                .with_context(|| {
                    format!(
                        "starting container {} ({})",
                        container.name(),
                        container.id()
                    )
                })
        })
        .await
    }

    async fn create_volume(
        &self,
        name: &str,
        labels: HashMap<String, String>,
    ) -> anyhow::Result<VolumeRow> {
        docker_http(VOLUME_CREATE, async {
            let volume = self.inner.create_volume(VolumeCreateRequest {
                name: Some(name.to_owned()),
                driver: Some("local".to_owned()),
                labels: Some(labels.clone()),
                ..Default::default()
            }).await.with_context(|| format!("creating volume {name}"))?;
            anyhow::ensure!(volume.name == name, "Docker returned a different volume name for {name}");
            anyhow::ensure!(volume.labels == labels, "volume {name} returned different ownership labels");
            anyhow::ensure!(volume.driver == "local", "volume {name} returned a different storage driver");
            Ok(VolumeRow { name: volume.name, labels: volume.labels, driver: volume.driver })
        }).await
    }

    async fn inspect_volume_by_name(&self, name: &str) -> anyhow::Result<Option<VolumeRow>> {
        docker_http(VOLUME_INSPECT, async {
            match self.inner.inspect_volume(name).await {
                Ok(volume) => {
                    anyhow::ensure!(volume.name == name, "Docker returned a different volume name for {name}");
                    Ok(Some(VolumeRow { name: volume.name, labels: volume.labels, driver: volume.driver }))
                }
                Err(e) if is_http_status(&e, 404) => Ok(None),
                Err(e) => Err(anyhow::Error::from(e).context(format!("inspecting volume {name}"))),
            }
        }).await
    }

    async fn remove_volume(&self, name: &str) -> anyhow::Result<()> {
        docker_http(VOLUME_REMOVE, async {
            match self
                .inner
                .remove_volume(name, None::<RemoveVolumeOptions>)
                .await
            {
                Ok(()) => Ok(()),
                Err(e) if is_http_status(&e, 404) => Ok(()),
                Err(e) => Err(anyhow::Error::from(e).context(format!("removing volume {name}"))),
            }
        })
        .await?;
        anyhow::ensure!(self.inspect_volume_by_name(name).await?.is_none(),
            "Docker volume {name} remains after removal");
        Ok(())
    }

    async fn create_network(
        &self,
        name: &str,
        labels: HashMap<String, String>,
        internal: bool,
    ) -> anyhow::Result<NetworkId> {
        docker_http(NETWORK_CREATE, async {
            let created = self.inner
                .create_network(NetworkCreateRequest {
                    name: name.to_owned(),
                    labels: Some(labels),
                    internal: Some(internal),
                    ..Default::default()
                })
                .await
                .with_context(|| format!("creating network {name}"))?;
            NetworkId::parse(&created.id).context("validating created network identity")
        })
        .await
    }

    async fn remove_network_by_id(&self, id: &NetworkId) -> anyhow::Result<()> {
        // Docker accepts either an ID or name. Preflight catches a current
        // same-string name collision; Engine exposes no conditional ID-only
        // delete, so a race remains between this check and the delete request.
        let Some(row) = self.inspect_network_by_id(id).await? else {
            return Ok(());
        };
        anyhow::ensure!(&row.id == id, "Docker network identity is ambiguous for {id}");
        docker_http(NETWORK_REMOVE, async {
            match self.inner.remove_network(id.as_str()).await {
                Ok(()) => {},
                Err(e) if is_http_status(&e, 404) => {},
                Err(e) => return Err(anyhow::Error::from(e).context(format!("removing network {id}"))),
            }
            anyhow::ensure!(
                self.inspect_network_by_id(id).await?.is_none(),
                "Docker network {id} remains after removal"
            );
            Ok(())
        })
        .await
    }

    async fn list_networks(&self, label_filters: &[&str]) -> anyhow::Result<Vec<NetworkRow>> {
        docker_http(NETWORK_LIST, async {
            let filters = build_label_filter(label_filters);
            let networks = self
                .inner
                .list_networks(Some(ListNetworksOptions { filters }))
                .await
                .context("listing networks")?;

            networks
                .into_iter()
                .map(|n| {
                    let name = n.name.context("listed network is missing its name")?;
                    let id = NetworkId::parse(&n.id.context("listed network is missing its ID")?)?;
                    let labels = n.labels.unwrap_or_default();
                    Ok(NetworkRow { id, name, labels })
                })
                .collect()
        })
        .await
    }

    async fn list_image_tags(&self, reference_filter: &str) -> anyhow::Result<Vec<String>> {
        docker_http(IMAGE_LIST, async {
            let mut filters = HashMap::new();
            filters.insert("reference".to_owned(), vec![reference_filter.to_owned()]);
            let images = self
                .inner
                .list_images(Some(ListImagesOptions {
                    filters: Some(filters),
                    ..Default::default()
                }))
                .await
                .context("listing images")?;

            let tags: Vec<String> = images
                .into_iter()
                .flat_map(|i| i.repo_tags)
                .filter(|t| !t.is_empty())
                .collect();
            Ok(tags)
        })
        .await
    }

    async fn remove_image(&self, name: &str) -> anyhow::Result<RemoveImageOutcome> {
        docker_http(IMAGE_REMOVE, async {
            match self
                .inner
                .remove_image(
                    name,
                    Some(RemoveImageOptions {
                        force: false,
                        noprune: false,
                        ..Default::default()
                    }),
                    None,
                )
                .await
            {
                Ok(_) => Ok(RemoveImageOutcome::Removed),
                Err(e) if is_http_status(&e, 404) => Ok(RemoveImageOutcome::NotFound),
                Err(ref e) if is_http_status(e, 409) => Ok(RemoveImageOutcome::InUse),
                Err(e) => {
                    let msg = e.to_string().to_ascii_lowercase();
                    if msg.contains("in use") || msg.contains("cannot be forced") {
                        Ok(RemoveImageOutcome::InUse)
                    } else {
                        Err(anyhow::Error::from(e).context(format!("removing image {name}")))
                    }
                }
            }
        })
        .await
    }

    async fn inspect_image_labels(&self, image: &str) -> anyhow::Result<HashMap<String, String>> {
        docker_http(IMAGE_INSPECT, async {
            match self.inner.inspect_image(image).await {
                Err(e) if is_http_status(&e, 404) => Ok(HashMap::new()),
                Err(e) => Err(anyhow::Error::from(e).context(format!("inspecting image {image}"))),
                Ok(info) => Ok(info
                    .config
                    .and_then(|c| c.labels)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|(_, v)| !v.is_empty())
                    .collect()),
            }
        })
        .await
    }

    async fn pull_image(&self, image: &str) -> anyhow::Result<()> {
        use bollard::query_parameters::CreateImageOptions;
        docker_http(IMAGE_PULL, async {
            let mut stream = self.inner.create_image(
                Some(CreateImageOptions {
                    from_image: Some(image.to_owned()),
                    ..Default::default()
                }),
                None,
                None,
            );
            while let Some(event) = stream.next().await {
                event.with_context(|| format!("pulling image {image}"))?;
            }
            Ok(())
        })
        .await
    }

    async fn exec_capture_by_id(
        &self,
        container: &ContainerHandle,
        cmd: &[&str],
    ) -> anyhow::Result<String> {
        let exec = docker_http(EXEC_CREATE, async {
            self.inner
                .create_exec(
                    container.id(),
                    CreateExecOptions {
                        cmd: Some(cmd.iter().map(ToString::to_string).collect()),
                        attach_stdout: Some(true),
                        attach_stderr: Some(true),
                        ..Default::default()
                    },
                )
                .await
                .with_context(|| {
                    format!("creating exec in {} ({})", container.name(), container.id())
                })
        })
        .await?;

        let output_buf = docker_http(EXEC_START, async {
            let start = self
                .inner
                .start_exec(&exec.id, None::<StartExecOptions>)
                .await
                .with_context(|| {
                    format!("starting exec in {} ({})", container.name(), container.id())
                })?;
            consume_exec_start(container.name(), start).await
        })
        .await?;

        let inspect = docker_http(EXEC_INSPECT, async {
            self.inner.inspect_exec(&exec.id).await.with_context(|| {
                format!(
                    "inspecting exec result in {} ({})",
                    container.name(),
                    container.id()
                )
            })
        })
        .await?;
        let exit_code = inspect.exit_code.unwrap_or(-1);
        if exit_code != 0 {
            return Err(DockerError::ExecNonZero {
                container: container.name().to_owned(),
                exit_code,
                output: output_buf.trim().to_owned(),
            }
            .into());
        }

        Ok(output_buf.trim().to_owned())
    }

    async fn inspect_network_by_name(&self, name: &str) -> anyhow::Result<Option<NetworkRow>> {
        docker_http(NETWORK_INSPECT, async {
            match self
                .inner
                .inspect_network(
                    name,
                    None::<bollard::query_parameters::InspectNetworkOptions>,
                )
                .await
            {
                Ok(n) => {
                    let id = NetworkId::parse(&n.id.context("inspected network is missing its ID")?)?;
                    let net_name = n.name.context("inspected network is missing its name")?;
                    let labels = n.labels.unwrap_or_default();
                    Ok(Some(NetworkRow {
                        id,
                        name: net_name,
                        labels,
                    }))
                }
                Err(e) if is_http_status(&e, 404) => Ok(None),
                Err(e) => Err(anyhow::Error::from(e).context(format!("inspecting network {name}"))),
            }
        })
        .await
    }

    async fn inspect_network_by_id(&self, id: &NetworkId) -> anyhow::Result<Option<NetworkRow>> {
        let row = self.inspect_network_by_name(id.as_str()).await?;
        if let Some(row) = &row {
            anyhow::ensure!(&row.id == id, "Docker returned a different network ID for {id}");
        }
        Ok(row)
    }
}

#[cfg(test)]
mod tests;
