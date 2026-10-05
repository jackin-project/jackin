// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use jackin_core::{
    ContainerHandle, ContainerInspection, ContainerRow, ContainerSpec, ContainerState, DockerApi,
    InstanceIndexEntry, InstanceStatus, NetworkRow, RemoveImageOutcome,
};
use serde_json::{Value, json};

use super::{AgentHydration, StatusArgs, collect_instances, render_status};

#[derive(Clone)]
struct ExecPlan {
    agents: Result<String, String>,
    branch: Result<String, String>,
    pr: Result<String, String>,
}

impl ExecPlan {
    fn ready() -> Self {
        Self {
            agents: Ok(serde_json::to_string(&[
                agent("badger", "active", "2026-10-03T10:00:00Z"),
                agent("otter", "exited", "2026-10-03T09:00:00Z"),
            ])
            .expect("agent fixture serializes")),
            branch: Ok("feature/status\n".to_owned()),
            pr: Ok(pr_json()),
        }
    }

    fn no_pr() -> Self {
        Self {
            pr: Err("gh unavailable".to_owned()),
            ..Self::ready()
        }
    }
}

struct MockDockerApi {
    inspections: HashMap<String, ContainerInspection>,
    exec: ExecPlan,
    calls: Arc<Mutex<Vec<String>>>,
}

impl MockDockerApi {
    fn new(inspections: HashMap<String, ContainerInspection>, exec: ExecPlan) -> Self {
        Self {
            inspections,
            exec,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn single(name: &str, state: ContainerState, handle: bool, exec: ExecPlan) -> Self {
        let inspection = ContainerInspection {
            handle: handle.then(|| {
                ContainerHandle::new(name, format!("id-{name}"))
                    .expect("fixture handle has non-empty name and id")
            }),
            state,
        };
        Self::new(HashMap::from([(name.to_owned(), inspection)]), exec)
    }

    fn calls(&self) -> Vec<String> {
        self.calls
            .lock()
            .expect("mock calls mutex is not poisoned")
            .clone()
    }

    fn record(&self, call: impl Into<String>) {
        self.calls
            .lock()
            .expect("mock calls mutex is not poisoned")
            .push(call.into());
    }

    fn reply(reply: &Result<String, String>) -> anyhow::Result<String> {
        reply.clone().map_err(|message| anyhow::anyhow!(message))
    }
}

impl DockerApi for MockDockerApi {
    fn controller_endpoint(&self) -> &jackin_core::ControllerEndpoint {
        static ENDPOINT: std::sync::OnceLock<jackin_core::ControllerEndpoint> = std::sync::OnceLock::new();
        ENDPOINT.get_or_init(|| jackin_core::ControllerEndpoint::Unix {
            socket: "/var/run/docker.sock".into(),
        })
    }
    async fn daemon_server_id(&self) -> anyhow::Result<jackin_core::DaemonServerId> {
        panic!("unexpected Docker operation")
    }

    async fn ping(&self) -> anyhow::Result<()> {
        panic!("MockDockerApi::ping was not expected")
    }

    async fn inspect_container_by_name(&self, name: &str) -> ContainerInspection {
        self.record(format!("inspect:{name}"));
        self.inspections
            .get(name)
            .cloned()
            .unwrap_or_else(|| panic!("unexpected inspect for {name:?}"))
    }

    async fn inspect_container_by_id(&self, container: &ContainerHandle) -> ContainerState {
        panic!(
            "MockDockerApi::inspect_container_by_id was not expected for {}",
            container.id()
        )
    }

    async fn container_init_pid_by_id(&self, container: &ContainerHandle) -> anyhow::Result<u32> {
        panic!(
            "MockDockerApi::container_init_pid_by_id was not expected for {}",
            container.id()
        )
    }

    async fn remove_container_by_id(&self, container: &ContainerHandle) -> anyhow::Result<()> {
        panic!(
            "MockDockerApi::remove_container_by_id was not expected for {}",
            container.id()
        )
    }

    async fn list_containers(
        &self,
        _label_filters: &[&str],
        _all: bool,
    ) -> anyhow::Result<Vec<ContainerRow>> {
        panic!("MockDockerApi::list_containers was not expected")
    }

    async fn create_container(
        &self,
        _name: &str,
        _spec: ContainerSpec,
    ) -> anyhow::Result<ContainerHandle> {
        panic!("MockDockerApi::create_container was not expected")
    }

    async fn start_container_by_id(&self, container: &ContainerHandle) -> anyhow::Result<()> {
        panic!(
            "MockDockerApi::start_container_by_id was not expected for {}",
            container.id()
        )
    }

    async fn create_volume(
        &self,
        _name: &str,
        _labels: HashMap<String, String>,
    ) -> anyhow::Result<jackin_core::VolumeRow> {
        panic!("unexpected Docker operation")
    }

    async fn inspect_volume_by_name(
        &self,
        _name: &str,
    ) -> anyhow::Result<Option<jackin_core::VolumeRow>> {
        panic!("unexpected Docker operation")
    }

    async fn remove_volume(&self, _name: &str) -> anyhow::Result<()> {
        panic!("MockDockerApi::remove_volume was not expected")
    }

    async fn create_network(
        &self,
        _name: &str,
        _labels: HashMap<String, String>,
        _internal: bool,
    ) -> anyhow::Result<jackin_core::NetworkId> {
        panic!("MockDockerApi::create_network was not expected")
    }

    async fn remove_network_by_id(&self, _id: &jackin_core::NetworkId) -> anyhow::Result<()> {
        panic!("MockDockerApi::remove_network was not expected")
    }

    async fn list_networks(&self, _label_filters: &[&str]) -> anyhow::Result<Vec<NetworkRow>> {
        panic!("MockDockerApi::list_networks was not expected")
    }

    async fn inspect_network_by_name(&self, _name: &str) -> anyhow::Result<Option<NetworkRow>> {
        panic!("MockDockerApi::inspect_network was not expected")
    }

    async fn inspect_network_by_id(
        &self,
        _id: &jackin_core::NetworkId,
    ) -> anyhow::Result<Option<NetworkRow>> {
        panic!("unexpected Docker operation")
    }

    async fn list_image_tags(&self, _reference_filter: &str) -> anyhow::Result<Vec<String>> {
        panic!("MockDockerApi::list_image_tags was not expected")
    }

    async fn remove_image(&self, _name: &str) -> anyhow::Result<RemoveImageOutcome> {
        panic!("MockDockerApi::remove_image was not expected")
    }

    async fn inspect_image_labels(&self, _image: &str) -> anyhow::Result<HashMap<String, String>> {
        panic!("MockDockerApi::inspect_image_labels was not expected")
    }

    async fn pull_image(&self, _image: &str) -> anyhow::Result<()> {
        panic!("MockDockerApi::pull_image was not expected")
    }

    async fn exec_capture_by_id(
        &self,
        container: &ContainerHandle,
        cmd: &[&str],
    ) -> anyhow::Result<String> {
        let command = cmd.join(" ");
        let (kind, reply) = if command.contains("agents --format json") {
            ("agents", &self.exec.agents)
        } else if command.contains("rev-parse --abbrev-ref HEAD") {
            ("branch", &self.exec.branch)
        } else if command.contains("gh pr view") {
            ("pr", &self.exec.pr)
        } else {
            panic!("unexpected exec command: {command}")
        };
        self.record(format!("exec:{kind}:{}", container.id()));
        Self::reply(reply)
    }
}

fn entry(id: &str, workspace: &str, container: &str) -> InstanceIndexEntry {
    InstanceIndexEntry {
        instance_id: id.to_owned(),
        container_base: container.to_owned(),
        workspace_name: Some(workspace.to_owned()),
        workspace_label: workspace.to_owned(),
        workdir: "/workspace/project".to_owned(),
        role_key: "agent-smith".to_owned(),
        agent_runtime: "codex".to_owned(),
        status: InstanceStatus::Active,
        updated_at: "2026-10-03T00:00:00Z".to_owned(),
    }
}

fn args(
    workspace: Option<&str>,
    instance_id: Option<&str>,
    detail: bool,
    state: Option<&str>,
    format: &str,
) -> StatusArgs {
    StatusArgs {
        workspace: workspace.map(str::to_owned),
        instance_id: instance_id.map(str::to_owned),
        detail,
        state: state.map(str::to_owned),
        format: format.to_owned(),
    }
}

fn agent(codename: &str, status: &str, started_at: &str) -> Value {
    json!({
        "codename": codename,
        "agent": "codex",
        "provider": "openai",
        "started_at": started_at,
        "exited_at": (status == "exited").then_some("2026-10-03T11:00:00Z"),
        "status": status,
        "is_self": false,
    })
}

fn pr_json() -> String {
    json!({
        "number": 42,
        "title": "Improve status hydration",
        "url": "https://github.com/example/repo/pull/42",
        "statusCheckRollup": [{
            "__typename": "CheckRun",
            "status": "COMPLETED",
            "conclusion": "SUCCESS",
            "name": "build"
        }]
    })
    .to_string()
}

fn find_instance<'a>(value: &'a Value, id: &str) -> Option<&'a Value> {
    if value.get("instance_id").and_then(Value::as_str) == Some(id) {
        return Some(value);
    }
    match value {
        Value::Array(values) => values.iter().find_map(|value| find_instance(value, id)),
        Value::Object(values) => values.values().find_map(|value| find_instance(value, id)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => None,
    }
}

fn rendered_json(args: &StatusArgs, rows: &[super::HydratedInstance]) -> Value {
    let mut output = Vec::new();
    render_status(args, rows, &mut output).expect("JSON status rendering succeeds");
    serde_json::from_slice(&output).expect("status output is valid JSON")
}

fn assert_full_json_row(value: &Value, id: &str) {
    let row = find_instance(value, id).expect("JSON output contains the requested instance");
    for field in [
        "container_base",
        "workspace",
        "role",
        "state",
        "branch",
        "pull_request",
        "agents",
        "agents_status",
    ] {
        assert!(row.get(field).is_some(), "JSON row lacks {field}");
    }
    assert_eq!(row["branch"], "feature/status");
    assert_eq!(row["pull_request"]["number"], 42);
    assert_eq!(row["agents_status"], "ready");
    assert!(row["agents"].is_array());
}

#[tokio::test]
async fn json_all_levels_are_fully_hydrated_even_without_detail() {
    let instance = entry("instance-1", "demo", "container-1");
    for (workspace, instance_id) in [
        (None, None),
        (Some("demo"), None),
        (Some("demo"), Some("instance-1")),
    ] {
        for detail in [false, true] {
            let args = args(workspace, instance_id, detail, None, "json");
            let docker = MockDockerApi::single(
                "container-1",
                ContainerState::Running,
                true,
                ExecPlan::ready(),
            );
            let rows = collect_instances(&args, std::slice::from_ref(&instance), &docker)
                .await
                .expect("instance hydration succeeds");
            assert_eq!(rows.len(), 1);
            assert_full_json_row(&rendered_json(&args, &rows), "instance-1");
            assert!(
                docker
                    .calls()
                    .iter()
                    .filter(|call| call.starts_with("exec:"))
                    .all(|call| call.ends_with(":id-container-1"))
            );
        }
    }
}

#[tokio::test]
async fn state_filter_has_the_same_result_at_each_status_level() {
    let running = entry("running-id", "demo", "container-running");
    let stopped = entry("stopped-id", "demo", "container-stopped");
    let entries = [running.clone(), stopped.clone()];
    for (workspace, instance_id) in [
        (None, None),
        (Some("demo"), None),
        (Some("demo"), Some("running-id")),
        (Some("demo"), Some("stopped-id")),
    ] {
        for (filter, expected) in [("running", "running-id"), ("stopped", "stopped-id")] {
            for format in ["human", "json"] {
                let args = args(workspace, instance_id, true, Some(filter), format);
                let docker = MockDockerApi::new(
                    HashMap::from([
                        (
                            "container-running".to_owned(),
                            ContainerInspection {
                                handle: Some(
                                    ContainerHandle::new("container-running", "id-running")
                                        .expect("running handle is valid"),
                                ),
                                state: ContainerState::Running,
                            },
                        ),
                        (
                            "container-stopped".to_owned(),
                            ContainerInspection {
                                handle: None,
                                state: ContainerState::Stopped {
                                    exit_code: 0,
                                    oom_killed: false,
                                },
                            },
                        ),
                    ]),
                    ExecPlan::no_pr(),
                );
                let rows = collect_instances(&args, &entries, &docker)
                    .await
                    .expect("state filtered hydration succeeds");
                let target_selected = instance_id.is_none_or(|id| id == expected);
                assert_eq!(rows.len(), if target_selected { 1 } else { 0 });
                if target_selected {
                    assert_eq!(rows[0].entry.instance_id, expected);
                    let other = if expected == "running-id" {
                        "stopped-id"
                    } else {
                        "running-id"
                    };
                    if format == "json" {
                        let value = rendered_json(&args, &rows);
                        assert!(find_instance(&value, expected).is_some());
                        assert!(find_instance(&value, other).is_none());
                    } else {
                        let mut output = Vec::new();
                        render_status(&args, &rows, &mut output)
                            .expect("filtered human render succeeds");
                        let output = String::from_utf8(output).expect("human status is UTF-8");
                        assert!(output.contains(expected));
                        assert!(!output.contains(other));
                    }
                } else {
                    let mut output = Vec::new();
                    render_status(&args, &rows, &mut output)
                        .expect("filtered empty render succeeds");
                    let output = String::from_utf8(output).expect("status is UTF-8");
                    assert!(!output.contains("running-id"));
                    assert!(!output.contains("stopped-id"));
                }
                if filter == "stopped" {
                    assert!(
                        docker.calls().iter().all(|call| !call.starts_with("exec:")),
                        "stopped filtering must not exec into a container"
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn running_inspection_without_a_handle_remains_visible_as_unavailable() {
    let instance = entry("instance-1", "demo", "container-1");
    let args = args(Some("demo"), Some("instance-1"), false, None, "json");
    let docker = MockDockerApi::single(
        "container-1",
        ContainerState::Running,
        false,
        ExecPlan::ready(),
    );
    let rows = collect_instances(&args, std::slice::from_ref(&instance), &docker)
        .await
        .expect("missing handle does not abort hydration");
    assert_eq!(rows.len(), 1);
    assert!(matches!(rows[0].agents, AgentHydration::Unavailable));
    assert!(docker.calls().iter().all(|call| !call.starts_with("exec:")));
    let value = rendered_json(&args, &rows);
    let row = find_instance(&value, "instance-1").expect("row survives missing handle");
    assert_eq!(row["state"], "running");
    assert_eq!(row["agents"], Value::Null);
    assert_eq!(row["agents_status"], "unavailable");
}

#[tokio::test]
async fn agent_socket_pending_and_malformed_registry_are_distinct() {
    let instance = entry("instance-1", "demo", "container-1");
    let args = args(Some("demo"), Some("instance-1"), false, None, "json");

    let pending = MockDockerApi::single(
        "container-1",
        ContainerState::Running,
        true,
        ExecPlan {
            agents: Ok("jackin-agents-pending\n".to_owned()),
            branch: Err("branch unavailable".to_owned()),
            pr: Err("gh unavailable".to_owned()),
        },
    );
    let pending_rows = collect_instances(&args, std::slice::from_ref(&instance), &pending)
        .await
        .expect("pending registry still hydrates the row");
    assert!(matches!(pending_rows[0].agents, AgentHydration::Pending));
    assert_eq!(
        rendered_json(&args, &pending_rows)["instances"][0]["agents_status"],
        "pending"
    );

    let malformed = MockDockerApi::single(
        "container-1",
        ContainerState::Running,
        true,
        ExecPlan {
            agents: Ok("{ definitely not json".to_owned()),
            branch: Ok("main\n".to_owned()),
            pr: Err("gh unavailable".to_owned()),
        },
    );
    let malformed_rows = collect_instances(&args, std::slice::from_ref(&instance), &malformed)
        .await
        .expect("malformed registry still hydrates the row");
    assert!(matches!(
        malformed_rows[0].agents,
        AgentHydration::Unavailable
    ));
    assert_eq!(
        rendered_json(&args, &malformed_rows)["instances"][0]["agents_status"],
        "unavailable"
    );
}

#[tokio::test]
async fn nonrunning_states_never_exec_and_keep_truthful_agent_status() {
    let instance = entry("instance-1", "demo", "container-1");
    let cases = [
        (
            ContainerState::Stopped {
                exit_code: 1,
                oom_killed: false,
            },
            "stopped",
        ),
        (ContainerState::Dead, "stopped"),
        (ContainerState::Created, "stopped"),
        (ContainerState::NotFound, "stopped"),
        (ContainerState::Paused, "unavailable"),
        (ContainerState::Removing, "unavailable"),
        (
            ContainerState::InspectUnavailable("daemon down".to_owned()),
            "unavailable",
        ),
        (ContainerState::Restarting, "pending"),
    ];
    for (state, expected_status) in cases {
        let args = args(Some("demo"), Some("instance-1"), false, None, "json");
        let docker = MockDockerApi::single("container-1", state, true, ExecPlan::ready());
        let rows = collect_instances(&args, std::slice::from_ref(&instance), &docker)
            .await
            .expect("nonrunning state hydration succeeds");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].agents.status(), expected_status);
        assert!(docker.calls().iter().all(|call| !call.starts_with("exec:")));
    }
}

#[tokio::test]
async fn ready_agents_are_sorted_active_first_then_time_and_codename() {
    let instance = entry("instance-1", "demo", "container-1");
    let agents = vec![
        agent("z-exited", "exited", "2026-10-03T09:00:00Z"),
        agent("z-active", "active", "2026-10-03T10:00:00Z"),
        agent("b-active", "active", "2026-10-03T09:00:00Z"),
        agent("a-exited", "exited", "2026-10-03T09:00:00Z"),
        agent("a-active", "active", "2026-10-03T09:00:00Z"),
    ];
    let mut exec = ExecPlan::ready();
    exec.agents = Ok(serde_json::to_string(&agents).expect("agent fixture serializes"));
    let docker = MockDockerApi::single("container-1", ContainerState::Running, true, exec);
    let args = args(Some("demo"), Some("instance-1"), false, None, "human");
    let rows = collect_instances(&args, std::slice::from_ref(&instance), &docker)
        .await
        .expect("agent hydration succeeds");
    let AgentHydration::Ready(agents) = &rows[0].agents else {
        panic!("ready fixture must produce ready agents")
    };
    let codenames: Vec<_> = agents.iter().map(|agent| agent.codename.as_str()).collect();
    assert_eq!(
        codenames,
        ["a-active", "b-active", "z-active", "a-exited", "z-exited"]
    );
    let mut output = Vec::new();
    render_status(&args, &rows, &mut output).expect("human detail rendering succeeds");
    let output = String::from_utf8(output).expect("human status is UTF-8");
    for pair in codenames.windows(2) {
        assert!(
            output.find(pair[0]).expect("first codename renders")
                < output.find(pair[1]).expect("second codename renders")
        );
    }
}

#[tokio::test]
async fn human_workspace_rows_include_hydrated_pull_request() {
    let instance = entry("instance-1", "demo", "container-1");
    let args = args(Some("demo"), None, false, None, "human");
    let docker = MockDockerApi::single(
        "container-1",
        ContainerState::Running,
        true,
        ExecPlan::ready(),
    );
    let rows = collect_instances(&args, std::slice::from_ref(&instance), &docker)
        .await
        .expect("workspace hydration succeeds");
    let mut output = Vec::new();
    render_status(&args, &rows, &mut output).expect("workspace rendering succeeds");
    let output = String::from_utf8(output).expect("human status is UTF-8");
    assert!(output.contains("#42  Improve status hydration"));
}

#[tokio::test]
async fn human_detail_flag_controls_full_rows_at_fleet_and_workspace_levels() {
    let instance = entry("instance-1", "demo", "container-1");
    let docker = MockDockerApi::single(
        "container-1",
        ContainerState::Running,
        true,
        ExecPlan::ready(),
    );
    let hydrate_args = args(None, None, false, None, "human");
    let rows = collect_instances(&hydrate_args, std::slice::from_ref(&instance), &docker)
        .await
        .expect("instance hydration succeeds");

    for workspace in [None, Some("demo")] {
        let compact_args = args(workspace, None, false, None, "human");
        let mut compact_output = Vec::new();
        render_status(&compact_args, &rows, &mut compact_output)
            .expect("compact human rendering succeeds");
        let compact_output = String::from_utf8(compact_output).expect("human status is UTF-8");
        assert!(!compact_output.contains("  branch   "));
        assert!(!compact_output.contains("  ci       "));
        assert!(!compact_output.contains("badger"));
        if workspace.is_some() {
            assert!(compact_output.contains("#42  Improve status hydration"));
        } else {
            assert!(!compact_output.contains("Improve status hydration"));
        }

        let detail_args = args(workspace, None, true, None, "human");
        let mut detail_output = Vec::new();
        render_status(&detail_args, &rows, &mut detail_output)
            .expect("detailed human rendering succeeds");
        let detail_output = String::from_utf8(detail_output).expect("human status is UTF-8");
        assert!(detail_output.contains("  branch   feature/status"));
        assert!(detail_output.contains("#42  Improve status hydration"));
        assert!(detail_output.contains("  ci       ✓ passing"));
        assert!(detail_output.contains("badger"));
    }
}

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("writer failed"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("writer failed"))
    }
}

#[tokio::test]
async fn render_status_propagates_writer_errors() {
    let instance = entry("instance-1", "demo", "container-1");
    let args = args(Some("demo"), None, false, None, "json");
    let docker = MockDockerApi::single(
        "container-1",
        ContainerState::Running,
        true,
        ExecPlan::ready(),
    );
    let rows = collect_instances(&args, std::slice::from_ref(&instance), &docker)
        .await
        .expect("instance hydration succeeds");
    for (workspace, instance_id) in [
        (None, None),
        (Some("demo"), None),
        (Some("demo"), Some("instance-1")),
    ] {
        for format in ["human", "json"] {
            let mode_args = StatusArgs {
                workspace: workspace.map(str::to_owned),
                instance_id: instance_id.map(str::to_owned),
                detail: true,
                state: None,
                format: format.to_owned(),
            };
            let error = render_status(&mode_args, &rows, &mut FailingWriter)
                .expect_err("writer error surfaces");
            assert!(error.to_string().contains("writer failed"));
        }
    }
}
