// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;
use std::io::{self, SeekFrom};
use std::os::unix::process::ExitStatusExt;
use std::pin::pin;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{
    Clock, EnvValue, FileSystem, GroupFault, HostPath, LineSink, OutputLine, OutputStream,
    ProcessError, ProcessOutput, ProcessRunner, ProcessSpec, ProcessStatus, ProcessStdin, Program,
    SecretBytes, SecretOutput, Timestamp,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::process_group::ProcessGroup;
use crate::service::HostPorts;

const TAIL_LIMIT: usize = 65536;
const DRAIN_GRACE: Duration = Duration::from_secs(2);
const READ_CHUNK: usize = 8192;
const FEED_CHUNK: usize = 64 * 1024;
const LINE_LIMIT: usize = 65536;
const WHOLE_LIMIT: usize = 16 * 1024 * 1024;
const LINE_CUT_MARK: &str = " [truncated]";

#[derive(Clone, Copy, Debug)]
pub struct SystemClock;

#[async_trait]
impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        Timestamp(i64::try_from(seconds).unwrap_or(i64::MAX))
    }

    fn utc_offset_secs(&self) -> i32 {
        chrono::Local::now().offset().local_minus_utc()
    }

    async fn sleep(&self, duration: Duration) {
        sleep(duration).await;
    }
}

impl HostPorts {
    pub fn system(fs: Arc<dyn FileSystem>, home: HostPath) -> HostPorts {
        HostPorts {
            clock: Arc::new(SystemClock),
            fs,
            runner: Arc::new(TokioProcessRunner),
            home,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TokioProcessRunner;

enum Buffer {
    Whole(Vec<u8>),
    Tail(VecDeque<u8>),
    Secret(Zeroizing<Vec<u8>>),
}

enum StreamFault {
    Read(io::Error),
    TooLarge,
}

enum DriveFault {
    Wait(io::Error),
    Stream(OutputStream, StreamFault),
}

impl Buffer {
    fn push(&mut self, chunk: &[u8]) -> Result<(), StreamFault> {
        match self {
            Buffer::Whole(bytes) => {
                if bytes.len() + chunk.len() > WHOLE_LIMIT {
                    return Err(StreamFault::TooLarge);
                }
                bytes.extend_from_slice(chunk);
            }
            Buffer::Tail(bytes) => {
                bytes.extend(chunk);
                let excess = bytes.len().saturating_sub(TAIL_LIMIT);
                if excess > 0 {
                    bytes.drain(..excess);
                }
            }
            Buffer::Secret(bytes) => {
                if bytes.len() + chunk.len() > WHOLE_LIMIT {
                    return Err(StreamFault::TooLarge);
                }
                if bytes.capacity() - bytes.len() < chunk.len() {
                    let wanted = (bytes.len() + chunk.len())
                        .max(bytes.capacity() * 2)
                        .max(READ_CHUNK);
                    let mut grown = Zeroizing::new(Vec::with_capacity(wanted));
                    grown.extend_from_slice(bytes);
                    *bytes = grown;
                }
                bytes.extend_from_slice(chunk);
            }
        }
        Ok(())
    }

    fn into_bytes(self) -> Vec<u8> {
        match self {
            Buffer::Whole(bytes) => bytes,
            Buffer::Secret(mut bytes) => std::mem::take(&mut *bytes),
            Buffer::Tail(bytes) => Vec::from(bytes),
        }
    }
}

#[derive(Default)]
struct Line {
    bytes: Zeroizing<Vec<u8>>,
    truncated: bool,
}

impl Line {
    fn extend(&mut self, piece: &[u8]) {
        let room = LINE_LIMIT - self.bytes.len();
        let kept = piece.len().min(room);
        self.bytes.extend_from_slice(&piece[..kept]);
        if kept < piece.len() {
            self.truncated = true;
        }
    }

    fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

struct GroupGuard {
    group: Option<ProcessGroup>,
}

impl GroupGuard {
    fn fire(&mut self) -> Option<ProcessGroup> {
        let group = self.group.take();
        if let Some(group) = group {
            group.strike();
        }
        group
    }
}

impl Drop for GroupGuard {
    fn drop(&mut self) {
        if let Some(group) = self.group.take() {
            drop(group.sweep());
        }
    }
}

async fn gone(group: Option<ProcessGroup>) -> Result<(), GroupFault> {
    match group {
        Some(group) => group.stop().await,
        None => Ok(()),
    }
}

fn left_running(program: Program) -> impl FnOnce(GroupFault) -> ProcessError {
    move |fault| ProcessError::Group { program, fault }
}

fn with_group(error: ProcessError, stopped: Result<(), GroupFault>) -> ProcessError {
    let Err(fault) = stopped else {
        return error;
    };
    match error {
        ProcessError::Timeout { program, after, .. } => ProcessError::Timeout {
            program,
            after,
            group: Some(fault),
        },
        ProcessError::Cancelled { program, .. } => ProcessError::Cancelled {
            program,
            group: Some(fault),
        },
        ProcessError::Spawn { program, .. }
        | ProcessError::OutputTooLarge { program, .. }
        | ProcessError::Read { program, .. }
        | ProcessError::Group { program, .. } => ProcessError::Group { program, fault },
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
            ProcessStdin::Bytes(_) | ProcessStdin::File { .. } => Stdio::piped(),
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .process_group(0);
    command
}

async fn deliver(sink: &LineSink, stream: OutputStream, line: &mut Line) -> bool {
    let mut text = String::from_utf8_lossy(&line.bytes).into_owned();
    if line.truncated {
        text.push_str(LINE_CUT_MARK);
    }
    *line = Line::default();
    sink.send(OutputLine { stream, text }).await.is_ok()
}

async fn idle(exited: &CancellationToken) {
    exited.cancelled().await;
    sleep(DRAIN_GRACE).await;
}

async fn pump<R>(
    mut pipe: R,
    buffer: &mut Buffer,
    lines: Option<(&LineSink, OutputStream)>,
    exited: &CancellationToken,
) -> Result<(), StreamFault>
where
    R: AsyncRead + Unpin,
{
    let mut chunk = Zeroizing::new([0_u8; READ_CHUNK]);
    let mut line = Line::default();
    let mut open = lines.is_some();
    loop {
        let count = tokio::select! {
            result = pipe.read(&mut *chunk) => result.map_err(StreamFault::Read)?,
            () = idle(exited) => 0,
        };
        if count == 0 {
            break;
        }
        let data = &chunk[..count];
        buffer.push(data)?;
        if let (true, Some((sink, stream))) = (open, lines) {
            let mut rest = data;
            while let Some(end) = rest.iter().position(|byte| *byte == b'\n') {
                line.extend(&rest[..end]);
                open = deliver(sink, stream, &mut line).await;
                rest = &rest[end + 1..];
            }
            line.extend(rest);
        }
    }
    if let (true, false, Some((sink, stream))) = (open, line.is_empty(), lines) {
        deliver(sink, stream, &mut line).await;
    }
    Ok(())
}

enum Input<'a> {
    None,
    Bytes(&'a [u8]),
    File(tokio::fs::File),
}

async fn open_input(spec: &ProcessSpec) -> Result<Input<'_>, ProcessError> {
    let refused = |message: String| ProcessError::Spawn {
        program: spec.program,
        message,
    };
    match &spec.stdin {
        ProcessStdin::Null => Ok(Input::None),
        ProcessStdin::Bytes(bytes) => Ok(Input::Bytes(bytes.expose())),
        ProcessStdin::File { path, offset } => {
            let mut file = tokio::fs::File::open(path.as_path())
                .await
                .map_err(|error| refused(error.to_string()))?;
            let meta = file
                .metadata()
                .await
                .map_err(|error| refused(error.to_string()))?;
            if !meta.is_file() {
                return Err(refused(format!(
                    "{} is not a file",
                    path.as_path().display()
                )));
            }
            file.seek(SeekFrom::Start(*offset))
                .await
                .map_err(|error| refused(error.to_string()))?;
            Ok(Input::File(file))
        }
    }
}

async fn feed(stdin: Option<ChildStdin>, input: Input<'_>) {
    let Some(mut pipe) = stdin else {
        return;
    };
    match input {
        Input::None => {}
        Input::Bytes(bytes) => drop(pipe.write_all(bytes).await),
        Input::File(mut file) => {
            let mut chunk = vec![0_u8; FEED_CHUNK];
            loop {
                let count = match file.read(&mut chunk).await {
                    Ok(0) | Err(_) => break,
                    Ok(count) => count,
                };
                if pipe.write_all(&chunk[..count]).await.is_err() {
                    break;
                }
            }
        }
    }
    drop(pipe.shutdown().await);
}

struct Pipes<'a> {
    stdin: Option<ChildStdin>,
    stdout: ChildStdout,
    stderr: ChildStderr,
    input: Input<'a>,
    out: &'a mut Buffer,
    err: &'a mut Buffer,
    lines: Option<&'a LineSink>,
}

async fn drive(child: &mut Child, pipes: Pipes<'_>) -> Result<ExitStatus, DriveFault> {
    let Pipes {
        stdin,
        stdout,
        stderr,
        input,
        out,
        err,
        lines,
    } = pipes;
    let exited = CancellationToken::new();
    let mut feeding = pin!(feed(stdin, input));
    let mut reading_out = pin!(pump(
        stdout,
        out,
        lines.map(|sink| (sink, OutputStream::Stdout)),
        &exited
    ));
    let mut reading_err = pin!(pump(
        stderr,
        err,
        lines.map(|sink| (sink, OutputStream::Stderr)),
        &exited
    ));
    let mut waiting = pin!(child.wait());
    let mut status = None;
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
                status = Some(result.map_err(DriveFault::Wait)?);
                exited.cancel();
            }
            result = &mut reading_out, if !out_done => {
                result.map_err(|fault| DriveFault::Stream(OutputStream::Stdout, fault))?;
                out_done = true;
            }
            result = &mut reading_err, if !err_done => {
                result.map_err(|fault| DriveFault::Stream(OutputStream::Stderr, fault))?;
                err_done = true;
            }
            () = &mut feeding, if !fed => fed = true,
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
        return Err(ProcessError::Cancelled {
            program,
            group: None,
        });
    }
    let input = open_input(spec).await?;
    let mut child = command_for(spec)
        .spawn()
        .map_err(|error| spawn_error(error.to_string()))?;
    let mut guard = GroupGuard {
        group: child.id().and_then(ProcessGroup::of_leader),
    };
    let timer = sleep(spec.timeout);
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        let stopped = gone(guard.fire()).await;
        return Err(with_group(
            spawn_error("the output pipes are missing".to_owned()),
            stopped,
        ));
    };
    let pipes = Pipes {
        stdin: child.stdin.take(),
        stdout,
        stderr,
        input,
        out: &mut out,
        err: &mut err,
        lines,
    };
    let outcome = tokio::select! {
        biased;
        () = cancel.cancelled() => Err(ProcessError::Cancelled { program, group: None }),
        () = timer => Err(ProcessError::Timeout { program, after: spec.timeout, group: None }),
        result = drive(&mut child, pipes) => result.map_err(|fault| match fault {
            DriveFault::Wait(error) => spawn_error(error.to_string()),
            DriveFault::Stream(stream, StreamFault::Read(error)) => ProcessError::Read {
                program,
                stream,
                message: error.to_string(),
            },
            DriveFault::Stream(stream, StreamFault::TooLarge) => ProcessError::OutputTooLarge {
                program,
                stream,
                limit: WHOLE_LIMIT,
            },
        }),
    };
    match outcome {
        Ok(status) => {
            gone(guard.fire()).await.map_err(left_running(program))?;
            Ok((status_of(status), out, err))
        }
        Err(error) => {
            let group = guard.fire();
            drop(child.kill().await);
            Err(with_group(error, gone(group).await))
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
            Buffer::Secret(Zeroizing::default()),
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
