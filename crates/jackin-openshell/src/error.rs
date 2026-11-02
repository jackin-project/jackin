// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{
    FsError, GatewayName, HostPath, OpenShellWorkspace, Phase, ProcessError, ProcessStatus,
    ProfileId, ProviderName, SandboxName, TerminalText,
};
use thiserror::Error;

use crate::policy::{ClosureViolation, RuleKey};

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("gateway {endpoint} is unreachable: {message}")]
    Unreachable { endpoint: String, message: String },
    #[error("the supervisor of {sandbox:?} is unreachable: {message}")]
    SandboxUnreachable {
        sandbox: SandboxName,
        message: String,
    },
    #[error("TLS error: {message}")]
    Tls { message: String },
    #[error("the gateway rejected the credentials")]
    Unauthenticated,
    #[error("permission denied: {what}")]
    PermissionDenied { what: String },
    #[error("not found: {what:?}")]
    NotFound { what: Missing },
    #[error("already exists: {what:?}")]
    AlreadyExists { what: Missing },
    #[error("the sandbox is not ready: {phase:?}")]
    NotReady { phase: Phase },
    #[error("the main process of {sandbox:?} has ended")]
    MainProcessFailed { sandbox: SandboxName },
    #[error("failed precondition: {message}")]
    FailedPrecondition { message: String },
    #[error("resource exhausted: {what}")]
    ResourceExhausted { what: String },
    #[error("invalid argument {field}: {message}")]
    InvalidArgument { field: String, message: String },
    #[error("{op:?} timed out after {after:?}")]
    Timeout { op: GatewayOp, after: Duration },
    #[error("{op:?} was cancelled")]
    Cancelled { op: GatewayOp },
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error("{op:?} failed with {status:?}: {tail:?}")]
    Transfer {
        op: TransferOp,
        status: ProcessStatus,
        tail: TerminalText,
    },
    #[error("the SSH host key does not match the gateway response")]
    HostKeyMismatch,
    #[error("the terminal was refused: {reason}")]
    TerminalRefused { reason: String },
    #[error("protocol error: {message}")]
    Protocol { message: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Missing {
    Sandbox(SandboxName),
    Provider(ProviderName),
    Profile(ProfileId),
    Workspace(OpenShellWorkspace),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GatewayOp {
    Health,
    Profile,
    Provider,
    Create,
    Find,
    Detail,
    Power,
    Delete,
    Program,
    ReadFiles,
    WriteFile,
    Upload,
    Download,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TransferOp {
    UploadFiltered,
    UploadGit,
    Download,
    Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("the terminal session is closed")]
pub struct SessionClosed;

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("gateway list failed with {status:?}: {tail:?}")]
    ListFailed {
        status: ProcessStatus,
        tail: TerminalText,
    },
    #[error("gateway list output cannot be parsed: {message}")]
    ListUnparsable { message: String },
    #[error("gateway {0:?} is not registered")]
    UnknownGateway(GatewayName),
    #[error("gateway {name:?} has the endpoint scheme {scheme:?}; only https is accepted")]
    InsecureEndpoint {
        name: GatewayName,
        scheme: Option<String>,
    },
    #[error("gateway {name:?} uses the authentication {auth:?}; only mtls is accepted")]
    UnsupportedAuth { name: GatewayName, auth: String },
    #[error("missing certificate {path:?}")]
    MissingCertificate { path: HostPath },
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error(transparent)]
    Fs(#[from] FsError),
}

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("policy syntax error: {message}")]
    Syntax { message: String },
    #[error("{key} must be a mapping")]
    NotAMapping { key: String },
    #[error("rule name {0} is reserved")]
    ReservedRuleName(String),
    #[error("invalid host {0}")]
    InvalidHost(String),
    #[error("rule {rule} has the invalid port {port}")]
    InvalidPort { rule: String, port: i64 },
    #[error("duplicate rule {0:?}")]
    DuplicateRule(RuleKey),
    #[error("profile {id}: {message}")]
    Profile { id: String, message: String },
}

#[derive(Debug, Error)]
pub enum ClosedPolicyError {
    #[error(transparent)]
    Policy(#[from] PolicyError),
    #[error("the policy overlaps provider hosts: {0:?}")]
    Violations(Vec<ClosureViolation>),
}

impl GatewayError {
    pub fn is_retryable(&self) -> bool {
        match self {
            GatewayError::Unreachable { .. }
            | GatewayError::SandboxUnreachable { .. }
            | GatewayError::Timeout { .. }
            | GatewayError::ResourceExhausted { .. } => true,
            GatewayError::Tls { .. }
            | GatewayError::Unauthenticated
            | GatewayError::PermissionDenied { .. }
            | GatewayError::NotFound { .. }
            | GatewayError::AlreadyExists { .. }
            | GatewayError::NotReady { .. }
            | GatewayError::MainProcessFailed { .. }
            | GatewayError::FailedPrecondition { .. }
            | GatewayError::InvalidArgument { .. }
            | GatewayError::Cancelled { .. }
            | GatewayError::Process(_)
            | GatewayError::Transfer { .. }
            | GatewayError::HostKeyMismatch
            | GatewayError::TerminalRefused { .. }
            | GatewayError::Protocol { .. } => false,
        }
    }
}
