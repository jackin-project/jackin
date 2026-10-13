//! Typed errors for jackin-instance fallible surfaces.
//!
//! Library code returns these via `thiserror`; binary boundaries may
//! still absorb them into `anyhow::Error` with `?`.

use std::path::PathBuf;

/// Failures produced by instance identity, index, and auth provisioning.
#[derive(Debug, thiserror::Error)]
pub enum InstanceError {
    #[error(
        "failed to read host {path}: {source} (run with --debug to capture the underlying error)"
    )]
    HostConfigRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("no parent directory for {path}")]
    NoParentDirectory { path: PathBuf },
    #[error("instance `{container_base}` has unknown agent runtime {agent_runtime:?}")]
    UnknownAgentRuntime {
        container_base: String,
        agent_runtime: String,
    },
    #[error("instance index missing at {path}")]
    IndexMissing { path: PathBuf },
    #[error("GitHub auth provisioning task panicked")]
    GithubAuthTaskPanicked,
    #[error("{agent} auth provisioning task panicked")]
    AuthProvisionTaskPanicked { agent: String },
    #[error("background agent auth provisioning task panicked")]
    BackgroundAuthTaskPanicked,
}

/// Sync-source folder failed agent-specific credential structure checks.
///
/// Display text is operator-facing (console auth form validation).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SyncSourceValidationError(pub String);

impl SyncSourceValidationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}
