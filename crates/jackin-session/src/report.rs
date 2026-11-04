// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use jackin_core::{HookKind, LaunchId, Phase, SandboxName, TerminalText};

use crate::attach::EnvFileProblem;
use crate::launch::{LaunchError, LaunchFailure};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchNextStep {
    Retry {
        sandbox: SandboxName,
        launch: LaunchId,
    },
}

pub fn error_chain(error: &dyn Error) -> String {
    let mut text = error.to_string();
    let mut last = text.clone();
    let mut source = error.source();
    while let Some(cause) = source {
        let line = cause.to_string();
        if line != last {
            text.push_str(": ");
            text.push_str(&line);
        }
        last = line;
        source = cause.source();
    }
    text
}

pub fn main_process_ended(sandbox: &SandboxName, phase: Phase) -> String {
    format!(
        "the main process of sandbox {sandbox} has ended ({phase}); only purge --force can remove it"
    )
}

pub fn sandbox_missing(sandbox: &SandboxName) -> String {
    format!("sandbox {sandbox} was not found")
}

pub fn env_file_problem(sandbox: &SandboxName, problem: &EnvFileProblem) -> String {
    match problem {
        EnvFileProblem::NoLaunchLabel => {
            format!("sandbox {sandbox} has no launch label, so its env file cannot be found")
        }
        EnvFileProblem::Missing => {
            format!(
                "sandbox {sandbox} has no env file; its launch never reached the credentials step"
            )
        }
        EnvFileProblem::Invalid(error) => {
            format!("the env file of sandbox {sandbox} is invalid: {error}")
        }
    }
}

pub fn hook_failure(hook: HookKind, exit_code: i32, output: &TerminalText) -> String {
    format!(
        "the {hook} hook exited with code {exit_code}\n{}",
        output.as_str()
    )
}

impl LaunchError {
    pub fn text(&self) -> String {
        match self {
            LaunchError::HookFailed {
                hook,
                exit_code,
                output,
            } => hook_failure(*hook, *exit_code, output),
            LaunchError::MainProcessEnded { sandbox, phase } => main_process_ended(sandbox, *phase),
            other => error_chain(other),
        }
    }
}

impl LaunchFailure {
    pub fn summary(&self) -> String {
        format!(
            "launch {} failed at step {}: {}",
            self.launch_id,
            self.step,
            self.error.text()
        )
    }

    pub fn next_step(&self) -> Option<LaunchNextStep> {
        let stale = matches!(*self.error, LaunchError::StaleSandbox { .. });
        self.sandbox
            .as_ref()
            .filter(|_| !stale)
            .map(|sandbox| LaunchNextStep::Retry {
                sandbox: sandbox.clone(),
                launch: self.launch_id.clone(),
            })
    }
}
