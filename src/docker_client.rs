use std::{collections::HashMap, ffi::OsStr, process::Command, sync::OnceLock};

use anyhow::{Context, bail};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerState {
    NotFound,
    InspectUnavailable(String),
    Running,
    Paused,
    Restarting,
    Removing,
    Created,
    Dead,
    Stopped { exit_code: i32, oom_killed: bool },
}

impl ContainerState {
    #[must_use]
    pub fn short_label(&self) -> String {
        match self {
            Self::Running => "running".to_string(),
            Self::Paused => "paused".to_string(),
            Self::Restarting => "restarting".to_string(),
            Self::Removing => "removing".to_string(),
            Self::Created => "created".to_string(),
            Self::Dead => "dead".to_string(),
            Self::Stopped {
                exit_code,
                oom_killed: false,
            } => format!("stopped exit:{exit_code}"),
            Self::Stopped {
                oom_killed: true, ..
            } => "stopped oom_killed".to_string(),
            Self::NotFound => "missing".to_string(),
            Self::InspectUnavailable(_) => "unavailable".to_string(),
        }
    }

    #[must_use]
    pub fn inspect_label(&self) -> String {
        match self {
            Self::InspectUnavailable(reason) => format!("unavailable: {reason}"),
            _ => self.short_label(),
        }
    }

    /// Returns `true` for every state except `NotFound`.
    #[must_use]
    pub const fn is_present(&self) -> bool {
        !matches!(self, Self::NotFound)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerRow {
    pub name: String,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRow {
    pub name: String,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveImageOutcome {
    Removed,
    InUse,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerSpec {
    pub image: String,
    pub hostname: Option<String>,
    pub env: Vec<String>,
    pub labels: HashMap<String, String>,
    pub network: String,
    pub binds: Vec<String>,
    pub entrypoint: Option<Vec<String>>,
    pub privileged: bool,
    pub workdir: Option<String>,
}

pub trait DockerApi {
    #[must_use]
    async fn inspect_container_state(&self, name: &str) -> ContainerState;
    async fn remove_container(&self, name: &str) -> anyhow::Result<()>;
    async fn list_containers(
        &self,
        label_filters: &[&str],
        all: bool,
    ) -> anyhow::Result<Vec<ContainerRow>>;
    async fn create_container(&self, name: &str, spec: ContainerSpec) -> anyhow::Result<()>;
    async fn start_container(&self, name: &str) -> anyhow::Result<()>;
    async fn remove_volume(&self, name: &str) -> anyhow::Result<()>;
    async fn create_network(
        &self,
        name: &str,
        labels: HashMap<String, String>,
    ) -> anyhow::Result<()>;
    async fn remove_network(&self, name: &str) -> anyhow::Result<()>;
    async fn list_networks(&self, label_filters: &[&str]) -> anyhow::Result<Vec<NetworkRow>>;
    async fn inspect_network(&self, name: &str) -> anyhow::Result<Option<NetworkRow>>;
    async fn list_image_tags(&self, reference_filter: &str) -> anyhow::Result<Vec<String>>;
    async fn remove_image(&self, name: &str) -> anyhow::Result<RemoveImageOutcome>;
    async fn inspect_image_labels(&self, image: &str) -> anyhow::Result<HashMap<String, String>>;
    async fn inspect_image_label(
        &self,
        image: &str,
        label: &str,
    ) -> anyhow::Result<Option<String>> {
        Ok(self.inspect_image_labels(image).await?.remove(label))
    }
    async fn pull_image(&self, image: &str) -> anyhow::Result<()>;
    async fn exec_capture(&self, container: &str, cmd: &[&str]) -> anyhow::Result<String>;
}

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
                "active Docker context uses TLS transport ({host}); this jackin build cannot mirror TLS Docker contexts for Bollard API calls"
            ),
            UnsupportedReason::ContextTlsMaterial => format!(
                "active Docker context for {host} includes TLS settings; this jackin build cannot mirror Docker context TLS material for Bollard API calls"
            ),
            UnsupportedReason::UnsupportedUri => {
                format!("active Docker context uses unsupported Docker host URI {host}")
            }
        };
        format!("{detail}. {OVERRIDE_HINT}")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DockerContextEndpoint {
    host: String,
    skip_tls_verify: bool,
    has_tls_material: bool,
}

impl DockerContextEndpoint {
    fn connection_choice(self) -> ConnectionChoice {
        // `host` is trimmed by `parse_docker_context_endpoint` before this struct is built.
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

const OVERRIDE_HINT: &str =
    "Set DOCKER_HOST to a unix:// or tcp:// endpoint reachable without TLS to override.";

fn context_host_supported_without_extra_settings(host: &str) -> bool {
    host.starts_with("unix://")
        || host.starts_with("tcp://")
        || host.starts_with("http://")
        || cfg!(windows) && host.starts_with("npipe://")
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
/// `connect()` is sync and called before any tokio runtime exists
/// (`src/app/mod.rs:1703` runs inside `std::thread::scope`), while `ShellRunner`
/// wraps `tokio::process::Command`.
fn connect_to_cli_docker_context() -> anyhow::Result<Docker> {
    let env_set = docker_host_env_is_set();
    // Skip the subprocess when DOCKER_HOST already wins per Docker CLI precedence.
    let ctx_endpoint = if env_set {
        None
    } else {
        cached_context_endpoint()
    };
    match choose_connection(env_set, ctx_endpoint) {
        ConnectionChoice::Defaults => Ok(Docker::connect_with_defaults()?),
        ConnectionChoice::Host(host) => {
            crate::debug_log!("docker", "connect context host {host}");
            Ok(Docker::connect_with_host(&host)?)
        }
        ConnectionChoice::Unsupported { reason, host } => {
            bail!(ConnectionChoice::unsupported_message(&reason, &host))
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
/// `docker context inspect` result across repeated `connect()` calls
/// (the console drift checks at `console/manager/input/save.rs` connect on each save).
fn cached_context_endpoint() -> Option<DockerContextEndpoint> {
    static CACHE: OnceLock<Option<DockerContextEndpoint>> = OnceLock::new();
    CACHE.get_or_init(active_docker_context_endpoint).clone()
}

fn active_docker_context_endpoint() -> Option<DockerContextEndpoint> {
    let mut cmd = Command::new("docker");
    cmd.args(["context", "inspect", "--format", "{{json .}}"]);
    // Belt-and-suspenders: `docker context inspect` already resolves the current
    // context via Docker CLI's `DOCKER_CONTEXT` lookup, but pinning the arg makes
    // the resolution explicit and survives any future CLI behaviour drift.
    if let Some(ctx) = std::env::var("DOCKER_CONTEXT")
        .ok()
        .filter(|v| !v.is_empty())
    {
        cmd.arg(ctx);
    }
    let output = match cmd.output() {
        Ok(output) => output,
        Err(err) => {
            crate::debug_log!("docker", "context inspect spawn failed: {err}");
            return None;
        }
    };
    if !output.status.success() {
        crate::debug_log!(
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
            crate::debug_log!(
                "docker",
                "context inspect json parse failed: {err} stdout={}",
                String::from_utf8_lossy(stdout).trim()
            );
            return None;
        }
    };
    let endpoint = context.endpoints.docker?;
    let host = endpoint.host.trim();
    let has_tls_material = context
        .tls_material
        .get("docker")
        .is_some_and(tls_material_present);
    Some(DockerContextEndpoint {
        host: host.to_string(),
        skip_tls_verify: endpoint.skip_tls_verify,
        has_tls_material,
    })
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
        "label".to_string(),
        label_filters.iter().map(ToString::to_string).collect(),
    );
    Some(map)
}

impl DockerApi for BollardDockerClient {
    async fn inspect_container_state(&self, name: &str) -> ContainerState {
        crate::debug_log!("docker", "inspect container {name}");
        let result = self
            .inner
            .inspect_container(name, None::<InspectContainerOptions>)
            .await;

        let state = match result {
            Err(ref e) if is_http_status(e, 404) => ContainerState::NotFound,
            Err(e) => ContainerState::InspectUnavailable(e.to_string()),
            Ok(info) => {
                let Some(state) = info.state else {
                    return ContainerState::InspectUnavailable("no state field".to_string());
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
        crate::debug_log!(
            "docker",
            "inspect container {name} -> {}",
            state.inspect_label()
        );
        state
    }

    async fn remove_container(&self, name: &str) -> anyhow::Result<()> {
        crate::debug_log!("docker", "rm -f {name}");
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
        crate::debug_log!(
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
                let name = raw_name.trim_start_matches('/').to_string();
                let labels = s.labels.unwrap_or_default();
                ContainerRow { name, labels }
            })
            .collect())
    }

    async fn create_container(&self, name: &str, spec: ContainerSpec) -> anyhow::Result<()> {
        crate::debug_log!("docker", "create container {name} image={}", spec.image);
        self.inner
            .create_container(
                Some(CreateContainerOptions {
                    name: Some(name.to_string()),
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
        crate::debug_log!("docker", "start container {name}");
        self.inner
            .start_container(name, None::<StartContainerOptions>)
            .await
            .with_context(|| format!("starting container {name}"))
    }

    async fn remove_volume(&self, name: &str) -> anyhow::Result<()> {
        crate::debug_log!("docker", "volume rm {name}");
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
    ) -> anyhow::Result<()> {
        crate::debug_log!("docker", "network create {name}");
        self.inner
            .create_network(NetworkCreateRequest {
                name: name.to_string(),
                labels: Some(labels),
                ..Default::default()
            })
            .await
            .with_context(|| format!("creating network {name}"))?;
        Ok(())
    }

    async fn remove_network(&self, name: &str) -> anyhow::Result<()> {
        crate::debug_log!("docker", "network rm {name}");
        match self.inner.remove_network(name).await {
            Ok(()) => Ok(()),
            Err(e) if is_http_status(&e, 404) => Ok(()),
            Err(e) => Err(anyhow::Error::from(e).context(format!("removing network {name}"))),
        }
    }

    async fn list_networks(&self, label_filters: &[&str]) -> anyhow::Result<Vec<NetworkRow>> {
        crate::debug_log!("docker", "network ls --filter label=...");
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
        crate::debug_log!("docker", "images --filter reference={reference_filter}");
        let mut filters = HashMap::new();
        filters.insert("reference".to_string(), vec![reference_filter.to_string()]);
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
        crate::debug_log!("docker", "rmi {name}");
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
        crate::debug_log!("docker", "inspect image:{image}");
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
        crate::debug_log!("docker", "pull {image}");
        let mut stream = self.inner.create_image(
            Some(CreateImageOptions {
                from_image: Some(image.to_string()),
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
        crate::debug_log!("docker", "exec {} {}", container, cmd.join(" "));
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
                anyhow::bail!(
                    "exec in {container} returned Detached — attach_stdout was set but exec ran detached"
                );
            }
        }

        let inspect = self
            .inner
            .inspect_exec(&exec.id)
            .await
            .with_context(|| format!("inspecting exec result in {container}"))?;
        let exit_code = inspect.exit_code.unwrap_or(-1);
        if exit_code != 0 {
            anyhow::bail!(
                "exec in {container} exited with code {exit_code}: {}",
                output_buf.trim()
            );
        }

        Ok(output_buf.trim().to_string())
    }

    async fn inspect_network(&self, name: &str) -> anyhow::Result<Option<NetworkRow>> {
        crate::debug_log!("docker", "network inspect {name}");
        match self
            .inner
            .inspect_network(
                name,
                None::<bollard::query_parameters::InspectNetworkOptions>,
            )
            .await
        {
            Ok(n) => {
                let net_name = n.name.unwrap_or_else(|| name.to_string());
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
pub(crate) struct FakeDockerClient {
    pub(crate) recorded: std::cell::RefCell<Vec<String>>,
    pub(crate) inspect_queue: std::cell::RefCell<std::collections::VecDeque<ContainerState>>,
    pub(crate) list_containers_queue:
        std::cell::RefCell<std::collections::VecDeque<Vec<ContainerRow>>>,
    pub(crate) list_networks_queue: std::cell::RefCell<std::collections::VecDeque<Vec<NetworkRow>>>,
    pub(crate) list_image_tags_queue: std::cell::RefCell<std::collections::VecDeque<Vec<String>>>,
    pub(crate) remove_image_queue:
        std::cell::RefCell<std::collections::VecDeque<RemoveImageOutcome>>,
    pub(crate) exec_capture_queue: std::cell::RefCell<std::collections::VecDeque<String>>,
    pub(crate) inspect_image_labels_queue:
        std::cell::RefCell<std::collections::VecDeque<HashMap<String, String>>>,
    pub(crate) inspect_network_queue:
        std::cell::RefCell<std::collections::VecDeque<Option<NetworkRow>>>,
    pub(crate) fail_with: Vec<(String, String)>,
    pub(crate) created_containers: std::cell::RefCell<Vec<(String, ContainerSpec)>>,
    pub(crate) created_networks: std::cell::RefCell<Vec<(String, HashMap<String, String>)>>,
}

#[cfg(test)]
impl Default for FakeDockerClient {
    fn default() -> Self {
        Self {
            recorded: std::cell::RefCell::new(Vec::new()),
            inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
            list_containers_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
            list_networks_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
            list_image_tags_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
            remove_image_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
            exec_capture_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
            inspect_image_labels_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
            inspect_network_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
            fail_with: Vec::new(),
            created_containers: std::cell::RefCell::new(Vec::new()),
            created_networks: std::cell::RefCell::new(Vec::new()),
        }
    }
}

#[cfg(test)]
impl FakeDockerClient {
    fn check_fail(&self, op: &str) -> anyhow::Result<()> {
        if let Some((_, msg)) = self
            .fail_with
            .iter()
            .find(|(pat, _)| op.contains(pat.as_str()))
        {
            anyhow::bail!("{msg}");
        }
        Ok(())
    }

    fn record(&self, entry: &str) {
        self.recorded.borrow_mut().push(entry.to_string());
    }

    fn ignore_if_missing(result: anyhow::Result<()>) -> anyhow::Result<()> {
        result.or_else(|e| {
            if e.to_string().to_ascii_lowercase().contains("no such") {
                Ok(())
            } else {
                Err(e)
            }
        })
    }

    fn pop_inspect(&self) -> ContainerState {
        self.inspect_queue
            .borrow_mut()
            .pop_front()
            .unwrap_or(ContainerState::NotFound)
    }

    fn pop_list_containers(&self) -> Vec<ContainerRow> {
        self.list_containers_queue
            .borrow_mut()
            .pop_front()
            .unwrap_or_default()
    }

    fn pop_list_networks(&self) -> Vec<NetworkRow> {
        self.list_networks_queue
            .borrow_mut()
            .pop_front()
            .unwrap_or_default()
    }

    fn pop_list_image_tags(&self) -> Vec<String> {
        self.list_image_tags_queue
            .borrow_mut()
            .pop_front()
            .unwrap_or_default()
    }

    fn pop_remove_image(&self) -> RemoveImageOutcome {
        self.remove_image_queue
            .borrow_mut()
            .pop_front()
            .expect("remove_image called but remove_image_queue is empty")
    }

    fn pop_exec_capture(&self) -> String {
        self.exec_capture_queue
            .borrow_mut()
            .pop_front()
            .unwrap_or_default()
    }

    fn pop_inspect_image_labels(&self) -> HashMap<String, String> {
        self.inspect_image_labels_queue
            .borrow_mut()
            .pop_front()
            .unwrap_or_default()
    }

    fn pop_inspect_network(&self) -> Option<NetworkRow> {
        self.inspect_network_queue
            .borrow_mut()
            .pop_front()
            .flatten()
    }
}

#[cfg(test)]
impl DockerApi for FakeDockerClient {
    async fn inspect_container_state(&self, name: &str) -> ContainerState {
        let op = format!("docker inspect {name}");
        self.record(&op);
        if let Some((_, msg)) = self
            .fail_with
            .iter()
            .find(|(pat, _)| op.contains(pat.as_str()))
        {
            let msg = msg.clone();
            let lower = msg.to_ascii_lowercase();
            if lower.contains("no such object")
                || lower.contains("no such container")
                || lower.contains("no such image")
            {
                return ContainerState::NotFound;
            }
            return ContainerState::InspectUnavailable(msg);
        }
        self.pop_inspect()
    }

    async fn remove_container(&self, name: &str) -> anyhow::Result<()> {
        let op = format!("docker rm -f {name}");
        self.record(&op);
        Self::ignore_if_missing(self.check_fail(&op))
    }

    async fn list_containers(
        &self,
        label_filters: &[&str],
        all: bool,
    ) -> anyhow::Result<Vec<ContainerRow>> {
        let filter_str = label_filters.join(" --filter ");
        let op = if all {
            format!("docker ps -a --filter {filter_str}")
        } else {
            format!("docker ps --filter {filter_str}")
        };
        self.record(&op);
        self.check_fail(&op)?;
        Ok(self.pop_list_containers())
    }

    async fn create_container(&self, name: &str, spec: ContainerSpec) -> anyhow::Result<()> {
        let op = format!("create_container:{name}");
        self.record(&op);
        self.check_fail(&op)?;
        self.created_containers
            .borrow_mut()
            .push((name.to_string(), spec));
        Ok(())
    }

    async fn start_container(&self, name: &str) -> anyhow::Result<()> {
        let op = format!("start_container:{name}");
        self.record(&op);
        self.check_fail(&op)
    }

    async fn remove_volume(&self, name: &str) -> anyhow::Result<()> {
        let op = format!("docker volume rm {name}");
        self.record(&op);
        Self::ignore_if_missing(self.check_fail(&op))
    }

    async fn create_network(
        &self,
        name: &str,
        labels: HashMap<String, String>,
    ) -> anyhow::Result<()> {
        let op = format!("docker network create {name}");
        self.record(&op);
        self.created_networks
            .borrow_mut()
            .push((name.to_string(), labels));
        self.check_fail(&op)
    }

    async fn remove_network(&self, name: &str) -> anyhow::Result<()> {
        let op = format!("docker network rm {name}");
        self.record(&op);
        Self::ignore_if_missing(self.check_fail(&op))
    }

    async fn list_networks(&self, label_filters: &[&str]) -> anyhow::Result<Vec<NetworkRow>> {
        let filter_str = label_filters.join(" --filter ");
        let op = format!("docker network ls --filter {filter_str}");
        self.record(&op);
        self.check_fail(&op)?;
        Ok(self.pop_list_networks())
    }

    async fn inspect_network(&self, name: &str) -> anyhow::Result<Option<NetworkRow>> {
        let op = format!("docker network inspect {name}");
        self.record(&op);
        self.check_fail(&op)?;
        Ok(self.pop_inspect_network())
    }

    async fn list_image_tags(&self, reference_filter: &str) -> anyhow::Result<Vec<String>> {
        let op = format!("docker images --filter reference={reference_filter}");
        self.record(&op);
        self.check_fail(&op)?;
        Ok(self.pop_list_image_tags())
    }

    async fn remove_image(&self, name: &str) -> anyhow::Result<RemoveImageOutcome> {
        let op = format!("docker rmi {name}");
        self.record(&op);
        self.check_fail(&op)?;
        Ok(self.pop_remove_image())
    }

    async fn inspect_image_labels(&self, image: &str) -> anyhow::Result<HashMap<String, String>> {
        let op = format!("docker inspect image:{image}");
        self.record(&op);
        self.check_fail(&op)?;
        Ok(self.pop_inspect_image_labels())
    }

    async fn pull_image(&self, image: &str) -> anyhow::Result<()> {
        let op = format!("docker pull {image}");
        self.record(&op);
        self.check_fail(&op)
    }

    async fn exec_capture(&self, container: &str, cmd: &[&str]) -> anyhow::Result<String> {
        let op = format!("docker exec {} {}", container, cmd.join(" "));
        self.record(&op);
        self.check_fail(&op)?;
        Ok(self.pop_exec_capture())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choose_connection_env_only_returns_defaults() {
        assert_eq!(choose_connection(true, None), ConnectionChoice::Defaults);
    }

    #[test]
    fn choose_connection_env_overrides_context() {
        assert_eq!(
            choose_connection(true, Some(context_endpoint("unix:///ignored"))),
            ConnectionChoice::Defaults
        );
    }

    #[test]
    fn choose_connection_uses_context_when_env_unset() {
        assert_eq!(
            choose_connection(false, Some(context_endpoint("unix:///ctx"))),
            ConnectionChoice::Host("unix:///ctx".to_string())
        );
    }

    #[test]
    fn choose_connection_rejects_ssh_context() {
        assert_eq!(
            choose_connection(false, Some(context_endpoint("ssh://me@docker-host"))),
            ConnectionChoice::unsupported(UnsupportedReason::SshTransport, "ssh://me@docker-host")
        );
    }

    #[test]
    fn choose_connection_rejects_https_context() {
        assert_eq!(
            choose_connection(false, Some(context_endpoint("https://docker-host:2376"))),
            ConnectionChoice::unsupported(
                UnsupportedReason::TlsTransport,
                "https://docker-host:2376"
            )
        );
    }

    #[test]
    fn choose_connection_rejects_context_with_tls_material() {
        let endpoint = DockerContextEndpoint {
            has_tls_material: true,
            ..context_endpoint("tcp://docker-host:2376")
        };
        assert_eq!(
            choose_connection(false, Some(endpoint)),
            ConnectionChoice::unsupported(
                UnsupportedReason::ContextTlsMaterial,
                "tcp://docker-host:2376"
            )
        );
    }

    #[test]
    fn choose_connection_rejects_context_with_tls_skip_verify() {
        let endpoint = DockerContextEndpoint {
            skip_tls_verify: true,
            ..context_endpoint("tcp://docker-host:2376")
        };
        assert_eq!(
            choose_connection(false, Some(endpoint)),
            ConnectionChoice::unsupported(
                UnsupportedReason::ContextTlsMaterial,
                "tcp://docker-host:2376"
            )
        );
    }

    #[test]
    fn choose_connection_rejects_context_with_unknown_uri() {
        assert_eq!(
            choose_connection(false, Some(context_endpoint("fd://0"))),
            ConnectionChoice::unsupported(UnsupportedReason::UnsupportedUri, "fd://0")
        );
    }

    #[test]
    fn unsupported_message_appends_override_hint() {
        let msg = ConnectionChoice::unsupported_message(
            &UnsupportedReason::SshTransport,
            "ssh://me@docker-host",
        );
        assert!(msg.contains("SSH transport"));
        assert!(msg.contains("ssh://me@docker-host"));
        assert!(msg.ends_with(OVERRIDE_HINT));
    }

    #[test]
    fn choose_connection_falls_back_to_defaults_when_no_context() {
        assert_eq!(choose_connection(false, None), ConnectionChoice::Defaults);
    }

    #[test]
    fn docker_host_env_is_set_from_recognises_unset_and_empty_as_unset() {
        assert!(!docker_host_env_is_set_from(None));
        assert!(!docker_host_env_is_set_from(Some(OsStr::new(""))));
    }

    #[test]
    fn docker_host_env_is_set_from_recognises_non_empty_as_set() {
        assert!(docker_host_env_is_set_from(Some(OsStr::new(
            "tcp://127.0.0.1:2375"
        ))));
        assert!(docker_host_env_is_set_from(Some(OsStr::new(
            "unix:///var/run/docker.sock"
        ))));
    }

    #[test]
    fn parse_docker_context_endpoint_reads_host_and_tls_flags() {
        let endpoint = parse_docker_context_endpoint(
            br#"{
                "Endpoints": {
                    "docker": {
                        "Host": "tcp://docker-host:2376",
                        "SkipTLSVerify": true
                    }
                },
                "TLSMaterial": {
                    "docker": ["ca.pem", "cert.pem", "key.pem"]
                }
            }"#,
        )
        .unwrap();
        assert_eq!(endpoint.host, "tcp://docker-host:2376");
        assert!(endpoint.skip_tls_verify);
        assert!(endpoint.has_tls_material);
    }

    #[test]
    fn parse_docker_context_endpoint_returns_none_without_docker_endpoint() {
        assert_eq!(
            parse_docker_context_endpoint(br#"{"Endpoints": {}, "TLSMaterial": {}}"#),
            None
        );
    }

    #[test]
    fn parse_docker_context_endpoint_returns_none_on_malformed_json() {
        assert_eq!(parse_docker_context_endpoint(b""), None);
        assert_eq!(parse_docker_context_endpoint(b"not json"), None);
        assert_eq!(
            parse_docker_context_endpoint(br#"{"Endpoints": {"docker"#),
            None
        );
    }

    #[test]
    fn tls_material_present_treats_emptiness_as_absent() {
        use serde_json::Value;
        assert!(!tls_material_present(&Value::Null));
        assert!(!tls_material_present(&serde_json::json!([])));
        assert!(!tls_material_present(&serde_json::json!({})));
        assert!(!tls_material_present(&Value::String(String::new())));
        assert!(!tls_material_present(&Value::String("   ".to_string())));
        assert!(!tls_material_present(&Value::Bool(false)));
    }

    #[test]
    fn tls_material_present_treats_populated_values_as_present() {
        use serde_json::Value;
        assert!(tls_material_present(&serde_json::json!(["ca.pem"])));
        assert!(tls_material_present(&serde_json::json!({"ca": "ca.pem"})));
        assert!(tls_material_present(&Value::String("ca.pem".to_string())));
        assert!(tls_material_present(&Value::Bool(true)));
        assert!(tls_material_present(&serde_json::json!(1)));
    }

    fn context_endpoint(host: &str) -> DockerContextEndpoint {
        DockerContextEndpoint {
            host: host.to_string(),
            ..DockerContextEndpoint::default()
        }
    }

    #[test]
    fn container_state_short_label() {
        let cases: &[(ContainerState, &str)] = &[
            (ContainerState::Running, "running"),
            (ContainerState::Paused, "paused"),
            (ContainerState::Restarting, "restarting"),
            (ContainerState::Removing, "removing"),
            (ContainerState::Created, "created"),
            (ContainerState::Dead, "dead"),
            (
                ContainerState::Stopped {
                    exit_code: 0,
                    oom_killed: false,
                },
                "stopped exit:0",
            ),
            (
                ContainerState::Stopped {
                    exit_code: 1,
                    oom_killed: false,
                },
                "stopped exit:1",
            ),
            (
                ContainerState::Stopped {
                    exit_code: 0,
                    oom_killed: true,
                },
                "stopped oom_killed",
            ),
            (ContainerState::NotFound, "missing"),
            (
                ContainerState::InspectUnavailable("reason".to_string()),
                "unavailable",
            ),
        ];

        for (state, expected) in cases {
            assert_eq!(
                state.short_label(),
                *expected,
                "short_label mismatch for {state:?}"
            );
        }
    }
}
