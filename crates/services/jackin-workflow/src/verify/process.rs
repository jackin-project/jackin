// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{BoundedOutput, MAX_TEST_OUTPUT_BYTES, TestExecution};
use crate::WorkflowError;
use std::time::{Duration, Instant};

const READER_JOIN_TIMEOUT: Duration = Duration::from_secs(5);

impl BoundedOutput {
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }

    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

pub(super) fn read_bounded(stream: impl std::io::Read) -> BoundedOutput {
    let mut stream = stream;
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8 * 1024];
    let mut truncated = false;
    loop {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                let remaining = MAX_TEST_OUTPUT_BYTES.saturating_sub(bytes.len());
                let kept = remaining.min(read);
                bytes.extend_from_slice(&buffer[..kept]);
                truncated |= kept < read;
            }
        }
    }
    BoundedOutput { bytes, truncated }
}

pub(super) fn run_host_command_until(
    command: &mut std::process::Command,
    deadline: Instant,
) -> Result<Option<std::process::ExitStatus>, WorkflowError> {
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|err| WorkflowError::blocked(format!("Docker helper failed to spawn: {err}")))?;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|err| WorkflowError::blocked(format!("Docker helper wait failed: {err}")))?
        {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            child.kill().map_err(|err| {
                WorkflowError::blocked(format!("Docker helper kill failed: {err}"))
            })?;
            child.wait().map_err(|err| {
                WorkflowError::blocked(format!("Docker helper reap failed: {err}"))
            })?;
            return Ok(None);
        }
        #[expect(
            clippy::disallowed_methods,
            reason = "bounded Docker helper deadline poll on the host verification path"
        )]
        std::thread::sleep(Duration::from_millis(10));
    }
}

impl TestExecution {
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        self.exit_code
    }

    #[must_use]
    pub const fn timed_out(&self) -> bool {
        self.timed_out
    }

    #[must_use]
    pub const fn stdout(&self) -> &BoundedOutput {
        &self.stdout
    }

    #[must_use]
    pub const fn stderr(&self) -> &BoundedOutput {
        &self.stderr
    }

    #[cfg(test)]
    pub fn from_test_parts(exit_code: i32, output: &str) -> Self {
        let text = output.as_bytes();
        let half = text.len() / 2;
        Self {
            exit_code,
            timed_out: false,
            stdout: BoundedOutput {
                bytes: text[..half].to_vec(),
                truncated: false,
            },
            stderr: BoundedOutput {
                bytes: text[half..].to_vec(),
                truncated: false,
            },
        }
    }

    #[must_use]
    pub fn exit_label(&self) -> String {
        if self.timed_out {
            "timeout".to_owned()
        } else if self.exit_code < 0 {
            "signal".to_owned()
        } else {
            format!("exit {}", self.exit_code)
        }
    }
}

pub(super) fn join_reader(
    handle: std::thread::JoinHandle<BoundedOutput>,
    stream: &str,
) -> Result<BoundedOutput, WorkflowError> {
    let deadline = Instant::now() + READER_JOIN_TIMEOUT;
    loop {
        if handle.is_finished() {
            return handle.join().map_err(|_| {
                WorkflowError::storage(format!("test worker {stream} reader panicked"))
            });
        }
        if Instant::now() >= deadline {
            return Err(WorkflowError::blocked(format!(
                "test worker {stream} reader did not finish; \
                 a child process may hold the pipe open"
            )));
        }
        #[expect(
            clippy::disallowed_methods,
            reason = "bounded reader join on the host verification path, off render/runtime threads"
        )]
        std::thread::sleep(Duration::from_millis(10));
    }
}
