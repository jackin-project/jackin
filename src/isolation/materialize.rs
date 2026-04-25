use crate::isolation::MountIsolation;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedWorkspace {
    pub workdir: String,
    pub mounts: Vec<MaterializedMount>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedMount {
    pub bind_src: String,
    pub dst: String,
    pub readonly: bool,
    pub isolation: MountIsolation,
}

/// Compute the host-side worktree path for an isolated mount.
/// Strips leading and trailing `/` from `dst` so the path is relative
/// when joined under `<container_state_dir>/isolated/`.
pub fn worktree_path_for(container_state_dir: &Path, dst: &str) -> PathBuf {
    let rel = dst.trim_matches('/');
    container_state_dir.join("isolated").join(rel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn materialized_mount_holds_isolation() {
        let m = MaterializedMount {
            bind_src: "/tmp/a".into(),
            dst: "/workspace/a".into(),
            readonly: false,
            isolation: MountIsolation::Worktree,
        };
        assert_eq!(m.isolation, MountIsolation::Worktree);
    }

    #[test]
    fn worktree_path_strips_leading_slash() {
        let base = PathBuf::from("/data/jackin-x");
        assert_eq!(
            worktree_path_for(&base, "/workspace/jackin"),
            PathBuf::from("/data/jackin-x/isolated/workspace/jackin")
        );
    }

    #[test]
    fn worktree_path_strips_trailing_slash() {
        let base = PathBuf::from("/data/jackin-x");
        assert_eq!(
            worktree_path_for(&base, "/workspace/jackin/"),
            PathBuf::from("/data/jackin-x/isolated/workspace/jackin")
        );
    }
}
