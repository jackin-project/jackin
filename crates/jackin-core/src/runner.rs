//! `CommandRunner` trait and `RunOptions`: the subprocess execution seam for
//! `docker`, `git`, and other external commands.
//!
//! `CommandRunner` is dependency-injected into the runtime pipeline so tests
//! can replace it with `FakeRunner` without spawning real processes. The
//! concrete `ShellRunner` implementation lives in `src/docker/mod.rs` until
//! it migrates to `jackin-runtime`.

use std::path::Path;
use std::sync::Arc;

use crate::build_log_sink::BuildLogSink;

/// Options that control how a command is executed.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug)]
pub struct RunOptions {
    pub capture_stderr: bool,
    pub capture_stdout: bool,
    pub quiet: bool,
    pub extra_env: Vec<(String, String)>,
    pub null_stdin: bool,
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
        }
    }
}

/// Subprocess execution seam for `docker`, `git`, and other external commands.
pub trait CommandRunner {
    async fn run(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &RunOptions,
    ) -> anyhow::Result<()>;
    async fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String>;
    /// Like `capture` but suppresses stdout from the debug stream and omits
    /// stderr from error messages. Use for commands whose output is a credential
    /// (e.g. `gh auth token`, `op read`) so the value never appears in debug logs.
    async fn capture_secret(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String>;
}
