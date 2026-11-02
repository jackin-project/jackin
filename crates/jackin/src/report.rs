// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use jackin_core::{HookKind, Phase, SandboxName, TerminalText, WorkspaceName};
use jackin_session::{
    AttachError, EnvFileProblem, LaunchError, LaunchFailure, LifecycleError, PlanError, PurgeError,
    RestartError, UnknownReason, WorkState,
};

use crate::error::CommandError;

pub(crate) fn hook_name(hook: HookKind) -> &'static str {
    match hook {
        HookKind::Setup => "setup",
        HookKind::Preflight => "preflight",
    }
}

pub(crate) fn phase_text(phase: Phase) -> String {
    match phase {
        Phase::Provisioning => "provisioning".to_owned(),
        Phase::Starting => "starting".to_owned(),
        Phase::Ready => "ready".to_owned(),
        Phase::Stopping => "stopping".to_owned(),
        Phase::Stopped => "stopped".to_owned(),
        Phase::Completed { exit_code } => format!("completed with exit code {exit_code}"),
        Phase::Error {
            exit_code: Some(code),
        } => format!("error with exit code {code}"),
        Phase::Error { exit_code: None } => "error without an exit code".to_owned(),
        Phase::Deleting => "deleting".to_owned(),
        Phase::Suspending => "suspending".to_owned(),
        Phase::Suspended => "suspended".to_owned(),
        Phase::Unknown => "unknown".to_owned(),
    }
}

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
        "the main process of sandbox {sandbox} has ended ({}); only purge --force can remove it",
        phase_text(phase)
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
        "the {} hook exited with code {exit_code}\n{}",
        hook_name(hook),
        output.as_str()
    )
}

fn workspace_unknown(workspace: &WorkspaceName) -> String {
    format!("workspace {workspace} is not in the config")
}

fn work_state(state: &WorkState) -> String {
    match state {
        WorkState::Exported => "exported".to_owned(),
        WorkState::Unexported(found) => {
            let mut parts = Vec::new();
            if found.uncommitted {
                parts.push("uncommitted changes".to_owned());
            }
            for branch in &found.branches {
                parts.push(format!("branch {branch} not exported"));
            }
            if let Some(commit) = &found.detached {
                parts.push(format!("detached HEAD {commit} not exported"));
            }
            for name in &found.worktrees {
                parts.push(format!("worktree {name} not exported"));
            }
            for name in &found.unusable {
                parts.push(format!("branch {} cannot be exported", name.as_str()));
            }
            parts.join(", ")
        }
        WorkState::Unknown(reason) => match reason {
            UnknownReason::NotRunning(phase) => {
                format!("cannot be inspected ({})", phase_text(*phase))
            }
            UnknownReason::WorkspaceUnknown(workspace) => {
                format!("cannot be inspected ({})", workspace_unknown(workspace))
            }
            UnknownReason::NoRepository => "cannot be inspected (no repository)".to_owned(),
            UnknownReason::InspectionFailed => "cannot be inspected".to_owned(),
        },
    }
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
    let step = format!("{:?}", failure.step).to_lowercase();
    let mut text = format!(
        "launch {} failed at step {step}: {}",
        failure.launch_id,
        launch_error(&failure.error)
    );
    if let Some(sandbox) = &failure.sandbox {
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
            "the work in sandbox {sandbox} is not exported ({}); run jackin export {sandbox} or pass --force",
            work_state(state)
        ),
        PurgeError::Gateway(error) => chain(error),
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
        other => chain(other),
    };
    format!("error: {body}")
}
