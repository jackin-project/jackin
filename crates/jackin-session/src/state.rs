// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{
    EnvFile, EnvFileError, EnvName, LaunchId, SandboxName, SupervisorReading, SupervisorState,
    sandbox_layout,
};
use jackin_openshell::{Gateway, GatewayError};
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub(crate) enum EnvReadError {
    Gateway(GatewayError),
    Missing,
    Invalid(EnvFileError),
}

impl From<GatewayError> for EnvReadError {
    fn from(error: GatewayError) -> EnvReadError {
        EnvReadError::Gateway(error)
    }
}

pub(crate) fn reading(bytes: Option<&[u8]>) -> SupervisorReading {
    let Some(bytes) = bytes else {
        return SupervisorReading::Unreadable;
    };
    match std::str::from_utf8(bytes) {
        Ok(text) => match SupervisorState::parse(text) {
            Ok(state) => SupervisorReading::Reported(state),
            Err(error) => SupervisorReading::Invalid(error.to_string()),
        },
        Err(_) => SupervisorReading::Invalid("the state is not valid UTF-8".to_owned()),
    }
}

pub(crate) async fn run_state(
    gateway: &dyn Gateway,
    sandbox: &SandboxName,
    cancel: &CancellationToken,
) -> Result<SupervisorReading, GatewayError> {
    let mut found = gateway
        .read_files(
            sandbox,
            std::slice::from_ref(&sandbox_layout::RUN_STATE),
            cancel,
        )
        .await?;
    Ok(reading(found.pop().flatten().as_deref()))
}

pub(crate) async fn env_file(
    gateway: &dyn Gateway,
    sandbox: &SandboxName,
    launch: &LaunchId,
    cancel: &CancellationToken,
) -> Result<BTreeMap<EnvName, String>, EnvReadError> {
    let path = sandbox_layout::env_file(launch);
    let mut found = gateway
        .read_files(sandbox, std::slice::from_ref(&path), cancel)
        .await?;
    let bytes = found.pop().flatten().ok_or(EnvReadError::Missing)?;
    let invalid = EnvReadError::Invalid;
    let text = String::from_utf8(bytes).map_err(|_| invalid(EnvFileError::Syntax { line: 1 }))?;
    EnvFile::parse(&text)
        .map(|file| file.values().clone())
        .map_err(invalid)
}
