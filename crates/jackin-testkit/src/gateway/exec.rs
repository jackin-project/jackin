// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::sync::Arc;

use jackin_core::{ProcessStatus, Sha256};
use jackin_openshell::{GatewayError, GatewayOp, ProgramOutcome, ProgramRun, ProgramStdin};
use tokio_util::sync::CancellationToken;

use super::state::{Cell, SANDBOX_ROOT};
use super::{ProgramScript, RunRecord};
use crate::exec::{Failure, HostProgram, run_command};

pub(super) fn host_path(root: &Path, sandbox_path: &str) -> Option<PathBuf> {
    if sandbox_path == SANDBOX_ROOT {
        return Some(root.to_path_buf());
    }
    sandbox_path
        .strip_prefix("/sandbox/")
        .map(|rest| root.join(rest))
}

pub(super) fn workdir_path(cell: &Cell, workdir: &str) -> Result<PathBuf, std::io::Error> {
    let path = host_path(&cell.root(), workdir)
        .unwrap_or_else(|| cell.dir.path().join(workdir.trim_start_matches('/')));
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
}

fn map_argument(root: &Path, argument: &str) -> String {
    let bytes = argument.as_bytes();
    let target = root.to_string_lossy();
    let mut mapped = String::with_capacity(argument.len());
    let mut copied = 0;
    let mut at = 0;
    while let Some(found) = argument[at..].find(SANDBOX_ROOT) {
        let start = at + found;
        let end = start + SANDBOX_ROOT.len();
        let opens = start == 0 || !(is_name_byte(bytes[start - 1]) || bytes[start - 1] == b'/');
        let closes = end == bytes.len() || !is_name_byte(bytes[end]);
        if opens && closes {
            mapped.push_str(&argument[copied..start]);
            mapped.push_str(&target);
            copied = end;
        }
        at = end;
    }
    mapped.push_str(&argument[copied..]);
    mapped
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
    cell: &Cell,
    run: &ProgramRun,
    cancel: &CancellationToken,
) -> Result<ProgramOutcome, GatewayError> {
    let root_dir = cell.root();
    let root = root_dir.as_path();
    let workdir = match &run.workdir {
        None => root.to_path_buf(),
        Some(workdir) => {
            workdir_path(cell, workdir.as_str()).map_err(|error| GatewayError::Protocol {
                message: error.to_string(),
            })?
        }
    };
    let mut command = program.command();
    command
        .args(run.args.iter().map(|argument| map_argument(root, argument)))
        .current_dir(workdir)
        .envs(
            run.env
                .iter()
                .map(|(name, value)| (name.as_str(), map_argument(root, value))),
        );
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
            group: None,
        }),
        Err(Failure::Cancelled) => Err(GatewayError::Cancelled {
            op: GatewayOp::Program,
            group: None,
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
        () = cancel.cancelled() => GatewayError::Cancelled { op: GatewayOp::Program, group: None },
        () = tokio::time::sleep(run.timeout) => GatewayError::Timeout {
            op: GatewayOp::Program,
            after: run.timeout, group: None },
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
            group: None,
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
            "/bin/sh" => run_host(HostProgram::Shell, cell, run, cancel).await?,
            "/usr/bin/git" => run_host(HostProgram::Git, cell, run, cancel).await?,
            "/usr/bin/printenv" => run_host(HostProgram::Printenv, cell, run, cancel).await?,
            "/usr/bin/find" => run_host(HostProgram::Find, cell, run, cancel).await?,
            "/usr/bin/mkdir" => run_host(HostProgram::Mkdir, cell, run, cancel).await?,
            "/usr/bin/rm" => run_host(HostProgram::Rm, cell, run, cancel).await?,
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
