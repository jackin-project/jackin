//! Typed config load / validate / migrate / editor errors.

/// Failures from jackin-config public surfaces.
///
/// Multi-line migration and remediation text use [`ConfigError::Message`] so
/// operator-facing wording stays byte-stable while still being typed.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{0}")]
    Message(String),
    #[error("unknown workspace {0}")]
    UnknownWorkspace(String),
    #[error("workspace {0:?} not found")]
    WorkspaceNotFound(String),
    #[error("workspace {0:?} already exists")]
    WorkspaceAlreadyExists(String),
    #[error("workspace {0:?} disappeared after create")]
    WorkspaceDisappearedAfterCreate(String),
    #[error("workspace {0} disappeared after edit")]
    WorkspaceDisappearedAfterEdit(String),
    #[error("workspace {0:?} must define workdir")]
    WorkdirRequired(String),
    #[error("workspace {0:?} workdir must be an absolute container path")]
    WorkdirNotAbsolute(String),
    #[error("workspace {0:?} must define at least one mount")]
    MountsRequired(String),
    #[error("mount source must be absolute: {0}")]
    MountSrcNotAbsolute(String),
    #[error("mount destination must be an absolute path: {0}")]
    MountDstNotAbsolute(String),
    #[error("duplicate mount destination: {0}")]
    DuplicateMountDst(String),
    #[error("mount source does not exist: {0}")]
    MountSrcMissing(String),
    #[error("global config.toml must not contain [workspaces] tables")]
    GlobalHasWorkspacesTable,
    #[error("invalid workspace filename {0}")]
    InvalidWorkspaceFilename(String),
    #[error("version must start with `v`")]
    VersionMissingVPrefix,
    #[error("missing major version")]
    VersionMissingMajor,
    #[error("major version must be greater than zero")]
    VersionMajorZero,
    #[error("version must look like v1, v1beta1, or v1alpha1")]
    VersionShape,
    #[error("no attempts made")]
    NoAttemptsMade,
}

impl ConfigError {
    pub fn msg(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}
