//! Lightweight detection of mount source type: plain folder vs git repo.
//! Used only for display — no functional effect on the workspace config.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum MountKind {
    /// Path doesn't exist on disk. Nothing to inspect.
    Missing,
    /// Path exists, not a git repo.
    Folder,
    /// Path is a git working copy. `origin` is `None` when the repo has no
    /// resolvable `origin` remote at all.
    Git {
        branch: GitBranch,
        origin: Option<GitOrigin>,
    },
}

/// Classification of a git repo's `origin` remote.
///
/// The two variants encode the "github vs other" distinction together
/// with the URLs that are reachable for that classification, so callers
/// can't ask for a web URL on a non-github remote or carry around an
/// unsynchronized `(host, remote_url, web_url)` triple.
#[derive(Debug, Clone)]
pub enum GitOrigin {
    /// Remote `origin` points at `github.com` (SSH `git@github.com:`,
    /// HTTPS `https://github.com/...`, or `ssh://git@github.com/...`).
    /// `web_url` is the resolved branch/commit page on github.com.
    Github { remote_url: String, web_url: String },
    /// Self-hosted gitea/forgejo, GitLab, Bitbucket, Azure DevOps, etc.
    /// We expose the raw remote URL but no web URL — we don't speak the
    /// branch-URL conventions of these hosts.
    Other { remote_url: String },
}

impl GitOrigin {
    /// `https://...` URL for the branch on github.com when applicable.
    /// `None` for non-github remotes — callers asking for an "open in
    /// browser" URL should gate on this rather than synthesizing one.
    pub fn web_url(&self) -> Option<&str> {
        match self {
            Self::Github { web_url, .. } => Some(web_url),
            Self::Other { .. } => None,
        }
    }

    /// Display prefix used by `MountKind::label`. Github gets a distinct
    /// prefix so the operator knows "open in browser" is wired up.
    const fn display_prefix(&self) -> &'static str {
        match self {
            Self::Github { .. } => "github",
            Self::Other { .. } => "git",
        }
    }
}

#[derive(Debug, Clone)]
pub enum GitBranch {
    Named(String),
    Detached { short_sha: String },
    Unknown,
}

pub fn inspect(src: &str) -> MountKind {
    let path = Path::new(src);
    if !path.exists() {
        return MountKind::Missing;
    }
    if !path.is_dir() {
        // A file, symlink to file, etc. Treat as folder-ish for display.
        return MountKind::Folder;
    }
    resolve_gitdirs(path).map_or(MountKind::Folder, |(work_dir, config_dir)| {
        // Branch comes from the worktree-specific gitdir (HEAD is per-worktree
        // even when the config lives in the common dir), while the origin URL
        // and host classification come from the common dir's `config`.
        let branch = parse_head(&work_dir);
        let origin = resolve_origin(&config_dir, &branch);
        MountKind::Git { branch, origin }
    })
}

/// Resolve the per-worktree and config-owning git directories for a
/// workdir. Returns `None` when `workdir` is not a git working copy.
///
/// The two paths are identical for a plain clone (`work_dir == config_dir
/// == <workdir>/.git`) and for a submodule (both point at the resolved
/// gitdir referenced by the `.git` file). They differ for a **git worktree**:
/// the `.git` file points at `<main>/.git/worktrees/<name>`, which has its
/// own `HEAD` but no `config` of its own — instead, a `commondir` file
/// points at the main repo's git-dir, where the remote URL lives.
///
/// Without following the `commondir` redirect, `resolve_host_and_url`
/// reads nothing, `GitHost` falls through to `Other`, and the label
/// renders as `git · branch` instead of `github · branch`.
fn resolve_gitdirs(workdir: &Path) -> Option<(PathBuf, PathBuf)> {
    let dotgit = workdir.join(".git");
    if dotgit.is_dir() {
        // Plain clone: HEAD and config both live here.
        return Some((dotgit.clone(), dotgit));
    }
    if !dotgit.is_file() {
        return None;
    }
    // File form — could be either a submodule (`.git` contains
    // "gitdir: <target>", target holds HEAD + config) or a git worktree
    // (target holds HEAD plus a `commondir` pointer to the shared config).
    let content = std::fs::read_to_string(&dotgit).ok()?;
    let gitdir_line = content.lines().find_map(|line| {
        line.strip_prefix("gitdir:")
            .map(|rest| rest.trim().to_string())
    })?;
    let abs = if Path::new(&gitdir_line).is_absolute() {
        PathBuf::from(gitdir_line)
    } else {
        workdir.join(gitdir_line)
    };
    if !abs.is_dir() {
        return None;
    }
    // If a `commondir` pointer exists, follow it. Its target holds the
    // shared config (and the main repo's refs/objects); the worktree's
    // gitdir still owns HEAD, so we keep `abs` as the work dir.
    // If `commondir` is missing or unreadable we fall through to treating
    // `abs` as its own config dir — plain-submodule behaviour.
    let commondir_ptr = abs.join("commondir");
    if commondir_ptr.is_file()
        && let Ok(raw) = std::fs::read_to_string(&commondir_ptr)
    {
        let rel = raw.trim();
        let common = if Path::new(rel).is_absolute() {
            PathBuf::from(rel)
        } else {
            abs.join(rel)
        };
        if common.is_dir() {
            return Some((abs, common));
        }
    }
    // Plain submodule (or unresolvable commondir): HEAD + config co-located.
    Some((abs.clone(), abs))
}

fn parse_head(git_dir: &Path) -> GitBranch {
    let head_path = git_dir.join("HEAD");
    let Ok(content) = std::fs::read_to_string(&head_path) else {
        return GitBranch::Unknown;
    };
    let trimmed = content.trim();
    trimmed.strip_prefix("ref: refs/heads/").map_or_else(
        || {
            if trimmed.len() >= 7 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
                // Detached HEAD — use first 7 chars as a short sha.
                GitBranch::Detached {
                    short_sha: trimmed[..7].to_string(),
                }
            } else {
                GitBranch::Unknown
            }
        },
        |rest| GitBranch::Named(rest.to_string()),
    )
}

/// Parse `<config_dir>/config` to find the origin remote's URL and
/// classify it. The config dir is the main repo's `.git` for worktrees
/// (see `resolve_gitdirs`) and the per-repo `.git` for plain
/// clones/submodules.
///
/// Returns:
/// - `Some(GitOrigin::Github { ... })` when origin lives on github.com
///   and resolves into a branch/commit web URL.
/// - `Some(GitOrigin::Other { ... })` for any other host, or for a
///   github URL whose web shape we can't synthesize (degenerate case).
/// - `None` when no remote is set.
fn resolve_origin(config_dir: &Path, branch: &GitBranch) -> Option<GitOrigin> {
    let config_path = config_dir.join("config");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let remote_url = parse_remote_origin_url(&content)?;
    if !remote_points_at_github(&remote_url) {
        return Some(GitOrigin::Other { remote_url });
    }
    let Some(base) = remote_to_web(&remote_url) else {
        return Some(GitOrigin::Other { remote_url });
    };
    let web_url = match branch {
        GitBranch::Named(b) => format!("{base}/tree/{b}"),
        GitBranch::Detached { short_sha } => format!("{base}/commit/{short_sha}"),
        GitBranch::Unknown => base,
    };
    Some(GitOrigin::Github {
        remote_url,
        web_url,
    })
}

/// Cheap predicate — does this remote URL live on `github.com`?
/// Covers the three forms we resolve in `remote_to_web`.
fn remote_points_at_github(remote: &str) -> bool {
    // scp-style SSH: `git@github.com:...`
    if let Some(rest) = remote.strip_prefix("git@") {
        return rest.starts_with("github.com:");
    }
    // ssh:// (optionally with `git@`)
    if let Some(rest) = remote.strip_prefix("ssh://") {
        let rest = rest.strip_prefix("git@").unwrap_or(rest);
        return rest.starts_with("github.com/");
    }
    // HTTP(S)
    remote.starts_with("https://github.com/") || remote.starts_with("http://github.com/")
}

fn parse_remote_origin_url(content: &str) -> Option<String> {
    let mut in_origin = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == r#"[remote "origin"]"# {
            in_origin = true;
            continue;
        }
        if trimmed.starts_with('[') {
            in_origin = false;
            continue;
        }
        if in_origin && let Some(rest) = trimmed.strip_prefix("url") {
            // "url = ..." or "url=..."
            let rest = rest.trim_start_matches([' ', '\t', '=']);
            return Some(rest.trim().to_string());
        }
    }
    None
}

/// Transform a GitHub remote URL into an `https://github.com/owner/repo`
/// base. Returns `None` for any non-GitHub remote — callers should gate
/// on `remote_points_at_github` first to classify the host.
///
/// Supported shapes (all trailing `.git` stripped):
/// - `git@github.com:owner/repo.git` → `https://github.com/owner/repo`
/// - `https://github.com/owner/repo.git` → `https://github.com/owner/repo`
/// - `ssh://git@github.com/owner/repo.git` → `https://github.com/owner/repo`
fn remote_to_web(remote: &str) -> Option<String> {
    if !remote_points_at_github(remote) {
        return None;
    }
    // Strip trailing .git
    let remote = remote.strip_suffix(".git").unwrap_or(remote);

    // SSH scp-style: git@github.com:owner/repo
    if let Some(rest) = remote.strip_prefix("git@")
        && let Some((host, path)) = rest.split_once(':')
    {
        return Some(format!("https://{host}/{path}"));
    }
    // HTTPS/HTTP as-is
    if remote.starts_with("https://") || remote.starts_with("http://") {
        return Some(remote.to_string());
    }
    // ssh://git@host/path
    if let Some(rest) = remote.strip_prefix("ssh://") {
        let rest = rest.strip_prefix("git@").unwrap_or(rest);
        if let Some((host, path)) = rest.split_once('/') {
            return Some(format!("https://{host}/{path}"));
        }
    }
    None
}

impl MountKind {
    /// Short label for display next to a mount. GitHub-hosted repos use a
    /// `github · …` prefix so the operator can tell at a glance which
    /// remotes have an `o`-opens-in-browser affordance wired up; everything
    /// else (self-hosted gitea, gitlab, no remote, …) keeps the generic
    /// `git · …` prefix.
    pub fn label(&self) -> String {
        match self {
            Self::Missing => "missing".to_string(),
            Self::Folder => "folder".to_string(),
            Self::Git { branch, origin } => {
                let prefix = origin.as_ref().map_or("git", |o| o.display_prefix());
                match branch {
                    GitBranch::Named(b) => format!("{prefix} · {b}"),
                    GitBranch::Detached { short_sha } => {
                        format!("{prefix} · detached {short_sha}")
                    }
                    GitBranch::Unknown => prefix.to_string(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
            Some("https://github.com/owner/repo".to_string())
        );
    }

    #[test]
    fn remote_to_web_https_github() {
        assert_eq!(
            remote_to_web("https://github.com/owner/repo.git"),
            Some("https://github.com/owner/repo".to_string())
        );
    }

    #[test]
    fn remote_to_web_ssh_protocol() {
        assert_eq!(
            remote_to_web("ssh://git@github.com/owner/repo.git"),
            Some("https://github.com/owner/repo".to_string())
        );
    }

    #[test]
    fn remote_to_web_returns_none_for_gitlab() {
        // GitLab is a non-GitHub host — `remote_to_web` no longer synthesises
        // a web URL for it. (Classification falls through to `GitHost::Other`
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
            Some("git@github.com:owner/repo.git".to_string())
        );
    }
}
