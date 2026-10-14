// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use anyhow::Result;

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

#[derive(Debug, Clone)]
pub struct AppleContainerInfo {
    pub name: String,

    pub status: String,
}

impl AppleContainerInfo {
    pub fn is_running(&self) -> bool {
        self.status.to_lowercase().contains("running")
    }
}

pub trait AppleContainerApi: Send + Sync {
    async fn run_container(&self, name: &str, spec: &AppleContainerSpec) -> Result<()>;

    async fn stop_container(&self, name: &str) -> Result<()>;

    async fn remove_container(&self, name: &str) -> Result<()>;

    async fn inspect_container(&self, name: &str) -> Result<Option<AppleContainerInfo>>;

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

    async fn stop_container(&self, name: &str) -> Result<()> {
        self.lifecycle(name, "stop").await
    }

    async fn remove_container(&self, name: &str) -> Result<()> {
        self.lifecycle(name, "rm").await
    }

    async fn inspect_container(&self, name: &str) -> Result<Option<AppleContainerInfo>> {
        Ok(self
            .list_containers(name)
            .await?
            .into_iter()
            .find(|c| c.name == name))
    }

    async fn list_containers(&self, name_prefix: &str) -> Result<Vec<AppleContainerInfo>> {
        let output = jackin_runtime_process_telemetry::process_telemetry::exec_async(
            &jackin_process::ExecRequest::new("container", ["ps", "--all", "--format", "json"]),
        )
        .await?;
        if !output.success {
            anyhow::bail!("container list exited unsuccessfully");
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let all = parse_all_containers_json(&stdout);
        Ok(all
            .into_iter()
            .filter(|c| c.name.starts_with(name_prefix))
            .collect())
    }
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

fn parse_all_containers_json(json_output: &str) -> Vec<AppleContainerInfo> {
    let trimmed = json_output.trim();
    if trimmed.is_empty() {
        return vec![];
    }

    let mut results = Vec::new();

    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(trimmed) {
        for item in arr {
            if let Some(info) = extract_container_info(&item) {
                results.push(info);
            }
        }
        return results;
    }

    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(line)
            && let Some(info) = extract_container_info(&obj)
        {
            results.push(info);
        }
    }

    results
}

fn extract_container_info(obj: &serde_json::Value) -> Option<AppleContainerInfo> {
    let name = obj
        .get("name")
        .or_else(|| obj.get("Name"))
        .and_then(|v| v.as_str())?
        .to_owned();
    let status = obj
        .get("status")
        .or_else(|| obj.get("Status"))
        .or_else(|| obj.get("state"))
        .or_else(|| obj.get("State"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_owned();
    Some(AppleContainerInfo { name, status })
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
pub struct FakeAppleContainerClient {
    pub containers: std::sync::Mutex<Vec<AppleContainerInfo>>,
}

#[cfg(any(test, feature = "test-support"))]
impl Default for FakeAppleContainerClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "test-support"))]
impl FakeAppleContainerClient {
    pub fn new() -> Self {
        Self {
            containers: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl AppleContainerApi for FakeAppleContainerClient {
    async fn run_container(&self, name: &str, _spec: &AppleContainerSpec) -> Result<()> {
        std::future::ready(()).await;
        if let Ok(mut containers) = self.containers.lock() {
            containers.push(AppleContainerInfo {
                name: name.to_owned(),
                status: "running".to_owned(),
            });
        }
        Ok(())
    }

    async fn stop_container(&self, name: &str) -> Result<()> {
        std::future::ready(()).await;
        if let Ok(mut containers) = self.containers.lock()
            && let Some(c) = containers.iter_mut().find(|c| c.name == name)
        {
            c.status = "stopped".to_owned();
        }
        Ok(())
    }

    async fn remove_container(&self, name: &str) -> Result<()> {
        std::future::ready(()).await;
        if let Ok(mut containers) = self.containers.lock() {
            containers.retain(|c| c.name != name);
        }
        Ok(())
    }

    async fn inspect_container(&self, name: &str) -> Result<Option<AppleContainerInfo>> {
        std::future::ready(()).await;
        Ok(self
            .containers
            .lock()
            .map(|containers| containers.iter().find(|c| c.name == name).cloned())
            .unwrap_or_default())
    }

    async fn list_containers(&self, name_prefix: &str) -> Result<Vec<AppleContainerInfo>> {
        std::future::ready(()).await;
        Ok(self
            .containers
            .lock()
            .map(|containers| {
                containers
                    .iter()
                    .filter(|c| c.name.starts_with(name_prefix))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests;
