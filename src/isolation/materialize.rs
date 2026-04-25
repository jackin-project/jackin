use crate::docker::CommandRunner;
use crate::isolation::MountIsolation;
use anyhow::Context;
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

use crate::workspace::MountConfig;

#[derive(Debug, Clone)]
pub struct PreflightContext {
    pub workspace_name: String,
    pub force: bool,
    pub interactive: bool,
}

/// Validation that must pass before `git worktree add`. Layout validation
/// (parent/child rejection) happens earlier at config-validation time;
/// this is per-mount.
pub fn preflight_worktree(
    mount: &MountConfig,
    ctx: &PreflightContext,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    // readonly is incompatible with worktree mode.
    anyhow::ensure!(
        !mount.readonly,
        "isolated mount `{}` cannot be readonly (isolation = worktree)",
        mount.dst
    );

    // Sensitive mount overlap.
    let sensitives = crate::workspace::find_sensitive_mounts(std::slice::from_ref(mount));
    if let Some(s) = sensitives.first() {
        anyhow::bail!(
            "isolated mount `{}` overlaps sensitive path `{}` ({}) (isolation = worktree)",
            mount.dst,
            s.src,
            s.reason
        );
    }

    let src = std::path::Path::new(&mount.src);

    // Mid-rebase / merge / cherry-pick guard.
    for marker in &[
        "rebase-merge",
        "rebase-apply",
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
    ] {
        if src.join(".git").join(marker).exists() {
            anyhow::bail!(
                "isolated mount `{}`: host repo `{}` is mid-{}; resolve before launching",
                mount.dst,
                mount.src,
                marker
            );
        }
    }

    // src must be a git repo *root* — toplevel must equal src.
    let toplevel = runner
        .capture(
            "git",
            &["-C", &mount.src, "rev-parse", "--show-toplevel"],
            None,
        )
        .with_context(|| {
            format!(
                "isolated mount `{}`: git rev-parse --show-toplevel",
                mount.dst
            )
        })?;
    let toplevel = toplevel.trim();
    let src_canon =
        std::fs::canonicalize(src).with_context(|| format!("canonicalize {}", mount.src))?;
    let top_canon =
        std::fs::canonicalize(toplevel).with_context(|| format!("canonicalize {toplevel}"))?;
    anyhow::ensure!(
        src_canon == top_canon,
        "isolated mount `{}`: src `{}` is inside repo `{}` but not its root",
        mount.dst,
        mount.src,
        toplevel
    );

    // Dirty tree check (separate test in 4.5).
    check_dirty_tree(mount, ctx, runner)?;

    Ok(())
}

#[allow(clippy::missing_const_for_fn, clippy::unnecessary_wraps)]
fn check_dirty_tree(
    _mount: &MountConfig,
    _ctx: &PreflightContext,
    _runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    Ok(()) // implemented in Task 4.5
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

    use crate::workspace::MountConfig;

    fn ctx() -> PreflightContext {
        PreflightContext {
            workspace_name: "jackin".into(),
            force: false,
            interactive: false,
        }
    }

    fn worktree_mount(dst: &str, src: &str) -> MountConfig {
        MountConfig {
            src: src.into(),
            dst: dst.into(),
            readonly: false,
            isolation: MountIsolation::Worktree,
        }
    }

    #[test]
    fn preflight_rejects_readonly() {
        let mut m = worktree_mount("/workspace/x", "/tmp/x");
        m.readonly = true;
        let mut runner = FakeRunner::default();
        let err = preflight_worktree(&m, &ctx(), &mut runner).unwrap_err();
        assert!(err.to_string().contains("cannot be readonly"));
    }

    #[test]
    fn preflight_rejects_sensitive_mount() {
        let home = directories::BaseDirs::new()
            .unwrap()
            .home_dir()
            .to_path_buf();
        let m = worktree_mount("/workspace/ssh", &home.join(".ssh").to_string_lossy());
        let mut runner = FakeRunner::default();
        let err = preflight_worktree(&m, &ctx(), &mut runner).unwrap_err();
        assert!(err.to_string().contains("sensitive"));
    }

    #[test]
    fn preflight_rejects_mid_rebase() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".git/rebase-merge")).unwrap();
        let m = worktree_mount("/workspace/x", &dir.path().to_string_lossy());
        let mut runner = FakeRunner::default();
        let err = preflight_worktree(&m, &ctx(), &mut runner).unwrap_err();
        assert!(err.to_string().contains("mid-rebase-merge"));
    }

    #[test]
    fn preflight_rejects_mid_merge() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git/MERGE_HEAD"), "x").unwrap();
        let m = worktree_mount("/workspace/x", &dir.path().to_string_lossy());
        let mut runner = FakeRunner::default();
        let err = preflight_worktree(&m, &ctx(), &mut runner).unwrap_err();
        assert!(err.to_string().contains("mid-MERGE_HEAD"));
    }

    #[test]
    fn preflight_rejects_mid_cherry_pick() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git/CHERRY_PICK_HEAD"), "x").unwrap();
        let m = worktree_mount("/workspace/x", &dir.path().to_string_lossy());
        let mut runner = FakeRunner::default();
        let err = preflight_worktree(&m, &ctx(), &mut runner).unwrap_err();
        assert!(err.to_string().contains("mid-CHERRY_PICK_HEAD"));
    }

    #[test]
    fn preflight_rejects_subdir_of_repo() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let m = worktree_mount("/workspace/x", &sub.to_string_lossy());
        let mut runner = fake_with_outputs(&[&dir.path().to_string_lossy()]);
        let err = preflight_worktree(&m, &ctx(), &mut runner).unwrap_err();
        assert!(err.to_string().contains("not its root"));
    }
}
