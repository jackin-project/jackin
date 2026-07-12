//! Typed config load / validate / migrate / editor errors.

/// Failures from jackin-config public surfaces.
///
/// Multi-line migration and remediation text use [`ConfigError::Message`] so
/// operator-facing wording stays byte-stable while still being typed.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Free-form operator-facing message (migrations, remediations, etc.).
    #[error("{0}")]
    Message(String),
    /// Lookup failed for a named workspace.
    #[error("unknown workspace {0}")]
    UnknownWorkspace(String),
    /// Workspace file/key not present on disk or in the editor.
    #[error("workspace {0:?} not found")]
    WorkspaceNotFound(String),
    /// Create failed because the name is already taken.
    #[error("workspace {0:?} already exists")]
    WorkspaceAlreadyExists(String),
    /// Create succeeded in-memory but the workspace vanished before persist.
    #[error("workspace {0:?} disappeared after create")]
    WorkspaceDisappearedAfterCreate(String),
    /// Edit succeeded in-memory but the workspace vanished before persist.
    #[error("workspace {0} disappeared after edit")]
    WorkspaceDisappearedAfterEdit(String),
    /// Workspace config omitted required `workdir`.
    #[error("workspace {0:?} must define workdir")]
    WorkdirRequired(String),
    /// Workspace `workdir` is not an absolute container path.
    #[error("workspace {0:?} workdir must be an absolute container path")]
    WorkdirNotAbsolute(String),
    /// Workspace has no mounts.
    #[error("workspace {0:?} must define at least one mount")]
    MountsRequired(String),
    /// Mount `src` is not absolute.
    #[error("mount source must be absolute: {0}")]
    MountSrcNotAbsolute(String),
    /// Mount `dst` is not absolute.
    #[error("mount destination must be an absolute path: {0}")]
    MountDstNotAbsolute(String),
    /// Two mounts share the same destination path.
    #[error("duplicate mount destination: {0}")]
    DuplicateMountDst(String),
    /// Mount `src` path does not exist on the host.
    #[error("mount source does not exist: {0}")]
    MountSrcMissing(String),
    /// Split config still has embedded `[workspaces]` in global `config.toml`.
    #[error("global config.toml must not contain [workspaces] tables")]
    GlobalHasWorkspacesTable,
    /// Workspace filename stem failed validation.
    #[error("invalid workspace filename {0}")]
    InvalidWorkspaceFilename(String),
    /// Schema version string missing the leading `v`.
    #[error("version must start with `v`")]
    VersionMissingVPrefix,
    /// Schema version string missing a major number.
    #[error("missing major version")]
    VersionMissingMajor,
    /// Schema major version is zero.
    #[error("major version must be greater than zero")]
    VersionMajorZero,
    /// Schema version string does not match `vN` / `vNalphaM` / `vNbetaM`.
    #[error("version must look like v1, v1beta1, or v1alpha1")]
    VersionShape,
    /// Internal: no resolution attempts were recorded.
    #[error("no attempts made")]
    NoAttemptsMade,
}

impl ConfigError {
    /// Wrap a free-form message as [`ConfigError::Message`].
    pub fn msg(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}
