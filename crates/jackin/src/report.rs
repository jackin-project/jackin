// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use jackin_core::{HookKind, Phase, SandboxName, TerminalText, WorkspaceName};
use jackin_session::{
    AttachError, EnvFileProblem, ExportError, LaunchError, LaunchFailure, LifecycleError,
    PlanError, PurgeError, RestartError,
};

use crate::error::CommandError;

pub(crate) fn chain(error: &dyn Error) -> String {
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

fn sandbox_ended(sandbox: &SandboxName, phase: Phase) -> String {
    format!(
        "the main process of sandbox {sandbox} has ended ({phase}); only purge --force can remove it"
    )
}

fn sandbox_missing(sandbox: &SandboxName) -> String {
    format!("sandbox {sandbox} was not found")
}

fn env_problem(sandbox: &SandboxName, problem: &EnvFileProblem) -> String {
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

fn hook_failure(hook: HookKind, exit_code: i32, output: &TerminalText) -> String {
    format!(
        "the {hook} hook exited with code {exit_code}\n{}",
        output.as_str()
    )
}

fn workspace_unknown(workspace: &WorkspaceName) -> String {
    format!("workspace {workspace} is not in the config")
}

fn launch_error(error: &LaunchError) -> String {
    match error {
        LaunchError::HookFailed {
            hook,
            exit_code,
            output,
        } => hook_failure(*hook, *exit_code, output),
        LaunchError::MainProcessEnded { sandbox, phase } => sandbox_ended(sandbox, *phase),
        other => chain(other),
    }
}

fn launch_failure(failure: &LaunchFailure) -> String {
    let mut text = format!(
        "launch {} failed at step {}: {}",
        failure.launch_id,
        failure.step,
        launch_error(&failure.error)
    );
    let stale = matches!(*failure.error, LaunchError::StaleSandbox { .. });
    if let Some(sandbox) = failure.sandbox.as_ref().filter(|_| !stale) {
        text.push_str(&format!(
            "\nsandbox {sandbox} remains; retry with: jackin load --launch-id {}",
            failure.launch_id
        ));
    }
    text
}

fn plan_error(error: &PlanError) -> String {
    match error {
        PlanError::EnvNeeded(questions) => {
            let names: Vec<&str> = questions
                .iter()
                .map(|question| question.name.as_str())
                .collect();
            format!(
                "the role needs values for {}; pass --env NAME=VALUE or run on a terminal",
                names.join(", ")
            )
        }
        PlanError::EnvUndeclared(names) => {
            let names: Vec<&str> = names.iter().map(jackin_core::EnvName::as_str).collect();
            format!("the role does not declare {}", names.join(", "))
        }
        other => chain(other),
    }
}

fn attach_error(error: &AttachError) -> String {
    match error {
        AttachError::NotFound(sandbox) => sandbox_missing(sandbox),
        AttachError::MainProcessEnded { sandbox, phase } => sandbox_ended(sandbox, *phase),
        AttachError::EnvFile { sandbox, problem } => env_problem(sandbox, problem),
        AttachError::Gateway(error) => chain(error),
    }
}

fn restart_error(error: &RestartError) -> String {
    match error {
        RestartError::MainProcessEnded { sandbox, phase } => sandbox_ended(sandbox, *phase),
        RestartError::AlreadyRunning { sandbox, attempt } => format!(
            "Codex in sandbox {sandbox} is already running as attempt {attempt}; attach without --restart"
        ),
        RestartError::EnvFile { sandbox, problem } => env_problem(sandbox, problem),
        RestartError::HookFailed {
            hook,
            exit_code,
            output,
        } => hook_failure(*hook, *exit_code, output),
        other => chain(other),
    }
}

fn lifecycle_error(error: &LifecycleError) -> String {
    match error {
        LifecycleError::NotFound(sandbox) => sandbox_missing(sandbox),
        LifecycleError::MainProcessEnded { sandbox, phase } => sandbox_ended(sandbox, *phase),
        other => chain(other),
    }
}

fn purge_error(error: &PurgeError) -> String {
    match error {
        PurgeError::WorkNotExported { sandbox, state } => format!(
            "the work in sandbox {sandbox} is not exported ({state}); run jackin export {sandbox} or pass --force"
        ),
        PurgeError::Gateway(error) => chain(error),
    }
}

fn export_error(error: &ExportError) -> String {
    match error {
        ExportError::NotFound(sandbox) => sandbox_missing(sandbox),
        ExportError::NotRunning { sandbox, phase } => {
            format!("sandbox {sandbox} is not running ({phase}); start it before exporting")
        }
        ExportError::WorkspaceUnknown(workspace) => workspace_unknown(workspace),
        other => chain(other),
    }
}

pub(crate) fn report(error: &CommandError) -> String {
    let body = match error {
        CommandError::Cancelled(text) => return text.clone(),
        CommandError::RoleFindings(findings) => {
            return findings
                .iter()
                .map(|finding| format!("error: {}", chain(finding)))
                .collect::<Vec<_>>()
                .join("\n");
        }
        CommandError::Plan(error) => plan_error(error),
        CommandError::Launch(failure) => launch_failure(failure),
        CommandError::Attach(error) => attach_error(error),
        CommandError::Restart(error) => restart_error(error),
        CommandError::Lifecycle(error) => lifecycle_error(error),
        CommandError::Purge(error) => purge_error(error),
        CommandError::Export(error) => export_error(error),
        other => chain(other),
    };
    format!("error: {body}")
}
