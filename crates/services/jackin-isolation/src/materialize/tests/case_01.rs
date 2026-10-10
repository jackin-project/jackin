// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn materialize_api_accepts_path_label_rejected_as_config_stem() {
    // Dual-semantics boundary: ad-hoc workdir paths are legal labels but not
    // WorkspaceName config stems.
    let path = "/home/op/projects/adhoc-ws";
    WorkspaceName::parse(path).unwrap_err();
    let label = WorkspaceLabel::parse(path).expect("path label");
    assert_eq!(label.as_str(), path);
    // PreflightContext carries the label type, not a free &str.
    let _ctx = PreflightContext {
        workspace_label: label,
        force: false,
        interactive: false,
    };
}

#[tokio::test]
async fn materialized_mount_holds_isolation() {
    let m = MaterializedMount {
        bind_src: "/tmp/a".into(),
        dst: "/workspace/a".into(),
        readonly: false,
        isolation: MountIsolation::Worktree,
        worktree_aux: None,
    };
    assert_eq!(m.isolation, MountIsolation::Worktree);
}

#[tokio::test]
async fn worktree_path_uses_container_name_as_basename() {
    // Container name as the worktree's basename gives globally
    // unique admin entry names in `<host_repo>/.git/worktrees/`.
    // The `worktree/repo/` segments mark git's territory inside
    // the per-container state dir.
    let base = PathBuf::from("/data/jackin-x");
    assert_eq!(
        worktree_path_for(&base, "/workspace/jackin", "jackin-the-architect"),
        PathBuf::from("/data/jackin-x/git/worktree/repo/workspace/jackin/jackin-the-architect"),
    );
}

#[tokio::test]
async fn worktree_path_strips_trailing_slash_in_dst() {
    let base = PathBuf::from("/data/jackin-x");
    assert_eq!(
        worktree_path_for(&base, "/workspace/jackin/", "jackin-x"),
        PathBuf::from("/data/jackin-x/git/worktree/repo/workspace/jackin/jackin-x"),
    );
}

#[tokio::test]
async fn clone_path_uses_clone_repo_layout() {
    let base = PathBuf::from("/data/jackin-x");
    assert_eq!(
        clone_path_for(&base, "/workspace/jackin", "jackin-x"),
        PathBuf::from("/data/jackin-x/git/clone/repo/workspace/jackin/jackin-x"),
    );
}

#[tokio::test]
async fn container_host_git_path_mirrors_dst_under_jackin_host() {
    assert_eq!(
        container_host_git_path("/Users/donbeave/Projects/jackin-project/jackin"),
        "/jackin/host/Users/donbeave/Projects/jackin-project/jackin/.git",
        "host .git destination mirrors host topology under /jackin/host/, ends in .git",
    );
    assert_eq!(
        container_host_git_path("/workspace/jackin/"),
        "/jackin/host/workspace/jackin/.git",
        "trailing slash on dst is stripped",
    );
}

#[tokio::test]
async fn strip_userinfo_removes_pat_from_https_origin() {
    assert_eq!(
        strip_userinfo("https://x-access-token:ghp_xxx@github.com/owner/repo.git".into()),
        "https://github.com/owner/repo.git",
    );
    assert_eq!(
        strip_userinfo("https://oauth2:token@gitlab.example.com/team/repo".into()),
        "https://gitlab.example.com/team/repo",
    );
    assert_eq!(
        strip_userinfo("http://user:pass@example.com/path".into()),
        "http://example.com/path",
    );
}

#[tokio::test]
async fn strip_userinfo_passes_clean_urls_through_unchanged() {
    for url in [
        "https://github.com/owner/repo",
        "https://github.com/owner/repo.git",
        "git@github.com:owner/repo.git",
        "ssh://git@github.com/owner/repo",
        "/host/local/path",
    ] {
        assert_eq!(strip_userinfo(url.into()), url);
    }
}

#[tokio::test]
async fn strip_userinfo_handles_authority_only_urls() {
    // No path component after the authority — still strip userinfo.
    assert_eq!(
        strip_userinfo("https://user:pw@github.com".into()),
        "https://github.com",
    );
}

#[tokio::test]
async fn host_git_paths_disambiguate_per_mount_in_one_container() {
    // Two isolated mounts on different host repos in the same
    // container must land at distinct container paths so multi-mount
    // workspaces don't collide. With the admin entry living
    // natively inside the host `.git/` mount, this single check is
    // enough — admin disambiguation is inherited from the host
    // mount disambiguation.
    assert_ne!(
        container_host_git_path("/workspace/proj-a"),
        container_host_git_path("/workspace/proj-b"),
    );
}

#[tokio::test]
async fn write_git_overrides_writes_two_files_with_correct_content() {
    let cdir = tempfile::TempDir::new().unwrap();

    let aux = write_git_overrides(
        cdir.path(),
        "/workspace/jackin",
        "jackin-the-architect",
        "/host/jackin",
    )
    .unwrap();

    // Auxiliary mount metadata reflects the design doc topology:
    // /jackin/host/<dst-tree>/.git for the host repo's .git/ mount,
    // with both override files layered on top of that mount.
    assert_eq!(aux.host_git_dir, "/host/jackin/.git");
    assert_eq!(aux.host_git_target, "/jackin/host/workspace/jackin/.git");
    assert_eq!(aux.git_file_target, "/workspace/jackin/.git");
    // gitdir back-pointer override target lives natively at
    // worktrees/<container>/gitdir inside the host .git/ mount.
    assert_eq!(
        aux.gitdir_back_target,
        "/jackin/host/workspace/jackin/.git/worktrees/jackin-the-architect/gitdir",
    );

    // Override file contents.
    let git_file = std::fs::read_to_string(&aux.git_file_override).unwrap();
    assert_eq!(
        git_file, "gitdir: /jackin/host/workspace/jackin/.git/worktrees/jackin-the-architect\n",
        "git-file redirects gitdir to the admin entry inside the host .git/ mount",
    );
    let gitdir_back = std::fs::read_to_string(&aux.gitdir_back_override).unwrap();
    assert_eq!(gitdir_back, "/workspace/jackin/.git\n");

    // Override files live under git/overrides/<dst-tree>/ on host,
    // with filenames matching their docker mount destinations
    // (.git, gitdir). No commondir override — git's on-disk default
    // (`commondir = ../..`) resolves correctly because the admin
    // entry is in-place inside the host .git/ mount.
    assert!(
        aux.git_file_override
            .ends_with("/git/overrides/workspace/jackin/.git"),
        "got {}",
        aux.git_file_override
    );
    assert!(
        aux.gitdir_back_override
            .ends_with("/git/overrides/workspace/jackin/gitdir"),
        "got {}",
        aux.gitdir_back_override
    );
}

#[tokio::test]
async fn write_git_overrides_is_idempotent() {
    let cdir = tempfile::TempDir::new().unwrap();

    let first = write_git_overrides(
        cdir.path(),
        "/workspace/jackin",
        "jackin-the-architect",
        "/host/jackin",
    )
    .unwrap();
    let second = write_git_overrides(
        cdir.path(),
        "/workspace/jackin",
        "jackin-the-architect",
        "/host/jackin",
    )
    .unwrap();
    // Same paths, same content — re-running on a reused worktree is safe.
    assert_eq!(first, second);
}

#[tokio::test]
async fn worktree_config_skips_when_already_enabled() {
    let mut runner = fake_with_outputs(&["true\n"]);
    let newly = ensure_worktree_config_enabled(Path::new("/repo"), &mut runner)
        .await
        .unwrap();
    assert!(!newly);
    assert_eq!(runner.run_recorded.len(), 0);
}

#[tokio::test]
async fn worktree_config_enables_and_bumps_format_version_from_zero() {
    let mut runner = fake_with_outputs(&["", "0"]);
    let newly = ensure_worktree_config_enabled(Path::new("/repo"), &mut runner)
        .await
        .unwrap();
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

#[tokio::test]
async fn worktree_config_skips_format_bump_when_already_one() {
    let mut runner = fake_with_outputs(&["", "1"]);
    ensure_worktree_config_enabled(Path::new("/repo"), &mut runner)
        .await
        .unwrap();
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

#[tokio::test]
async fn preflight_rejects_readonly() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    let mut m = worktree_mount("/workspace/x", &dir.path().to_string_lossy());
    m.readonly = true;
    let mut runner = FakeRunner::default();
    let err = preflight_worktree(&m, &ctx(), &mut runner)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("cannot be readonly"));
}

#[tokio::test]
async fn preflight_rejects_sensitive_mount() {
    let home = directories::BaseDirs::new()
        .unwrap()
        .home_dir()
        .to_path_buf();
    let m = worktree_mount("/workspace/ssh", &home.join(".ssh").to_string_lossy());
    let mut runner = FakeRunner::default();
    let err = preflight_worktree(&m, &ctx(), &mut runner)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("sensitive"));
}

#[tokio::test]
async fn preflight_rejects_mid_rebase() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".git/rebase-merge")).unwrap();
    let m = worktree_mount("/workspace/x", &dir.path().to_string_lossy());
    let mut runner = FakeRunner::default();
    let err = preflight_worktree(&m, &ctx(), &mut runner)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("mid-rebase-merge"));
}

#[tokio::test]
async fn preflight_rejects_mid_merge() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    std::fs::write(dir.path().join(".git/MERGE_HEAD"), "x").unwrap();
    let m = worktree_mount("/workspace/x", &dir.path().to_string_lossy());
    let mut runner = FakeRunner::default();
    let err = preflight_worktree(&m, &ctx(), &mut runner)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("mid-MERGE_HEAD"));
}

#[tokio::test]
async fn preflight_rejects_mid_cherry_pick() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    std::fs::write(dir.path().join(".git/CHERRY_PICK_HEAD"), "x").unwrap();
    let m = worktree_mount("/workspace/x", &dir.path().to_string_lossy());
    let mut runner = FakeRunner::default();
    let err = preflight_worktree(&m, &ctx(), &mut runner)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("mid-CHERRY_PICK_HEAD"));
}

#[tokio::test]
async fn preflight_rejects_subdir_of_repo() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    let sub = dir.path().join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    let m = worktree_mount("/workspace/x", &sub.to_string_lossy());
    let mut runner = fake_with_outputs(&[&dir.path().to_string_lossy()]);
    let err = preflight_worktree(&m, &ctx(), &mut runner)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not its root"));
}

#[tokio::test]
async fn dirty_tree_rejected_non_interactive_no_force() {
    let repo = make_repo_root();
    let m = worktree_mount("/workspace/x", &repo.path().to_string_lossy());
    let mut runner = fake_with_repo_and_status(repo.path(), dirty_porcelain());
    let mut c = ctx();
    c.force = false;
    c.interactive = false;
    let err = preflight_worktree(&m, &c, &mut runner).await.unwrap_err();
    assert!(err.to_string().contains("dirty"));
    assert!(err.to_string().contains("--force"));
}

#[tokio::test]
async fn dirty_tree_passes_with_force_non_interactive() {
    let repo = make_repo_root();
    let m = worktree_mount("/workspace/x", &repo.path().to_string_lossy());
    let mut runner = fake_with_repo_and_status(repo.path(), dirty_porcelain());
    let mut c = ctx();
    c.force = true;
    preflight_worktree(&m, &c, &mut runner).await.unwrap();
}

#[tokio::test]
async fn clean_tree_passes() {
    let repo = make_repo_root();
    let m = worktree_mount("/workspace/x", &repo.path().to_string_lossy());
    let mut runner = fake_with_repo_and_status(repo.path(), ignored_only_porcelain());
    preflight_worktree(&m, &ctx(), &mut runner).await.unwrap();
}
