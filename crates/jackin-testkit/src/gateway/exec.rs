// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::sync::Arc;

use jackin_core::{ProcessStatus, Sha256};
use jackin_openshell::{GatewayError, GatewayOp, ProgramOutcome, ProgramRun, ProgramStdin};
use tokio_util::sync::CancellationToken;

use super::state::{Cell, SANDBOX_ROOT};
use super::{ProgramScript, RunRecord};
use crate::exec::{Failure, HostProgram, SANDBOX_PATH, run_command};

pub(super) fn host_path(root: &Path, sandbox_path: &str) -> Option<PathBuf> {
    if sandbox_path == SANDBOX_ROOT {
        return Some(root.to_path_buf());
    }
    sandbox_path
        .strip_prefix("/sandbox/")
        .map(|rest| root.join(rest))
}

fn map_argument(root: &Path, argument: &str) -> String {
    host_path(root, argument).map_or_else(
        || argument.to_owned(),
        |mapped| mapped.to_string_lossy().into_owned(),
    )
}

fn outcome(code: i32, stdout: Vec<u8>, stderr: Vec<u8>) -> ProgramOutcome {
    ProgramOutcome {
        exit_code: code,
        stdout,
        stderr,
    }
}

fn exit_code(status: ProcessStatus) -> i32 {
    match status {
        ProcessStatus::Exited(code) => code,
        ProcessStatus::Signaled(signal) => 128 + signal,
    }
}

async fn run_host(
    program: HostProgram,
    root: &Path,
    run: &ProgramRun,
    cancel: &CancellationToken,
) -> Result<ProgramOutcome, GatewayError> {
    let workdir = match &run.workdir {
        None => root.to_path_buf(),
        Some(workdir) => {
            host_path(root, workdir.as_str()).ok_or_else(|| GatewayError::InvalidArgument {
                field: "workdir".to_owned(),
                message: "the fake maps only /sandbox".to_owned(),
            })?
        }
    };
    let mut command = program.command();
    command
        .args(run.args.iter().map(|argument| map_argument(root, argument)))
        .current_dir(workdir)
        .env_clear()
        .env("PATH", SANDBOX_PATH)
        .envs(run.env.iter().map(|(name, value)| (name.as_str(), value)));
    let stdin = match &run.stdin {
        ProgramStdin::Closed => None,
        ProgramStdin::Data(bytes) => Some(bytes.expose().to_vec()),
    };
    match run_command(command, stdin, run.timeout, cancel).await {
        Ok(finished) => Ok(outcome(
            exit_code(finished.status),
            finished.stdout,
            finished.stderr,
        )),
        Err(Failure::Spawn(error)) => Err(GatewayError::Protocol {
            message: format!("the fake cannot start {}: {error}", run.program.as_str()),
        }),
        Err(Failure::Timeout) => Err(GatewayError::Timeout {
            op: GatewayOp::Program,
            after: run.timeout,
        }),
        Err(Failure::Cancelled) => Err(GatewayError::Cancelled {
            op: GatewayOp::Program,
        }),
    }
}

async fn sha256sum(root: &Path, run: &ProgramRun) -> ProgramOutcome {
    let Some(argument) = run.args.first() else {
        return outcome(1, Vec::new(), b"sha256sum: missing file operand\n".to_vec());
    };
    match tokio::fs::read(map_argument(root, argument)).await {
        Ok(bytes) => outcome(
            0,
            format!("{}  {argument}\n", Sha256::of(&bytes).as_str()).into_bytes(),
            Vec::new(),
        ),
        Err(error) => outcome(
            1,
            Vec::new(),
            format!("sha256sum: {argument}: {error}\n").into_bytes(),
        ),
    }
}

async fn stall(run: &ProgramRun, cancel: &CancellationToken) -> GatewayError {
    tokio::select! {
        () = cancel.cancelled() => GatewayError::Cancelled { op: GatewayOp::Program },
        () = tokio::time::sleep(run.timeout) => GatewayError::Timeout {
            op: GatewayOp::Program,
            after: run.timeout,
        },
    }
}

async fn touch(root: &Path, run: &ProgramRun, finished: ProgramOutcome) -> ProgramOutcome {
    if finished.exit_code != 0 {
        return finished;
    }
    let Some(marker) = &run.touch_on_success else {
        return finished;
    };
    let created = match host_path(root, marker.as_str()) {
        Some(path) => tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
            .is_ok(),
        None => false,
    };
    if created {
        finished
    } else {
        outcome(1, finished.stdout, finished.stderr)
    }
}

pub(super) async fn run_program(
    cell: &Arc<Cell>,
    script: Option<ProgramScript>,
    stalled: bool,
    run: &ProgramRun,
    cancel: &CancellationToken,
) -> Result<ProgramOutcome, GatewayError> {
    if cancel.is_cancelled() {
        return Err(GatewayError::Cancelled {
            op: GatewayOp::Program,
        });
    }
    if stalled {
        return Err(stall(run, cancel).await);
    }
    let root = cell.root();
    let finished = if let Some(script) = script {
        outcome(script.exit_code, script.stdout, script.stderr)
    } else {
        match run.program.as_str() {
            "/bin/sh" => run_host(HostProgram::Shell, &root, run, cancel).await?,
            "/usr/bin/git" => run_host(HostProgram::Git, &root, run, cancel).await?,
            "/usr/bin/printenv" => run_host(HostProgram::Printenv, &root, run, cancel).await?,
            "/usr/bin/sha256sum" => sha256sum(&root, run).await,
            other => outcome(
                127,
                Vec::new(),
                format!("{other}: not found\n").into_bytes(),
            ),
        }
    };
    let finished = touch(&root, run, finished).await;
    cell.state().runs.push((
        run.program.clone(),
        RunRecord {
            args: run.args.clone(),
            workdir: run.workdir.clone(),
            env: run.env.clone(),
            exit_code: finished.exit_code,
        },
    ));
    Ok(finished)
}
