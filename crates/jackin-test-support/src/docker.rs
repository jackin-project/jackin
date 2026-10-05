//! `FakeDockerClient`: an in-memory `jackin_core::DockerApi` fake.

#![expect(
    clippy::expect_used,
    reason = "test support fixture setup should fail immediately with source location"
)]

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use jackin_core::{
    ContainerHandle, ContainerInspection, ContainerRow, ContainerSpec, ContainerState, DaemonServerId, DockerApi,
    NetworkId, NetworkRow, RemoveImageOutcome, VolumeRow,
};

#[derive(Debug)]
pub struct FakeDockerClient {
    /// Explicit test daemon identity; replace to model daemon continuity breaks.
    pub daemon_server_id: std::cell::RefCell<DaemonServerId>,
    /// Script exact `/info` responses, including failures, for continuity tests.
    pub daemon_server_id_queue:
        std::cell::RefCell<std::collections::VecDeque<Result<DaemonServerId, String>>>,
    /// Explicit controller transport for endpoint admission fixtures.
    pub controller_endpoint: jackin_core::ControllerEndpoint,
    pub recorded: std::cell::RefCell<Vec<String>>,
    pub inspect_queue: std::cell::RefCell<std::collections::VecDeque<ContainerState>>,
    /// Optional state sequence for ID-bound inspections. This is separate
    /// from name lookup state so tests can model a container exiting after
    /// create/start without pretending a name lookup returned a replacement.
    pub inspect_by_id_queue: std::cell::RefCell<std::collections::VecDeque<ContainerState>>,
    /// Per-container-name inspect overrides, checked before `inspect_queue`.
    /// Lets a test pin one container's state by name regardless of how many
    /// other (queue-order-dependent) inspects run first.
    pub inspect_state_by_name: std::cell::RefCell<HashMap<String, ContainerState>>,
    /// Current daemon IDs returned when a name is resolved or a container is
    /// created. Tests can change this map to model same-name replacement.
    pub container_id_by_name: std::cell::RefCell<HashMap<String, String>>,
    /// Current network name mappings; mutations still target captured IDs.
    pub network_id_by_name: std::cell::RefCell<HashMap<String, NetworkId>>,
    /// Named volume metadata; captures Docker's name-based volume semantics.
    pub volumes_by_name: std::cell::RefCell<HashMap<String, VolumeRow>>,
    /// ID-bound lifecycle operations, retained separately from the historical
    /// human-readable operation log.
    pub bound_operations: std::cell::RefCell<Vec<String>>,
    pub list_containers_queue: std::cell::RefCell<std::collections::VecDeque<Vec<ContainerRow>>>,
    pub list_networks_queue: std::cell::RefCell<std::collections::VecDeque<Vec<NetworkRow>>>,
    pub list_image_tags_queue: std::cell::RefCell<std::collections::VecDeque<Vec<String>>>,
    pub remove_image_queue: std::cell::RefCell<std::collections::VecDeque<RemoveImageOutcome>>,
    pub exec_capture_queue: std::cell::RefCell<std::collections::VecDeque<String>>,
    pub inspect_image_labels_queue:
        std::cell::RefCell<std::collections::VecDeque<HashMap<String, String>>>,
    pub inspect_network_queue: std::cell::RefCell<std::collections::VecDeque<Option<NetworkRow>>>,
    pub fail_with: Vec<(String, String)>,
    pub created_containers: std::cell::RefCell<Vec<(String, ContainerSpec)>>,
    /// `(name, labels, internal)` — tracks networks created via `DockerApi::create_network`.
    #[expect(
        clippy::type_complexity,
        reason = "test record tuple mirrors the API signature; factoring adds indirection without clarity"
    )]
    pub created_networks: std::cell::RefCell<Vec<(String, HashMap<String, String>, bool)>>,
    /// Optional test-only observation hook invoked for every Docker operation.
    pub operation_hook: Option<fn(&str)>,
}

impl Default for FakeDockerClient {
    fn default() -> Self {
        Self {
            daemon_server_id: std::cell::RefCell::new(DaemonServerId::parse("jackin-test-daemon").expect("valid test daemon ID")),
            daemon_server_id_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
            controller_endpoint: jackin_core::ControllerEndpoint::Unix {
                socket: "/var/run/docker.sock".into(),
            },
            recorded: std::cell::RefCell::new(Vec::new()),
            inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
            inspect_by_id_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
            inspect_state_by_name: std::cell::RefCell::new(HashMap::new()),
            container_id_by_name: std::cell::RefCell::new(HashMap::new()),
            network_id_by_name: std::cell::RefCell::new(HashMap::new()),
            volumes_by_name: std::cell::RefCell::new(HashMap::new()),
            bound_operations: std::cell::RefCell::new(Vec::new()),
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
            operation_hook: None,
        }
    }
}

impl FakeDockerClient {
    /// Replace the actual server ID reported by this fake.
    pub fn set_daemon_server_id(&self, id: DaemonServerId) {
        *self.daemon_server_id.borrow_mut() = id;
    }

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
        self.recorded.borrow_mut().push(entry.to_owned());
        if let Some(hook) = self.operation_hook {
            hook(entry);
        }
    }

    fn handle_for(&self, name: &str) -> ContainerHandle {
        let id = self
            .container_id_by_name
            .borrow()
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_owned());
        ContainerHandle::new(name, id).expect("fake container handle must be non-empty")
    }

    fn record_bound(&self, operation: &str, container: &ContainerHandle) {
        self.bound_operations
            .borrow_mut()
            .push(format!("{operation}:{}", container.id()));
    }

    fn owns_current_name(&self, container: &ContainerHandle) -> bool {
        self.container_id_by_name
            .borrow()
            .get(container.name())
            .is_none_or(|id| id == container.id())
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

impl DockerApi for FakeDockerClient {
    fn controller_endpoint(&self) -> &jackin_core::ControllerEndpoint {
        &self.controller_endpoint
    }
    async fn daemon_server_id(&self) -> anyhow::Result<DaemonServerId> {
        std::future::ready(()).await;
        let op = "docker info";
        self.record(op);
        self.check_fail(op)?;
        if let Some(response) = self.daemon_server_id_queue.borrow_mut().pop_front() {
            return response.map_err(|reason| anyhow::anyhow!("reading fake Docker daemon server identity: {reason}"));
        }
        Ok(self.daemon_server_id.borrow().clone())
    }

    async fn ping(&self) -> anyhow::Result<()> {
        std::future::ready(()).await;
        self.record("docker ping");
        self.check_fail("docker ping")
    }

    async fn inspect_container_by_name(&self, name: &str) -> ContainerInspection {
        std::future::ready(()).await;
        let op = format!("docker inspect {name}");
        self.record(&op);
        let state = if let Some((_, msg)) = self
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
                ContainerState::NotFound
            } else {
                ContainerState::InspectUnavailable(msg)
            }
        } else if let Some(state) = self.inspect_state_by_name.borrow().get(name) {
            state.clone()
        } else {
            self.pop_inspect()
        };
        let handle = (!matches!(
            state,
            ContainerState::NotFound | ContainerState::InspectUnavailable(_)
        ))
        .then(|| self.handle_for(name));
        ContainerInspection { handle, state }
    }

    async fn container_init_pid_by_id(&self, container: &ContainerHandle) -> anyhow::Result<u32> {
        std::future::ready(()).await;
        self.record(&format!("docker inspect init-pid {}", container.id()));
        anyhow::bail!("fake runtime has no host process identity proof")
    }

    async fn inspect_container_by_id(&self, container: &ContainerHandle) -> ContainerState {
        std::future::ready(()).await;
        self.record_bound("inspect", container);
        if !self.owns_current_name(container) {
            ContainerState::NotFound
        } else if let Some(state) = self.inspect_by_id_queue.borrow_mut().pop_front() {
            state
        } else if let Some(state) = self.inspect_state_by_name.borrow().get(container.name()) {
            state.clone()
        } else {
            self.pop_inspect()
        }
    }

    async fn remove_container_by_id(&self, container: &ContainerHandle) -> anyhow::Result<()> {
        std::future::ready(()).await;
        self.record_bound("remove", container);
        let name = container.name();
        let op = format!("docker rm -f {name}");
        self.record(&op);
        Self::ignore_if_missing(self.check_fail(&op))?;
        if self.owns_current_name(container) {
            self.container_id_by_name.borrow_mut().remove(name);
            self.inspect_state_by_name.borrow_mut().remove(name);
        }
        Ok(())
    }

    async fn list_containers(
        &self,
        label_filters: &[&str],
        all: bool,
    ) -> anyhow::Result<Vec<ContainerRow>> {
        std::future::ready(()).await;
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

    async fn create_container(
        &self,
        name: &str,
        spec: ContainerSpec,
    ) -> anyhow::Result<ContainerHandle> {
        std::future::ready(()).await;
        let op = format!("create_container:{name}");
        self.record(&op);
        self.check_fail(&op)?;
        self.created_containers
            .borrow_mut()
            .push((name.to_owned(), spec));
        let handle = self.handle_for(name);
        self.container_id_by_name
            .borrow_mut()
            .insert(name.to_owned(), handle.id().to_owned());
        self.inspect_state_by_name
            .borrow_mut()
            .insert(name.to_owned(), ContainerState::Created);
        Ok(handle)
    }

    async fn start_container_by_id(&self, container: &ContainerHandle) -> anyhow::Result<()> {
        std::future::ready(()).await;
        self.record_bound("start", container);
        let name = container.name();
        let op = format!("start_container:{name}");
        self.record(&op);
        self.check_fail(&op)?;
        if self.owns_current_name(container) {
            self.container_id_by_name
                .borrow_mut()
                .insert(name.to_owned(), container.id().to_owned());
            self.inspect_state_by_name
                .borrow_mut()
                .insert(name.to_owned(), ContainerState::Running);
        }
        Ok(())
    }

    async fn create_volume(
        &self,
        name: &str,
        labels: HashMap<String, String>,
    ) -> anyhow::Result<VolumeRow> {
        std::future::ready(()).await;
        let op = format!("docker volume create {name}");
        self.record(&op);
        self.check_fail(&op)?;
        let mut volumes = self.volumes_by_name.borrow_mut();
        let row = volumes.entry(name.to_owned()).or_insert_with(|| VolumeRow {
            name: name.to_owned(), labels: labels.clone(), driver: "local".to_owned(),
        });
        anyhow::ensure!(row.name == name && row.labels == labels && row.driver == "local", "volume {name} returned different ownership metadata or storage driver");
        Ok(row.clone())
    }

    async fn inspect_volume_by_name(&self, name: &str) -> anyhow::Result<Option<VolumeRow>> {
        std::future::ready(()).await;
        let op = format!("docker volume inspect {name}");
        self.record(&op);
        self.check_fail(&op)?;
        Ok(self.volumes_by_name.borrow().get(name).cloned())
    }

    async fn remove_volume(&self, name: &str) -> anyhow::Result<()> {
        std::future::ready(()).await;
        let op = format!("docker volume rm {name}");
        self.record(&op);
        Self::ignore_if_missing(self.check_fail(&op))?;
        self.volumes_by_name.borrow_mut().remove(name);
        Ok(())
    }

    async fn create_network(
        &self,
        name: &str,
        labels: HashMap<String, String>,
        internal: bool,
    ) -> anyhow::Result<NetworkId> {
        std::future::ready(()).await;
        let op = format!("docker network create {name}");
        self.record(&op);
        self.created_networks
            .borrow_mut()
            .push((name.to_owned(), labels, internal));
        self.check_fail(&op)?;
        anyhow::ensure!(!self.network_id_by_name.borrow().contains_key(name), "network {name} already exists");
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        name.hash(&mut hasher);
        self.created_networks.borrow().len().hash(&mut hasher);
        let id = NetworkId::parse(&format!("{:016x}", hasher.finish()).repeat(4))?;
        self.network_id_by_name.borrow_mut().insert(name.to_owned(), id.clone());
        Ok(id)
    }

    async fn remove_network_by_id(&self, id: &NetworkId) -> anyhow::Result<()> {
        std::future::ready(()).await;
        self.bound_operations.borrow_mut().push(format!("remove_network:{}", id.as_str()));
        let name = self.network_id_by_name.borrow().iter()
            .find_map(|(name, current)| (current == id).then(|| name.clone()))
            .unwrap_or_else(|| id.as_str().to_owned());
        let op = format!("docker network rm {name}");
        self.record(&op);
        Self::ignore_if_missing(self.check_fail(&op))?;
        self.network_id_by_name.borrow_mut().retain(|_, current| current != id);
        Ok(())
    }

    async fn list_networks(&self, label_filters: &[&str]) -> anyhow::Result<Vec<NetworkRow>> {
        std::future::ready(()).await;
        let filter_str = label_filters.join(" --filter ");
        let op = format!("docker network ls --filter {filter_str}");
        self.record(&op);
        self.check_fail(&op)?;
        Ok(self.pop_list_networks())
    }

    async fn inspect_network_by_name(&self, name: &str) -> anyhow::Result<Option<NetworkRow>> {
        std::future::ready(()).await;
        let op = format!("docker network inspect {name}");
        self.record(&op);
        self.check_fail(&op)?;
        let row = self.pop_inspect_network();
        if let Some(row) = &row {
            self.network_id_by_name.borrow_mut().insert(row.name.clone(), row.id.clone());
        }
        Ok(row)
    }

    async fn inspect_network_by_id(&self, id: &NetworkId) -> anyhow::Result<Option<NetworkRow>> {
        std::future::ready(()).await;
        self.bound_operations.borrow_mut().push(format!("inspect_network:{}", id.as_str()));
        let name = self.network_id_by_name.borrow().iter()
            .find_map(|(name, current)| (current == id).then(|| name.clone()))
            .unwrap_or_else(|| id.as_str().to_owned());
        let op = format!("docker network inspect {name}");
        self.record(&op);
        self.check_fail(&op)?;
        let row = self.pop_inspect_network();
        if let Some(row) = &row {
            anyhow::ensure!(&row.id == id, "mock returned a different network ID for {id}");
        }
        Ok(row)
    }

    async fn list_image_tags(&self, reference_filter: &str) -> anyhow::Result<Vec<String>> {
        std::future::ready(()).await;
        let op = format!("docker images --filter reference={reference_filter}");
        self.record(&op);
        self.check_fail(&op)?;
        Ok(self.pop_list_image_tags())
    }

    async fn remove_image(&self, name: &str) -> anyhow::Result<RemoveImageOutcome> {
        std::future::ready(()).await;
        let op = format!("docker rmi {name}");
        self.record(&op);
        self.check_fail(&op)?;
        Ok(self.pop_remove_image())
    }

    async fn inspect_image_labels(&self, image: &str) -> anyhow::Result<HashMap<String, String>> {
        std::future::ready(()).await;
        let op = format!("docker inspect image:{image}");
        self.record(&op);
        self.check_fail(&op)?;
        Ok(self.pop_inspect_image_labels())
    }

    async fn pull_image(&self, image: &str) -> anyhow::Result<()> {
        std::future::ready(()).await;
        let op = format!("docker pull {image}");
        self.record(&op);
        self.check_fail(&op)
    }

    async fn exec_capture_by_id(
        &self,
        container: &ContainerHandle,
        cmd: &[&str],
    ) -> anyhow::Result<String> {
        std::future::ready(()).await;
        self.record_bound("exec", container);
        let op = format!("docker exec {} {}", container.name(), cmd.join(" "));
        self.record(&op);
        self.check_fail(&op)?;
        Ok(self.pop_exec_capture())
    }
}
