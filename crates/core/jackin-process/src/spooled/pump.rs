// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Caller-thread asynchronous pipe draining and spool-budget enforcement.

use std::fs::File;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::owned_group::OwnedGroup;
use super::{
    SpoolCleanupObservation, SpoolExecutionObservation, SpoolFailure, SpoolFailureKind,
    SpoolPipeObservation,
};

const POLL_INTERVAL: Duration = Duration::from_millis(10);
const BUFFER_SIZE: usize = 16 * 1024;
pub(super) const WRITE_INTERRUPTION_BUDGET: usize = 8;

pub(super) struct PumpReport {
    pub(super) execution: SpoolExecutionObservation,
    pub(super) pipes: SpoolPipeObservation,
    pub(super) cleanup: SpoolCleanupObservation,
    pub(super) target_status: Option<std::process::ExitStatus>,
    pub(super) stdout_accepted: usize,
    pub(super) stderr_accepted: usize,
    pub(super) failures: Vec<SpoolFailure>,
}

pub(super) struct PumpInputs<Stdout, Stderr> {
    pub(super) stdout: Option<Stdout>,
    pub(super) stderr: Option<Stderr>,
    pub(super) failures: Vec<SpoolFailure>,
}

pub(super) struct PumpSinks<Stdout: Write, Stderr: Write> {
    pub(super) stdout: StreamSink<Stdout>,
    pub(super) stderr: StreamSink<Stderr>,
}

pub(super) struct PumpControls<'a> {
    pub(super) request_deadline: Option<Instant>,
    pub(super) cancellation: &'a AtomicBool,
}

pub(super) async fn supervise(
    mut group: OwnedGroup,
    stdout_file: &mut File,
    stderr_file: &mut File,
    stdout_limit: usize,
    stderr_limit: usize,
    request_deadline: Option<Instant>,
    cancellation: &AtomicBool,
) -> PumpReport {
    let (stdout_child, stderr_child) = group.take_output_pipes();
    let mut failures = Vec::new();
    let stdout_pipe = register_pipe(
        stdout_child,
        "stdout",
        &mut failures,
        tokio::process::ChildStdout::from_std,
    );
    let stderr_pipe = register_pipe(
        stderr_child,
        "stderr",
        &mut failures,
        tokio::process::ChildStderr::from_std,
    );
    supervise_streams(
        group,
        PumpInputs {
            stdout: stdout_pipe,
            stderr: stderr_pipe,
            failures,
        },
        PumpSinks {
            stdout: StreamSink::new(stdout_file, stdout_limit),
            stderr: StreamSink::new(stderr_file, stderr_limit),
        },
        PumpControls {
            request_deadline,
            cancellation,
        },
    )
    .await
}

pub(super) async fn supervise_streams<Stdout, Stderr, StdoutWriter, StderrWriter>(
    group: OwnedGroup,
    inputs: PumpInputs<Stdout, Stderr>,
    sinks: PumpSinks<StdoutWriter, StderrWriter>,
    controls: PumpControls<'_>,
) -> PumpReport
where
    Stdout: tokio::io::AsyncRead + Unpin,
    Stderr: tokio::io::AsyncRead + Unpin,
    StdoutWriter: Write,
    StderrWriter: Write,
{
    PumpContext::new(group, inputs, sinks, controls).run().await
}

struct PumpContext<'a, Stdout, Stderr, StdoutWriter: Write, StderrWriter: Write> {
    group: OwnedGroup,
    stdout: PumpStream<Stdout, StdoutWriter>,
    stderr: PumpStream<Stderr, StderrWriter>,
    controls: PumpControls<'a>,
    execution: SpoolExecutionObservation,
    finalizing: bool,
    failures: Vec<SpoolFailure>,
}

struct PumpStream<Reader, Writer: Write> {
    pipe: Option<Reader>,
    sink: StreamSink<Writer>,
    buffer: [u8; BUFFER_SIZE],
    active: bool,
    eof: bool,
}

impl<Reader, Writer: Write> PumpStream<Reader, Writer> {
    fn new(pipe: Option<Reader>, sink: StreamSink<Writer>) -> Self {
        let active = pipe.is_some();
        Self {
            pipe,
            sink,
            buffer: [0; BUFFER_SIZE],
            active,
            eof: false,
        }
    }
}

enum PumpEvent {
    Control,
    Stdout(io::Result<usize>),
    Stderr(io::Result<usize>),
}

impl<'a, Stdout, Stderr, StdoutWriter, StderrWriter>
    PumpContext<'a, Stdout, Stderr, StdoutWriter, StderrWriter>
where
    Stdout: tokio::io::AsyncRead + Unpin,
    Stderr: tokio::io::AsyncRead + Unpin,
    StdoutWriter: Write,
    StderrWriter: Write,
{
    fn new(
        mut group: OwnedGroup,
        inputs: PumpInputs<Stdout, Stderr>,
        sinks: PumpSinks<StdoutWriter, StderrWriter>,
        controls: PumpControls<'a>,
    ) -> Self {
        let stdout = PumpStream::new(inputs.stdout, sinks.stdout);
        let stderr = PumpStream::new(inputs.stderr, sinks.stderr);
        let finalizing = !stdout.active || !stderr.active;
        if finalizing {
            group.begin_finalization();
        }
        Self {
            group,
            stdout,
            stderr,
            controls,
            execution: SpoolExecutionObservation::default(),
            finalizing,
            failures: inputs.failures,
        }
    }

    async fn next_event(&mut self, interval: &mut tokio::time::Interval) -> PumpEvent {
        tokio::select! {
            _ = interval.tick() => PumpEvent::Control,
            result = read_pipe(&mut self.stdout.pipe, &mut self.stdout.buffer), if self.stdout.active => {
                PumpEvent::Stdout(result)
            }
            result = read_pipe(&mut self.stderr.pipe, &mut self.stderr.buffer), if self.stderr.active => {
                PumpEvent::Stderr(result)
            }
        }
    }

    async fn run(mut self) -> PumpReport {
        use tokio::time::MissedTickBehavior;

        let mut interval = tokio::time::interval(POLL_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            if self.should_stop() {
                break;
            }
            match self.next_event(&mut interval).await {
                PumpEvent::Control => self.control_tick(),
                PumpEvent::Stdout(result) => self.stdout_read(result),
                PumpEvent::Stderr(result) => self.stderr_read(result),
            }
        }

        drop(self.stdout.pipe.take());
        drop(self.stderr.pipe.take());
        while !self.group.cleanup_complete() {
            self.group.cleanup_step(&mut self.failures);
            if !self.group.cleanup_complete() {
                let remaining = self
                    .group
                    .cleanup_deadline()
                    .map_or(Duration::ZERO, |deadline| {
                        deadline.saturating_duration_since(Instant::now())
                    });
                tokio::time::sleep(POLL_INTERVAL.min(remaining)).await;
            }
        }

        record_missing_eof(&mut self.failures, "stdout", self.stdout.eof);
        record_missing_eof(&mut self.failures, "stderr", self.stderr.eof);
        self.execution.cancelled |= self.controls.cancellation.load(Ordering::Acquire);
        let cleanup = self.group.into_report(self.failures);
        PumpReport {
            execution: self.execution,
            pipes: SpoolPipeObservation {
                stdout_eof: self.stdout.eof,
                stderr_eof: self.stderr.eof,
            },
            cleanup: SpoolCleanupObservation {
                group_drained: cleanup.group_drained,
                recovery: cleanup.recovery,
            },
            target_status: cleanup.target_status,
            stdout_accepted: self.stdout.sink.accepted,
            stderr_accepted: self.stderr.sink.accepted,
            failures: cleanup.failures,
        }
    }

    fn should_stop(&self) -> bool {
        if !self.finalizing || !self.group.cleanup_complete() {
            return false;
        }
        let deadline = self.group.cleanup_deadline().unwrap_or_else(Instant::now);
        (!self.stdout.active && !self.stderr.active) || Instant::now() >= deadline
    }

    fn control_tick(&mut self) {
        if !self.finalizing {
            if self.controls.cancellation.load(Ordering::Acquire) {
                self.execution.cancelled = true;
                self.begin_finalization();
            } else if self
                .controls
                .request_deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
            {
                self.execution.timed_out = true;
                self.begin_finalization();
            } else {
                match self.group.observe_target_exit() {
                    Ok(true) => self.begin_finalization(),
                    Ok(false) => {}
                    Err(error) => {
                        self.failures.push(SpoolFailure {
                            kind: SpoolFailureKind::Execution,
                            message: format!("observing requested child exit: {error}"),
                        });
                        self.begin_finalization();
                    }
                }
            }
        }
        if self.finalizing && !self.group.cleanup_complete() {
            self.group.cleanup_step(&mut self.failures);
        }
    }

    fn begin_finalization(&mut self) {
        self.group.begin_finalization();
        self.finalizing = true;
    }

    fn stdout_read(&mut self, result: io::Result<usize>) {
        handle_stream_read(
            &mut self.group,
            &mut self.stdout,
            "stdout",
            result,
            &mut self.failures,
            &mut self.finalizing,
        );
    }

    fn stderr_read(&mut self, result: io::Result<usize>) {
        handle_stream_read(
            &mut self.group,
            &mut self.stderr,
            "stderr",
            result,
            &mut self.failures,
            &mut self.finalizing,
        );
    }
}

async fn read_pipe<Reader: tokio::io::AsyncRead + Unpin>(
    pipe: &mut Option<Reader>,
    buffer: &mut [u8],
) -> io::Result<usize> {
    use tokio::io::AsyncReadExt as _;

    match pipe {
        Some(pipe) => pipe.read(buffer).await,
        None => Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "output pipe is closed",
        )),
    }
}

fn handle_stream_read<Reader, Writer: Write>(
    group: &mut OwnedGroup,
    stream: &mut PumpStream<Reader, Writer>,
    name: &'static str,
    result: io::Result<usize>,
    failures: &mut Vec<SpoolFailure>,
    finalizing: &mut bool,
) {
    match result {
        Ok(0) => {
            stream.eof = true;
            stream.active = false;
            stream.pipe.take();
        }
        Ok(count) => {
            if write_chunk(&mut stream.sink, name, &stream.buffer[..count], failures) {
                group.begin_finalization();
                *finalizing = true;
            }
        }
        Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
        Err(error) => {
            failures.push(SpoolFailure {
                kind: SpoolFailureKind::Capture,
                message: format!("reading {name} pipe: {error}"),
            });
            stream.active = false;
            stream.pipe.take();
            group.begin_finalization();
            *finalizing = true;
        }
    }
}

pub(super) fn record_missing_eof(
    failures: &mut Vec<SpoolFailure>,
    stream: &str,
    reached_eof: bool,
) {
    if !reached_eof {
        failures.push(SpoolFailure {
            kind: SpoolFailureKind::Capture,
            message: format!(
                "{stream} output pipe did not reach EOF before the cleanup deadline; accepted prefix retained"
            ),
        });
    }
}

fn register_pipe<Source, Target>(
    child_pipe: Option<Source>,
    name: &str,
    failures: &mut Vec<SpoolFailure>,
    convert: impl FnOnce(Source) -> io::Result<Target>,
) -> Option<Target> {
    let Some(pipe) = child_pipe else {
        failures.push(SpoolFailure {
            kind: SpoolFailureKind::Capture,
            message: format!("requested child has no {name} pipe"),
        });
        return None;
    };
    match convert(pipe) {
        Ok(pipe) => Some(pipe),
        Err(error) => {
            failures.push(SpoolFailure {
                kind: SpoolFailureKind::Capture,
                message: format!("registering {name} pipe with caller-thread I/O driver: {error}"),
            });
            None
        }
    }
}

pub(super) struct StreamSink<W: Write> {
    pub(super) writer: W,
    limit: usize,
    pub(super) accepted: usize,
    write_failed: bool,
    overflow: bool,
}

impl<W: Write> StreamSink<W> {
    pub(super) fn new(writer: W, limit: usize) -> Self {
        Self {
            writer,
            limit,
            accepted: 0,
            write_failed: false,
            overflow: false,
        }
    }
}

pub(super) fn write_chunk<W: Write>(
    sink: &mut StreamSink<W>,
    name: &str,
    chunk: &[u8],
    failures: &mut Vec<SpoolFailure>,
) -> bool {
    if sink.write_failed || sink.overflow {
        return false;
    }
    let remaining = sink.limit.saturating_sub(sink.accepted);
    let prefix = chunk.len().min(remaining);
    if prefix > 0 {
        let (written, result) = write_all_counted(&mut sink.writer, &chunk[..prefix]);
        sink.accepted += written;
        if let Err(error) = result {
            sink.write_failed = true;
            failures.push(SpoolFailure {
                kind: SpoolFailureKind::Capture,
                message: format!("writing {name} spool: {error}"),
            });
            return true;
        }
    }
    if prefix < chunk.len() {
        sink.overflow = true;
        failures.push(SpoolFailure {
            kind: SpoolFailureKind::Capture,
            message: format!(
                "{name} output exceeded spool budget {}; retained accepted prefix",
                sink.limit
            ),
        });
        return true;
    }
    false
}

fn write_all_counted<W: Write>(writer: &mut W, mut bytes: &[u8]) -> (usize, io::Result<()>) {
    let mut written = 0;
    let mut interrupted = 0;
    while !bytes.is_empty() {
        match writer.write(bytes) {
            Ok(0) => {
                return (
                    written,
                    Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "failed to write the complete spool chunk",
                    )),
                );
            }
            Ok(count) => {
                written += count;
                bytes = &bytes[count..];
                interrupted = 0;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                interrupted += 1;
                if interrupted >= WRITE_INTERRUPTION_BUDGET {
                    return (
                        written,
                        Err(io::Error::new(
                            io::ErrorKind::Interrupted,
                            "spool write interruption budget exhausted",
                        )),
                    );
                }
            }
            Err(error) => return (written, Err(error)),
        }
    }
    (written, Ok(()))
}
