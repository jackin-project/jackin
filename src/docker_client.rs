use std::collections::HashMap;

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

// ── ContainerState ────────────────────────────────────────────────────────

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

// ── Other public types ────────────────────────────────────────────────────

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

// ── DockerApi trait ───────────────────────────────────────────────────────

pub trait DockerApi {
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
    async fn inspect_image_label(&self, image: &str, label: &str)
    -> anyhow::Result<Option<String>>;
    async fn pull_image(&self, image: &str) -> anyhow::Result<()>;
    async fn exec_capture(&self, container: &str, cmd: &[&str]) -> anyhow::Result<String>;
}

// ── BollardDockerClient ───────────────────────────────────────────────────

pub struct BollardDockerClient {
    inner: Docker,
}

impl BollardDockerClient {
    pub fn connect() -> anyhow::Result<Self> {
        let inner =
            Docker::connect_with_local_defaults().context("failed to connect to Docker daemon")?;
        Ok(Self { inner })
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
        let result = self
            .inner
            .inspect_container(name, None::<InspectContainerOptions>)
            .await;

        match result {
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
        }
    }

    async fn remove_container(&self, name: &str) -> anyhow::Result<()> {
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
        self.inner
            .start_container(name, None::<StartContainerOptions>)
            .await
            .with_context(|| format!("starting container {name}"))
    }

    async fn remove_volume(&self, name: &str) -> anyhow::Result<()> {
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
        match self.inner.remove_network(name).await {
            Ok(()) => Ok(()),
            Err(e) if is_http_status(&e, 404) => Ok(()),
            Err(e) => Err(anyhow::Error::from(e).context(format!("removing network {name}"))),
        }
    }

    async fn list_networks(&self, label_filters: &[&str]) -> anyhow::Result<Vec<NetworkRow>> {
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

    async fn inspect_image_label(
        &self,
        image: &str,
        label: &str,
    ) -> anyhow::Result<Option<String>> {
        match self.inner.inspect_image(image).await {
            Err(e) if is_http_status(&e, 404) => Ok(None),
            Err(e) => Err(anyhow::Error::from(e).context(format!("inspecting image {image}"))),
            Ok(info) => {
                let value = info
                    .config
                    .and_then(|c| c.labels)
                    .and_then(|labels| labels.get(label).cloned())
                    .filter(|s| !s.is_empty());
                Ok(value)
            }
        }
    }

    async fn pull_image(&self, image: &str) -> anyhow::Result<()> {
        use bollard::query_parameters::CreateImageOptions;
        crate::tui::emit_debug_line("pull", image);
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

// ── FakeDockerClient (test only) ──────────────────────────────────────────

#[cfg(test)]
pub struct FakeDockerClient {
    pub recorded: std::cell::RefCell<Vec<String>>,
    pub inspect_queue: std::cell::RefCell<std::collections::VecDeque<ContainerState>>,
    pub list_containers_queue: std::cell::RefCell<std::collections::VecDeque<Vec<ContainerRow>>>,
    pub list_networks_queue: std::cell::RefCell<std::collections::VecDeque<Vec<NetworkRow>>>,
    pub list_image_tags_queue: std::cell::RefCell<std::collections::VecDeque<Vec<String>>>,
    pub remove_image_queue: std::cell::RefCell<std::collections::VecDeque<RemoveImageOutcome>>,
    pub exec_capture_queue: std::cell::RefCell<std::collections::VecDeque<String>>,
    pub inspect_image_label_queue: std::cell::RefCell<std::collections::VecDeque<Option<String>>>,
    pub inspect_network_queue: std::cell::RefCell<std::collections::VecDeque<Option<NetworkRow>>>,
    pub fail_with: Vec<(String, String)>,
    pub created_containers: std::cell::RefCell<Vec<(String, ContainerSpec)>>,
    pub created_networks: std::cell::RefCell<Vec<(String, HashMap<String, String>)>>,
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
            inspect_image_label_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
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

    fn pop_inspect_image_label(&self) -> Option<String> {
        self.inspect_image_label_queue
            .borrow_mut()
            .pop_front()
            .unwrap_or(None)
    }

    fn pop_inspect_network(&self) -> Option<NetworkRow> {
        self.inspect_network_queue
            .borrow_mut()
            .pop_front()
            .unwrap_or(None)
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
        self.created_containers
            .borrow_mut()
            .push((name.to_string(), spec));
        self.check_fail(&op)
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

    async fn inspect_image_label(
        &self,
        image: &str,
        label: &str,
    ) -> anyhow::Result<Option<String>> {
        let op = format!("docker inspect image:{image} label:{label}");
        self.record(&op);
        self.check_fail(&op)?;
        Ok(self.pop_inspect_image_label())
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
