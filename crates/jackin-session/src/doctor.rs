// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{
    Compatibility, FileSystem, GatewayName, HostPath, ProcessRunner, ProcessSpec, ProcessStatus,
    ProcessStdin, Program, TerminalText,
};
use jackin_openshell::{Gateway, GatewayError, registered_gateways};
use tokio_util::sync::CancellationToken;

use crate::project::check_git;

const VERSION_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct DoctorInput<'a> {
    pub runner: &'a dyn ProcessRunner,
    pub fs: &'a dyn FileSystem,
    pub gateway: GatewayAccess<'a>,
    pub engine_socket: HostPath,
    pub pinned: &'a Compatibility,
}

#[derive(Clone, Copy, Debug)]
pub enum GatewayAccess<'a> {
    NotChosen,
    Connected(&'a dyn Gateway),
    Failed {
        name: &'a GatewayName,
        error: &'a GatewayError,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CheckId {
    OpenshellBinary,
    SshBinary,
    GitBinary,
    GatewayRegistered,
    GatewayHealth,
    PinnedVersion,
    EngineSocket,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
    Skip,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub id: CheckId,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoctorReport {
    pub checks: Vec<Check>,
}

impl DoctorReport {
    pub fn is_healthy(&self) -> bool {
        self.checks
            .iter()
            .all(|check| check.status != CheckStatus::Fail)
    }
}

fn check(id: CheckId, status: CheckStatus, detail: impl Into<String>) -> Check {
    Check {
        id,
        status,
        detail: detail.into(),
    }
}

async fn binary(
    runner: &dyn ProcessRunner,
    cancel: &CancellationToken,
    id: CheckId,
    program: Program,
    flag: &str,
) -> Check {
    let spec = ProcessSpec {
        program,
        args: vec![flag.to_owned()],
        cwd: None,
        env: Vec::new(),
        env_remove_prefixes: vec!["OPENSHELL_".to_owned()],
        stdin: ProcessStdin::Null,
        timeout: VERSION_TIMEOUT,
    };
    match runner.run(&spec, None, cancel).await {
        Ok(output) if output.status == ProcessStatus::Exited(0) => {
            let mut text = output.stdout;
            text.extend(output.stderr);
            check(id, CheckStatus::Pass, clean(&text))
        }
        Ok(output) => check(
            id,
            CheckStatus::Fail,
            format!("{} {flag} ended with {:?}", program.binary(), output.status),
        ),
        Err(error) => check(id, CheckStatus::Fail, error.to_string()),
    }
}

async fn git(runner: &dyn ProcessRunner, cancel: &CancellationToken) -> Check {
    match check_git(runner, cancel).await {
        Ok(version) => check(CheckId::GitBinary, CheckStatus::Pass, version.to_string()),
        Err(error) => check(CheckId::GitBinary, CheckStatus::Fail, error.to_string()),
    }
}

fn clean(bytes: &[u8]) -> String {
    TerminalText::sanitize(bytes).as_str().trim().to_owned()
}

fn same_version(reported: &str, pinned: &str) -> bool {
    reported.strip_prefix('v').unwrap_or(reported) == pinned.strip_prefix('v').unwrap_or(pinned)
}

async fn registered(
    input: &DoctorInput<'_>,
    cancel: &CancellationToken,
    name: &GatewayName,
) -> Check {
    match registered_gateways(input.runner, cancel).await {
        Ok(list) if list.iter().any(|entry| &entry.name == name) => check(
            CheckId::GatewayRegistered,
            CheckStatus::Pass,
            name.to_string(),
        ),
        Ok(_) => check(
            CheckId::GatewayRegistered,
            CheckStatus::Fail,
            format!("{name} is not registered with openshell"),
        ),
        Err(error) => check(
            CheckId::GatewayRegistered,
            CheckStatus::Fail,
            error.to_string(),
        ),
    }
}

async fn gateway_checks(input: &DoctorInput<'_>, cancel: &CancellationToken) -> [Check; 3] {
    let skipped = |id| check(id, CheckStatus::Skip, "no gateway chosen");
    match input.gateway {
        GatewayAccess::NotChosen => [
            skipped(CheckId::GatewayRegistered),
            skipped(CheckId::GatewayHealth),
            skipped(CheckId::PinnedVersion),
        ],
        GatewayAccess::Failed { name, error } => [
            registered(input, cancel, name).await,
            check(CheckId::GatewayHealth, CheckStatus::Fail, error.to_string()),
            check(
                CheckId::PinnedVersion,
                CheckStatus::Skip,
                "the gateway did not connect",
            ),
        ],
        GatewayAccess::Connected(gateway) => {
            let (health, version) = match gateway.health().await {
                Ok(report) => (
                    if report.serving {
                        check(CheckId::GatewayHealth, CheckStatus::Pass, "serving")
                    } else {
                        check(CheckId::GatewayHealth, CheckStatus::Fail, "not serving")
                    },
                    if same_version(&report.version, &input.pinned.openshell_version) {
                        check(
                            CheckId::PinnedVersion,
                            CheckStatus::Pass,
                            clean(report.version.as_bytes()),
                        )
                    } else {
                        check(
                            CheckId::PinnedVersion,
                            CheckStatus::Fail,
                            format!(
                                "the gateway runs {}, jackin is pinned to {}",
                                clean(report.version.as_bytes()),
                                input.pinned.openshell_version
                            ),
                        )
                    },
                ),
                Err(error) => (
                    check(CheckId::GatewayHealth, CheckStatus::Fail, error.to_string()),
                    check(
                        CheckId::PinnedVersion,
                        CheckStatus::Skip,
                        "the gateway did not answer",
                    ),
                ),
            };
            [
                registered(input, cancel, gateway.name()).await,
                health,
                version,
            ]
        }
    }
}

async fn engine(input: &DoctorInput<'_>) -> Check {
    match input.fs.stat(&input.engine_socket).await {
        Ok(Some(_)) => check(CheckId::EngineSocket, CheckStatus::Pass, "present"),
        Ok(None) => check(
            CheckId::EngineSocket,
            CheckStatus::Warn,
            "the engine socket is missing; the Docker capability and image builds need it",
        ),
        Err(error) => check(CheckId::EngineSocket, CheckStatus::Warn, error.to_string()),
    }
}

pub async fn doctor(input: &DoctorInput<'_>, cancel: &CancellationToken) -> DoctorReport {
    let [registered, health, version] = gateway_checks(input, cancel).await;
    DoctorReport {
        checks: vec![
            binary(
                input.runner,
                cancel,
                CheckId::OpenshellBinary,
                Program::Openshell,
                "--version",
            )
            .await,
            binary(input.runner, cancel, CheckId::SshBinary, Program::Ssh, "-V").await,
            git(input.runner, cancel).await,
            registered,
            health,
            version,
            engine(input).await,
        ],
    }
}
