//! `FakeDockerClient`: an in-memory `jackin_core::DockerApi` fake.

#![expect(
    clippy::expect_used,
    reason = "test support fixture setup should fail immediately with source location"
)]

use std::collections::HashMap;

use jackin_core::{
    ContainerHandle, ContainerInspection, ContainerRow, ContainerSpec, ContainerState, DockerApi,
    NetworkRow, RemoveImageOutcome,
};

#[derive(Debug)]
pub struct FakeDockerClient {
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
            recorded: std::cell::RefCell::new(Vec::new()),
            inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
            inspect_by_id_queue: std::cell::RefCell::new(std::collections::VecDeque::new()),
            inspect_state_by_name: std::cell::RefCell::new(HashMap::new()),
            container_id_by_name: std::cell::RefCell::new(HashMap::new()),
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
    async fn ping(&self) -> anyhow::Result<()> {
        self.record("docker ping");
        self.check_fail("docker ping")
    }

    async fn inspect_container_by_name(&self, name: &str) -> ContainerInspection {
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

    async fn inspect_container_by_id(&self, container: &ContainerHandle) -> ContainerState {
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

    async fn remove_volume(&self, name: &str) -> anyhow::Result<()> {
        let op = format!("docker volume rm {name}");
        self.record(&op);
        Self::ignore_if_missing(self.check_fail(&op))
    }

    async fn create_network(
        &self,
        name: &str,
        labels: HashMap<String, String>,
        internal: bool,
    ) -> anyhow::Result<()> {
        let op = format!("docker network create {name}");
        self.record(&op);
        self.created_networks
            .borrow_mut()
            .push((name.to_owned(), labels, internal));
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

    async fn exec_capture_by_id(
        &self,
        container: &ContainerHandle,
        cmd: &[&str],
    ) -> anyhow::Result<String> {
        self.record_bound("exec", container);
        let op = format!("docker exec {} {}", container.name(), cmd.join(" "));
        self.record(&op);
        self.check_fail(&op)?;
        Ok(self.pop_exec_capture())
    }
}
