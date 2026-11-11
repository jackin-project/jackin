// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::time::Duration;

use jackin_core::{
    CREDENTIAL_FILE_MAX_BYTES, FsError, GatewayName, GroupFault, HostPath, OpenShellWorkspace,
    OutputStream, Phase, ProcessError, ProcessStatus, ProfileId, ProviderName, SandboxName,
    SandboxPath, TerminalText, plain, plain_duration, plain_list,
};
use thiserror::Error;

use crate::policy::{ClosureViolation, RuleKey};

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("gateway {} is unreachable: {}", plain(.endpoint), plain(.message))]
    Unreachable { endpoint: String, message: String },
    #[error("the supervisor of {sandbox} is unreachable: {}", plain(.message))]
    SandboxUnreachable {
        sandbox: SandboxName,
        message: String,
    },
    #[error("TLS error: {}", plain(.message))]
    Tls { message: String },
    #[error("the gateway rejected the credentials")]
    Unauthenticated,
    #[error("permission denied: {}", plain(.what))]
    PermissionDenied { what: String },
    #[error("not found: {what}")]
    NotFound { what: Missing },
    #[error("already exists: {what}")]
    AlreadyExists { what: Missing },
    #[error("the sandbox is not ready: {phase}")]
    NotReady { phase: Phase },
    #[error("the main process of {sandbox} has ended")]
    MainProcessFailed { sandbox: SandboxName },
    #[error("failed precondition: {}", plain(.message))]
    FailedPrecondition { message: String },
    #[error("resource exhausted: {}", plain(.what))]
    ResourceExhausted { what: String },
    #[error("invalid argument {}: {}", plain(.field), plain(.message))]
    InvalidArgument { field: String, message: String },
    #[error("{op} timed out after {}{}", plain_duration(.after), GroupFault::note(.group.as_ref()))]
    Timeout {
        op: GatewayOp,
        after: Duration,
        group: Option<GroupFault>,
    },
    #[error("{op} was cancelled{}", GroupFault::note(.group.as_ref()))]
    Cancelled {
        op: GatewayOp,
        group: Option<GroupFault>,
    },
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error("{op} failed with {status}: {tail}")]
    Transfer {
        op: TransferOp,
        status: ProcessStatus,
        tail: TerminalText,
    },
    #[error("the remote file has {size} bytes, more than the limit of {limit}")]
    DownloadTooLarge { size: u64, limit: u64 },
    #[error("the remote path is a {kind}, not a regular file")]
    NotRegularFile { kind: TerminalText },
    #[error("the {stream} output of {op} is larger than {limit} bytes")]
    OutputTooLarge {
        op: GatewayOp,
        stream: OutputStream,
        limit: usize,
    },
    #[error("the SSH host key does not match the gateway response")]
    HostKeyMismatch,
    #[error("the terminal was refused: {}", plain(.reason))]
    TerminalRefused { reason: String },
    #[error("refused to write {}: a directory on its path below /sandbox is a symlink, or the target is a directory", plain(.path.as_str()))]
    UnsafePath { path: SandboxPath },
    #[error("protocol error: {}", plain(.message))]
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
    UploadTree,
    UploadFile,
    Download,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TransferOp {
    UploadFiltered,
    UploadFile,
    Download,
    Digest,
    Size,
}

impl fmt::Display for Missing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Missing::Sandbox(name) => write!(f, "sandbox {name}"),
            Missing::Provider(name) => write!(f, "provider {name}"),
            Missing::Profile(id) => write!(f, "profile {id}"),
            Missing::Workspace(name) => write!(f, "workspace {name}"),
        }
    }
}

impl fmt::Display for GatewayOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            GatewayOp::Health => "health",
            GatewayOp::Profile => "profile",
            GatewayOp::Provider => "provider",
            GatewayOp::Create => "create",
            GatewayOp::Find => "find",
            GatewayOp::Detail => "detail",
            GatewayOp::Power => "power",
            GatewayOp::Delete => "delete",
            GatewayOp::Program => "program",
            GatewayOp::ReadFiles => "read files",
            GatewayOp::WriteFile => "write file",
            GatewayOp::UploadTree => "upload tree",
            GatewayOp::UploadFile => "upload file",
            GatewayOp::Download => "download",
            GatewayOp::Terminal => "terminal",
        })
    }
}

impl fmt::Display for TransferOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TransferOp::UploadFiltered => "upload filtered",
            TransferOp::UploadFile => "upload file",
            TransferOp::Download => "download",
            TransferOp::Digest => "digest",
            TransferOp::Size => "size",
        })
    }
}

fn scheme_text(scheme: Option<&str>) -> String {
    scheme.map_or_else(
        || "no endpoint scheme".to_owned(),
        |scheme| format!("the endpoint scheme {}", plain(scheme)),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("the terminal session is closed")]
pub struct SessionClosed;

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("gateway list failed with {status}: {tail}")]
    ListFailed {
        status: ProcessStatus,
        tail: TerminalText,
    },
    #[error("gateway list output cannot be parsed: {}", plain(.message))]
    ListUnparsable { message: String },
    #[error("gateway {0} is not registered")]
    UnknownGateway(GatewayName),
    #[error("gateway {name} has {}; only https is accepted", scheme_text(.scheme.as_deref()))]
    InsecureEndpoint {
        name: GatewayName,
        scheme: Option<String>,
    },
    #[error(
        "gateway {name} has an endpoint with credentials; register it without a user or password"
    )]
    EndpointCredentials { name: GatewayName },
    #[error("gateway {name} uses the authentication {}; only mtls is accepted", plain(.auth))]
    UnsupportedAuth { name: GatewayName, auth: String },
    #[error("missing certificate {path}")]
    MissingCertificate { path: HostPath },
    #[error("certificate {path} is not a regular file")]
    CertificateNotRegular { path: HostPath },
    #[error(
        "certificate {path} is larger than {} bytes",
        CREDENTIAL_FILE_MAX_BYTES
    )]
    CertificateTooLarge { path: HostPath },
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error(transparent)]
    Fs(#[from] FsError),
}

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("policy syntax error: {}", plain(.message))]
    Syntax { message: String },
    #[error("{} must be a mapping", plain(.key))]
    NotAMapping { key: String },
    #[error("rule name {} is reserved", plain(.0))]
    ReservedRuleName(String),
    #[error("invalid host {}", plain(.0))]
    InvalidHost(String),
    #[error("landlock.compatibility must be hard_requirement, found {}", plain(.found.as_deref().unwrap_or("no value")))]
    LandlockMode { found: Option<String> },
    #[error("filesystem_policy must list at least one path in read_only or read_write")]
    NoFilesystemPaths,
    #[error("read_write path {} names the filesystem root", plain(.path))]
    ReadWriteRoot { path: String },
    #[error("read_write path {} is /proc, /sys or below them, which alias the filesystem root", plain(.path))]
    ReadWriteAlias { path: String },
    #[error("the gateway would refuse the policy: {}", plain_list(.violations))]
    Gateway {
        violations: Vec<openshell_policy::PolicyViolation>,
    },
    #[error("rule {} has the invalid port {port}", plain(.rule))]
    InvalidPort { rule: String, port: i64 },
    #[error("duplicate rule {0}")]
    DuplicateRule(RuleKey),
    #[error("profile {}: {}", plain(.id), plain(.message))]
    Profile { id: String, message: String },
}

#[derive(Debug, Error)]
pub enum ClosedPolicyError {
    #[error(transparent)]
    Policy(#[from] PolicyError),
    #[error("the policy overlaps provider hosts: {}", plain_list(.0))]
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
            | GatewayError::OutputTooLarge { .. }
            | GatewayError::DownloadTooLarge { .. }
            | GatewayError::NotRegularFile { .. }
            | GatewayError::HostKeyMismatch
            | GatewayError::TerminalRefused { .. }
            | GatewayError::UnsafePath { .. }
            | GatewayError::Protocol { .. } => false,
        }
    }
}
