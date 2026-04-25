use crate::docker::CommandRunner;
use crate::isolation::state::{IsolationRecord, remove_record};
use std::path::Path;

/// Force-delete an isolated worktree and its scratch branch.
/// Tolerates missing host repo / already-removed worktree (best-effort).
/// Removes the corresponding record from `isolation.json`.
pub fn force_cleanup_isolated(
    record: &IsolationRecord,
    container_state_dir: &Path,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let host_repo_exists = std::path::Path::new(&record.original_src).exists();

    if host_repo_exists {
        let _ = runner.run(
            "git",
            &[
                "-C",
                &record.original_src,
                "worktree",
                "remove",
                "--force",
                &record.worktree_path,
            ],
            None,
            &crate::docker::RunOptions {
                quiet: true,
                ..Default::default()
            },
        );
        let _ = runner.run(
            "git",
            &[
                "-C",
                &record.original_src,
                "branch",
                "-D",
                &record.scratch_branch,
            ],
            None,
            &crate::docker::RunOptions {
                quiet: true,
                ..Default::default()
            },
        );
    }

    // Belt-and-suspenders: nuke the worktree directory if git left anything.
    let wt = std::path::Path::new(&record.worktree_path);
    if wt.exists() {
        let _ = std::fs::remove_dir_all(wt);
    }

    remove_record(container_state_dir, &record.mount_dst)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isolation::MountIsolation;
    use crate::isolation::state::{CleanupStatus, read_records, write_records};
    use crate::runtime::test_support::FakeRunner;
    use tempfile::TempDir;

    fn rec_for(repo: &Path, container_dir: &Path) -> IsolationRecord {
        let wt = container_dir.join("isolated/workspace/jackin");
        std::fs::create_dir_all(&wt).unwrap();
        IsolationRecord {
            workspace: "jackin".into(),
            mount_dst: "/workspace/jackin".into(),
            original_src: repo.to_string_lossy().into(),
            isolation: MountIsolation::Worktree,
            worktree_path: wt.to_string_lossy().into(),
            scratch_branch: "jackin/scratch/x".into(),
            base_commit: "abc".into(),
            selector_key: "x".into(),
            container_name: "jackin-x".into(),
            cleanup_status: CleanupStatus::Active,
        }
    }

    #[test]
    fn force_cleanup_runs_git_and_removes_record() {
        let repo_dir = TempDir::new().unwrap();
        let container_dir = TempDir::new().unwrap();
        let rec = rec_for(repo_dir.path(), container_dir.path());
        write_records(container_dir.path(), std::slice::from_ref(&rec)).unwrap();

        let mut runner = FakeRunner::default();
        force_cleanup_isolated(&rec, container_dir.path(), &mut runner).unwrap();

        assert!(
            runner
                .run_recorded
                .iter()
                .any(|c| c.contains("worktree remove --force"))
        );
        assert!(
            runner
                .run_recorded
                .iter()
                .any(|c| c.contains("branch -D jackin/scratch/x"))
        );
        assert!(read_records(container_dir.path()).unwrap().is_empty());
    }

    #[test]
    fn force_cleanup_tolerates_missing_host_repo() {
        let container_dir = TempDir::new().unwrap();
        let rec = IsolationRecord {
            original_src: "/nonexistent/path".into(),
            ..rec_for(std::path::Path::new("/tmp"), container_dir.path())
        };
        write_records(container_dir.path(), std::slice::from_ref(&rec)).unwrap();

        let mut runner = FakeRunner::default();
        force_cleanup_isolated(&rec, container_dir.path(), &mut runner).unwrap();
        assert!(
            runner.run_recorded.is_empty(),
            "should skip git when src missing"
        );
        assert!(read_records(container_dir.path()).unwrap().is_empty());
    }

    #[test]
    fn force_cleanup_is_idempotent_when_worktree_already_gone() {
        let repo_dir = TempDir::new().unwrap();
        let container_dir = TempDir::new().unwrap();
        let mut rec = rec_for(repo_dir.path(), container_dir.path());
        write_records(container_dir.path(), std::slice::from_ref(&rec)).unwrap();
        // Pre-delete the worktree dir.
        std::fs::remove_dir_all(&rec.worktree_path).unwrap();
        rec.worktree_path = format!("{}-gone", rec.worktree_path);

        let mut runner = FakeRunner::default();
        force_cleanup_isolated(&rec, container_dir.path(), &mut runner).unwrap();
        assert!(read_records(container_dir.path()).unwrap().is_empty());
    }
}
