// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
#[cfg(any(test, feature = "test-support"))]
use std::future::Future;
use std::path::PathBuf;
#[cfg(any(test, feature = "test-support"))]
use std::sync::Mutex;

use anyhow::Result;
use serde::de::{self, IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppleContainerMount {
    pub source: PathBuf,
    pub target: PathBuf,
    pub readonly: bool,
}

impl AppleContainerMount {
    pub fn new(source: impl Into<PathBuf>, target: impl Into<PathBuf>, readonly: bool) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            readonly,
        }
    }
}

pub const BACKEND_NAME: &str = "apple-container";

pub const DOCKER_BACKEND_NAME: &str = "docker";

#[derive(Debug, Clone)]
pub struct AppleContainerSpec {
    pub image: String,
    pub user: String,
    pub env: Vec<(String, String)>,
    pub env_file: Option<PathBuf>,
    pub mounts: Vec<AppleContainerMount>,
    pub caps_add: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppleContainerStatus {
    Running,
    Stopped,
    Unknown,
    Stopping,
}

impl AppleContainerStatus {
    pub const fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppleContainerInfo {
    pub id: String,
    pub status: AppleContainerStatus,
}

impl AppleContainerInfo {
    pub const fn is_running(&self) -> bool {
        self.status.is_running()
    }
}

pub trait AppleContainerApi: Send + Sync {
    async fn run_container(&self, name: &str, spec: &AppleContainerSpec) -> Result<()>;

    async fn start_container(&self, name: &str) -> Result<()>;

    async fn stop_container(&self, name: &str) -> Result<()>;

    async fn remove_container(&self, name: &str) -> Result<()>;

    /// Inspect one exact container ID; absence and inventory errors remain distinct.
    async fn inspect_container(&self, name: &str) -> Result<Option<AppleContainerInfo>>;

    /// Return the prefix-filtered view of a completely validated inventory.
    async fn list_containers(&self, name_prefix: &str) -> Result<Vec<AppleContainerInfo>>;
}

#[derive(Debug)]
pub struct AppleContainerClient;

impl AppleContainerClient {
    pub const fn new() -> Self {
        Self
    }

    async fn lifecycle(&self, name: &str, sub: &str) -> Result<()> {
        let output = jackin_runtime_process_telemetry::process_telemetry::exec_async(
            &jackin_process::ExecRequest::new("container", [sub, name]),
        )
        .await?;
        if !output.success {
            anyhow::bail!("container lifecycle command exited unsuccessfully");
        }
        Ok(())
    }

    async fn list_containers_with_runner(
        &self,
        runner: &impl AppleContainerCommandRunner,
        name_prefix: &str,
    ) -> Result<Vec<AppleContainerInfo>> {
        let request = container_list_request();
        let output = runner.exec(&request).await?;
        anyhow::ensure!(output.success, "container list exited unsuccessfully");

        // Parse and validate the complete inventory before applying the search prefix.
        Ok(parse_all_containers_json(&output.stdout)?
            .into_iter()
            .filter(|container| container.id.starts_with(name_prefix))
            .collect())
    }

    async fn inspect_container_with_runner(
        &self,
        runner: &impl AppleContainerCommandRunner,
        container_id: &str,
    ) -> Result<Option<AppleContainerInfo>> {
        Ok(self
            .list_containers_with_runner(runner, container_id)
            .await?
            .into_iter()
            .find(|container| container.id == container_id))
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub async fn list_containers_with_command_runner(
        &self,
        runner: &impl AppleContainerCommandRunner,
        name_prefix: &str,
    ) -> Result<Vec<AppleContainerInfo>> {
        self.list_containers_with_runner(runner, name_prefix).await
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub async fn inspect_container_with_command_runner(
        &self,
        runner: &impl AppleContainerCommandRunner,
        container_id: &str,
    ) -> Result<Option<AppleContainerInfo>> {
        self.inspect_container_with_runner(runner, container_id)
            .await
    }
}

impl Default for AppleContainerClient {
    fn default() -> Self {
        Self::new()
    }
}

impl AppleContainerApi for AppleContainerClient {
    async fn run_container(&self, name: &str, spec: &AppleContainerSpec) -> Result<()> {
        anyhow::ensure!(
            spec.user == jackin_runtime_identity::identity::CAPSULE_SUPERVISOR_USER,
            "apple-container capsule supervisor must launch as root"
        );
        let args = container_run_args(name, spec);

        let output = jackin_runtime_process_telemetry::process_telemetry::exec_async(
            &jackin_process::ExecRequest::new("container", &args),
        )
        .await?;
        if !output.success {
            anyhow::bail!("container run exited unsuccessfully");
        }
        Ok(())
    }

    async fn start_container(&self, name: &str) -> Result<()> {
        self.lifecycle(name, "start").await
    }

    async fn stop_container(&self, name: &str) -> Result<()> {
        self.lifecycle(name, "stop").await
    }

    async fn remove_container(&self, name: &str) -> Result<()> {
        self.lifecycle(name, "rm").await
    }

    async fn inspect_container(&self, name: &str) -> Result<Option<AppleContainerInfo>> {
        self.inspect_container_with_runner(&ProcessTelemetryRunner, name)
            .await
    }

    async fn list_containers(&self, name_prefix: &str) -> Result<Vec<AppleContainerInfo>> {
        self.list_containers_with_runner(&ProcessTelemetryRunner, name_prefix)
            .await
    }
}

#[doc(hidden)]
#[cfg(any(test, feature = "test-support"))]
pub trait AppleContainerCommandRunner: Send + Sync {
    async fn exec(
        &self,
        request: &jackin_process::ExecRequest,
    ) -> Result<jackin_process::ExecResult>;
}

#[cfg(not(any(test, feature = "test-support")))]
trait AppleContainerCommandRunner: Send + Sync {
    async fn exec(
        &self,
        request: &jackin_process::ExecRequest,
    ) -> Result<jackin_process::ExecResult>;
}

struct ProcessTelemetryRunner;

impl AppleContainerCommandRunner for ProcessTelemetryRunner {
    async fn exec(
        &self,
        request: &jackin_process::ExecRequest,
    ) -> Result<jackin_process::ExecResult> {
        jackin_runtime_process_telemetry::process_telemetry::exec_async(request).await
    }
}

fn container_list_request() -> jackin_process::ExecRequest {
    jackin_process::ExecRequest::new("container", ["list", "--all", "--format", "json"])
}

fn container_run_args(name: &str, spec: &AppleContainerSpec) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = vec![
        "run".into(),
        "--name".into(),
        name.into(),
        "-d".into(),
        "--user".into(),
        spec.user.clone().into(),
    ];

    if let Some(path) = &spec.env_file {
        args.extend(["--env-file".into(), path.as_os_str().to_owned()]);
    }
    for (key, value) in &spec.env {
        args.extend(["-e".into(), format!("{key}={value}").into()]);
    }
    for mount in &spec.mounts {
        let readonly = if mount.readonly { ":ro" } else { "" };
        args.extend([
            "-v".into(),
            format!(
                "{}:{}{readonly}",
                mount.source.display(),
                mount.target.display()
            )
            .into(),
        ]);
    }
    for capability in &spec.caps_add {
        args.extend(["--cap-add".into(), capability.into()]);
    }
    args.extend([spec.image.clone().into(), "jackin-capsule".into()]);
    args
}

fn parse_all_containers_json(json_output: &[u8]) -> Result<Vec<AppleContainerInfo>> {
    std::str::from_utf8(json_output)
        .map_err(|_| anyhow::anyhow!("Apple Container inventory is not valid UTF-8"))?;
    let rows: Vec<AppleContainerRow> = serde_json::from_slice(json_output).map_err(|_| {
        anyhow::anyhow!("Apple Container inventory is not valid JSON or has an unsupported shape")
    })?;

    let mut ids = HashSet::with_capacity(rows.len());
    let mut containers = Vec::with_capacity(rows.len());
    for row in rows {
        anyhow::ensure!(
            !row.id.is_empty(),
            "Apple Container inventory has an empty id"
        );
        anyhow::ensure!(
            !row.configuration.id.is_empty(),
            "Apple Container inventory has an empty configuration id"
        );
        anyhow::ensure!(
            row.id == row.configuration.id,
            "Apple Container inventory id does not match configuration id"
        );
        anyhow::ensure!(
            ids.insert(row.id.clone()),
            "Apple Container inventory contains a duplicate id"
        );
        containers.push(AppleContainerInfo {
            id: row.id,
            status: row.status.state,
        });
    }
    Ok(containers)
}

struct AppleContainerRow {
    id: String,
    configuration: AppleContainerConfiguration,
    status: AppleContainerStatusObject,
}

const CONFIG_FIELD: &str = "configuration";

fn set_once<'de, A, T>(
    slot: &mut Option<T>,
    map: &mut A,
    field: &'static str,
) -> std::result::Result<(), A::Error>
where
    A: MapAccess<'de>,
    T: Deserialize<'de>,
{
    if slot.is_some() {
        return Err(de::Error::duplicate_field(field));
    }
    *slot = Some(map.next_value()?);
    Ok(())
}

fn skip_map_value<'de, A>(map: &mut A) -> std::result::Result<(), A::Error>
where
    A: MapAccess<'de>,
{
    let _: IgnoredAny = map.next_value()?;
    Ok(())
}

impl<'de> Deserialize<'de> for AppleContainerRow {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RowVisitor;

        impl<'de> Visitor<'de> for RowVisitor {
            type Value = AppleContainerRow;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an Apple Container inventory row object")
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut id = None;
                let mut configuration = None;
                let mut status = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "id" => set_once(&mut id, &mut map, "id")?,
                        "configuration" => set_once(&mut configuration, &mut map, CONFIG_FIELD)?,
                        "status" => set_once(&mut status, &mut map, "status")?,
                        _ => skip_map_value(&mut map)?,
                    }
                }
                Ok(AppleContainerRow {
                    id: id.ok_or_else(|| de::Error::missing_field("id"))?,
                    configuration: configuration
                        .ok_or_else(|| de::Error::missing_field("configuration"))?,
                    status: status.ok_or_else(|| de::Error::missing_field("status"))?,
                })
            }
        }

        deserializer.deserialize_map(RowVisitor)
    }
}

struct AppleContainerConfiguration {
    id: String,
}

impl<'de> Deserialize<'de> for AppleContainerConfiguration {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ConfigurationVisitor;

        impl<'de> Visitor<'de> for ConfigurationVisitor {
            type Value = AppleContainerConfiguration;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an Apple Container configuration object")
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut id = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "id" => set_once(&mut id, &mut map, "id")?,
                        _ => skip_map_value(&mut map)?,
                    }
                }
                Ok(AppleContainerConfiguration {
                    id: id.ok_or_else(|| de::Error::missing_field("id"))?,
                })
            }
        }

        deserializer.deserialize_map(ConfigurationVisitor)
    }
}

struct AppleContainerStatusObject {
    state: AppleContainerStatus,
}

impl<'de> Deserialize<'de> for AppleContainerStatusObject {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StatusVisitor;

        impl<'de> Visitor<'de> for StatusVisitor {
            type Value = AppleContainerStatusObject;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an Apple Container status object")
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut state = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "state" => set_once(&mut state, &mut map, "state")?,
                        _ => skip_map_value(&mut map)?,
                    }
                }
                Ok(AppleContainerStatusObject {
                    state: state.ok_or_else(|| de::Error::missing_field("state"))?,
                })
            }
        }

        deserializer.deserialize_map(StatusVisitor)
    }
}

impl<'de> Deserialize<'de> for AppleContainerStatus {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let state = String::deserialize(deserializer)?;
        match state.as_str() {
            "running" => Ok(Self::Running),
            "stopped" => Ok(Self::Stopped),
            "unknown" => Ok(Self::Unknown),
            "stopping" => Ok(Self::Stopping),
            _ => Err(de::Error::custom("unrecognized Apple Container state")),
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Default)]
pub struct FakeAppleContainerClient {
    pub containers: Mutex<Vec<AppleContainerInfo>>,
    pub inspect_calls: Mutex<Vec<String>>,
    pub start_calls: Mutex<Vec<String>>,
}

#[cfg(any(test, feature = "test-support"))]
impl FakeAppleContainerClient {
    pub fn new() -> Self {
        Self::default()
    }

    fn run_container_now(&self, name: &str) -> Result<()> {
        let mut containers = self
            .containers
            .lock()
            .map_err(|_| anyhow::anyhow!("fake Apple Container inventory lock is poisoned"))?;
        containers.push(AppleContainerInfo {
            id: name.to_owned(),
            status: AppleContainerStatus::Running,
        });
        Ok(())
    }

    fn start_container_now(&self, name: &str) -> Result<()> {
        self.start_calls
            .lock()
            .map_err(|_| anyhow::anyhow!("fake Apple Container start-call lock is poisoned"))?
            .push(name.to_owned());
        let mut containers = self
            .containers
            .lock()
            .map_err(|_| anyhow::anyhow!("fake Apple Container inventory lock is poisoned"))?;
        let container = containers
            .iter_mut()
            .find(|container| container.id == name)
            .ok_or_else(|| anyhow::anyhow!("fake Apple Container is absent"))?;
        container.status = AppleContainerStatus::Running;
        Ok(())
    }

    fn stop_container_now(&self, name: &str) -> Result<()> {
        let mut containers = self
            .containers
            .lock()
            .map_err(|_| anyhow::anyhow!("fake Apple Container inventory lock is poisoned"))?;
        if let Some(container) = containers.iter_mut().find(|container| container.id == name) {
            container.status = AppleContainerStatus::Stopped;
        }
        Ok(())
    }

    fn remove_container_now(&self, name: &str) -> Result<()> {
        self.containers
            .lock()
            .map_err(|_| anyhow::anyhow!("fake Apple Container inventory lock is poisoned"))?
            .retain(|container| container.id != name);
        Ok(())
    }

    fn inspect_container_now(&self, name: &str) -> Result<Option<AppleContainerInfo>> {
        self.inspect_calls
            .lock()
            .map_err(|_| anyhow::anyhow!("fake Apple Container inspect-call lock is poisoned"))?
            .push(name.to_owned());
        Ok(self
            .containers
            .lock()
            .map_err(|_| anyhow::anyhow!("fake Apple Container inventory lock is poisoned"))?
            .iter()
            .find(|container| container.id == name)
            .cloned())
    }

    fn list_containers_now(&self, name_prefix: &str) -> Result<Vec<AppleContainerInfo>> {
        Ok(self
            .containers
            .lock()
            .map_err(|_| anyhow::anyhow!("fake Apple Container inventory lock is poisoned"))?
            .iter()
            .filter(|container| container.id.starts_with(name_prefix))
            .cloned()
            .collect())
    }
}

#[cfg(any(test, feature = "test-support"))]
impl AppleContainerApi for FakeAppleContainerClient {
    fn run_container(
        &self,
        name: &str,
        _spec: &AppleContainerSpec,
    ) -> impl Future<Output = Result<()>> {
        std::future::poll_fn(move |_| std::task::Poll::Ready(self.run_container_now(name)))
    }

    fn start_container(&self, name: &str) -> impl Future<Output = Result<()>> {
        std::future::poll_fn(move |_| std::task::Poll::Ready(self.start_container_now(name)))
    }

    fn stop_container(&self, name: &str) -> impl Future<Output = Result<()>> {
        std::future::poll_fn(move |_| std::task::Poll::Ready(self.stop_container_now(name)))
    }

    fn remove_container(&self, name: &str) -> impl Future<Output = Result<()>> {
        std::future::poll_fn(move |_| std::task::Poll::Ready(self.remove_container_now(name)))
    }

    fn inspect_container(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Option<AppleContainerInfo>>> {
        std::future::poll_fn(move |_| std::task::Poll::Ready(self.inspect_container_now(name)))
    }

    fn list_containers(
        &self,
        name_prefix: &str,
    ) -> impl Future<Output = Result<Vec<AppleContainerInfo>>> {
        std::future::poll_fn(move |_| std::task::Poll::Ready(self.list_containers_now(name_prefix)))
    }
}

#[cfg(test)]
mod tests;
