// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn cleanup_removes_only_owned_worktree_registration_and_exact_ref() {
    let repo = TempDir::new().unwrap();
    let container = TempDir::new().unwrap();
    let rec = rec_for(repo.path(), container.path());
    let sibling = repo.path().join(".git/worktrees/unrelated");
    std::fs::create_dir_all(&sibling).unwrap();
    std::fs::write(sibling.join("gitdir"), "/unrelated/.git\n").unwrap();
    std::fs::write(sibling.join("HEAD"), "ref: refs/heads/unrelated\n").unwrap();
    save(&rec, container.path());
    let mut runner = FakeRunner::with_capture_queue([tip_row(&rec), "files".into(), String::new()]);
    force_cleanup_isolated(&rec, container.path(), &mut runner)
        .await
        .unwrap();
    assert!(!Path::new(&rec.worktree_path).exists());
    assert!(
        !repo
            .path()
            .join(".git/worktrees/registered-with-git-suffix1")
            .exists()
    );
    assert!(sibling.exists());
    assert!(read_records(container.path()).unwrap().is_empty());
    assert!(runner.run_recorded.is_empty());
    assert!(
        !repo
            .path()
            .join(".git/refs/heads/jackin/scratch/jackin-x")
            .exists()
    );
    assert!(
        runner
            .run_options
            .iter()
            .all(|opts| opts.pinned_cwd.is_some())
    );
    assert!(
        !runner
            .recorded
            .iter()
            .any(|cmd| cmd.contains("worktree remove") || cmd.contains("worktree prune"))
    );
}

#[tokio::test]
async fn cleanup_is_idempotent_after_worktree_and_registration_are_absent() {
    let repo = TempDir::new().unwrap();
    let container = TempDir::new().unwrap();
    let rec = rec_for(repo.path(), container.path());
    std::fs::remove_dir_all(&rec.worktree_path).unwrap();
    std::fs::remove_dir_all(repo.path().join(".git/worktrees")).unwrap();
    save(&rec, container.path());
    let mut runner = FakeRunner::with_capture_queue([
        String::new(),
        "files".into(),
        "sha1".into(),
        String::new(),
    ]);
    force_cleanup_isolated(&rec, container.path(), &mut runner)
        .await
        .unwrap();
    assert!(runner.run_recorded.is_empty());
    assert!(read_records(container.path()).unwrap().is_empty());
}

#[tokio::test]
async fn missing_registration_does_not_authorize_branch_deletion() {
    let repo = TempDir::new().unwrap();
    let container = TempDir::new().unwrap();
    let rec = rec_for(repo.path(), container.path());
    std::fs::remove_dir_all(repo.path().join(".git/worktrees")).unwrap();
    save(&rec, container.path());
    let mut runner = FakeRunner::with_capture_queue([tip_row(&rec)]);
    let error = force_cleanup_isolated(&rec, container.path(), &mut runner)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("without matching"));
    assert!(runner.run_recorded.is_empty());
    assert!(Path::new(&rec.worktree_path).exists());
    assert_eq!(read_records(container.path()).unwrap().len(), 1);
}

#[tokio::test]
async fn inspection_failure_retains_every_resource_and_record() {
    let repo = TempDir::new().unwrap();
    let container = TempDir::new().unwrap();
    let rec = rec_for(repo.path(), container.path());
    save(&rec, container.path());
    let mut runner = FakeRunner {
        fail_on: vec!["for-each-ref".into()],
        ..Default::default()
    };
    let error = force_cleanup_isolated(&rec, container.path(), &mut runner)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("record retained"));
    assert!(runner.run_recorded.is_empty());
    assert!(Path::new(&rec.worktree_path).exists());
    assert!(
        repo.path()
            .join(".git/worktrees/registered-with-git-suffix1")
            .exists()
    );
    assert_eq!(read_records(container.path()).unwrap().len(), 1);
}

#[tokio::test]
async fn deletion_failure_retains_registration_for_retry() {
    let repo = TempDir::new().unwrap();
    let container = TempDir::new().unwrap();
    let rec = rec_for(repo.path(), container.path());
    save(&rec, container.path());
    let lock = repo.path().join(".git/packed-refs.lock");
    std::fs::write(&lock, "held-lock").unwrap();
    let mut runner = FakeRunner::with_capture_queue([tip_row(&rec), "files".into()]);
    let error = force_cleanup_isolated(&rec, container.path(), &mut runner)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("record retained"));
    assert!(
        repo.path()
            .join(".git/worktrees/registered-with-git-suffix1")
            .exists()
    );
    assert_eq!(read_records(container.path()).unwrap().len(), 1);
    assert!(Path::new(&rec.worktree_path).exists());
    std::fs::remove_file(&lock).unwrap();
    let mut retry = FakeRunner::with_capture_queue([tip_row(&rec), "files".into(), String::new()]);
    force_cleanup_isolated(&rec, container.path(), &mut retry)
        .await
        .unwrap();
    assert!(read_records(container.path()).unwrap().is_empty());
}

#[tokio::test]
async fn inconclusive_or_present_verification_retains_registration_and_record() {
    for verification in [
        "malformed\n".to_owned(),
        String::from("refs/heads/jackin/scratch/jackin-x\t") + TIP + "\t\tEND\n",
    ] {
        let repo = TempDir::new().unwrap();
        let container = TempDir::new().unwrap();
        let rec = rec_for(repo.path(), container.path());
        save(&rec, container.path());
        // Queue: initial scratch_tip (tip present), rev-parse
        // --show-ref-format ("files"; tip non-empty so no object-format
        // query), pre-deletion reinspect (tip still present), then the
        // post-deletion verification under test (malformed or present).
        let mut runner = FakeRunner::with_capture_queue([
            tip_row(&rec),
            "files".into(),
            tip_row(&rec),
            verification,
        ]);
        let error = force_cleanup_isolated(&rec, container.path(), &mut runner)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("cleanup journal retained"),
            "{error}"
        );
        assert!(
            repo.path()
                .join(".git/worktrees/registered-with-git-suffix1")
                .exists()
        );
        assert_eq!(read_records(container.path()).unwrap().len(), 1);
    }
}

#[tokio::test]
async fn missing_host_and_mismatched_branch_fail_before_deletion() {
    for missing_host in [false, true] {
        let repo = TempDir::new().unwrap();
        let container = TempDir::new().unwrap();
        let mut rec = rec_for(repo.path(), container.path());
        if missing_host {
            rec.original_src = repo.path().join("missing").to_string_lossy().into();
        } else {
            rec.scratch_branch = "main".into();
        }
        save(&rec, container.path());
        let mut runner = FakeRunner::default();
        assert!(
            force_cleanup_isolated(&rec, container.path(), &mut runner)
                .await
                .is_err()
        );
        assert!(runner.recorded.is_empty());
        assert!(Path::new(&rec.worktree_path).exists());
        assert_eq!(read_records(container.path()).unwrap().len(), 1);
    }
}

#[tokio::test]
async fn existing_host_refuses_outside_missing_outside_symlink_and_parent_segments_before_git() {
    for kind in ["outside", "missing", "symlink", "parent"] {
        let repo = TempDir::new().unwrap();
        let container = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let canary = outside.path().join("canary");
        std::fs::write(&canary, "keep").unwrap();
        let mut rec = rec_for(repo.path(), container.path());
        rec.worktree_path = match kind {
            "outside" => outside.path().to_path_buf(),
            "missing" => outside.path().join("absent"),
            "symlink" => {
                let link = container.path().join("link");
                std::os::unix::fs::symlink(outside.path(), &link).unwrap();
                link
            }
            "parent" => container
                .path()
                .join("isolated/../isolated/workspace/jackin"),
            _ => unreachable!(),
        }
        .to_string_lossy()
        .into();
        save(&rec, container.path());
        let mut runner = FakeRunner::default();
        assert!(
            force_cleanup_isolated(&rec, container.path(), &mut runner)
                .await
                .is_err(),
            "{kind}"
        );
        assert!(runner.recorded.is_empty(), "{kind}");
        assert_eq!(std::fs::read_to_string(&canary).unwrap(), "keep");
        assert_eq!(read_records(container.path()).unwrap().len(), 1);
    }
}

#[tokio::test]
async fn ambiguous_registration_refuses_cleanup() {
    let repo = TempDir::new().unwrap();
    let container = TempDir::new().unwrap();
    let rec = rec_for(repo.path(), container.path());
    let duplicate = repo.path().join(".git/worktrees/duplicate");
    std::fs::create_dir_all(&duplicate).unwrap();
    std::fs::write(
        duplicate.join("gitdir"),
        format!("{}/.git\n", rec.worktree_path),
    )
    .unwrap();
    std::fs::write(duplicate.join("commondir"), "../..\n").unwrap();
    std::fs::write(duplicate.join("HEAD"), "ref: refs/heads/feature\n").unwrap();
    std::fs::remove_dir_all(&rec.worktree_path).unwrap();
    save(&rec, container.path());
    let mut runner = FakeRunner::default();
    let error = force_cleanup_isolated(&rec, container.path(), &mut runner)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("ambiguous worktree registration"),
        "{error:#}"
    );
    assert!(runner.recorded.is_empty());

    assert_eq!(read_records(container.path()).unwrap().len(), 1);
}

#[tokio::test]
async fn clone_cleanup_unlinks_nested_symlinks_without_following_victim() {
    let repo = TempDir::new().unwrap();
    let container = TempDir::new().unwrap();
    let victim = TempDir::new().unwrap();
    let canary = victim.path().join("canary");
    std::fs::write(&canary, "keep").unwrap();
    let clone =
        crate::materialize::clone_path_for(container.path(), "/workspace/jackin", "jackin-x");
    std::fs::create_dir_all(&clone).unwrap();
    let rec = IsolationRecord {
        isolation: MountIsolation::Clone,
        worktree_path: clone.to_string_lossy().into(),
        ..rec_for(repo.path(), container.path())
    };
    std::os::unix::fs::symlink(victim.path(), Path::new(&rec.worktree_path).join("link")).unwrap();
    save(&rec, container.path());
    let mut runner = FakeRunner::default();
    force_cleanup_isolated(&rec, container.path(), &mut runner)
        .await
        .unwrap();
    assert_eq!(std::fs::read_to_string(&canary).unwrap(), "keep");
    assert!(runner.recorded.is_empty());
    assert!(read_records(container.path()).unwrap().is_empty());
}

#[tokio::test]
async fn purge_continues_independent_records_and_reports_failed_identity() {
    let repo = TempDir::new().unwrap();
    let container = TempDir::new().unwrap();
    let good = rec_for(repo.path(), container.path());
    let bad = IsolationRecord {
        scratch_branch: "main".into(),
        mount_dst: "/workspace/bad".into(),
        ..good.clone()
    };
    write_records(container.path(), &[bad, good]).unwrap();
    let mut runner = FakeRunner::with_capture_queue([
        String::new(),
        "files".into(),
        "sha1".into(),
        String::new(),
    ]);
    let error = purge_isolated_for_container(container.path(), &mut runner)
        .await
        .unwrap_err();
    assert!(error.to_string().contains('1'));
    let remaining = read_records(container.path()).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].mount_dst, "/workspace/bad");
}

#[tokio::test]
async fn verification_capture_error_after_deletion_retains_retry_authority() {
    let repo = TempDir::new().unwrap();
    let container = TempDir::new().unwrap();
    let rec = rec_for(repo.path(), container.path());
    save(&rec, container.path());
    let mut runner = VerifyFailureRunner {
        // Initial scratch_tip (tip present), rev-parse --show-ref-format,
        // pre-deletion reinspect (tip still present); the 4th call
        // (verify-deletion) is failed by the wrapper above.
        inner: FakeRunner::with_capture_queue([tip_row(&rec), "files".into(), tip_row(&rec)]),
        ..Default::default()
    };
    let error = force_cleanup_isolated(&rec, container.path(), &mut runner)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("cleanup journal retained"),
        "{error}"
    );
    assert!(runner.inner.run_recorded.is_empty());
    assert!(
        !repo
            .path()
            .join(".git/refs/heads/jackin/scratch/jackin-x")
            .exists()
    );
    assert!(
        repo.path()
            .join(".git/worktrees/registered-with-git-suffix1")
            .exists()
    );
    assert_eq!(read_records(container.path()).unwrap().len(), 1);
}

#[tokio::test]
async fn live_symbolic_scratch_and_sibling_checkout_refuse_before_deletion() {
    for symbolic in [false, true] {
        let repo = TempDir::new().unwrap();
        let container = TempDir::new().unwrap();
        let rec = rec_for(repo.path(), container.path());
        save(&rec, container.path());
        let mut runner = if symbolic {
            FakeRunner::with_capture_queue([format!(
                "refs/heads/{}\t{TIP}\trefs/heads/main\tEND\n",
                rec.scratch_branch
            )])
        } else {
            let sibling = repo.path().join(".git/worktrees/sibling");
            std::fs::create_dir_all(&sibling).unwrap();
            std::fs::write(sibling.join("gitdir"), "/sibling/.git\n").unwrap();
            std::fs::write(
                sibling.join("HEAD"),
                format!("ref: refs/heads/{}\n", rec.scratch_branch),
            )
            .unwrap();
            FakeRunner::default()
        };
        let error = force_cleanup_isolated(&rec, container.path(), &mut runner)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("record retained"));
        assert!(runner.run_recorded.is_empty());
        assert!(Path::new(&rec.worktree_path).exists());
        assert_eq!(read_records(container.path()).unwrap().len(), 1);
    }
}
