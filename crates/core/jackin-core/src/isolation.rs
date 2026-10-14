// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct ParseMountIsolationError(String);

impl std::fmt::Display for ParseMountIsolationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid isolation `{}`; expected one of: shared, worktree, clone",
            self.0
        )
    }
}

impl std::error::Error for ParseMountIsolationError {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MountIsolation {
    #[default]
    Shared,
    Worktree,
    Clone,
}

impl MountIsolation {
    pub const fn is_shared(&self) -> bool {
        matches!(self, Self::Shared)
    }

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
