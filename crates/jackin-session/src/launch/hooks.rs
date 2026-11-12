// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::time::Duration;

use jackin_core::{EnvName, HookKind, SandboxName, SandboxPath, TerminalText, sandbox_layout};
use jackin_openshell::{Gateway, GatewayError, ProgramRun, ProgramStdin};
use tokio_util::sync::CancellationToken;

use crate::attach::RestartError;
use crate::launch::LaunchError;
use crate::state::EnvReadError;

#[derive(Debug)]
pub(crate) enum HookFailure {
    Gateway(GatewayError),
    Failed {
        hook: HookKind,
        exit_code: i32,
        output: TerminalText,
    },
}

impl From<GatewayError> for HookFailure {
    fn from(error: GatewayError) -> HookFailure {
        HookFailure::Gateway(error)
    }
}

impl From<HookFailure> for LaunchError {
    fn from(failure: HookFailure) -> LaunchError {
        match failure {
            HookFailure::Gateway(error) => LaunchError::Gateway(error),
            HookFailure::Failed {
                hook,
                exit_code,
                output,
            } => LaunchError::HookFailed {
                hook,
                exit_code,
                output,
            },
        }
    }
}

impl From<EnvReadError> for LaunchError {
    fn from(error: EnvReadError) -> LaunchError {
        match error {
            EnvReadError::Gateway(error) => LaunchError::Gateway(error),
            EnvReadError::Missing => LaunchError::ProbeInvalid {
                message: "the env file is missing".to_owned(),
            },
            EnvReadError::Invalid(error) => LaunchError::ProbeInvalid {
                message: format!("the env file is invalid: {error}"),
            },
        }
    }
}

impl From<HookFailure> for RestartError {
    fn from(failure: HookFailure) -> RestartError {
        match failure {
            HookFailure::Gateway(error) => RestartError::Gateway(error),
            HookFailure::Failed {
                hook,
                exit_code,
                output,
            } => RestartError::HookFailed {
                hook,
                exit_code,
                output,
            },
        }
    }
}

pub(crate) struct HookRun<'a> {
    pub(crate) gateway: &'a dyn Gateway,
    pub(crate) sandbox: &'a SandboxName,
    pub(crate) kind: HookKind,
    pub(crate) env: &'a BTreeMap<EnvName, String>,
    pub(crate) touch_on_success: Option<SandboxPath>,
    pub(crate) timeout: Duration,
}

pub(crate) async fn run(hook: HookRun<'_>, cancel: &CancellationToken) -> Result<(), HookFailure> {
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
    Err(HookFailure::Failed {
        hook: hook.kind,
        exit_code: outcome.exit_code,
        output: TerminalText::sanitize(&output),
    })
}
