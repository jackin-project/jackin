use crate::debug_log;
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
    debug_log!(
        "isolation",
        "force_cleanup_isolated: container={c} mount={d} branch={b} worktree={w} host_repo_exists={exists}",
        c = record.container_name,
        d = record.mount_dst,
        b = record.scratch_branch,
        w = record.worktree_path,
        exists = host_repo_exists,
    );

    if host_repo_exists {
        debug_log!(
            "isolation",
            "git -C {src} worktree remove --force {wt}",
            src = record.original_src,
            wt = record.worktree_path,
        );
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
        debug_log!(
            "isolation",
            "git -C {src} branch -D {branch}",
            src = record.original_src,
            branch = record.scratch_branch,
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
    } else {
        debug_log!(
            "isolation",
            "skipping git cleanup: host repo {src} no longer exists",
            src = record.original_src,
        );
    }

    // Belt-and-suspenders: nuke the worktree directory if git left anything.
    let wt = std::path::Path::new(&record.worktree_path);
    if wt.exists() {
        debug_log!(
            "isolation",
            "fallback rm -rf {wt} (git did not remove it)",
            wt = record.worktree_path,
        );
        let _ = std::fs::remove_dir_all(wt);
    }

    remove_record(container_state_dir, &record.mount_dst)?;
    Ok(())
}

/// Force-cleanup every record in a container's isolation.json. Used by purge.
pub fn purge_isolated_for_container(
    container_state_dir: &Path,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let records = crate::isolation::state::read_records(container_state_dir)?;
    debug_log!(
        "isolation",
        "purge_isolated_for_container: {n} record(s) under {dir}",
        n = records.len(),
        dir = container_state_dir.display(),
    );
    for rec in records {
        if let Err(e) = force_cleanup_isolated(&rec, container_state_dir, runner) {
            eprintln!(
                "[jackin] warning: failed to clean up isolated mount `{}`: {e}",
                rec.mount_dst
            );
        }
    }
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

    #[test]
    fn purge_isolated_for_container_runs_force_cleanup_for_each_record() {
        let repo_dir = TempDir::new().unwrap();
        let container_dir = TempDir::new().unwrap();
        let r1 = rec_for(repo_dir.path(), container_dir.path());
        let mut r2 = rec_for(repo_dir.path(), container_dir.path());
        r2.mount_dst = "/workspace/docs".into();
        let wt2 = container_dir.path().join("isolated/workspace/docs");
        std::fs::create_dir_all(&wt2).unwrap();
        r2.worktree_path = wt2.to_string_lossy().into();
        r2.scratch_branch = "jackin/scratch/x-2".into();
        let records = vec![r1.clone(), r2.clone()];
        write_records(container_dir.path(), &records).unwrap();

        let mut runner = FakeRunner::default();
        purge_isolated_for_container(container_dir.path(), &mut runner).unwrap();

        let removes = runner
            .run_recorded
            .iter()
            .filter(|c| c.contains("worktree remove --force"))
            .count();
        let branches = runner
            .run_recorded
            .iter()
            .filter(|c| c.contains("branch -D"))
            .count();
        assert_eq!(removes, 2);
        assert_eq!(branches, 2);
        assert!(read_records(container_dir.path()).unwrap().is_empty());
    }

    #[test]
    fn purge_isolated_is_noop_when_no_records() {
        let container_dir = TempDir::new().unwrap();
        let mut runner = FakeRunner::default();
        purge_isolated_for_container(container_dir.path(), &mut runner).unwrap();
        assert!(runner.run_recorded.is_empty());
    }
}
