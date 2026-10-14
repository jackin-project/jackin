#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Paths(#[from] jackin_core::PathsError),
    #[error(transparent)]
    WorkspaceName(#[from] jackin_core::WorkspaceNameError),
    #[error(transparent)]
    TomlSerialize(#[from] toml::ser::Error),
    #[error(transparent)]
    TomlDeserialize(#[from] toml::de::Error),
    #[error(transparent)]
    TomlEdit(#[from] toml_edit::TomlError),
    #[error(transparent)]
    Collapse(#[from] crate::planner::CollapseError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
    #[error(transparent)]
    TelemetryOwned(Box<ConfigError>),
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
    #[error("another jackin❯ process is editing the config{holder}")]
    ConfigLockTimeout { holder: String },
}

pub type ConfigResult<T> = Result<T, ConfigError>;

impl ConfigError {
    pub fn msg(message: std::fmt::Arguments<'_>) -> Self {
        Self::Message(message.to_string())
    }

    pub(crate) fn telemetry_owned(self) -> Self {
        if self.is_telemetry_owned() {
            self
        } else {
            Self::TelemetryOwned(Box::new(self))
        }
    }

    pub(crate) const fn is_telemetry_owned(&self) -> bool {
        matches!(self, Self::TelemetryOwned(_))
    }
}

#[cfg(test)]
mod tests;
