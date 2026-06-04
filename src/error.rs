/// Operator-facing error codes and friendly messages for user-visible failures.
///
/// `JackinError` is the surface-level error type rendered at the binary entry
/// point. Internal anyhow chains propagate as-is until they reach `main.rs`,
/// where a downcast to `JackinError` triggers a structured friendly block.
/// Unrecognized errors fall back to `{error:#}`.
use thiserror::Error;

/// Stable error codes for `jackin` operator-visible failures.
///
/// Each code maps to a docs page at `docs/content/docs/reference/errors/<code>.mdx`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    E001,
    E002,
    E003,
    E004,
    E005,
    E006,
    E007,
    E008,
    E009,
    E010,
    E011,
    E012,
    E013,
    E014,
    E015,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::E001 => "E001",
            Self::E002 => "E002",
            Self::E003 => "E003",
            Self::E004 => "E004",
            Self::E005 => "E005",
            Self::E006 => "E006",
            Self::E007 => "E007",
            Self::E008 => "E008",
            Self::E009 => "E009",
            Self::E010 => "E010",
            Self::E011 => "E011",
            Self::E012 => "E012",
            Self::E013 => "E013",
            Self::E014 => "E014",
            Self::E015 => "E015",
        }
    }
}

/// Structured hint for fixing a `JackinError`.
#[derive(Debug, Clone)]
pub struct UserMessage {
    pub code: ErrorCode,
    pub headline: &'static str,
    pub what_to_try: &'static str,
    pub more_detail: Option<&'static str>,
}

impl UserMessage {
    fn new(code: ErrorCode, headline: &'static str, what_to_try: &'static str) -> Self {
        Self {
            code,
            headline,
            what_to_try,
            more_detail: None,
        }
    }

    fn with_detail(mut self, detail: &'static str) -> Self {
        self.more_detail = Some(detail);
        self
    }

    /// Render the friendly block to stderr.
    pub fn render(&self) {
        use owo_colors::OwoColorize;
        eprintln!(
            "{} [{}] {}",
            "error:".red().bold(),
            self.code.as_str(),
            self.headline
        );
        eprintln!("  {}", "→ what to try:".yellow());
        eprintln!("    {}", self.what_to_try);
        if let Some(detail) = self.more_detail {
            eprintln!("  {}", "→ more detail:".dimmed());
            eprintln!("    {}", detail.dimmed());
        }
    }
}

/// Catalogue of the top ~15 operator-visible failure modes.
///
/// Each variant is convertible to a `UserMessage` with stable code, headline,
/// and remediation hint. Internal details (the underlying `anyhow` chain) are
/// carried as `source` and surfaced via `--debug` only.
#[derive(Debug, Error)]
pub enum JackinError {
    #[error("Docker daemon not reachable")]
    DockerDaemonUnreachable {
        #[source]
        source: anyhow::Error,
    },

    #[error("Docker version too old")]
    DockerVersionTooOld { found: String, required: String },

    #[error("Out of disk space for image build")]
    OutOfDiskSpace { path: String },

    #[error("Role manifest invalid: {detail}")]
    RoleManifestInvalid { detail: String },

    #[error("Role manifest version unsupported: {version}")]
    RoleManifestVersionUnsupported { version: u32 },

    #[error("Role source not trusted: {role}")]
    RoleSourceNotTrusted { role: String },

    #[error("Workspace not found: {name}")]
    WorkspaceNotFound { name: String },

    #[error("Workspace config version unsupported: {version}")]
    WorkspaceConfigVersionUnsupported { version: u32 },

    #[error("Container name conflict: {name}")]
    ContainerNameConflict { name: String },

    #[error("DinD sidecar failed health check")]
    DindHealthCheckFailed {
        #[source]
        source: anyhow::Error,
    },

    #[error("Port conflict on DinD TLS port {port}")]
    DindPortConflict { port: u16 },

    #[error("GitHub CLI authentication failed")]
    GhAuthFailed,

    #[error("1Password CLI not signed in")]
    OpNotSignedIn,

    #[error("Capsule binary download failed")]
    CapsuleDownloadFailed {
        #[source]
        source: anyhow::Error,
    },

    #[error("Worktree materialization conflict: {path}")]
    WorktreeConflict { path: String },
}

impl JackinError {
    /// Return the structured user-facing message for this error.
    pub fn user_message(&self) -> UserMessage {
        match self {
            Self::DockerDaemonUnreachable { .. } => UserMessage::new(
                ErrorCode::E001,
                "Docker daemon not reachable",
                "Start Docker Desktop, OrbStack, or run `colima start`. Check `DOCKER_HOST` if using a remote daemon.",
            ).with_detail("Run `jackin doctor` to see all pre-flight check results."),

            Self::DockerVersionTooOld { found, required } => UserMessage::new(
                ErrorCode::E002,
                "Docker version too old",
                "Upgrade Docker to the latest stable release.",
            ).with_detail(Box::leak(format!("Found {found}, need ≥{required}").into_boxed_str())),

            Self::OutOfDiskSpace { path } => UserMessage::new(
                ErrorCode::E003,
                "Out of disk space for image build",
                "Run `jackin prune` or `docker system prune` to reclaim space.",
            ).with_detail(Box::leak(format!("Filesystem containing {path} is nearly full.").into_boxed_str())),

            Self::RoleManifestInvalid { .. } => UserMessage::new(
                ErrorCode::E004,
                "Role manifest (jackin.role.toml) is invalid",
                "Fix the syntax or schema errors shown above and re-run.",
            ),

            Self::RoleManifestVersionUnsupported { version } => UserMessage::new(
                ErrorCode::E005,
                "Role manifest version is not supported by this jackin binary",
                "Upgrade jackin (`brew upgrade jackin`) or pin the role to a compatible manifest version.",
            ).with_detail(Box::leak(format!("Manifest declares version {version}.").into_boxed_str())),

            Self::RoleSourceNotTrusted { role } => UserMessage::new(
                ErrorCode::E006,
                "Role source is not in the trusted list",
                "Run `jackin config trust add <role>` to trust this role, or verify the source URL is correct.",
            ).with_detail(Box::leak(format!("Role: {role}").into_boxed_str())),

            Self::WorkspaceNotFound { name } => UserMessage::new(
                ErrorCode::E007,
                "Workspace not found",
                "Run `jackin workspace list` to see saved workspaces, or `jackin workspace create` to add one.",
            ).with_detail(Box::leak(format!("No workspace named {name:?}.").into_boxed_str())),

            Self::WorkspaceConfigVersionUnsupported { version } => UserMessage::new(
                ErrorCode::E008,
                "Workspace config version is not supported",
                "Upgrade jackin to read this config version.",
            ).with_detail(Box::leak(format!("Config declares version {version}.").into_boxed_str())),

            Self::ContainerNameConflict { name } => UserMessage::new(
                ErrorCode::E009,
                "Container name already in use",
                "Run `jackin prune` to remove stale containers, or `docker rm <name>` to remove the specific one.",
            ).with_detail(Box::leak(format!("Container: {name}").into_boxed_str())),

            Self::DindHealthCheckFailed { .. } => UserMessage::new(
                ErrorCode::E010,
                "Docker-in-Docker sidecar failed its health check",
                "Run with `--debug` and share the run id to diagnose. Try `jackin purge` to clean up and re-launch.",
            ),

            Self::DindPortConflict { port } => UserMessage::new(
                ErrorCode::E011,
                "Port conflict on DinD TLS port",
                "Another process is using the DinD TLS port. Stop it or configure a different port.",
            ).with_detail(Box::leak(format!("Port {port} is in use.").into_boxed_str())),

            Self::GhAuthFailed => UserMessage::new(
                ErrorCode::E012,
                "GitHub CLI authentication failed",
                "Run `gh auth login` to authenticate, then re-run.",
            ),

            Self::OpNotSignedIn => UserMessage::new(
                ErrorCode::E013,
                "1Password CLI is not signed in",
                "Run `op signin` and re-run, or remove `op://` references from your workspace env vars.",
            ),

            Self::CapsuleDownloadFailed { .. } => UserMessage::new(
                ErrorCode::E014,
                "Failed to download jackin-capsule binary",
                "Check your internet connection and retry. Run with `--debug` for the download URL and error detail.",
            ),

            Self::WorktreeConflict { path } => UserMessage::new(
                ErrorCode::E015,
                "Worktree materialization conflict",
                "Run `jackin prune isolation` to clean up stale worktrees, then re-run.",
            ).with_detail(Box::leak(format!("Conflict at: {path}").into_boxed_str())),
        }
    }
}
