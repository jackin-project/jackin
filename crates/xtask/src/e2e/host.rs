// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use rustix::process::{Pid, Signal, kill_process_group, test_kill_process_group};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

const DEFAULT_OUTPUT_CAP: usize = 64 * 1024 * 1024;
const GROUP_KILL_ROUNDS: u32 = 100;
const GROUP_KILL_PAUSE: Duration = Duration::from_millis(20);

#[derive(Clone, Debug)]
pub struct HostCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<String, String>,
    pub timeout: Duration,
    pub output_cap: usize,
}

impl HostCommand {
    #[must_use]
    pub fn new(program: impl Into<String>, timeout: Duration) -> HostCommand {
        HostCommand {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
            timeout,
            output_cap: DEFAULT_OUTPUT_CAP,
        }
    }

    #[must_use]
    pub fn args<S: Into<String>>(mut self, args: impl IntoIterator<Item = S>) -> HostCommand {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> HostCommand {
        self.cwd = Some(cwd.into());
        self
    }

    #[must_use]
    pub fn env(mut self, env: BTreeMap<String, String>) -> HostCommand {
        self.env = env;
        self
    }

    #[must_use]
    pub fn output_cap(mut self, cap: usize) -> HostCommand {
        self.output_cap = cap;
        self
    }

    #[must_use]
    pub fn display(&self) -> String {
        let mut text = self.program.clone();
        for arg in &self.args {
            text.push(' ');
            text.push_str(arg);
        }
        text
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    Code(i32),
    Signal(i32),
}

impl Exit {
    #[must_use]
    pub fn success(self) -> bool {
        self == Exit::Code(0)
    }

    #[must_use]
    pub fn describe(self) -> String {
        match self {
            Exit::Code(code) => format!("exit {code}"),
            Exit::Signal(signal) => format!("signal {signal}"),
        }
    }
}

#[derive(Debug)]
pub struct HostOutput {
    pub exit: Exit,
    pub stdout: Zeroizing<Vec<u8>>,
    pub stderr: Zeroizing<Vec<u8>>,
}

impl HostOutput {
    #[must_use]
    pub fn new(exit: Exit, stdout: Vec<u8>, stderr: Vec<u8>) -> HostOutput {
        HostOutput {
            exit,
            stdout: Zeroizing::new(stdout),
            stderr: Zeroizing::new(stderr),
        }
    }

    #[must_use]
    pub fn ok(&self) -> bool {
        self.exit.success()
    }

    #[must_use]
    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    #[must_use]
    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

#[derive(Debug, Error)]
pub enum HostError {
    #[error("cannot start {program}: {message}")]
    Spawn { program: String, message: String },
    #[error("{program} timed out after {after:?}")]
    Timeout { program: String, after: Duration },
    #[error("{program} was cancelled")]
    Cancelled { program: String },
    #[error("{program} wrote more than {limit} bytes to {stream}")]
    OutputTooLarge {
        program: String,
        stream: &'static str,
        limit: usize,
    },
    #[error("reading {stream} of {program} failed: {message}")]
    Read {
        program: String,
        stream: &'static str,
        message: String,
    },
}

#[async_trait]
pub trait HostProcess: Send + Sync + Debug {
    async fn run(
        &self,
        command: &HostCommand,
        cancel: &CancellationToken,
    ) -> Result<HostOutput, HostError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RealHost {}

async fn read_capped<R: AsyncRead + Unpin>(
    reader: Option<R>,
    program: &str,
    stream: &'static str,
    cap: usize,
) -> Result<Vec<u8>, HostError> {
    let Some(mut reader) = reader else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    let mut chunk = vec![0_u8; 16 * 1024];
    loop {
        let count = reader
            .read(&mut chunk)
            .await
            .map_err(|error| HostError::Read {
                program: program.to_owned(),
                stream,
                message: error.to_string(),
            })?;
        if count == 0 {
            return Ok(out);
        }
        if out.len() + count > cap {
            return Err(HostError::OutputTooLarge {
                program: program.to_owned(),
                stream,
                limit: cap,
            });
        }
        out.extend_from_slice(&chunk[..count]);
    }
}

async fn kill_group(pid: Option<u32>) {
    let Some(pid) = pid
        .and_then(|raw| i32::try_from(raw).ok())
        .and_then(Pid::from_raw)
    else {
        return;
    };
    for _ in 0..GROUP_KILL_ROUNDS {
        if kill_process_group(pid, Signal::KILL).is_err() {
            return;
        }
        if test_kill_process_group(pid).is_err() {
            return;
        }
        tokio::time::sleep(GROUP_KILL_PAUSE).await;
    }
}

enum Interrupted {
    Timeout,
    Cancelled,
}

#[async_trait]
impl HostProcess for RealHost {
    async fn run(
        &self,
        command: &HostCommand,
        cancel: &CancellationToken,
    ) -> Result<HostOutput, HostError> {
        let program = command.program.clone();
        let mut process = Command::new(&command.program);
        process
            .args(&command.args)
            .env_clear()
            .envs(&command.env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .process_group(0);
        if let Some(cwd) = &command.cwd {
            process.current_dir(cwd);
        }
        let mut child = process.spawn().map_err(|error| HostError::Spawn {
            program: program.clone(),
            message: error.to_string(),
        })?;
        let pid = child.id();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let cap = command.output_cap;
        let work = async {
            let (out, err) = tokio::try_join!(
                read_capped(stdout, &program, "stdout", cap),
                read_capped(stderr, &program, "stderr", cap)
            )?;
            let status = child.wait().await.map_err(|error| HostError::Read {
                program: program.clone(),
                stream: "status",
                message: error.to_string(),
            })?;
            Ok::<_, HostError>((out, err, status))
        };
        let outcome = tokio::select! {
            result = work => Ok(result),
            () = tokio::time::sleep(command.timeout) => Err(Interrupted::Timeout),
            () = cancel.cancelled() => Err(Interrupted::Cancelled),
        };
        match outcome {
            Ok(Ok((out, err, status))) => {
                let exit = status
                    .code()
                    .map(Exit::Code)
                    .or_else(|| status.signal().map(Exit::Signal))
                    .unwrap_or(Exit::Code(-1));
                Ok(HostOutput::new(exit, out, err))
            }
            Ok(Err(error)) => {
                kill_group(pid).await;
                drop(child.wait().await);
                Err(error)
            }
            Err(interrupted) => {
                kill_group(pid).await;
                drop(child.wait().await);
                Err(match interrupted {
                    Interrupted::Timeout => HostError::Timeout {
                        program,
                        after: command.timeout,
                    },
                    Interrupted::Cancelled => HostError::Cancelled { program },
                })
            }
        }
    }
}
