// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{
    ProcessError, ProcessRunner, ProcessSpec, ProcessStatus, ProcessStdin, Program, Secret,
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroize;

const MUSE_KEYCHAIN_ARGS: [&str; 6] = [
    "find-generic-password",
    "-s",
    "ai.meta.dev.credentials",
    "-a",
    "meta",
    "-w",
];

const BROKER_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SecretBrokerError {
    Unavailable,
    InvalidResponse,
}

#[derive(Deserialize)]
struct MuseKeychainReply {
    access_token: WipingString,
}

#[derive(Deserialize)]
#[serde(transparent)]
struct WipingString(String);

impl WipingString {
    fn into_secret(mut self) -> Secret {
        Secret::new(std::mem::take(&mut self.0))
    }
}

impl Drop for WipingString {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub(crate) async fn muse_keychain_access_token(
    runner: &dyn ProcessRunner,
    cancel: &CancellationToken,
) -> Result<Secret, SecretBrokerError> {
    let spec = ProcessSpec {
        program: Program::Security,
        args: MUSE_KEYCHAIN_ARGS.map(str::to_owned).to_vec(),
        cwd: None,
        env: Vec::new(),
        env_remove_prefixes: Vec::new(),
        stdin: ProcessStdin::Null,
        timeout: BROKER_TIMEOUT,
    };
    let output = runner
        .run_secret(&spec, cancel)
        .await
        .map_err(|error| match error {
            ProcessError::Spawn { .. } => SecretBrokerError::Unavailable,
            _ => SecretBrokerError::InvalidResponse,
        })?;
    if output.status != ProcessStatus::Exited(0) {
        return Err(SecretBrokerError::InvalidResponse);
    }
    let response: MuseKeychainReply = serde_json::from_slice(output.stdout.expose())
        .map_err(|_| SecretBrokerError::InvalidResponse)?;
    if response.access_token.0.trim().is_empty() {
        return Err(SecretBrokerError::InvalidResponse);
    }
    Ok(response.access_token.into_secret())
}

pub(crate) async fn onepassword_secret(
    runner: &dyn ProcessRunner,
    reference: &str,
    account: Option<&str>,
    cancel: &CancellationToken,
) -> Result<Secret, SecretBrokerError> {
    let mut args = vec!["read".to_owned()];
    if let Some(account) = account {
        args.extend(["--account".to_owned(), account.to_owned()]);
    }
    args.push(reference.to_owned());
    let spec = ProcessSpec {
        program: Program::Op,
        args,
        cwd: None,
        env: Vec::new(),
        env_remove_prefixes: Vec::new(),
        stdin: ProcessStdin::Null,
        timeout: BROKER_TIMEOUT,
    };
    let output = runner
        .run_secret(&spec, cancel)
        .await
        .map_err(|_| SecretBrokerError::InvalidResponse)?;
    if output.status != ProcessStatus::Exited(0) {
        return Err(SecretBrokerError::InvalidResponse);
    }
    let response = std::str::from_utf8(output.stdout.expose())
        .map_err(|_| SecretBrokerError::InvalidResponse)?
        .trim();
    if response.is_empty() {
        return Err(SecretBrokerError::InvalidResponse);
    }
    Ok(Secret::new(response.to_owned()))
}
