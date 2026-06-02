//! Mount isolation: `MountIsolation` enum and the sub-modules that implement
//! the three isolation strategies.
//!
//! * `Shared` — read-write bind mount of the host path; no git operations.
//! * `Worktree` — git-worktree clone of the host repo, finalized post-attach.
//! * `Clone` — full directory copy, finalized post-attach.
//!
//! Sub-modules: `materialize` (produces bind specs from `WorkspaceConfig`),
//! `finalize` (post-attach preserve-vs-clean policy), `cleanup` (forced
//! removal), `state` (`IsolationRecord` persistence), `branch` (worktree
//! branch naming).

pub mod branch;
pub mod cleanup;
pub mod finalize;
pub mod materialize;
pub mod state;

pub use jackin_core::MountIsolation;
pub use jackin_core::ParseMountIsolationError;

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

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
        assert_eq!(
            MountIsolation::from_str("clone").unwrap(),
            MountIsolation::Clone
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
    fn default_is_shared() {
        assert_eq!(MountIsolation::default(), MountIsolation::Shared);
    }

    #[test]
    fn is_shared_predicate() {
        assert!(MountIsolation::Shared.is_shared());
        assert!(!MountIsolation::Worktree.is_shared());
        assert!(!MountIsolation::Clone.is_shared());
    }

    #[test]
    fn display_renders_canonical_lowercase() {
        assert_eq!(MountIsolation::Shared.to_string(), "shared");
        assert_eq!(MountIsolation::Worktree.to_string(), "worktree");
        assert_eq!(MountIsolation::Clone.to_string(), "clone");
    }
}
