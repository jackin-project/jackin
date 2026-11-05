// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{
    EnvValue, GatewayName, HostPath, OutputStream, ProcessError, ProcessOutput, ProcessRunner,
    ProcessSpec, ProcessStatus, ProcessStdin, SecretOutput, StdFileSystem, plain,
};
use jackin_openshell::{GrpcGateway, GrpcGatewayConfig, resolve_gateway};
use serde_json::Value;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use super::clock::{Budgets, SystemClock};
use super::engine::SocketEngine;
use super::host::{Exit, HostCommand, HostError, HostProcess, RealHost};
use super::pty::PortablePty;
use super::report::Sink;
use super::{Boundaries, E2eArgs};
use crate::repo::cargo_program;

const BUILD_TIMEOUT: Duration = Duration::from_mins(30);
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(600);
const CALL_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Error)]
pub enum ProductionError {
    #[error("{}", plain(.0))]
    Setup(String),
}

fn setup(message: impl Into<String>) -> ProductionError {
    ProductionError::Setup(message.into())
}

#[derive(Debug)]
pub struct ToolRunner {
    host: Arc<dyn HostProcess>,
    env: BTreeMap<String, String>,
}

impl ToolRunner {
    #[must_use]
    pub fn new(host: Arc<dyn HostProcess>, env: BTreeMap<String, String>) -> ToolRunner {
        ToolRunner { host, env }
    }
}

fn spawn_error(spec: &ProcessSpec, message: &str) -> ProcessError {
    ProcessError::Spawn {
        program: spec.program,
        message: message.to_owned(),
    }
}

#[async_trait]
impl ProcessRunner for ToolRunner {
    async fn run(
        &self,
        spec: &ProcessSpec,
        _lines: Option<&jackin_core::LineSink>,
        cancel: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        if !matches!(spec.stdin, ProcessStdin::Null) {
            return Err(spawn_error(spec, "xtask e2e runs tools without stdin only"));
        }
        let mut env: BTreeMap<String, String> = self
            .env
            .iter()
            .filter(|(name, _)| {
                !spec
                    .env_remove_prefixes
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
            })
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        for (name, value) in &spec.env {
            match value {
                EnvValue::Plain(text) => {
                    env.insert(name.clone(), text.clone());
                }
                EnvValue::Secret(_) => {
                    return Err(spawn_error(spec, "xtask e2e passes no secret to a tool"));
                }
            }
        }
        let mut command = HostCommand::new(spec.program.binary(), spec.timeout)
            .args(
                spec.program
                    .leading_args()
                    .iter()
                    .map(|part| (*part).to_owned()),
            )
            .args(spec.args.clone())
            .env(env);
        if let Some(cwd) = &spec.cwd {
            command = command.cwd(cwd.as_path());
        }
        match self.host.run(&command, cancel).await {
            Ok(output) => Ok(ProcessOutput {
                status: match output.exit {
                    Exit::Code(code) => ProcessStatus::Exited(code),
                    Exit::Signal(signal) => ProcessStatus::Signaled(signal),
                },
                stdout: output.stdout.to_vec(),
                stderr: output.stderr.to_vec(),
            }),
            Err(HostError::Timeout { after, group, .. }) => Err(ProcessError::Timeout {
                program: spec.program,
                after,
                group,
            }),
            Err(HostError::Cancelled { group, .. }) => Err(ProcessError::Cancelled {
                program: spec.program,
                group,
            }),
            Err(HostError::Group { fault, .. }) => Err(ProcessError::Group {
                program: spec.program,
                fault,
            }),
            Err(HostError::OutputTooLarge { limit, .. }) => Err(ProcessError::OutputTooLarge {
                program: spec.program,
                stream: OutputStream::Stdout,
                limit,
            }),
            Err(other) => Err(spawn_error(spec, &other.to_string())),
        }
    }

    async fn run_secret(
        &self,
        spec: &ProcessSpec,
        _cancel: &CancellationToken,
    ) -> Result<SecretOutput, ProcessError> {
        Err(spawn_error(
            spec,
            "xtask e2e does not read secret tool output",
        ))
    }
}

fn random_tag() -> Result<String, ProductionError> {
    let mut bytes = [0_u8; 2];
    getrandom::fill(&mut bytes)
        .map_err(|error| setup(format!("no entropy for the run tag: {error}")))?;
    Ok(super::redact::hex(&bytes))
}

fn repo_root() -> Result<PathBuf, ProductionError> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| setup("cannot find the repository root"))
}

async fn build_jackin(
    host: &dyn HostProcess,
    env: &BTreeMap<String, String>,
    root: &Path,
    cancel: &CancellationToken,
) -> Result<PathBuf, ProductionError> {
    let command = HostCommand::new(
        cargo_program().to_string_lossy().into_owned(),
        BUILD_TIMEOUT,
    )
    .args([
        "build",
        "--locked",
        "-p",
        "jackin",
        "--message-format=json-render-diagnostics",
    ])
    .cwd(root)
    .env(env.clone());
    let output = host
        .run(&command, cancel)
        .await
        .map_err(|error| setup(format!("cargo build of jackin failed to run: {error}")))?;
    if !output.ok() {
        return Err(setup(format!(
            "cargo build of jackin exited {}: {}",
            output.exit.describe(),
            output.stderr_text().lines().last().unwrap_or("")
        )));
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|message| {
            message.get("reason").and_then(Value::as_str) == Some("compiler-artifact")
        })
        .filter(|message| {
            message.pointer("/target/name").and_then(Value::as_str) == Some("jackin")
                && message
                    .pointer("/target/kind")
                    .and_then(Value::as_array)
                    .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"))
        })
        .find_map(|message| {
            message
                .get("executable")
                .and_then(Value::as_str)
                .map(PathBuf::from)
        })
        .ok_or_else(|| setup("cargo build reported no jackin executable"))
}

pub async fn production(args: &E2eArgs) -> Result<Boundaries, ProductionError> {
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let root = repo_root()?;
    let host: Arc<dyn HostProcess> = Arc::new(RealHost::default());
    let cancel = CancellationToken::new();
    let jackin = match &args.jackin {
        Some(path) => std::fs::canonicalize(path).map_err(|_| {
            setup(format!(
                "the jackin binary {} does not exist",
                path.display()
            ))
        })?,
        None if args.sweep => PathBuf::from("jackin"),
        None => build_jackin(host.as_ref(), &env, &root, &cancel).await?,
    };
    let home = env.get("HOME").cloned();
    let codex_home = args
        .codex_home
        .clone()
        .or_else(|| home.as_ref().map(|home| Path::new(home).join(".codex")))
        .ok_or_else(|| setup("HOME is not set and --codex-home was not given"))?;
    let config_base = env
        .get("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|home| Path::new(home).join(".config")))
        .ok_or_else(|| setup("HOME is not set"))?;
    let registry = HostPath::parse(&config_base.join("openshell"))
        .map_err(|error| setup(format!("the OpenShell registry path: {error}")))?;
    let name = GatewayName::parse(&args.gateway)
        .map_err(|error| setup(format!("the gateway name: {error}")))?;
    let runner: Arc<dyn ProcessRunner> = Arc::new(ToolRunner::new(
        Arc::clone(&host),
        super::scratch::base_env(&env),
    ));
    let resolved = resolve_gateway(
        runner.as_ref(),
        &StdFileSystem::default(),
        &registry,
        &name,
        &cancel,
    )
    .await
    .map_err(|error| {
        setup(format!(
            "cannot resolve the gateway {}: {error}",
            args.gateway
        ))
    })?;
    let workspace = jackin_core::OpenShellWorkspace::parse("default")
        .map_err(|error| setup(error.to_string()))?;
    let gateway = GrpcGateway::connect(GrpcGatewayConfig {
        gateway: resolved,
        workspace,
        runner,
        transfer_timeout: TRANSFER_TIMEOUT,
        call_timeout: CALL_TIMEOUT,
        cancel: cancel.clone(),
    })
    .await
    .map_err(|error| {
        setup(format!(
            "cannot connect to the gateway {}: {error}",
            args.gateway
        ))
    })?;
    let tag = random_tag()?;
    let log_dir = args
        .log_dir
        .clone()
        .unwrap_or_else(|| root.join("target/xtask-e2e").join(&tag));
    let docker_host = env.get("DOCKER_HOST").cloned();
    Ok(Boundaries {
        host,
        pty: Arc::new(PortablePty::default()),
        gateway: Arc::new(gateway),
        engine: Arc::new(SocketEngine::from_docker_host(docker_host.as_deref())),
        clock: Arc::new(SystemClock::new()),
        budgets: Budgets::production(),
        jackin,
        repo_root: root,
        codex_home,
        scratch_root: std::env::temp_dir().join(super::scratch::scratch_dir_name()),
        log_dir,
        lock_dir: std::env::temp_dir(),
        tag,
        cancel,
        stdout: Sink::stdout(),
        stderr: Sink::stderr(),
        env,
    })
}
