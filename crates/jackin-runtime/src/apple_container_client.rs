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

/// Backend name used in CLI flags, config keys, and instance manifests.
pub const BACKEND_NAME: &str = "apple-container";

/// Backend name for the default Docker + `DinD` backend.
pub const DOCKER_BACKEND_NAME: &str = "docker";

/// Specification for launching a role container via `container run`.
#[derive(Debug, Clone)]
pub struct AppleContainerSpec {
    /// Role OCI image reference.
    pub image: String,
    /// Environment variables to inject (`-e KEY=VALUE` flags).
    pub env: Vec<(String, String)>,
    /// Bind mounts (`-v host_path:container_path` flags).
    pub mounts: Vec<(PathBuf, PathBuf)>,
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
    async fn exec_attach(&self, name: &str) -> Result<tokio::process::Child>;

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
        jackin_diagnostics::telemetry_debug!(
            "apple-container",
            "container_state action={sub} name={name}"
        );
        let output =
            jackin_process::exec_async(&jackin_process::ExecRequest::new("container", [sub, name]))
                .await?;
        if !output.success {
            let stderr = String::from_utf8_lossy(&output.stderr);
            jackin_diagnostics::telemetry_debug!(
                "apple-container",
                "container_state action={sub} name={name} result=failure reason={}",
                stderr.trim()
            );
            anyhow::bail!("container {sub} failed: {}", stderr.trim());
        }
        jackin_diagnostics::telemetry_debug!(
            "apple-container",
            "container_state action={sub} name={name} result=ok"
        );
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
        let mut args: Vec<std::ffi::OsString> =
            vec!["run".into(), "--name".into(), name.into(), "-d".into()];

        for (k, v) in &spec.env {
            args.extend(["-e".into(), format!("{k}={v}").into()]);
        }
        for (host, container) in &spec.mounts {
            args.extend([
                "-v".into(),
                format!("{}:{}", host.display(), container.display()).into(),
            ]);
        }
        for cap in &spec.caps_add {
            args.extend(["--cap-add".into(), cap.into()]);
        }
        args.extend([spec.image.clone().into(), "jackin-capsule".into()]);

        jackin_diagnostics::telemetry_debug!(
            "apple-container",
            "container_run name={name} image={}",
            spec.image
        );

        let output =
            jackin_process::exec_async(&jackin_process::ExecRequest::new("container", &args))
                .await?;
        if !output.success {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("container run failed: {}", stderr.trim());
        }
        jackin_diagnostics::telemetry_debug!("apple-container", "container_run name={name} ok");
        Ok(())
    }

    async fn exec_attach(&self, name: &str) -> Result<tokio::process::Child> {
        jackin_diagnostics::telemetry_debug!(
            "apple-container",
            "attach transport=container-exec name={name}"
        );
        let request =
            jackin_process::ExecRequest::new("container", ["exec", "-it", name, "jackin-capsule"])
                .stdin_mode(jackin_process::StdioMode::Inherit)
                .stdout_mode(jackin_process::StdioMode::Inherit)
                .stderr_mode(jackin_process::StdioMode::Inherit);
        let child = jackin_process::spawn_async(&request)?;
        Ok(child)
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
        let output = jackin_process::exec_async(&jackin_process::ExecRequest::new(
            "container",
            ["ps", "--all", "--format", "json"],
        ))
        .await?;
        if !output.success {
            // Distinguish "command failed" (CLI missing, daemon down, perms)
            // from "no containers". Returning Ok(vec![]) here would mask the
            // failure as an empty list, making is_container_running report
            // false and producing inexplicable reconnect behavior.
            let stderr = String::from_utf8_lossy(&output.stderr);
            jackin_diagnostics::telemetry_debug!(
                "apple-container",
                "container ps failed: {}",
                stderr.trim()
            );
            anyhow::bail!("container ps failed: {}", stderr.trim());
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let all = parse_all_containers_json(&stdout);
        Ok(all
            .into_iter()
            .filter(|c| c.name.starts_with(name_prefix))
            .collect())
    }
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
impl AppleContainerApi for FakeAppleContainerClient {
    async fn run_container(&self, name: &str, _spec: &AppleContainerSpec) -> Result<()> {
        self.containers.lock().unwrap().push(AppleContainerInfo {
            name: name.to_owned(),
            status: "running".to_owned(),
        });
        Ok(())
    }

    async fn exec_attach(&self, _name: &str) -> Result<tokio::process::Child> {
        // Fake exec — returns a command that exits immediately.
        let request = jackin_process::ExecRequest::new("true", None::<&str>)
            .stdin_mode(jackin_process::StdioMode::Inherit)
            .stdout_mode(jackin_process::StdioMode::Inherit)
            .stderr_mode(jackin_process::StdioMode::Inherit);
        let child = jackin_process::spawn_async(&request)?;
        Ok(child)
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
