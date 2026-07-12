//! Tests for `materialize`.
use super::*;
use jackin_core::{WorkspaceLabel, WorkspaceName};
use std::path::PathBuf;

#[test]
fn materialize_api_accepts_path_label_rejected_as_config_stem() {
    // Dual-semantics boundary: ad-hoc workdir paths are legal labels but not
    // WorkspaceName config stems.
    let path = "/home/op/projects/adhoc-ws";
    assert!(WorkspaceName::parse(path).is_err());
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

use jackin_test_support::FakeRunner;
use std::collections::VecDeque;

fn fake_with_outputs(outputs: &[&str]) -> FakeRunner {
    FakeRunner {
        capture_queue: VecDeque::from(outputs.iter().map(ToString::to_string).collect::<Vec<_>>()),
        ..Default::default()
    }
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

use jackin_config::MountConfig;

fn ctx() -> PreflightContext {
    PreflightContext {
        workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
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

#[tokio::test]
async fn preflight_rejects_readonly() {
    let mut m = worktree_mount("/workspace/x", "/tmp/x");
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

fn dirty_porcelain() -> &'static str {
    " M src/foo.rs\n?? new.rs\n"
}

fn ignored_only_porcelain() -> &'static str {
    ""
}

fn make_repo_root() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".git")).unwrap();
    dir
}

fn fake_with_repo_and_status(repo: &Path, status: &str) -> FakeRunner {
    // Capture queue order: rev-parse --show-toplevel, status --porcelain
    fake_with_outputs(&[&repo.to_string_lossy(), status])
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

use crate::state::{CleanupStatus, read_records};
use jackin_config::ResolvedWorkspace;

fn resolved_with_one_isolated(repo: &Path, dst: &str) -> ResolvedWorkspace {
    ResolvedWorkspace {
        name: String::new(),
        label: "jackin".into(),
        workdir: dst.into(),
        mounts: vec![MountConfig {
            src: repo.to_string_lossy().into(),
            dst: dst.into(),
            readonly: false,
            isolation: MountIsolation::Worktree,
        }],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
    }
}

fn resolved_with_one_clone(repo: &Path, dst: &str) -> ResolvedWorkspace {
    ResolvedWorkspace {
        name: String::new(),
        label: "jackin".into(),
        workdir: dst.into(),
        mounts: vec![MountConfig {
            src: repo.to_string_lossy().into(),
            dst: dst.into(),
            readonly: false,
            isolation: MountIsolation::Clone,
        }],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
    }
}

#[tokio::test]
async fn first_materialization_runs_worktree_add_and_writes_record() {
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-the-architect");
    std::fs::create_dir_all(&container_dir).unwrap();

    let resolved = resolved_with_one_isolated(repo.path(), "/workspace/jackin");
    // capture queue (in order materialize_workspace will request):
    //   preflight: rev-parse --show-toplevel
    //   preflight: status --porcelain
    //   ensure_worktree_config: extensions.worktreeConfig --get
    //   ensure_worktree_config: core.repositoryformatversion --get
    //   rev-parse HEAD
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(), // rev-parse --show-toplevel (preflight)
        "",                             // status --porcelain (clean)
        "",                             // extensions.worktreeConfig --get (not enabled)
        "0",                            // core.repositoryformatversion --get
        "deadbeef\n",                   // rev-parse HEAD
    ]);

    let mat = materialize_workspace(
        &resolved,
        &container_dir,
        "the-architect",
        "jackin-the-architect",
        &WorkspaceLabel::parse("jackin").unwrap(),
        &PreflightContext {
            workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(mat.mounts.len(), 1);
    let m = &mat.mounts[0];
    assert!(
        m.bind_src
            .contains("/git/worktree/repo/workspace/jackin/jackin-the-architect"),
        "worktree subdir basename = container name, under git/worktree/repo/<dst-tree>/; got {}",
        m.bind_src
    );
    assert_eq!(m.dst, "/workspace/jackin");
    assert_eq!(m.isolation, MountIsolation::Worktree);

    // git worktree add should have been invoked.
    assert!(
        runner
            .run_recorded
            .iter()
            .any(|c| c.contains("worktree add"))
    );

    // record persisted; branch follows Model B (container name verbatim).
    let recs = read_records(&container_dir).unwrap();
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0].cleanup_status, CleanupStatus::Active);
    assert_eq!(recs[0].base_commit, "deadbeef");
    assert_eq!(
        recs[0].scratch_branch,
        "jackin/scratch/jackin-the-architect"
    );
}

#[tokio::test]
async fn shared_mounts_pass_through_unchanged() {
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();
    let resolved = ResolvedWorkspace {
        name: String::new(),
        label: "jackin".into(),
        workdir: "/workspace/x".into(),
        mounts: vec![MountConfig {
            src: "/tmp/cache".into(),
            dst: "/workspace/cache".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
    };
    let mut runner = FakeRunner::default();
    let mat = materialize_workspace(
        &resolved,
        &container_dir,
        "x",
        "jackin-x",
        &WorkspaceLabel::parse("jackin").unwrap(),
        &PreflightContext {
            workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(mat.mounts[0].bind_src, "/tmp/cache");
    assert_eq!(mat.mounts[0].isolation, MountIsolation::Shared);
    assert!(
        runner.run_recorded.is_empty(),
        "no git ops for shared mounts"
    );
}

#[tokio::test]
async fn clone_materialization_runs_local_shared_clone_and_writes_record() {
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();

    let resolved = resolved_with_one_clone(repo.path(), "/workspace/jackin");
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(), // rev-parse --show-toplevel
        "",                             // status --porcelain
        "deadbeef\n",                   // rev-parse HEAD
        "https://github.com/jackin-project/jackin\n", // remote get-url origin
    ]);

    let mat = materialize_workspace(
        &resolved,
        &container_dir,
        "x",
        "jackin-x",
        &WorkspaceLabel::parse("jackin").unwrap(),
        &PreflightContext {
            workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(mat.mounts[0].isolation, MountIsolation::Clone);
    assert!(mat.mounts[0].worktree_aux.is_none());
    assert!(
        mat.mounts[0]
            .bind_src
            .contains("/git/clone/repo/workspace/jackin/jackin-x"),
        "got {}",
        mat.mounts[0].bind_src,
    );
    assert!(
        runner
            .run_recorded
            .iter()
            .any(|c| c.contains("git clone --local"))
    );
    assert!(
        !runner.run_recorded.iter().any(|c| c.contains("checkout")),
        "clone mode must not create or switch to a scratch branch: {:?}",
        runner.run_recorded
    );
    // Origin rewritten from bind-mount loopback to host's upstream.
    assert!(
        runner
            .run_recorded
            .iter()
            .any(|c| c.contains("remote set-url origin https://github.com/jackin-project/jackin")),
        "expected `git remote set-url origin <upstream>` in: {:?}",
        runner.run_recorded
    );
    let recs = read_records(&container_dir).unwrap();
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0].isolation, MountIsolation::Clone);
    assert_eq!(recs[0].base_commit, "deadbeef");
    assert_eq!(recs[0].scratch_branch, "");
}

#[tokio::test]
async fn clone_materialization_normalizes_ssh_origin_to_https() {
    // SCP-form host origin would point the clone at an SSH endpoint
    // the container has no key for; rewrite must land on HTTPS so
    // `gh auth git-credential` can authenticate the push.
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();

    let resolved = resolved_with_one_clone(repo.path(), "/workspace/jackin");
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(), // rev-parse --show-toplevel
        "",                             // status --porcelain
        "deadbeef\n",                   // rev-parse HEAD
        "git@github.com:jackin-project/jackin.git\n", // remote get-url origin (SCP form)
    ]);

    materialize_workspace(
        &resolved,
        &container_dir,
        "x",
        "jackin-x",
        &WorkspaceLabel::parse("jackin").unwrap(),
        &PreflightContext {
            workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .unwrap();

    assert!(
        runner
            .run_recorded
            .iter()
            .any(|c| c
                .contains("remote set-url origin https://github.com/jackin-project/jackin.git")),
        "expected SCP-form origin to be normalized to HTTPS in: {:?}",
        runner.run_recorded
    );
}

#[tokio::test]
async fn clone_materialization_skips_origin_rewrite_when_host_origin_is_empty() {
    // Ok-arm with whitespace-only output: `trimmed.is_empty()`
    // collapses to `None`, no rewrite emitted.
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();

    let resolved = resolved_with_one_clone(repo.path(), "/workspace/jackin");
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(), // rev-parse --show-toplevel
        "",                             // status --porcelain
        "deadbeef\n",                   // rev-parse HEAD
        "   \r\n\t  ",                  // remote get-url origin (whitespace-only)
    ]);

    materialize_workspace(
        &resolved,
        &container_dir,
        "x",
        "jackin-x",
        &WorkspaceLabel::parse("jackin").unwrap(),
        &PreflightContext {
            workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .unwrap();

    assert!(
        !runner
            .run_recorded
            .iter()
            .any(|c| c.contains("remote set-url")),
        "expected no `git remote set-url` when host origin is empty; got: {:?}",
        runner.run_recorded
    );
}

#[tokio::test]
async fn clone_materialization_falls_through_when_host_has_no_origin_remote() {
    // Err with `No such remote 'origin'` (fresh init, never pushed)
    // — fall through to loopback, do not abort.
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();

    let resolved = resolved_with_one_clone(repo.path(), "/workspace/jackin");
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(), // rev-parse --show-toplevel
        "",                             // status --porcelain
        "deadbeef\n",                   // rev-parse HEAD
    ]);
    runner.fail_with.push((
        "remote get-url origin".into(),
        "fatal: No such remote 'origin'".into(),
    ));

    materialize_workspace(
        &resolved,
        &container_dir,
        "x",
        "jackin-x",
        &WorkspaceLabel::parse("jackin").unwrap(),
        &PreflightContext {
            workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .expect("legitimate `No such remote` should fall through, not abort");

    assert!(
        !runner
            .run_recorded
            .iter()
            .any(|c| c.contains("remote set-url")),
        "expected no `git remote set-url` when host has no origin; got: {:?}",
        runner.run_recorded
    );
}

#[tokio::test]
async fn clone_materialization_aborts_when_get_url_fails_unexpectedly() {
    // Permission denied / corrupt config — anything that isn't a
    // `No such remote` signal — must abort the launch rather than
    // silently fall through to a loopback origin and misroute the
    // operator's pushes.
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();

    let resolved = resolved_with_one_clone(repo.path(), "/workspace/jackin");
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(), // rev-parse --show-toplevel
        "",                             // status --porcelain
        "deadbeef\n",                   // rev-parse HEAD
    ]);
    runner.fail_with.push((
        "remote get-url origin".into(),
        "fatal: unable to read config: Permission denied".into(),
    ));

    let err = materialize_workspace(
        &resolved,
        &container_dir,
        "x",
        "jackin-x",
        &WorkspaceLabel::parse("jackin").unwrap(),
        &PreflightContext {
            workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .expect_err("unexpected get-url failure should abort the launch");

    let chain = format!("{err:#}");
    assert!(
        chain.contains("failed to read host repo")
            && chain.contains("loopback origin")
            && chain.contains("misroute pushes"),
        "abort message should explain the failure mode and remediation; got: {chain}",
    );
}

#[tokio::test]
async fn clone_materialization_strips_embedded_credentials_from_host_origin() {
    // Credentialed host origin (PAT baked into `.git/config`) must
    // not land verbatim in the per-container clone.
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();

    let resolved = resolved_with_one_clone(repo.path(), "/workspace/jackin");
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(), // rev-parse --show-toplevel
        "",                             // status --porcelain
        "deadbeef\n",                   // rev-parse HEAD
        "https://x-access-token:ghp_secretsecretsecret@github.com/jackin-project/jackin.git\n",
    ]);

    materialize_workspace(
        &resolved,
        &container_dir,
        "x",
        "jackin-x",
        &WorkspaceLabel::parse("jackin").unwrap(),
        &PreflightContext {
            workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .unwrap();

    let setting = runner
        .run_recorded
        .iter()
        .find(|c| c.contains("remote set-url"))
        .expect("set-url should still run with a credential-stripped URL");
    assert!(
        setting.contains("https://github.com/jackin-project/jackin.git"),
        "set-url should target the credential-stripped URL; got: {setting}",
    );
    assert!(
        !setting.contains("ghp_secretsecretsecret") && !setting.contains("x-access-token"),
        "credentials must not appear in the set-url command; got: {setting}",
    );
}

#[tokio::test]
async fn clone_reuse_skips_git_ops_when_git_dir_exists() {
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();

    let dst = "/workspace/jackin";
    let cp = clone_path_for(&container_dir, dst, "jackin-x");
    std::fs::create_dir_all(cp.join(".git")).unwrap();
    crate::state::write_records(
        &container_dir,
        std::slice::from_ref(&IsolationRecord {
            workspace: "jackin".into(),
            mount_dst: dst.into(),
            original_src: repo.path().to_string_lossy().into(),
            isolation: MountIsolation::Clone,
            worktree_path: cp.to_string_lossy().into(),
            scratch_branch: "jackin/scratch/jackin-x".into(),
            base_commit: "abc".into(),
            selector_key: "x".into(),
            container_name: "jackin-x".into(),
            cleanup_status: CleanupStatus::Active,
        }),
    )
    .unwrap();

    let resolved = resolved_with_one_clone(repo.path(), dst);
    let mut runner = FakeRunner::default();
    let mat = materialize_workspace(
        &resolved,
        &container_dir,
        "x",
        "jackin-x",
        &WorkspaceLabel::parse("jackin").unwrap(),
        &PreflightContext {
            workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(mat.mounts[0].bind_src, cp.to_string_lossy());
    assert!(runner.run_recorded.is_empty(), "no git ops on clone reuse");
}

#[tokio::test]
async fn second_materialization_with_existing_record_skips_git_ops() {
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();

    let dst = "/workspace/jackin";
    let wt_path = worktree_path_for(&container_dir, dst, "jackin-x");
    std::fs::create_dir_all(&wt_path).unwrap();
    std::fs::write(wt_path.join(".git"), "gitdir: /elsewhere").unwrap();
    crate::state::write_records(
        &container_dir,
        std::slice::from_ref(&IsolationRecord {
            workspace: "jackin".into(),
            mount_dst: dst.into(),
            original_src: repo.path().to_string_lossy().into(),
            isolation: MountIsolation::Worktree,
            worktree_path: wt_path.to_string_lossy().into(),
            scratch_branch: "jackin/scratch/x".into(),
            base_commit: "abc".into(),
            selector_key: "x".into(),
            container_name: "jackin-x".into(),
            cleanup_status: CleanupStatus::Active,
        }),
    )
    .unwrap();

    let resolved = resolved_with_one_isolated(repo.path(), dst);
    let mut runner = FakeRunner::default();
    let mat = materialize_workspace(
        &resolved,
        &container_dir,
        "x",
        "jackin-x",
        &WorkspaceLabel::parse("jackin").unwrap(),
        &PreflightContext {
            workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .unwrap();
    assert_eq!(mat.mounts[0].bind_src, wt_path.to_string_lossy());
    assert!(runner.run_recorded.is_empty(), "no git ops on reuse");
}

#[tokio::test]
async fn drift_when_recorded_src_differs_errors_before_git_ops() {
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();

    let dst = "/workspace/jackin";
    let wt_path = worktree_path_for(&container_dir, dst, "jackin-x");
    std::fs::create_dir_all(&wt_path).unwrap();
    crate::state::write_records(
        &container_dir,
        std::slice::from_ref(&IsolationRecord {
            workspace: "jackin".into(),
            mount_dst: dst.into(),
            original_src: "/different/src".into(),
            isolation: MountIsolation::Worktree,
            worktree_path: wt_path.to_string_lossy().into(),
            scratch_branch: "jackin/scratch/x".into(),
            base_commit: "abc".into(),
            selector_key: "x".into(),
            container_name: "jackin-x".into(),
            cleanup_status: CleanupStatus::Active,
        }),
    )
    .unwrap();

    let resolved = resolved_with_one_isolated(repo.path(), dst);
    let mut runner = FakeRunner::default();
    let err = materialize_workspace(
        &resolved,
        &container_dir,
        "x",
        "jackin-x",
        &WorkspaceLabel::parse("jackin").unwrap(),
        &PreflightContext {
            workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("source drift") || msg.contains("differs"));
    assert!(msg.contains("/different/src"));
    assert!(runner.run_recorded.is_empty(), "no git ops on drift error");
}

// Removed test `two_isolated_mounts_same_repo_get_dst_suffixed_branches` —
// workspace validation now rejects this case at the layout level
// (`validate_isolation_layout` rule 2). The corresponding suffix
// logic in materialize is gone; coverage moves to
// `workspace::tests::isolation_layout_rejects_two_worktree_mounts_on_same_repo`.

fn write_loose_branch(repo: &Path, branch: &str, content: &str) {
    let mut p = repo.join(".git").join("refs").join("heads");
    for seg in branch.split('/') {
        p = p.join(seg);
    }
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, content).unwrap();
}

fn write_packed_refs(repo: &Path, contents: &str) {
    std::fs::write(repo.join(".git").join("packed-refs"), contents).unwrap();
}

#[tokio::test]
async fn find_local_branch_tip_reads_loose_ref_sha() {
    let repo = make_repo_root();
    let path = repo.path().to_string_lossy();
    assert_eq!(find_local_branch_tip(&path, "jackin/scratch/x"), None);
    write_loose_branch(repo.path(), "jackin/scratch/x", "deadbeefcafe\n");
    assert_eq!(
        find_local_branch_tip(&path, "jackin/scratch/x"),
        Some("deadbeefcafe".into()),
    );
}

#[tokio::test]
async fn find_local_branch_tip_reads_packed_refs_sha() {
    let repo = make_repo_root();
    write_packed_refs(
        repo.path(),
        "# pack-refs with: peeled fully-peeled sorted\n\
             1111111111111111111111111111111111111111 refs/heads/main\n\
             2222222222222222222222222222222222222222 refs/heads/jackin/scratch/x\n\
             ^abcd1234abcd1234abcd1234abcd1234abcd1234\n",
    );
    let path = repo.path().to_string_lossy();
    assert_eq!(
        find_local_branch_tip(&path, "jackin/scratch/x"),
        Some("2222222222222222222222222222222222222222".into()),
    );
    assert_eq!(find_local_branch_tip(&path, "jackin/scratch/missing"), None);
}

#[tokio::test]
async fn find_local_branch_tip_loose_ref_wins_over_packed_refs() {
    // git semantics: loose refs override packed-refs entries.
    // Critical because base_commit feeds finalize's safety
    // classifier, and a wrong SHA there can authorize deletion
    // of operator work.
    let repo = make_repo_root();
    write_loose_branch(repo.path(), "jackin/scratch/x", "1010101010101010\n");
    write_packed_refs(
        repo.path(),
        "9999999999999999999999999999999999999999 refs/heads/jackin/scratch/x\n",
    );
    assert_eq!(
        find_local_branch_tip(&repo.path().to_string_lossy(), "jackin/scratch/x"),
        Some("1010101010101010".into()),
    );
}

#[tokio::test]
async fn find_local_branch_tip_rejects_symref_loose_content() {
    // `git symbolic-ref refs/heads/<x> refs/heads/main` writes
    // `ref: refs/heads/main\n`. Returning that verbatim as the
    // SHA poisons IsolationRecord.base_commit.
    let repo = make_repo_root();
    write_loose_branch(repo.path(), "jackin/scratch/x", "ref: refs/heads/main\n");
    assert_eq!(
        find_local_branch_tip(&repo.path().to_string_lossy(), "jackin/scratch/x"),
        None,
    );
}

#[tokio::test]
async fn find_local_branch_tip_empty_loose_falls_through_to_packed() {
    // A 0-byte ref file (interrupted git op, third-party
    // tooling) must not yield Some("") and must not block the
    // packed-refs lookup.
    let repo = make_repo_root();
    write_loose_branch(repo.path(), "jackin/scratch/x", "");
    write_packed_refs(
        repo.path(),
        "abcdef1234567890abcdef1234567890abcdef12 refs/heads/jackin/scratch/x\n",
    );
    assert_eq!(
        find_local_branch_tip(&repo.path().to_string_lossy(), "jackin/scratch/x"),
        Some("abcdef1234567890abcdef1234567890abcdef12".into()),
    );
}

#[tokio::test]
async fn find_local_branch_tip_skips_malformed_packed_refs_lines() {
    let repo = make_repo_root();
    write_packed_refs(
        repo.path(),
        "# header only\n\
             ^abcd\n\
             noseparator\n\
             1111111111111111111111111111111111111111\trefs/heads/jackin/scratch/x\n",
    );
    // Tab-separated row also resolves (split_once now matches
    // any ASCII whitespace, defensive against non-stock writers).
    assert_eq!(
        find_local_branch_tip(&repo.path().to_string_lossy(), "jackin/scratch/x"),
        Some("1111111111111111111111111111111111111111".into()),
    );
}

#[tokio::test]
async fn stale_scratch_branch_is_adopted_when_record_absent() {
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-the-architect");
    std::fs::create_dir_all(&container_dir).unwrap();

    write_loose_branch(
        repo.path(),
        "jackin/scratch/jackin-the-architect",
        "feedbeefcafebabefeedbeefcafebabefeedbeef\n",
    );

    let resolved = resolved_with_one_isolated(repo.path(), "/workspace/jackin");
    // fake_with_outputs is positional: order must match
    // materialize_workspace's runner.capture() sequence.
    // Adopted branch tip is read directly from the loose ref —
    // no runner.capture() entry is consumed for it.
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(),
        "",
        "true\n",
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef\n",
    ]);

    let mat = materialize_workspace(
        &resolved,
        &container_dir,
        "the-architect",
        "jackin-the-architect",
        &WorkspaceLabel::parse("jackin").unwrap(),
        &PreflightContext {
            workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .unwrap();

    assert_eq!(mat.mounts.len(), 1);

    let prune_idx = runner
        .run_recorded
        .iter()
        .position(|c| c.contains("worktree prune"))
        .expect("worktree prune should be invoked on adopt");
    let add_idx = runner
        .run_recorded
        .iter()
        .position(|c| c.contains("worktree add"))
        .expect("worktree add should be invoked");
    assert!(
        prune_idx < add_idx,
        "prune must run before add; got prune@{prune_idx} add@{add_idx}: {:?}",
        runner.run_recorded,
    );
    let add = &runner.run_recorded[add_idx];
    assert!(
        !add.split_whitespace().any(|t| t == "-b" || t == "--branch"),
        "adopt path must not pass -b/--branch; got {add}",
    );
    assert!(
        add.ends_with(" jackin/scratch/jackin-the-architect"),
        "adopt add must end with the existing branch as the last positional arg; got {add}",
    );

    let recs = read_records(&container_dir).unwrap();
    assert_eq!(recs.len(), 1);
    // base_commit is host_head, NOT branch tip — see the comment
    // above the adopt arm in materialize_one. Asserting the
    // branch-tip value here would silently re-introduce the
    // data-loss regression flagged in PR #219 review.
    assert_eq!(
        recs[0].base_commit,
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
    );
    assert_eq!(
        recs[0].scratch_branch,
        "jackin/scratch/jackin-the-architect",
    );
}

#[tokio::test]
async fn adopt_aborts_when_worktree_prune_fails() {
    // Prune failure must short-circuit before `worktree add`,
    // otherwise the add proceeds against an inconsistent admin
    // index and risks corrupting state.
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();
    write_loose_branch(repo.path(), "jackin/scratch/jackin-x", "abc123\n");
    let resolved = resolved_with_one_isolated(repo.path(), "/workspace/jackin");
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(),
        "",
        "true\n",
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef\n",
    ]);
    runner.fail_on.push("worktree prune".into());

    let err = materialize_workspace(
        &resolved,
        &container_dir,
        "x",
        "jackin-x",
        &WorkspaceLabel::parse("jackin").unwrap(),
        &PreflightContext {
            workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("worktree prune"));
    assert!(
        !runner
            .run_recorded
            .iter()
            .any(|c| c.contains("worktree add")),
        "worktree add must not run when prune fails; got {:?}",
        runner.run_recorded,
    );
    assert!(
        read_records(&container_dir).unwrap().is_empty(),
        "no record on prune failure",
    );
}

#[tokio::test]
async fn fresh_materialization_uses_dash_b_when_branch_absent() {
    let repo = make_repo_root();
    let data = tempfile::TempDir::new().unwrap();
    let container_dir = data.path().join("jackin-x");
    std::fs::create_dir_all(&container_dir).unwrap();
    let resolved = resolved_with_one_isolated(repo.path(), "/workspace/jackin");
    let mut runner = fake_with_outputs(&[
        &repo.path().to_string_lossy(),
        "",
        "true\n",
        "cafef00dcafef00dcafef00dcafef00dcafef00d\n",
    ]);
    materialize_workspace(
        &resolved,
        &container_dir,
        "x",
        "jackin-x",
        &WorkspaceLabel::parse("jackin").unwrap(),
        &PreflightContext {
            workspace_label: WorkspaceLabel::parse("jackin").unwrap(),
            force: false,
            interactive: false,
        },
        &mut runner,
    )
    .await
    .unwrap();
    let add = runner
        .run_recorded
        .iter()
        .find(|c| c.contains("worktree add"))
        .expect("worktree add should have been invoked");
    assert!(
        add.split_whitespace().any(|t| t == "-b"),
        "fresh path must use -b; got {add}",
    );
    assert!(
        !runner
            .run_recorded
            .iter()
            .any(|c| c.contains("worktree prune")),
    );
    let recs = read_records(&container_dir).unwrap();
    assert_eq!(
        recs[0].base_commit,
        "cafef00dcafef00dcafef00dcafef00dcafef00d",
    );
}

#[tokio::test]
async fn docker_mount_order_is_length_ascending() {
    let mat = MaterializedWorkspace {
        workdir: "/workspace".into(),
        mounts: vec![
            MaterializedMount {
                bind_src: "/cache".into(),
                dst: "/workspace/proj/target".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
                worktree_aux: None,
            },
            MaterializedMount {
                bind_src: "/wt".into(),
                dst: "/workspace/proj".into(),
                readonly: false,
                isolation: MountIsolation::Worktree,
                worktree_aux: None,
            },
        ],
        keep_awake_enabled: false,
    };
    let ordered = mount_order_for_docker(&mat);
    assert_eq!(ordered[0].dst, "/workspace/proj");
    assert_eq!(ordered[1].dst, "/workspace/proj/target");
}

#[tokio::test]
async fn docker_mount_order_is_stable_for_same_length() {
    let mat = MaterializedWorkspace {
        workdir: "/workspace".into(),
        mounts: vec![
            MaterializedMount {
                bind_src: "/a".into(),
                dst: "/workspace/aa".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
                worktree_aux: None,
            },
            MaterializedMount {
                bind_src: "/b".into(),
                dst: "/workspace/bb".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
                worktree_aux: None,
            },
        ],
        keep_awake_enabled: false,
    };
    let ordered = mount_order_for_docker(&mat);
    assert_eq!(ordered[0].dst, "/workspace/aa");
    assert_eq!(ordered[1].dst, "/workspace/bb");
}
