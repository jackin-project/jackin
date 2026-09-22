// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Apple Container backend client.
//!
//! Unlike the Docker backend which uses the bollard async Rust API,
//! Apple Container has no Rust API — all lifecycle operations shell out
//! to the `container` CLI using the shared process transport.
//!
//! This module defines the `AppleContainerApi` trait and its production
//! implementation `AppleContainerClient`. A `FakeAppleContainerClient`
//! is provided for tests.
//!
//! # Backend Name
//!
//! The backend identifier string is `"apple-container"`. Used in CLI flags,
//! config schema, instance manifests, and telemetry.
//!
//! # Prerequisites
//!
//! - macOS 26 ARM with `apple/container` v0.11.0+ installed
//! - `JACKIN_CAPSULE_FORCE_DAEMON=1` injected by `container run` (not a
//!   static Dockerfile ENV — that would break the Docker backend)
//!
//! Basic container lifecycle works without `DinD`; rootless `DinD` inside the
//! VM is separately gated on Phase 0 validation (see `inner_docker_enabled`).

use std::path::PathBuf;

use anyhow::Result;

/// Bind mount passed to Apple `container run`.
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

/// Backend name used in CLI flags, config keys, and instance manifests.
pub const BACKEND_NAME: &str = "apple-container";

/// Backend name for the default Docker + `DinD` backend.
pub const DOCKER_BACKEND_NAME: &str = "docker";

/// Specification for launching a role container via `container run`.
#[derive(Debug, Clone)]
pub struct AppleContainerSpec {
    /// Role OCI image reference.
    pub image: String,
    /// UID:GID for the capsule supervisor. Must be `0:0`; session children
    /// drop to their admitted per-instance identities inside the capsule.
    pub user: String,
    /// Non-sensitive jackin runtime metadata injected as `-e KEY=VALUE`.
    pub env: Vec<(String, String)>,
    /// Host-only environment file read by `container run --env-file`.
    pub env_file: Option<PathBuf>,
    /// Bind mounts (`-v host_path:container_path[:ro]` flags).
    pub mounts: Vec<AppleContainerMount>,
    /// Linux capabilities to grant (`--cap-add CAP_NAME` flags).
    pub caps_add: Vec<String>,
}

/// Container status parsed from `container ps` JSON output.
#[derive(Debug, Clone)]
pub struct AppleContainerInfo {
    pub name: String,
    /// Status string from the container runtime: "running", "stopped", etc.
    pub status: String,
}

impl AppleContainerInfo {
    // TODO(apple-container): once Phase 0 pins the `container ps` JSON schema,
    // parse `status` into a typed enum in `extract_container_info` and match on
    // it here, instead of this substring heuristic (which would also match a
    // hypothetical "not-running"). Empirical scaffold until the schema is known.
    pub fn is_running(&self) -> bool {
        self.status.to_lowercase().contains("running")
    }
}

/// Trait for the apple-container backend lifecycle operations.
///
/// All methods shell out to the `container` CLI via `jackin-process`.
/// The Docker backend uses bollard (typed async API) — this trait is NOT
/// compatible with `DockerApi` and requires a separate implementation.
pub trait AppleContainerApi: Send + Sync {
    /// Start a new container from the given spec.
    /// Equivalent to: `container run --name <name> [flags] <spec.image> jackin-capsule`
    async fn run_container(&self, name: &str, spec: &AppleContainerSpec) -> Result<()>;

    /// Attach to a running container and return the child process handle.
    /// The caller pipes stdio through the child for the interactive session.
    /// Equivalent to: `container exec -it <name> jackin-capsule`
    /// Stop a running container.
    /// Equivalent to: `container stop <name>`
    async fn stop_container(&self, name: &str) -> Result<()>;

    /// Remove a container (must be stopped first).
    /// Equivalent to: `container rm <name>`
    async fn remove_container(&self, name: &str) -> Result<()>;

    /// Inspect a container. Returns `None` if the container does not exist.
    /// Resolved from `container ps --format json` filtered to the exact name.
    async fn inspect_container(&self, name: &str) -> Result<Option<AppleContainerInfo>>;

    /// List containers whose names start with `name_prefix`.
    /// Equivalent to: `container ps [--all] --format json`
    async fn list_containers(&self, name_prefix: &str) -> Result<Vec<AppleContainerInfo>>;
}

/// Production implementation — shells out to the `container` CLI.
#[derive(Debug)]
pub struct AppleContainerClient;

impl AppleContainerClient {
    pub const fn new() -> Self {
        Self
    }

    /// Run a no-output `container <sub> <name>` lifecycle command, logging the
    /// outcome and bailing on a non-zero exit. Shared by `stop`/`remove`, which
    /// differ only in the subcommand.
    async fn lifecycle(&self, name: &str, sub: &str) -> Result<()> {
        let output = crate::process_telemetry::exec_async(&jackin_process::ExecRequest::new(
            "container",
            [sub, name],
        ))
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
            spec.user == crate::runtime::identity::CAPSULE_SUPERVISOR_USER,
            "apple-container capsule supervisor must launch as root"
        );
        let args = container_run_args(name, spec);

        let output = crate::process_telemetry::exec_async(&jackin_process::ExecRequest::new(
            "container",
            &args,
        ))
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
        // Single `container ps` codepath: filter the listing to the exact name.
        Ok(self
            .list_containers(name)
            .await?
            .into_iter()
            .find(|c| c.name == name))
    }

    async fn list_containers(&self, name_prefix: &str) -> Result<Vec<AppleContainerInfo>> {
        let output = crate::process_telemetry::exec_async(&jackin_process::ExecRequest::new(
            "container",
            ["ps", "--all", "--format", "json"],
        ))
        .await?;
        if !output.success {
            // Distinguish "command failed" (CLI missing, daemon down, perms)
            // from "no containers". Returning Ok(vec![]) here would mask the
            // failure as an empty list, making is_container_running report
            // false and producing inexplicable reconnect behavior.
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

/// Parse `container ps --format json` output into container info records.
/// The exact JSON schema is determined empirically during Phase 0 testing;
/// this implementation handles the most common shapes (array or NDJSON).
fn parse_all_containers_json(json_output: &str) -> Vec<AppleContainerInfo> {
    let trimmed = json_output.trim();
    if trimmed.is_empty() {
        return vec![];
    }

    // apple/container may emit a JSON array or newline-delimited JSON objects.
    let mut results = Vec::new();

    // Try as a JSON array first.
    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(trimmed) {
        for item in arr {
            if let Some(info) = extract_container_info(&item) {
                results.push(info);
            }
        }
        return results;
    }

    // Try newline-delimited JSON objects.
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

/// Test double for unit tests that do not want to shell out to the `container` CLI.
#[cfg(test)]
#[derive(Debug)]
pub struct FakeAppleContainerClient {
    pub containers: std::sync::Mutex<Vec<AppleContainerInfo>>,
}

#[cfg(test)]
impl Default for FakeAppleContainerClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl FakeAppleContainerClient {
    pub fn new() -> Self {
        Self {
            containers: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::unused_async_trait_impl,
    reason = "the Apple-container test double implements the async CLI seam with immediate in-memory results"
)]
impl AppleContainerApi for FakeAppleContainerClient {
    async fn run_container(&self, name: &str, _spec: &AppleContainerSpec) -> Result<()> {
        self.containers.lock().unwrap().push(AppleContainerInfo {
            name: name.to_owned(),
            status: "running".to_owned(),
        });
        Ok(())
    }

    async fn stop_container(&self, name: &str) -> Result<()> {
        {
            let mut containers = self.containers.lock().unwrap();
            if let Some(c) = containers.iter_mut().find(|c| c.name == name) {
                c.status = "stopped".to_owned();
            }
        }
        Ok(())
    }

    async fn remove_container(&self, name: &str) -> Result<()> {
        self.containers.lock().unwrap().retain(|c| c.name != name);
        Ok(())
    }

    async fn inspect_container(&self, name: &str) -> Result<Option<AppleContainerInfo>> {
        Ok(self
            .containers
            .lock()
            .unwrap()
            .iter()
            .find(|c| c.name == name)
            .cloned())
    }

    async fn list_containers(&self, name_prefix: &str) -> Result<Vec<AppleContainerInfo>> {
        Ok(self
            .containers
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.name.starts_with(name_prefix))
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests;
