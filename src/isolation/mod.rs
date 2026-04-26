use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

pub mod branch;
pub mod cleanup;
pub mod finalize;
pub mod materialize;
pub mod state;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MountIsolation {
    #[default]
    Shared,
    Worktree,
}

impl MountIsolation {
    pub const fn is_shared(&self) -> bool {
        matches!(self, Self::Shared)
    }

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Worktree => "worktree",
        }
    }
}

impl fmt::Display for MountIsolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MountIsolation {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "shared" => Ok(Self::Shared),
            "worktree" => Ok(Self::Worktree),
            other => {
                anyhow::bail!("invalid isolation `{other}`; expected one of: shared, worktree")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_lowercase_variants() {
        assert_eq!(
            MountIsolation::from_str("shared").unwrap(),
            MountIsolation::Shared
        );
        assert_eq!(
            MountIsolation::from_str("worktree").unwrap(),
            MountIsolation::Worktree
        );
    }

    #[test]
    fn rejects_share_alias() {
        let err = MountIsolation::from_str("share").unwrap_err();
        assert!(err.to_string().contains("invalid isolation `share`"));
    }

    #[test]
    fn rejects_unknown_spelling() {
        let err = MountIsolation::from_str("Worktree").unwrap_err();
        assert!(err.to_string().contains("invalid isolation `Worktree`"));
    }

    #[test]
    fn rejects_clone_until_implemented() {
        // `clone` is documented in the roadmap as a planned future mode
        // (V1.1) but is not in V1's enum vocabulary — make sure it falls
        // through to the standard "invalid isolation" error rather than
        // silently parsing or being treated as a reserved keyword.
        let err = MountIsolation::from_str("clone").unwrap_err();
        assert!(err.to_string().contains("invalid isolation `clone`"));
        assert!(err.to_string().contains("shared, worktree"));
    }

    #[test]
    fn default_is_shared() {
        assert_eq!(MountIsolation::default(), MountIsolation::Shared);
    }

    #[test]
    fn is_shared_predicate() {
        assert!(MountIsolation::Shared.is_shared());
        assert!(!MountIsolation::Worktree.is_shared());
    }

    #[test]
    fn display_renders_canonical_lowercase() {
        assert_eq!(MountIsolation::Shared.to_string(), "shared");
        assert_eq!(MountIsolation::Worktree.to_string(), "worktree");
    }
}
