// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Cleanup authority, positive verification, and partial-failure regressions.
use super::*;
use crate::MountIsolation;
use crate::state::{CleanupStatus, read_records, write_records};
use jackin_test_support::FakeRunner;
use tempfile::TempDir;

const TIP: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn rec_for(repo: &Path, container_dir: &Path) -> IsolationRecord {
    let wt = crate::materialize::worktree_path_for(container_dir, "/workspace/jackin", "jackin-x");
    std::fs::create_dir_all(&wt).unwrap();
    let admin = repo.join(".git/worktrees/registered-with-git-suffix1");
    std::fs::create_dir_all(&admin).unwrap();
    std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::write(wt.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
    std::fs::write(
        admin.join("gitdir"),
        format!("{}\n", wt.join(".git").display()),
    )
    .unwrap();
    std::fs::write(admin.join("commondir"), "../..\n").unwrap();
    std::fs::write(
        admin.join("HEAD"),
        "ref: refs/heads/jackin/scratch/jackin-x\n",
    )
    .unwrap();
    IsolationRecord {
        workspace_name: Some(jackin_core::WorkspaceName::parse("jackin").unwrap()),
        mount_dst: "/workspace/jackin".into(),
        original_src: repo.to_string_lossy().into(),
        isolation: MountIsolation::Worktree,
        worktree_path: wt.to_string_lossy().into(),
        scratch_branch: "jackin/scratch/jackin-x".into(),
        base_commit: TIP.into(),
        selector_key: "x".into(),
        container_name: "jackin-x".into(),
        cleanup_status: CleanupStatus::Active,
    }
}

fn save(rec: &IsolationRecord, container: &Path) {
    write_records(container, std::slice::from_ref(rec)).unwrap();
}

fn tip_row(rec: &IsolationRecord) -> String {
    let reference = Path::new(&rec.original_src)
        .join(".git/refs/heads")
        .join(&rec.scratch_branch);
    std::fs::create_dir_all(reference.parent().unwrap()).unwrap();
    std::fs::write(&reference, format!("{TIP}\n")).unwrap();
    format!("refs/heads/{}\t{TIP}\t\tEND\n", rec.scratch_branch)
}

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

#[derive(Default)]
struct VerifyFailureRunner {
    inner: FakeRunner,
    captures: usize,
}
impl CommandRunner for VerifyFailureRunner {
    async fn run(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &jackin_core::RunOptions,
    ) -> anyhow::Result<()> {
        self.inner.run(program, args, cwd, opts).await
    }
    async fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.inner.capture(program, args, cwd).await
    }
    async fn capture_with_options(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &jackin_core::RunOptions,
    ) -> anyhow::Result<String> {
        self.captures += 1;
        // Production issues 4 capture_with_options calls before the
        // post-deletion verification: initial scratch_tip, rev-parse
        // --show-ref-format, pre-deletion reinspect, then the
        // verify-deletion scratch_tip. Fail that 4th call so the
        // reference is already deleted when the error strikes.
        if self.captures == 4 {
            anyhow::bail!("verification capture failed");
        }
        self.inner
            .capture_with_options(program, args, cwd, opts)
            .await
    }
    async fn capture_secret(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.inner.capture_secret(program, args, cwd).await
    }
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

async fn fixture_git(repo: &Path, args: &[&str]) -> String {
    let output = tokio::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

async fn real_record(repo: &Path, container: &Path) -> IsolationRecord {
    fixture_git(repo, &["init", "--initial-branch=main"]).await;
    fixture_git(
        repo,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "fixture",
        ],
    )
    .await;
    let worktree =
        crate::materialize::worktree_path_for(container, "/workspace/jackin", "jackin-x");
    let base_commit = fixture_git(repo, &["rev-parse", "HEAD"]).await;
    fixture_git(
        repo,
        &[
            "worktree",
            "add",
            "-b",
            "jackin/scratch/jackin-x",
            worktree.to_str().unwrap(),
        ],
    )
    .await;
    IsolationRecord {
        workspace_name: Some(jackin_core::WorkspaceName::parse("jackin").unwrap()),
        mount_dst: "/workspace/jackin".into(),
        original_src: repo.to_string_lossy().into(),
        isolation: MountIsolation::Worktree,
        worktree_path: worktree.to_string_lossy().into(),
        scratch_branch: "jackin/scratch/jackin-x".into(),
        base_commit,
        selector_key: "x".into(),
        container_name: "jackin-x".into(),
        cleanup_status: CleanupStatus::Active,
    }
}

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
