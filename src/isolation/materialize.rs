use crate::docker::CommandRunner;
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

/// Enable `extensions.worktreeConfig` on a host repo if not already set.
/// Returns Ok(true) when newly enabled (caller may print a notice),
/// Ok(false) when already enabled.
pub fn ensure_worktree_config_enabled(
    repo: &Path,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<bool> {
    let current = runner
        .capture(
            "git",
            &[
                "-C",
                &repo.to_string_lossy(),
                "config",
                "--get",
                "extensions.worktreeConfig",
            ],
            None,
        )
        .unwrap_or_default();
    if current.trim() == "true" {
        return Ok(false);
    }
    let format_version = runner
        .capture(
            "git",
            &[
                "-C",
                &repo.to_string_lossy(),
                "config",
                "--get",
                "core.repositoryformatversion",
            ],
            None,
        )
        .unwrap_or_default();
    if format_version.trim() == "0" || format_version.trim().is_empty() {
        runner.run(
            "git",
            &[
                "-C",
                &repo.to_string_lossy(),
                "config",
                "core.repositoryformatversion",
                "1",
            ],
            None,
            &crate::docker::RunOptions::default(),
        )?;
    }
    runner.run(
        "git",
        &[
            "-C",
            &repo.to_string_lossy(),
            "config",
            "extensions.worktreeConfig",
            "true",
        ],
        None,
        &crate::docker::RunOptions::default(),
    )?;
    Ok(true)
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

    use crate::runtime::test_support::FakeRunner;
    use std::collections::VecDeque;

    fn fake_with_outputs(outputs: &[&str]) -> FakeRunner {
        FakeRunner {
            capture_queue: VecDeque::from(
                outputs.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ),
            ..Default::default()
        }
    }

    #[test]
    fn worktree_config_skips_when_already_enabled() {
        let mut runner = fake_with_outputs(&["true\n"]);
        let newly = ensure_worktree_config_enabled(Path::new("/repo"), &mut runner).unwrap();
        assert!(!newly);
        assert_eq!(runner.run_recorded.len(), 0);
    }

    #[test]
    fn worktree_config_enables_and_bumps_format_version_from_zero() {
        let mut runner = fake_with_outputs(&["", "0"]);
        let newly = ensure_worktree_config_enabled(Path::new("/repo"), &mut runner).unwrap();
        assert!(newly);
        assert!(
            runner
                .run_recorded
                .iter()
                .any(|c| c.contains("core.repositoryformatversion 1"))
        );
        assert!(
            runner
                .run_recorded
                .iter()
                .any(|c| c.contains("extensions.worktreeConfig true"))
        );
    }

    #[test]
    fn worktree_config_skips_format_bump_when_already_one() {
        let mut runner = fake_with_outputs(&["", "1"]);
        ensure_worktree_config_enabled(Path::new("/repo"), &mut runner).unwrap();
        assert!(
            !runner
                .run_recorded
                .iter()
                .any(|c| c.contains("core.repositoryformatversion"))
        );
        assert!(
            runner
                .run_recorded
                .iter()
                .any(|c| c.contains("extensions.worktreeConfig true"))
        );
    }
}
