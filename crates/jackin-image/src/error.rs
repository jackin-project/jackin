//! Typed errors for image build, agent binary, and capsule binary paths.

/// Failures from derived-image context, binary artifact fetch, and capsule
/// provenance verification.
#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("{0}")]
    Message(String),
    #[error("no attempts made")]
    NoAttemptsMade,
    #[error("expected a 64-char hex sha256, got {got:?}")]
    InvalidSha256Hex { got: String },
    #[error("{archive} is missing member {member}")]
    ArchiveMemberMissing {
        archive: std::path::PathBuf,
        member: String,
    },
    #[error("hook {path} is not a regular file")]
    HookNotRegularFile { path: String },
    #[error("refusing to include symlink in build context: {path}")]
    SymlinkInBuildContext { path: String },
    #[error("invalid role repo: derived build context does not support symlinks: {path}")]
    RoleRepoSymlink { path: String },
    #[error("missing string field {pointer}")]
    MissingJsonString { pointer: String },
    #[error("missing integer field {pointer}")]
    MissingJsonInteger { pointer: String },
    #[error("field {pointer} is not an integer string")]
    JsonIntegerString { pointer: String },
    #[error("no URI SAN found in Fulcio certificate")]
    NoUriSan,
}

impl ImageError {
    pub fn msg(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}
