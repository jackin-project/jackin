// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const TIP: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

pub(super) fn rec_for(repo: &Path, container_dir: &Path) -> IsolationRecord {
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

pub(super) fn save(rec: &IsolationRecord, container: &Path) {
    write_records(container, std::slice::from_ref(rec)).unwrap();
}

pub(super) fn tip_row(rec: &IsolationRecord) -> String {
    let reference = Path::new(&rec.original_src)
        .join(".git/refs/heads")
        .join(&rec.scratch_branch);
    std::fs::create_dir_all(reference.parent().unwrap()).unwrap();
    std::fs::write(&reference, format!("{TIP}\n")).unwrap();
    format!("refs/heads/{}\t{TIP}\t\tEND\n", rec.scratch_branch)
}

#[derive(Default)]
pub(super) struct VerifyFailureRunner {
    pub(super) inner: FakeRunner,
    pub(super) captures: usize,
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

pub(super) async fn fixture_git(repo: &Path, args: &[&str]) -> String {
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

pub(super) async fn real_record(repo: &Path, container: &Path) -> IsolationRecord {
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
