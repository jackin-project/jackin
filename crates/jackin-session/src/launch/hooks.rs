// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::time::Duration;

use jackin_core::{
    EnvFile, EnvName, HookKind, LaunchId, SandboxName, SandboxPath, TerminalText, sandbox_layout,
};
use jackin_openshell::{Gateway, ProgramRun, ProgramStdin};
use tokio_util::sync::CancellationToken;

use crate::launch::LaunchError;

pub(super) async fn read_env(
    gateway: &dyn Gateway,
    sandbox: &SandboxName,
    launch: &LaunchId,
) -> Result<BTreeMap<EnvName, String>, LaunchError> {
    let path = sandbox_layout::env_file(launch);
    let mut found = gateway
        .read_files(sandbox, std::slice::from_ref(&path))
        .await?;
    let bytes = found
        .pop()
        .flatten()
        .ok_or_else(|| LaunchError::ProbeInvalid {
            message: format!("{} is missing", path.as_str()),
        })?;
    let text = String::from_utf8(bytes).map_err(|_| LaunchError::ProbeInvalid {
        message: format!("{} is not valid UTF-8", path.as_str()),
    })?;
    let file = EnvFile::parse(&text).map_err(|error| LaunchError::ProbeInvalid {
        message: format!("{} is invalid: {error}", path.as_str()),
    })?;
    Ok(file.values().clone())
}

pub(super) struct HookRun<'a> {
    pub(super) gateway: &'a dyn Gateway,
    pub(super) sandbox: &'a SandboxName,
    pub(super) kind: HookKind,
    pub(super) env: &'a BTreeMap<EnvName, String>,
    pub(super) touch_on_success: Option<SandboxPath>,
    pub(super) timeout: Duration,
}

pub(super) async fn run(hook: HookRun<'_>, cancel: &CancellationToken) -> Result<(), LaunchError> {
    let outcome = hook
        .gateway
        .run_program(
            &ProgramRun {
                sandbox: hook.sandbox.clone(),
                program: sandbox_layout::hook_path(hook.kind),
                args: Vec::new(),
                workdir: Some(sandbox_layout::PROJECT.clone()),
                env: hook.env.clone(),
                stdin: ProgramStdin::Closed,
                touch_on_success: hook.touch_on_success,
                timeout: hook.timeout,
            },
            cancel,
        )
        .await?;
    if outcome.exit_code == 0 {
        return Ok(());
    }
    let mut output = outcome.stdout;
    output.extend(outcome.stderr);
    Err(LaunchError::HookFailed {
        hook: hook.kind,
        exit_code: outcome.exit_code,
        output: TerminalText::sanitize(&output),
    })
}
