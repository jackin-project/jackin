// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn real_git_cleanup_removes_owned_registration_and_preserves_host_head() {
    let repo = TempDir::new().unwrap();
    let container = TempDir::new().unwrap();
    let rec = real_record(repo.path(), container.path()).await;
    save(&rec, container.path());
    let mut runner = jackin_docker::ShellRunner::default();
    force_cleanup_isolated(&rec, container.path(), &mut runner)
        .await
        .unwrap();
    assert!(!Path::new(&rec.worktree_path).exists());
    assert_eq!(
        fixture_git(repo.path(), &["rev-parse", "HEAD"]).await,
        rec.base_commit
    );
    assert_eq!(
        fixture_git(
            repo.path(),
            &[
                "for-each-ref",
                "--format=%(refname)",
                "refs/heads/jackin/scratch/jackin-x"
            ]
        )
        .await,
        ""
    );
    assert!(
        !fixture_git(repo.path(), &["worktree", "list", "--porcelain"])
            .await
            .contains(&rec.worktree_path)
    );
    assert!(read_records(container.path()).unwrap().is_empty());
}

#[tokio::test]
async fn real_git_gone_upstream_is_clean_without_remote_reachability() {
    // Policy: `[gone]` upstream means merged-and-pruned; assess treats the
    // worktree as clean without any remote-reachability proof. Pinned by
    // jackin-core `upstream_gone_is_treated_as_merged_clean`.
    let repo = TempDir::new().unwrap();
    let container = TempDir::new().unwrap();
    let rec = real_record(repo.path(), container.path()).await;
    let wt = Path::new(&rec.worktree_path);
    fixture_git(
        wt,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "unmerged",
        ],
    )
    .await;
    fixture_git(
        repo.path(),
        &["remote", "add", "origin", "/nonexistent-fixture-remote"],
    )
    .await;
    fixture_git(
        repo.path(),
        &["config", "branch.jackin/scratch/jackin-x.remote", "origin"],
    )
    .await;
    fixture_git(
        repo.path(),
        &[
            "config",
            "branch.jackin/scratch/jackin-x.merge",
            "refs/heads/deleted",
        ],
    )
    .await;
    let mut runner = jackin_docker::ShellRunner::default();
    assert_eq!(
        jackin_core::assess_worktree(&rec.worktree_path, &rec.base_commit, &mut runner, |_| {})
            .await
            .unwrap(),
        jackin_core::WorktreeState::Clean
    );
    assert!(wt.exists());
}

#[tokio::test]
async fn clone_wrong_mode_and_contained_parent_targets_keep_canaries() {
    for parent in [false, true] {
        let repo = TempDir::new().unwrap();
        let container = TempDir::new().unwrap();
        let original = rec_for(repo.path(), container.path());
        let target = if parent {
            container.path().join("git")
        } else {
            std::path::PathBuf::from(&original.worktree_path)
        };
        let canary = target.join("canary");
        std::fs::write(&canary, "keep").unwrap();
        let rec = IsolationRecord {
            isolation: MountIsolation::Clone,
            worktree_path: target.to_string_lossy().into(),
            ..original
        };
        save(&rec, container.path());
        let mut runner = FakeRunner::default();
        let error = force_cleanup_isolated(&rec, container.path(), &mut runner)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("clone path does not match"));
        assert_eq!(std::fs::read_to_string(&canary).unwrap(), "keep");
        assert!(runner.recorded.is_empty());
        assert_eq!(read_records(container.path()).unwrap().len(), 1);
    }
}

#[tokio::test]
async fn malformed_or_missing_sibling_head_preserves_cleanup_authority() {
    for missing in [false, true] {
        let repo = TempDir::new().unwrap();
        let container = TempDir::new().unwrap();
        let rec = rec_for(repo.path(), container.path());
        let sibling = repo.path().join(".git/worktrees/sibling");
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::write(sibling.join("gitdir"), "/sibling/.git\n").unwrap();
        if !missing {
            std::fs::write(sibling.join("HEAD"), "invalid\n").unwrap();
        }
        save(&rec, container.path());
        let mut runner = FakeRunner::default();
        let error = force_cleanup_isolated(&rec, container.path(), &mut runner)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("record retained"));
        assert!(runner.recorded.is_empty());
        assert!(Path::new(&rec.worktree_path).exists());
    }
}

#[tokio::test]
async fn real_git_sibling_and_indirect_host_checkout_remain_resolvable() {
    for indirect in [false, true] {
        let repo = TempDir::new().unwrap();
        let container = TempDir::new().unwrap();
        let sibling_root = TempDir::new().unwrap();
        let rec = real_record(repo.path(), container.path()).await;
        let sibling = sibling_root.path().join("sibling");
        if indirect {
            fixture_git(
                repo.path(),
                &[
                    "symbolic-ref",
                    "refs/heads/alias",
                    "refs/heads/jackin/scratch/jackin-x",
                ],
            )
            .await;
            fixture_git(repo.path(), &["symbolic-ref", "HEAD", "refs/heads/alias"]).await;
        } else {
            fixture_git(
                repo.path(),
                &[
                    "worktree",
                    "add",
                    "--force",
                    sibling.to_str().unwrap(),
                    "jackin/scratch/jackin-x",
                ],
            )
            .await;
        }
        save(&rec, container.path());
        let mut runner = jackin_docker::ShellRunner::default();
        let error = force_cleanup_isolated(&rec, container.path(), &mut runner)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("record retained"));
        assert!(Path::new(&rec.worktree_path).exists());
        assert_eq!(
            fixture_git(
                if indirect { repo.path() } else { &sibling },
                &["rev-parse", "HEAD"]
            )
            .await,
            rec.base_commit
        );
        assert_eq!(read_records(container.path()).unwrap().len(), 1);
    }
}

#[tokio::test]
async fn real_git_descendant_ref_directory_symlink_preserves_external_canary() {
    let repo = TempDir::new().unwrap();
    let container = TempDir::new().unwrap();
    let external = TempDir::new().unwrap();
    let rec = real_record(repo.path(), container.path()).await;
    let ref_parent = repo.path().join(".git/refs/heads/jackin/scratch");
    let saved = external.path().join("scratch");
    std::fs::rename(&ref_parent, &saved).unwrap();
    std::os::unix::fs::symlink(&saved, &ref_parent).unwrap();
    let canary = saved.join("jackin-x");
    let before = std::fs::read(&canary).unwrap();
    save(&rec, container.path());
    let mut runner = jackin_docker::ShellRunner::default();
    let error = force_cleanup_isolated(&rec, container.path(), &mut runner)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("record retained"));
    assert_eq!(std::fs::read(&canary).unwrap(), before);
    assert!(Path::new(&rec.worktree_path).exists());
    assert_eq!(read_records(container.path()).unwrap().len(), 1);
}
