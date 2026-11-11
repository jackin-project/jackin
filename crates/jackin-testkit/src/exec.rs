// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io;
use std::os::unix::process::ExitStatusExt;
use std::process::Stdio;
use std::time::Duration;

use jackin_core::ProcessStatus;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

pub(crate) const SANDBOX_PATH: &str = "/usr/bin:/bin:/usr/local/bin:/opt/homebrew/bin";

#[derive(Debug)]
pub(crate) struct Finished {
    pub(crate) status: ProcessStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

#[derive(Debug)]
pub(crate) enum Failure {
    Spawn(io::Error),
    Timeout,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostProgram {
    Shell,
    Git,
    Printenv,
    Find,
    Mkdir,
    Rm,
}

impl HostProgram {
    pub(crate) fn command(self) -> Command {
        let mut command = match self {
            HostProgram::Shell => Command::new("/bin/sh"),
            HostProgram::Git => Command::new("/usr/bin/git"),
            HostProgram::Printenv => Command::new("/usr/bin/printenv"),
            HostProgram::Find => Command::new("/usr/bin/find"),
            HostProgram::Mkdir => Command::new("/bin/mkdir"),
            HostProgram::Rm => Command::new("/bin/rm"),
        };
        command.env_clear().env("PATH", SANDBOX_PATH);
        if self == HostProgram::Git {
            command.env("GIT_CONFIG_NOSYSTEM", "1");
        }
        command
    }
}

pub(crate) fn status_of(status: std::process::ExitStatus) -> ProcessStatus {
    match (status.code(), status.signal()) {
        (Some(code), _) => ProcessStatus::Exited(code),
        (None, Some(signal)) => ProcessStatus::Signaled(signal),
        (None, None) => ProcessStatus::Exited(-1),
    }
}

pub(crate) async fn run_command(
    mut command: Command,
    stdin: Option<Vec<u8>>,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<Finished, Failure> {
    command
        .kill_on_drop(true)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(Failure::Spawn)?;
    let feed = child.stdin.take();
    let work = async {
        let feeding = async {
            if let (Some(mut pipe), Some(bytes)) = (feed, stdin) {
                drop(pipe.write_all(&bytes).await);
                drop(pipe.shutdown().await);
            }
        };
        let (output, ()) = tokio::join!(child.wait_with_output(), feeding);
        output
    };
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(Failure::Cancelled),
        finished = tokio::time::timeout(timeout, work) => match finished {
            Err(_) => Err(Failure::Timeout),
            Ok(Err(error)) => Err(Failure::Spawn(error)),
            Ok(Ok(output)) => Ok(Finished {
                status: status_of(output.status),
                stdout: output.stdout,
                stderr: output.stderr,
            }),
        },
    }
}
