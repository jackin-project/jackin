// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
/// can't ask for a web URL on a non-github remote or hold separate
/// `host`, `remote_url`, and `web_url` fields that drift out of sync.
#[derive(Debug, Clone)]
pub enum GitOrigin {
    /// Remote `origin` points at `github.com` (SSH `git@github.com:`,
    /// HTTPS `https://github.com/...`, or `ssh://git@github.com/...`).
    /// `web_url` is the resolved branch/commit page on github.com.
    ///
    /// `remote_url` is preserved here for diagnostic and Debug output
    /// even when a caller only needs `web_url`.
    Github { remote_url: String, web_url: String },
    /// Self-hosted gitea/forgejo, GitLab, Bitbucket, Azure DevOps, etc.
    /// We expose the raw remote URL but no web URL — we don't speak the
    /// branch-URL conventions of these hosts.
    Other { remote_url: String },
}

impl GitOrigin {
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
        // and its github/other classification come from the common dir's `config`.
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
/// Without following the `commondir` redirect, `resolve_origin`
/// reads nothing, the origin resolves to `None`, and the label
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
            .map(|rest| rest.trim().to_owned())
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
        |rest| GitBranch::Named(rest.to_owned()),
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
            return Some(rest.trim().to_owned());
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
        return Some(remote.to_owned());
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
            Self::Missing => "missing".to_owned(),
            Self::Folder => "folder".to_owned(),
            Self::Git { branch, origin } => {
                let prefix = origin.as_ref().map_or("git", |o| o.display_prefix());
                match branch {
                    GitBranch::Named(b) => format!("{prefix} · {b}"),
                    GitBranch::Detached { short_sha } => {
                        format!("{prefix} · detached {short_sha}")
                    }
                    GitBranch::Unknown => prefix.to_owned(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
