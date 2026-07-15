// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `CommandRunner` trait and `RunOptions`: the subprocess execution seam for
//! `docker`, `git`, and other external commands.
//!
//! `CommandRunner` is dependency-injected into the runtime pipeline so tests
//! can replace it with `FakeRunner` without spawning real processes.
//!
//! Canonical engines:
//! - **async host** — `jackin_docker::shell_runner::ShellRunner` (honors
//!   [`RunOptions::timeout`]).
//! - **sync capsule** — `jackin_capsule`'s `wait_child_with_timeout` /
//!   `WaitOutcome` engine for PID-1-aware waits.
//!
//! New wrappers must route through one of these rather than hand-rolling
//! spawn/status/capture/timeout.

use std::path::Path;
use std::sync::Arc;

use crate::build_log_sink::BuildLogSink;

/// Options that control how a command is executed.
#[expect(
    clippy::struct_excessive_bools,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
#[derive(Clone, Debug)]
pub struct RunOptions {
    /// Capture stderr into the result/error payload.
    pub capture_stderr: bool,
    /// Capture stdout into the result payload.
    pub capture_stdout: bool,
    /// Suppress host-side process noise where the runner supports it.
    pub quiet: bool,
    /// Extra environment variables applied for this invocation only.
    pub extra_env: Vec<(String, String)>,
    /// Redirect stdin from `/dev/null` instead of inheriting.
    pub null_stdin: bool,
    /// When capturing, also stream output to the host debug surface.
    pub stream_captured_output: bool,
    /// The command needs the real terminal (an interactive `docker exec -it`
    /// multiplexer/shell client). Such commands must inherit stdio and are
    /// never captured — capturing denies the TTY and blocks forever on the
    /// long-lived session, even under `--debug` or while a rich surface was
    /// active.
    pub interactive: bool,
    /// Tee captured output into the build-log sink so the loading cockpit can
    /// show a live view. Only the derived-image `docker build` sets this.
    pub tee_to_build_log: bool,
    /// The sink that receives tee-captured build output when `tee_to_build_log` is
    /// true. Injected by the runtime entry point (`jackin-runtime`) before
    /// docker-build invocations; `None` suppresses teeing.
    pub build_log_sink: Option<Arc<dyn BuildLogSink>>,
    /// Deadline for the child process. `None` = no deadline.
    /// Enforced by implementors that own real processes (`ShellRunner`);
    /// fakes may ignore it.
    pub timeout: Option<std::time::Duration>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            capture_stderr: false,
            capture_stdout: false,
            quiet: false,
            extra_env: Vec::new(),
            null_stdin: false,
            stream_captured_output: true,
            interactive: false,
            tee_to_build_log: false,
            build_log_sink: None,
            timeout: None,
        }
    }
}

/// Subprocess execution seam for `docker`, `git`, and other external commands.
pub trait CommandRunner {
    /// Run `program` with `args`, applying `opts`; fails on non-zero exit.
    async fn run(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &RunOptions,
    ) -> anyhow::Result<()>;
    /// Run and return captured stdout (and typically stderr on failure).
    async fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String>;
    /// Like [`CommandRunner::capture`] but suppresses stdout from the debug stream and omits
    /// stderr from error messages. Use for commands whose output is a credential
    /// (e.g. `gh auth token`, `op read`) so the value never appears in debug logs.
    async fn capture_secret(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String>;
}
