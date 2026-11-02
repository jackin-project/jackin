// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;
use std::future::pending;
use std::io;
use std::os::unix::process::ExitStatusExt;
use std::pin::{Pin, pin};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{
    EnvValue, LineSink, OutputLine, OutputStream, ProcessError, ProcessOutput, ProcessRunner,
    ProcessSpec, ProcessStatus, ProcessStdin, SecretBytes, SecretOutput,
};
use rustix::process::{Pid, Signal, kill_process_group};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::time::{Sleep, sleep};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroize;

const TAIL_LIMIT: usize = 65536;
const DRAIN_GRACE: Duration = Duration::from_secs(2);
const READ_CHUNK: usize = 8192;

#[derive(Clone, Copy, Debug)]
pub struct SystemClock;

#[derive(Clone, Copy, Debug, Default)]
pub struct TokioProcessRunner;

enum Buffer {
    Whole(Vec<u8>),
    Tail(VecDeque<u8>),
    Secret(Vec<u8>),
}

impl Buffer {
    fn push(&mut self, chunk: &[u8]) {
        match self {
            Buffer::Whole(bytes) => bytes.extend_from_slice(chunk),
            Buffer::Tail(bytes) => {
                bytes.extend(chunk);
                let excess = bytes.len().saturating_sub(TAIL_LIMIT);
                if excess > 0 {
                    bytes.drain(..excess);
                }
            }
            Buffer::Secret(bytes) => {
                if bytes.capacity() - bytes.len() < chunk.len() {
                    let wanted = (bytes.len() + chunk.len())
                        .max(bytes.capacity() * 2)
                        .max(READ_CHUNK);
                    let mut grown = Vec::with_capacity(wanted);
                    grown.extend_from_slice(bytes);
                    bytes.zeroize();
                    *bytes = grown;
                }
                bytes.extend_from_slice(chunk);
            }
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        match self {
            Buffer::Whole(bytes) | Buffer::Secret(bytes) => bytes,
            Buffer::Tail(bytes) => Vec::from(bytes),
        }
    }
}

struct GroupGuard {
    group: Option<Pid>,
}

impl GroupGuard {
    fn fire(&mut self) {
        kill_group(self.group.take());
    }

    fn disarm(&mut self) {
        self.group = None;
    }
}

impl Drop for GroupGuard {
    fn drop(&mut self) {
        self.fire();
    }
}

fn kill_group(group: Option<Pid>) {
    if let Some(group) = group {
        match kill_process_group(group, Signal::KILL) {
            Ok(()) | Err(_) => {}
        }
    }
}

fn status_of(status: ExitStatus) -> ProcessStatus {
    match (status.code(), status.signal()) {
        (Some(code), _) => ProcessStatus::Exited(code),
        (None, Some(signal)) => ProcessStatus::Signaled(signal),
        (None, None) => ProcessStatus::Exited(-1),
    }
}

fn command_for(spec: &ProcessSpec) -> Command {
    let mut command = Command::new(spec.program.binary());
    command.args(spec.program.leading_args()).args(&spec.args);
    if let Some(cwd) = &spec.cwd {
        command.current_dir(cwd.as_path());
    }
    for (name, _) in std::env::vars_os() {
        let removed = spec.env_remove_prefixes.iter().any(|prefix| {
            !prefix.is_empty() && name.as_encoded_bytes().starts_with(prefix.as_bytes())
        });
        if removed {
            command.env_remove(&name);
        }
    }
    for (name, value) in &spec.env {
        match value {
            EnvValue::Plain(text) => command.env(name, text),
            EnvValue::Secret(secret) => command.env(name, secret.expose()),
        };
    }
    command
        .stdin(match spec.stdin {
            ProcessStdin::Null => Stdio::null(),
            ProcessStdin::Bytes(_) => Stdio::piped(),
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .process_group(0);
    command
}

async fn deliver(sink: &LineSink, stream: OutputStream, pending: &mut Vec<u8>) -> bool {
    let text = String::from_utf8_lossy(pending).into_owned();
    pending.clear();
    sink.send(OutputLine { stream, text }).await.is_ok()
}

async fn pump<R>(mut pipe: R, buffer: &mut Buffer, lines: Option<(&LineSink, OutputStream)>)
where
    R: AsyncRead + Unpin,
{
    let mut chunk = [0_u8; READ_CHUNK];
    let mut line = Vec::new();
    let mut open = lines.is_some();
    while let Ok(count @ 1..) = pipe.read(&mut chunk).await {
        let data = &chunk[..count];
        buffer.push(data);
        if let (true, Some((sink, stream))) = (open, lines) {
            let mut rest = data;
            while let Some(end) = rest.iter().position(|byte| *byte == b'\n') {
                line.extend_from_slice(&rest[..end]);
                open = deliver(sink, stream, &mut line).await;
                rest = &rest[end + 1..];
            }
            line.extend_from_slice(rest);
        }
    }
    if let (true, false, Some((sink, stream))) = (open, line.is_empty(), lines) {
        deliver(sink, stream, &mut line).await;
    }
    chunk.zeroize();
    line.zeroize();
}

async fn feed(stdin: Option<ChildStdin>, bytes: &[u8]) {
    if let Some(mut pipe) = stdin {
        drop(pipe.write_all(bytes).await);
        drop(pipe.shutdown().await);
    }
}

async fn expire(grace: &mut Option<Pin<Box<Sleep>>>) {
    match grace {
        Some(timer) => timer.as_mut().await,
        None => pending().await,
    }
}

struct Pipes<'a> {
    stdin: Option<ChildStdin>,
    stdout: ChildStdout,
    stderr: ChildStderr,
    input: &'a [u8],
    out: &'a mut Buffer,
    err: &'a mut Buffer,
    lines: Option<&'a LineSink>,
    group: Option<Pid>,
}

async fn drive(child: &mut Child, pipes: Pipes<'_>) -> io::Result<ExitStatus> {
    let Pipes {
        stdin,
        stdout,
        stderr,
        input,
        out,
        err,
        lines,
        group,
    } = pipes;
    let mut feeding = pin!(feed(stdin, input));
    let mut reading_out = pin!(pump(
        stdout,
        out,
        lines.map(|sink| (sink, OutputStream::Stdout))
    ));
    let mut reading_err = pin!(pump(
        stderr,
        err,
        lines.map(|sink| (sink, OutputStream::Stderr))
    ));
    let mut waiting = pin!(child.wait());
    let mut status = None;
    let mut grace = None;
    let (mut fed, mut out_done, mut err_done) = (false, false, false);
    loop {
        if let Some(status) = status
            && out_done
            && err_done
        {
            return Ok(status);
        }
        tokio::select! {
            result = &mut waiting, if status.is_none() => {
                status = Some(result?);
                grace = Some(Box::pin(sleep(DRAIN_GRACE)));
            }
            () = &mut reading_out, if !out_done => out_done = true,
            () = &mut reading_err, if !err_done => err_done = true,
            () = &mut feeding, if !fed => fed = true,
            () = expire(&mut grace), if status.is_some() => {
                kill_group(group);
                if let Some(done) = status {
                    return Ok(done);
                }
            }
        }
    }
}

async fn execute(
    spec: &ProcessSpec,
    lines: Option<&LineSink>,
    mut out: Buffer,
    mut err: Buffer,
    cancel: &CancellationToken,
) -> Result<(ProcessStatus, Buffer, Buffer), ProcessError> {
    let program = spec.program;
    let spawn_error = |message: String| ProcessError::Spawn { program, message };
    if cancel.is_cancelled() {
        return Err(ProcessError::Cancelled { program });
    }
    let mut child = command_for(spec)
        .spawn()
        .map_err(|error| spawn_error(error.to_string()))?;
    let mut guard = GroupGuard {
        group: child
            .id()
            .and_then(|id| i32::try_from(id).ok())
            .and_then(Pid::from_raw),
    };
    let timer = sleep(spec.timeout);
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        guard.fire();
        return Err(spawn_error("the output pipes are missing".to_owned()));
    };
    let pipes = Pipes {
        stdin: child.stdin.take(),
        stdout,
        stderr,
        input: match &spec.stdin {
            ProcessStdin::Null => &[],
            ProcessStdin::Bytes(bytes) => bytes.expose(),
        },
        out: &mut out,
        err: &mut err,
        lines,
        group: guard.group,
    };
    let outcome = tokio::select! {
        biased;
        () = cancel.cancelled() => Err(ProcessError::Cancelled { program }),
        () = timer => Err(ProcessError::Timeout { program, after: spec.timeout }),
        result = drive(&mut child, pipes) => result.map_err(|error| spawn_error(error.to_string())),
    };
    match outcome {
        Ok(status) => {
            guard.disarm();
            Ok((status_of(status), out, err))
        }
        Err(error) => {
            guard.fire();
            drop(child.kill().await);
            Err(error)
        }
    }
}

#[async_trait]
impl ProcessRunner for TokioProcessRunner {
    async fn run(
        &self,
        spec: &ProcessSpec,
        lines: Option<&LineSink>,
        cancel: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        let (out, err) = match lines {
            Some(_) => (Buffer::Tail(VecDeque::new()), Buffer::Tail(VecDeque::new())),
            None => (Buffer::Whole(Vec::new()), Buffer::Whole(Vec::new())),
        };
        let (status, out, err) = execute(spec, lines, out, err, cancel).await?;
        Ok(ProcessOutput {
            status,
            stdout: out.into_bytes(),
            stderr: err.into_bytes(),
        })
    }

    async fn run_secret(
        &self,
        spec: &ProcessSpec,
        cancel: &CancellationToken,
    ) -> Result<SecretOutput, ProcessError> {
        let (status, out, err) = execute(
            spec,
            None,
            Buffer::Secret(Vec::new()),
            Buffer::Whole(Vec::new()),
            cancel,
        )
        .await?;
        Ok(SecretOutput {
            status,
            stdout: SecretBytes::new(out.into_bytes()),
            stderr: err.into_bytes(),
        })
    }
}
