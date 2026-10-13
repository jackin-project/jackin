// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `mount_info`.
use super::*;
use tempfile::tempdir;

#[test]
fn missing_path_reports_missing() {
    let temp = tempdir().unwrap();
    let missing = temp.path().join("nope");
    assert!(matches!(
        inspect(missing.to_str().unwrap()),
        MountKind::Missing
    ));
}

#[test]
fn plain_folder_reports_folder() {
    let temp = tempdir().unwrap();
    assert!(matches!(
        inspect(temp.path().to_str().unwrap()),
        MountKind::Folder
    ));
}

#[test]
fn git_repo_reports_branch() {
    let temp = tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    std::fs::create_dir(&git_dir).unwrap();
    std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    let result = inspect(temp.path().to_str().unwrap());
    match result {
        MountKind::Git {
            branch: GitBranch::Named(b),
            ..
        } => assert_eq!(b, "main"),
        other => panic!("expected Git {{ branch: Named }}, got {other:?}"),
    }
}

#[test]
fn detached_head_reports_short_sha() {
    let temp = tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    std::fs::create_dir(&git_dir).unwrap();
    std::fs::write(
        git_dir.join("HEAD"),
        "abcdef1234567890abcdef1234567890abcdef12\n",
    )
    .unwrap();
    let result = inspect(temp.path().to_str().unwrap());
    match result {
        MountKind::Git {
            branch: GitBranch::Detached { short_sha },
            ..
        } => {
            assert_eq!(short_sha, "abcdef1");
        }
        other => panic!("expected Git {{ branch: Detached }}, got {other:?}"),
    }
}

#[test]
fn inspect_classifies_github_remote_as_github_origin() {
    let temp = tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    std::fs::create_dir(&git_dir).unwrap();
    std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::write(
        git_dir.join("config"),
        r#"[remote "origin"]
    url = git@github.com:owner/repo.git
"#,
    )
    .unwrap();
    let result = inspect(temp.path().to_str().unwrap());
    match result {
        MountKind::Git {
            origin: Some(GitOrigin::Github { web_url, .. }),
            ..
        } => {
            assert_eq!(web_url, "https://github.com/owner/repo/tree/main");
        }
        other => panic!("expected Git {{ origin: Some(Github), .. }}, got {other:?}"),
    }
}

#[test]
fn inspect_classifies_gitlab_remote_as_other_origin() {
    let temp = tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    std::fs::create_dir(&git_dir).unwrap();
    std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::write(
        git_dir.join("config"),
        r#"[remote "origin"]
    url = git@gitlab.com:owner/repo.git
"#,
    )
    .unwrap();
    let result = inspect(temp.path().to_str().unwrap());
    match result {
        MountKind::Git {
            origin: Some(GitOrigin::Other { remote_url }),
            ..
        } => {
            assert_eq!(remote_url, "git@gitlab.com:owner/repo.git");
        }
        other => panic!("expected Git {{ origin: Some(Other), .. }}, got {other:?}"),
    }
}

#[test]
fn inspect_classifies_repo_without_remote_as_no_origin() {
    let temp = tempdir().unwrap();
    let git_dir = temp.path().join(".git");
    std::fs::create_dir(&git_dir).unwrap();
    std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    // No config file at all — simulates `git init` without a remote.
    let result = inspect(temp.path().to_str().unwrap());
    match result {
        MountKind::Git { origin: None, .. } => {}
        other => panic!("expected Git {{ origin: None, .. }}, got {other:?}"),
    }
}

#[test]
fn submodule_gitfile_resolves() {
    let temp = tempdir().unwrap();
    let actual_gitdir = temp.path().join("parent_repo_gitdir");
    std::fs::create_dir(&actual_gitdir).unwrap();
    std::fs::write(actual_gitdir.join("HEAD"), "ref: refs/heads/feature-x\n").unwrap();

    let workdir = temp.path().join("submodule");
    std::fs::create_dir(&workdir).unwrap();
    std::fs::write(
        workdir.join(".git"),
        format!("gitdir: {}\n", actual_gitdir.display()),
    )
    .unwrap();

    let result = inspect(workdir.to_str().unwrap());
    match result {
        MountKind::Git {
            branch: GitBranch::Named(b),
            ..
        } => assert_eq!(b, "feature-x"),
        other => panic!("expected submodule resolution, got {other:?}"),
    }
}

/// Git worktree layout — HEAD lives in the worktree-specific gitdir,
/// but the repo's `config` (with the remote URL) lives at the target
/// of a `commondir` pointer. Without following the pointer the host
/// falls through to `Other` and the label renders as `git · branch`
/// instead of `github · branch`.
#[test]
fn worktree_gitfile_resolves_to_commondir() {
    let temp = tempdir().unwrap();

    // Main repo's .git directory with the shared config.
    let main_gitdir = temp.path().join("main_repo").join(".git");
    std::fs::create_dir_all(&main_gitdir).unwrap();
    std::fs::write(
        main_gitdir.join("config"),
        r#"[remote "origin"]
    url = git@github.com:owner/repo.git
"#,
    )
    .unwrap();

    // Worktree-specific gitdir under main_repo/.git/worktrees/feat.
    // Owns HEAD + a `commondir` relative pointer back to the main .git.
    let worktree_gitdir = main_gitdir.join("worktrees").join("feat");
    std::fs::create_dir_all(&worktree_gitdir).unwrap();
    std::fs::write(worktree_gitdir.join("HEAD"), "ref: refs/heads/feature-x\n").unwrap();
    std::fs::write(worktree_gitdir.join("commondir"), "../..\n").unwrap();

    // The worktree checkout itself — .git is a file pointing at the
    // worktree-specific gitdir (absolute path, the form `git worktree
    // add` writes).
    let worktree = temp.path().join("feat_tree");
    std::fs::create_dir(&worktree).unwrap();
    std::fs::write(
        worktree.join(".git"),
        format!("gitdir: {}\n", worktree_gitdir.display()),
    )
    .unwrap();

    let result = inspect(worktree.to_str().unwrap());
    match result {
        MountKind::Git {
            branch: GitBranch::Named(b),
            origin:
                Some(GitOrigin::Github {
                    remote_url,
                    web_url,
                }),
        } => {
            assert_eq!(b, "feature-x", "branch should come from worktree HEAD");
            assert_eq!(
                remote_url, "git@github.com:owner/repo.git",
                "origin must be resolved from commondir's config"
            );
            assert_eq!(web_url, "https://github.com/owner/repo/tree/feature-x");
        }
        other => panic!("expected Git {{ origin: Github, branch: feature-x }}, got {other:?}"),
    }
}

/// Same as `worktree_gitfile_resolves_to_commondir` but the `commondir`
/// pointer is an absolute path. Exercises the absolute branch of the
/// commondir-resolution logic.
#[test]
fn worktree_commondir_with_absolute_path() {
    let temp = tempdir().unwrap();

    let main_gitdir = temp.path().join("main_repo").join(".git");
    std::fs::create_dir_all(&main_gitdir).unwrap();
    std::fs::write(
        main_gitdir.join("config"),
        r#"[remote "origin"]
    url = https://github.com/owner/repo.git
"#,
    )
    .unwrap();

    let worktree_gitdir = main_gitdir.join("worktrees").join("abs_feat");
    std::fs::create_dir_all(&worktree_gitdir).unwrap();
    std::fs::write(worktree_gitdir.join("HEAD"), "ref: refs/heads/abs-branch\n").unwrap();
    // Absolute commondir pointer — what real-world `git worktree add`
    // writes on some platforms/versions.
    std::fs::write(
        worktree_gitdir.join("commondir"),
        format!("{}\n", main_gitdir.display()),
    )
    .unwrap();

    let worktree = temp.path().join("abs_tree");
    std::fs::create_dir(&worktree).unwrap();
    std::fs::write(
        worktree.join(".git"),
        format!("gitdir: {}\n", worktree_gitdir.display()),
    )
    .unwrap();

    let result = inspect(worktree.to_str().unwrap());
    match result {
        MountKind::Git {
            branch: GitBranch::Named(b),
            origin:
                Some(GitOrigin::Github {
                    remote_url,
                    web_url,
                }),
        } => {
            assert_eq!(b, "abs-branch");
            assert_eq!(remote_url, "https://github.com/owner/repo.git");
            assert_eq!(web_url, "https://github.com/owner/repo/tree/abs-branch");
        }
        other => {
            panic!("expected Github worktree resolution via absolute commondir, got {other:?}")
        }
    }
}

/// Regression: plain submodules (no `commondir`) must continue to
/// resolve HEAD + config from the same gitdir. Guards against the
/// commondir-following logic accidentally dropping the `config` when
/// no pointer is present.
#[test]
fn submodule_gitfile_still_resolves_host_end_to_end() {
    let temp = tempdir().unwrap();
    let actual_gitdir = temp.path().join("parent_repo_gitdir");
    std::fs::create_dir(&actual_gitdir).unwrap();
    std::fs::write(actual_gitdir.join("HEAD"), "ref: refs/heads/submain\n").unwrap();
    // No `commondir` file — this is a plain submodule, so config
    // lives directly in `actual_gitdir`.
    std::fs::write(
        actual_gitdir.join("config"),
        r#"[remote "origin"]
    url = git@github.com:owner/submod.git
"#,
    )
    .unwrap();

    let workdir = temp.path().join("submodule");
    std::fs::create_dir(&workdir).unwrap();
    std::fs::write(
        workdir.join(".git"),
        format!("gitdir: {}\n", actual_gitdir.display()),
    )
    .unwrap();

    let result = inspect(workdir.to_str().unwrap());
    match result {
        MountKind::Git {
            branch: GitBranch::Named(b),
            origin:
                Some(GitOrigin::Github {
                    remote_url,
                    web_url,
                }),
        } => {
            assert_eq!(b, "submain");
            assert_eq!(remote_url, "git@github.com:owner/submod.git");
            assert_eq!(web_url, "https://github.com/owner/submod/tree/submain");
        }
        other => panic!("expected submodule to resolve with GitOrigin::Github, got {other:?}"),
    }
}

/// Build a `MountKind::Git` value for label tests from a github
/// `owner/repo` slug and a branch. The remote and web URLs are derived
/// from the slug + branch using the same shapes `resolve_origin`
/// produces for real github remotes — keeps tests focused on label
/// behavior instead of URL bookkeeping.
fn github_mount(owner_repo: &str, branch: GitBranch) -> MountKind {
    let base = format!("https://github.com/{owner_repo}");
    let web_url = match &branch {
        GitBranch::Named(b) => format!("{base}/tree/{b}"),
        GitBranch::Detached { short_sha } => format!("{base}/commit/{short_sha}"),
        GitBranch::Unknown => base.clone(),
    };
    MountKind::Git {
        branch,
        origin: Some(GitOrigin::Github {
            remote_url: format!("{base}.git"),
            web_url,
        }),
    }
}

/// Build a `MountKind::Git` for label tests with a non-github origin.
fn other_mount(remote_url: &str, branch: GitBranch) -> MountKind {
    MountKind::Git {
        branch,
        origin: Some(GitOrigin::Other {
            remote_url: remote_url.into(),
        }),
    }
}

/// Build a `MountKind::Git` for label tests with no resolvable origin.
fn no_origin_mount(branch: GitBranch) -> MountKind {
    MountKind::Git {
        branch,
        origin: None,
    }
}

#[test]
fn label_formats_generic_git() {
    // Non-GitHub (or unresolved remote) mounts use the generic
    // `git · …` prefix.
    assert_eq!(MountKind::Missing.label(), "missing");
    assert_eq!(MountKind::Folder.label(), "folder");
    assert_eq!(
        no_origin_mount(GitBranch::Named("main".into())).label(),
        "git · main"
    );
    assert_eq!(
        other_mount(
            "git@gitlab.com:o/r.git",
            GitBranch::Detached {
                short_sha: "abc1234".into()
            },
        )
        .label(),
        "git · detached abc1234"
    );
    assert_eq!(no_origin_mount(GitBranch::Unknown).label(), "git");
}

#[test]
fn label_formats_github_host() {
    // GitHub-hosted mounts get a `github · …` prefix so the operator
    // can tell which rows have an "open in browser" affordance.
    assert_eq!(
        github_mount("owner/repo", GitBranch::Named("main".into())).label(),
        "github · main"
    );
    assert_eq!(
        github_mount(
            "owner/repo",
            GitBranch::Detached {
                short_sha: "abc1234".into()
            },
        )
        .label(),
        "github · detached abc1234"
    );
    assert_eq!(
        github_mount("owner/repo", GitBranch::Unknown).label(),
        "github"
    );
}

// ── URL parser tests ──────────────────────────────────────────────

#[test]
fn remote_to_web_ssh_github() {
    assert_eq!(
        remote_to_web("git@github.com:owner/repo.git"),
        Some("https://github.com/owner/repo".to_owned())
    );
}

#[test]
fn remote_to_web_https_github() {
    assert_eq!(
        remote_to_web("https://github.com/owner/repo.git"),
        Some("https://github.com/owner/repo".to_owned())
    );
}

#[test]
fn remote_to_web_ssh_protocol() {
    assert_eq!(
        remote_to_web("ssh://git@github.com/owner/repo.git"),
        Some("https://github.com/owner/repo".to_owned())
    );
}

#[test]
fn remote_to_web_returns_none_for_gitlab() {
    // GitLab is a non-GitHub host — `remote_to_web` no longer synthesises
    // a web URL for it. (Classification falls through to `GitOrigin::Other`
    // at the `resolve_host_and_url` layer.)
    assert_eq!(remote_to_web("git@gitlab.com:owner/repo.git"), None);
    assert_eq!(remote_to_web("https://gitlab.com/owner/repo.git"), None);
}

#[test]
fn remote_to_web_returns_none_for_self_hosted() {
    // Any non-github.com host yields None.
    assert_eq!(remote_to_web("git@git.example.com:owner/repo.git"), None);
    assert_eq!(remote_to_web("ssh://git@forge.internal/owner/repo"), None);
}

#[test]
fn remote_points_at_github_covers_all_three_forms() {
    assert!(remote_points_at_github("git@github.com:o/r.git"));
    assert!(remote_points_at_github("https://github.com/o/r.git"));
    assert!(remote_points_at_github("http://github.com/o/r"));
    assert!(remote_points_at_github("ssh://git@github.com/o/r.git"));
    assert!(remote_points_at_github("ssh://github.com/o/r.git"));
    // Non-GitHub hosts reject.
    assert!(!remote_points_at_github("git@gitlab.com:o/r.git"));
    assert!(!remote_points_at_github("https://gitlab.com/o/r.git"));
    assert!(!remote_points_at_github("git@git.example.com:o/r.git"));
    // GitHub-lookalike subdomain does not count.
    assert!(!remote_points_at_github(
        "https://github.com.evil.example/o/r"
    ));
}

#[test]
fn parse_origin_url_from_config() {
    let config = r#"
[core]
    repositoryformatversion = 0

[remote "origin"]
    url = git@github.com:owner/repo.git
    fetch = +refs/heads/*:refs/remotes/origin/*

[branch "main"]
    remote = origin
"#;
    assert_eq!(
        parse_remote_origin_url(config),
        Some("git@github.com:owner/repo.git".to_owned())
    );
}
