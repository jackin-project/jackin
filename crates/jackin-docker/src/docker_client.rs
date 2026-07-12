//! `BollardDockerClient`: concrete async Docker daemon implementation.
//!
//! The `DockerApi` trait, pure data types (`ContainerState`, `ContainerRow`,
//! etc.) are re-exported from `jackin-core` so all consumer crates depend on
//! the trait, not the bollard implementation.
//!
//! Not responsible for: subprocess-level `docker` CLI invocations
//! (`shell_runner.rs`), or the launch pipeline orchestration.

use std::{collections::HashMap, ffi::OsStr, process::Command, sync::OnceLock};

use crate::DockerError;
use anyhow::Context;
use bollard::Docker;
use bollard::container::LogOutput;
use bollard::exec::{CreateExecOptions, StartExecOptions, StartExecResults};
use bollard::models::{
    ContainerCreateBody, ContainerStateStatusEnum, HostConfig, NetworkCreateRequest,
};
use bollard::query_parameters::{
    CreateContainerOptions, InspectContainerOptions, ListContainersOptions, ListImagesOptions,
    ListNetworksOptions, RemoveContainerOptions, RemoveImageOptions, RemoveVolumeOptions,
    StartContainerOptions,
};
use futures_util::StreamExt;

pub use jackin_core::{
    ContainerRow, ContainerSpec, ContainerState, DockerApi, NetworkRow, RemoveImageOutcome,
};

#[derive(Debug)]
pub struct BollardDockerClient {
    inner: Docker,
}

impl BollardDockerClient {
    pub fn connect() -> anyhow::Result<Self> {
        let inner =
            connect_to_cli_docker_context().context("failed to connect to Docker daemon")?;
        Ok(Self { inner })
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
            jackin_diagnostics::debug_log!(
                "docker",
                "context endpoint host empty; using bollard defaults"
            );
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
fn connect_to_cli_docker_context() -> anyhow::Result<Docker> {
    let env_set = docker_host_env_is_set();
    // Skip the subprocess when DOCKER_HOST already wins per Docker CLI precedence.
    let ctx_endpoint = if env_set {
        None
    } else {
        cached_context_endpoint()
    };
    match choose_connection(env_set, ctx_endpoint) {
        ConnectionChoice::Defaults => {
            Docker::connect_with_defaults().context("connect to Docker daemon via bollard defaults")
        }
        ConnectionChoice::Host(host) => {
            jackin_diagnostics::debug_log!("docker", "connect context host {host}");
            Docker::connect_with_host(&host)
                .with_context(|| format!("connect to Docker host {host}"))
        }
        ConnectionChoice::Unsupported { reason, host } => {
            Err(DockerError::Message(ConnectionChoice::unsupported_message(&reason, &host)).into())
        }
    }
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

fn docker_host_env_is_set() -> bool {
    docker_host_env_is_set_from(std::env::var_os("DOCKER_HOST").as_deref())
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
        jackin_diagnostics::debug_log!("docker", "context endpoint cache hit host={}", cached.host);
        return Some(cached.clone());
    }
    let endpoint = active_docker_context_endpoint()?;
    drop(CACHE.set(endpoint.clone()));
    Some(endpoint)
}

fn active_docker_context_endpoint() -> Option<DockerContextEndpoint> {
    let mut cmd = Command::new("docker");
    cmd.args(["context", "inspect", "--format", "{{json .}}"]);
    // Pin `DOCKER_CONTEXT` so resolution survives any future Docker CLI drift,
    // even though `docker context inspect` already honors it today.
    if let Some(ctx) = std::env::var("DOCKER_CONTEXT")
        .ok()
        .filter(|v| !v.is_empty())
    {
        cmd.arg(ctx);
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "Docker context inspection runs before launch render/runtime work begins"
    )]
    let output = match cmd.output() {
        Ok(output) => output,
        Err(err) => {
            jackin_diagnostics::debug_log!("docker", "context inspect spawn failed: {err}");
            return None;
        }
    };
    if !output.status.success() {
        jackin_diagnostics::debug_log!(
            "docker",
            "context inspect exit={:?} stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return None;
    }
    parse_docker_context_endpoint(&output.stdout)
}

fn parse_docker_context_endpoint(stdout: &[u8]) -> Option<DockerContextEndpoint> {
    let context: DockerContextInspect = match serde_json::from_slice(stdout) {
        Ok(context) => context,
        Err(err) => {
            jackin_diagnostics::debug_log!(
                "docker",
                "context inspect json parse failed: {err} stdout={}",
                String::from_utf8_lossy(stdout).trim()
            );
            return None;
        }
    };
    let Some(endpoint) = context.endpoints.docker else {
        jackin_diagnostics::debug_log!(
            "docker",
            "context inspect missing Endpoints.docker stdout={}",
            String::from_utf8_lossy(stdout).trim()
        );
        return None;
    };
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

impl DockerApi for BollardDockerClient {
    async fn ping(&self) -> anyhow::Result<()> {
        self.inner
            .ping()
            .await
            .map(|_| ())
            .context("pinging Docker daemon")
    }

    async fn inspect_container_state(&self, name: &str) -> ContainerState {
        jackin_diagnostics::debug_log!("docker", "inspect container {name}");
        let result = self
            .inner
            .inspect_container(name, None::<InspectContainerOptions>)
            .await;

        let state = match result {
            Err(ref e) if is_http_status(e, 404) => ContainerState::NotFound,
            Err(e) => ContainerState::InspectUnavailable(e.to_string()),
            Ok(info) => {
                let Some(state) = info.state else {
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
        };
        jackin_diagnostics::debug_log!(
            "docker",
            "inspect container {name} -> {}",
            state.inspect_label()
        );
        state
    }

    async fn remove_container(&self, name: &str) -> anyhow::Result<()> {
        jackin_diagnostics::debug_log!("docker", "rm -f {name}");
        match self
            .inner
            .remove_container(
                name,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await
        {
            Ok(()) => Ok(()),
            Err(e) if is_http_status(&e, 404) => Ok(()),
            Err(e) => Err(anyhow::Error::from(e).context(format!("removing container {name}"))),
        }
    }

    async fn list_containers(
        &self,
        label_filters: &[&str],
        all: bool,
    ) -> anyhow::Result<Vec<ContainerRow>> {
        jackin_diagnostics::debug_log!(
            "docker",
            "ps{} --filter label=...",
            if all { " -a" } else { "" }
        );
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

        Ok(summaries
            .into_iter()
            .map(|s| {
                let raw_name = s
                    .names
                    .unwrap_or_default()
                    .into_iter()
                    .next()
                    .unwrap_or_default();
                let name = raw_name.trim_start_matches('/').to_owned();
                let labels = s.labels.unwrap_or_default();
                ContainerRow { name, labels }
            })
            .collect())
    }

    async fn create_container(&self, name: &str, spec: ContainerSpec) -> anyhow::Result<()> {
        jackin_diagnostics::debug_log!("docker", "create container {name} image={}", spec.image);
        self.inner
            .create_container(
                Some(CreateContainerOptions {
                    name: Some(name.to_owned()),
                    ..Default::default()
                }),
                ContainerCreateBody {
                    image: Some(spec.image),
                    hostname: spec.hostname,
                    env: Some(spec.env),
                    labels: Some(spec.labels),
                    host_config: Some(HostConfig {
                        network_mode: Some(spec.network),
                        binds: Some(spec.binds),
                        privileged: Some(spec.privileged),
                        ..Default::default()
                    }),
                    entrypoint: spec.entrypoint,
                    working_dir: spec.workdir,
                    ..Default::default()
                },
            )
            .await
            .with_context(|| format!("creating container {name}"))?;
        Ok(())
    }

    async fn start_container(&self, name: &str) -> anyhow::Result<()> {
        jackin_diagnostics::debug_log!("docker", "start container {name}");
        self.inner
            .start_container(name, None::<StartContainerOptions>)
            .await
            .with_context(|| format!("starting container {name}"))
    }

    async fn remove_volume(&self, name: &str) -> anyhow::Result<()> {
        jackin_diagnostics::debug_log!("docker", "volume rm {name}");
        match self
            .inner
            .remove_volume(name, None::<RemoveVolumeOptions>)
            .await
        {
            Ok(()) => Ok(()),
            Err(e) if is_http_status(&e, 404) => Ok(()),
            Err(e) => Err(anyhow::Error::from(e).context(format!("removing volume {name}"))),
        }
    }

    async fn create_network(
        &self,
        name: &str,
        labels: HashMap<String, String>,
        internal: bool,
    ) -> anyhow::Result<()> {
        jackin_diagnostics::debug_log!("docker", "network create {name} internal={internal}");
        self.inner
            .create_network(NetworkCreateRequest {
                name: name.to_owned(),
                labels: Some(labels),
                internal: Some(internal),
                ..Default::default()
            })
            .await
            .with_context(|| format!("creating network {name}"))?;
        Ok(())
    }

    async fn remove_network(&self, name: &str) -> anyhow::Result<()> {
        jackin_diagnostics::debug_log!("docker", "network rm {name}");
        match self.inner.remove_network(name).await {
            Ok(()) => Ok(()),
            Err(e) if is_http_status(&e, 404) => Ok(()),
            Err(e) => Err(anyhow::Error::from(e).context(format!("removing network {name}"))),
        }
    }

    async fn list_networks(&self, label_filters: &[&str]) -> anyhow::Result<Vec<NetworkRow>> {
        jackin_diagnostics::debug_log!("docker", "network ls --filter label=...");
        let filters = build_label_filter(label_filters);
        let networks = self
            .inner
            .list_networks(Some(ListNetworksOptions { filters }))
            .await
            .context("listing networks")?;

        Ok(networks
            .into_iter()
            .filter_map(|n| {
                let name = n.name?;
                let labels = n.labels.unwrap_or_default();
                Some(NetworkRow { name, labels })
            })
            .collect())
    }

    async fn list_image_tags(&self, reference_filter: &str) -> anyhow::Result<Vec<String>> {
        jackin_diagnostics::debug_log!("docker", "images --filter reference={reference_filter}");
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
    }

    async fn remove_image(&self, name: &str) -> anyhow::Result<RemoveImageOutcome> {
        jackin_diagnostics::debug_log!("docker", "rmi {name}");
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
    }

    async fn inspect_image_labels(&self, image: &str) -> anyhow::Result<HashMap<String, String>> {
        jackin_diagnostics::debug_log!("docker", "inspect image:{image}");
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
    }

    async fn pull_image(&self, image: &str) -> anyhow::Result<()> {
        use bollard::query_parameters::CreateImageOptions;
        jackin_diagnostics::debug_log!("docker", "pull {image}");
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
    }

    async fn exec_capture(&self, container: &str, cmd: &[&str]) -> anyhow::Result<String> {
        let redacted_cmd = cmd
            .iter()
            .map(|arg| jackin_diagnostics::redact::redact_text(arg).into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        jackin_diagnostics::debug_log!("docker", "exec {} {}", container, redacted_cmd);
        let exec = self
            .inner
            .create_exec(
                container,
                CreateExecOptions {
                    cmd: Some(cmd.iter().map(ToString::to_string).collect()),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .with_context(|| format!("creating exec in {container}"))?;

        let mut output_buf = String::new();
        match self
            .inner
            .start_exec(&exec.id, None::<StartExecOptions>)
            .await
            .with_context(|| format!("starting exec in {container}"))?
        {
            StartExecResults::Attached { mut output, .. } => {
                while let Some(chunk) = output.next().await {
                    match chunk.with_context(|| format!("reading exec output from {container}"))? {
                        LogOutput::StdOut { message } | LogOutput::StdErr { message } => {
                            output_buf.push_str(&String::from_utf8_lossy(&message));
                        }
                        _ => {}
                    }
                }
            }
            StartExecResults::Detached => {
                return Err(DockerError::ExecDetached {
                    container: container.to_owned(),
                }
                .into());
            }
        }

        let inspect = self
            .inner
            .inspect_exec(&exec.id)
            .await
            .with_context(|| format!("inspecting exec result in {container}"))?;
        let exit_code = inspect.exit_code.unwrap_or(-1);
        if exit_code != 0 {
            return Err(DockerError::ExecNonZero {
                container: container.to_owned(),
                exit_code,
                output: output_buf.trim().to_owned(),
            }
            .into());
        }

        Ok(output_buf.trim().to_owned())
    }

    async fn inspect_network(&self, name: &str) -> anyhow::Result<Option<NetworkRow>> {
        jackin_diagnostics::debug_log!("docker", "network inspect {name}");
        match self
            .inner
            .inspect_network(
                name,
                None::<bollard::query_parameters::InspectNetworkOptions>,
            )
            .await
        {
            Ok(n) => {
                let net_name = n.name.unwrap_or_else(|| name.to_owned());
                let labels = n.labels.unwrap_or_default();
                Ok(Some(NetworkRow {
                    name: net_name,
                    labels,
                }))
            }
            Err(e) if is_http_status(&e, 404) => Ok(None),
            Err(e) => Err(anyhow::Error::from(e).context(format!("inspecting network {name}"))),
        }
    }
}

#[cfg(test)]
mod tests;
