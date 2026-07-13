//! Typed errors for Docker client / shell runner / download helpers.

/// Failures from subprocess runs, Docker exec attach, and HTTP downloads.
#[derive(Debug, thiserror::Error)]
pub enum DockerError {
    #[error("command timed out after {secs}s: {program}")]
    CommandTimeout { secs: f64, program: String },
    #[error("command failed: {program} {args}")]
    CommandFailed { program: String, args: String },
    #[error("command failed: {program} {args}: {stderr}")]
    CommandFailedWithStderr {
        program: String,
        args: String,
        stderr: String,
    },
    #[error("command failed: {program} {args} (captured output in diagnostics run {run_id})")]
    CommandFailedDebugRun {
        program: String,
        args: String,
        run_id: String,
    },
    #[error(
        "command failed: {program} {args} (output suppressed; rerun with --debug to capture it in diagnostics run {run_id})"
    )]
    CommandFailedSuppressed {
        program: String,
        args: String,
        run_id: String,
    },
    #[error("command failed: {program} {args} (stderr: {stderr}; captured output suppressed)")]
    CommandFailedStderrSummary {
        program: String,
        args: String,
        stderr: String,
    },
    #[error("command failed: {program} {args} (captured output suppressed)")]
    CommandFailedCapturedSuppressed { program: String, args: String },
    #[error("command failed: {program} {args} (see stderr above)")]
    CommandFailedSeeStderr { program: String, args: String },
    #[error("Docker build command failed")]
    DockerBuildFailed,
    #[error("exec in {container} returned Detached — attach_stdout was set but exec ran detached")]
    ExecDetached { container: String },
    #[error("exec in {container} exited with code {exit_code}: {output}")]
    ExecNonZero {
        container: String,
        exit_code: i64,
        output: String,
    },
    #[error("building shared HTTP client: {0}")]
    HttpClientBuild(String),
    #[error("{url} failed: HTTP {status}")]
    HttpStatus { url: String, status: String },
    #[error("prefetch {url}: {detail}")]
    Prefetch { url: String, detail: String },
    #[error("server at {url} does not support Range requests; cannot download in parallel")]
    RangeUnsupported { url: String },
    #[error("download task panicked for {url}: {detail}")]
    DownloadTaskPanicked { url: String, detail: String },
    #[error("download of {url} timed out after {timeout:?}")]
    DownloadTimeout {
        url: String,
        timeout: std::time::Duration,
    },
    #[error("{0}")]
    Message(String),
}
