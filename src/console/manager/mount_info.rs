//! Lightweight detection of mount source type: plain folder vs git repo.
//! Used only for display — no functional effect on the workspace config.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum MountKind {
    /// Path doesn't exist on disk. Nothing to inspect.
    Missing,
    /// Path exists, not a git repo.
    Folder,
    /// Path is a git working copy.
    Git {
        branch: GitBranch,
        /// Which host the remote `origin` lives on — affects the label
        /// (`github` vs `git`) and whether a web URL is resolvable.
        host: GitHost,
        /// URL for the branch on the git host, if resolvable.
        /// Only populated for `GitHost::Github`; always `None` for `Other`.
        /// Example: `<https://github.com/owner/repo/tree/main>`
        web_url: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitHost {
    /// Remote `origin` points at `github.com` in any of the supported forms
    /// (SSH `git@github.com:`, HTTPS `https://github.com/...`, or
    /// `ssh://git@github.com/...`).
    Github,
    /// Anything else — self-hosted gitea/forgejo, GitLab, Bitbucket,
    /// Azure DevOps, or a repo with no resolvable remote. Label collapses
    /// to the generic `git` prefix.
    Other,
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
        // even when the config lives in the common dir), while the remote URL
        // and host classification come from the common dir's `config`.
        let branch = parse_head(&work_dir);
        let (host, web_url) = resolve_host_and_url(&config_dir, &branch);
        MountKind::Git {
            branch,
            host,
            web_url,
        }
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

/// Parse `<config_dir>/config` to find the origin remote's URL, classify
/// the host, and (for GitHub only) transform into a web URL for the given
/// branch. The config dir is the main repo's `.git` for worktrees (see
/// `resolve_gitdirs`) and the per-repo `.git` for plain clones/submodules.
///
/// Returns `(GitHost, Option<String>)`:
/// - `GitHost::Github` + `Some(url)` when origin lives on github.com
/// - `GitHost::Other` + `None` for any other host, or when no remote is set
fn resolve_host_and_url(config_dir: &Path, branch: &GitBranch) -> (GitHost, Option<String>) {
    let config_path = config_dir.join("config");
    let Ok(content) = std::fs::read_to_string(&config_path) else {
        return (GitHost::Other, None);
    };
    let Some(remote_url) = parse_remote_origin_url(&content) else {
        return (GitHost::Other, None);
    };
    if !remote_points_at_github(&remote_url) {
        return (GitHost::Other, None);
    }
    let Some(base) = remote_to_web(&remote_url) else {
        return (GitHost::Other, None);
    };
    let url = match branch {
        GitBranch::Named(b) => format!("{base}/tree/{b}"),
        GitBranch::Detached { short_sha } => format!("{base}/commit/{short_sha}"),
        GitBranch::Unknown => base,
    };
    (GitHost::Github, Some(url))
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
            Self::Git { branch, host, .. } => {
                let prefix = match host {
                    GitHost::Github => "github",
                    GitHost::Other => "git",
                };
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
    fn inspect_classifies_github_remote_as_github_host() {
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
                host,
                web_url: Some(url),
                ..
            } => {
                assert_eq!(host, GitHost::Github);
                assert_eq!(url, "https://github.com/owner/repo/tree/main");
            }
            other => panic!("expected Git {{ host: Github, web_url: Some }}, got {other:?}"),
        }
    }

    #[test]
    fn inspect_classifies_gitlab_remote_as_other_host_with_no_url() {
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
            MountKind::Git { host, web_url, .. } => {
                assert_eq!(host, GitHost::Other);
                assert!(
                    web_url.is_none(),
                    "non-GitHub remote must not yield a web URL: {web_url:?}"
                );
            }
            other => panic!("expected Git {{ host: Other }}, got {other:?}"),
        }
    }

    #[test]
    fn inspect_classifies_repo_without_remote_as_other_host() {
        let temp = tempdir().unwrap();
        let git_dir = temp.path().join(".git");
        std::fs::create_dir(&git_dir).unwrap();
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        // No config file at all — simulates `git init` without a remote.
        let result = inspect(temp.path().to_str().unwrap());
        match result {
            MountKind::Git { host, web_url, .. } => {
                assert_eq!(host, GitHost::Other);
                assert!(web_url.is_none());
            }
            other => panic!("expected Git {{ host: Other }}, got {other:?}"),
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
                host,
                web_url: Some(url),
            } => {
                assert_eq!(b, "feature-x", "branch should come from worktree HEAD");
                assert_eq!(
                    host,
                    GitHost::Github,
                    "host must be resolved from commondir's config"
                );
                assert_eq!(url, "https://github.com/owner/repo/tree/feature-x");
            }
            other => panic!(
                "expected Git {{ host: Github, web_url: Some, branch: feature-x }}, got {other:?}"
            ),
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
                host: GitHost::Github,
                web_url: Some(url),
            } => {
                assert_eq!(b, "abs-branch");
                assert_eq!(url, "https://github.com/owner/repo/tree/abs-branch");
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
                host: GitHost::Github,
                web_url: Some(url),
            } => {
                assert_eq!(b, "submain");
                assert_eq!(url, "https://github.com/owner/submod/tree/submain");
            }
            other => panic!("expected submodule to resolve with GitHost::Github, got {other:?}"),
        }
    }

    #[test]
    fn label_formats_generic_git() {
        // Non-GitHub (or unresolved remote) mounts use the generic
        // `git · …` prefix.
        assert_eq!(MountKind::Missing.label(), "missing");
        assert_eq!(MountKind::Folder.label(), "folder");
        assert_eq!(
            MountKind::Git {
                branch: GitBranch::Named("main".into()),
                host: GitHost::Other,
                web_url: None,
            }
            .label(),
            "git · main"
        );
        assert_eq!(
            MountKind::Git {
                branch: GitBranch::Detached {
                    short_sha: "abc1234".into()
                },
                host: GitHost::Other,
                web_url: None,
            }
            .label(),
            "git · detached abc1234"
        );
        assert_eq!(
            MountKind::Git {
                branch: GitBranch::Unknown,
                host: GitHost::Other,
                web_url: None,
            }
            .label(),
            "git"
        );
    }

    #[test]
    fn label_formats_github_host() {
        // GitHub-hosted mounts get a `github · …` prefix so the operator
        // can tell which rows have an "open in browser" affordance.
        assert_eq!(
            MountKind::Git {
                branch: GitBranch::Named("main".into()),
                host: GitHost::Github,
                web_url: Some("https://github.com/owner/repo/tree/main".into()),
            }
            .label(),
            "github · main"
        );
        assert_eq!(
            MountKind::Git {
                branch: GitBranch::Detached {
                    short_sha: "abc1234".into()
                },
                host: GitHost::Github,
                web_url: Some("https://github.com/owner/repo/commit/abc1234".into()),
            }
            .label(),
            "github · detached abc1234"
        );
        assert_eq!(
            MountKind::Git {
                branch: GitBranch::Unknown,
                host: GitHost::Github,
                web_url: Some("https://github.com/owner/repo".into()),
            }
            .label(),
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
