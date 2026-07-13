//! `MountIsolation`: the three isolation strategies for workspace mounts.

use serde::{Deserialize, Serialize};

/// Parse error for `MountIsolation`.
#[derive(Debug, thiserror::Error)]
#[error("invalid isolation `{0}`; expected one of: shared, worktree, clone")]
pub struct ParseMountIsolationError(String);

/// Controls how a workspace mount is isolated between the host and the
/// container.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MountIsolation {
    /// Read-write bind mount of the host path; no git operations.
    #[default]
    Shared,
    /// Git-worktree clone of the host repo, finalized post-attach.
    Worktree,
    /// Full directory copy, finalized post-attach.
    Clone,
}

impl MountIsolation {
    /// `true` when this is the default read-write bind-mount strategy.
    pub const fn is_shared(&self) -> bool {
        matches!(self, Self::Shared)
    }

    /// Canonical lowercase config/wire spelling (`"shared"`, `"worktree"`, `"clone"`).
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Worktree => "worktree",
            Self::Clone => "clone",
        }
    }
}

impl std::fmt::Display for MountIsolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for MountIsolation {
    type Err = ParseMountIsolationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "shared" => Ok(Self::Shared),
            "worktree" => Ok(Self::Worktree),
            "clone" => Ok(Self::Clone),
            other => Err(ParseMountIsolationError(other.to_owned())),
        }
    }
}
