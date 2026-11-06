// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::WorkspaceName;
use jackin_session::{
    AttachError, ExportError, LaunchFailure, LaunchNextStep, LifecycleError, PlanError, PurgeError,
    RestartError, env_file_problem, error_chain, hook_failure, main_process_ended, sandbox_missing,
};

use crate::error::CommandError;

fn workspace_unknown(workspace: &WorkspaceName) -> String {
    format!("workspace {workspace} is not in the config")
}

fn launch_failure(failure: &LaunchFailure) -> String {
    let mut text = failure.summary();
    if let Some(LaunchNextStep::Retry { sandbox, launch }) = failure.next_step() {
        text.push_str(&format!(
            "\nsandbox {sandbox} remains; retry with: jackin load --launch-id {launch}"
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
        other => error_chain(other),
    }
}

fn attach_error(error: &AttachError) -> String {
    match error {
        AttachError::NotFound(sandbox) => sandbox_missing(sandbox),
        AttachError::MainProcessEnded { sandbox, phase } => main_process_ended(sandbox, *phase),
        AttachError::EnvFile { sandbox, problem } => env_file_problem(sandbox, problem),
        AttachError::Gateway(error) => error_chain(error),
    }
}

fn restart_error(error: &RestartError) -> String {
    match error {
        RestartError::MainProcessEnded { sandbox, phase } => main_process_ended(sandbox, *phase),
        RestartError::AlreadyRunning { sandbox, attempt } => format!(
            "Codex in sandbox {sandbox} is already running as attempt {attempt}; attach without --restart"
        ),
        RestartError::EnvFile { sandbox, problem } => env_file_problem(sandbox, problem),
        RestartError::HookFailed {
            hook,
            exit_code,
            output,
        } => hook_failure(*hook, *exit_code, output),
        other => error_chain(other),
    }
}

fn lifecycle_error(error: &LifecycleError) -> String {
    match error {
        LifecycleError::NotFound(sandbox) => sandbox_missing(sandbox),
        LifecycleError::MainProcessEnded { sandbox, phase } => main_process_ended(sandbox, *phase),
        other => error_chain(other),
    }
}

fn purge_error(error: &PurgeError) -> String {
    match error {
        PurgeError::WorkNotExported { sandbox, state } => format!(
            "the work in sandbox {sandbox} is not exported ({state}); run jackin export {sandbox} or pass --force"
        ),
        PurgeError::Replaced { sandbox } => format!(
            "sandbox {sandbox} was replaced, or cannot be told apart from a replacement, while its work was inspected; nothing was deleted; run jackin purge {sandbox} again"
        ),
        PurgeError::Cancelled => "cancelled".to_owned(),
        PurgeError::Gateway(error) => error_chain(error),
    }
}

fn export_error(error: &ExportError) -> String {
    match error {
        ExportError::NotFound(sandbox) => sandbox_missing(sandbox),
        ExportError::NotRunning { sandbox, phase } => {
            format!("sandbox {sandbox} is not running ({phase}); start it before exporting")
        }
        ExportError::WorkspaceUnknown(workspace) => workspace_unknown(workspace),
        other => error_chain(other),
    }
}

pub(crate) fn report(error: &CommandError) -> String {
    let body = match error {
        CommandError::Cancelled(text) => return text.clone(),
        CommandError::RoleFindings(findings) => {
            return findings
                .iter()
                .map(|finding| format!("error: {}", error_chain(finding)))
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
        other => error_chain(other),
    };
    format!("error: {body}")
}
