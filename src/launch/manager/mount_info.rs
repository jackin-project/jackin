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
        /// URL for the branch on the git host, if resolvable.
        /// Example: `<https://github.com/owner/repo/tree/main>`
        ///
        /// NOTE: not read by the ratatui render path because `Paragraph`
        /// strips raw ESC bytes needed for OSC 8 hyperlinks. Retained for
        /// future use when a raw-terminal-write path is available.
        #[allow(dead_code)]
        web_url: Option<String>,
    },
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
    resolve_git_dir(path).map_or(MountKind::Folder, |gd| {
        let branch = parse_head(&gd);
        let web_url = resolve_web_url(&gd, &branch);
        MountKind::Git { branch, web_url }
    })
}

/// Resolve the `.git` directory for a workdir. Returns None if not a git repo.
fn resolve_git_dir(workdir: &Path) -> Option<PathBuf> {
    let dotgit = workdir.join(".git");
    if dotgit.is_dir() {
        return Some(dotgit);
    }
    if dotgit.is_file() {
        // Submodule: .git file contains "gitdir: <path>" (relative or absolute).
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
        if abs.is_dir() {
            return Some(abs);
        }
    }
    None
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

/// Parse `<git_dir>/config` to find the origin remote's URL,
/// then transform into a web URL for the given branch.
fn resolve_web_url(git_dir: &Path, branch: &GitBranch) -> Option<String> {
    let config_path = git_dir.join("config");
    let content = std::fs::read_to_string(&config_path).ok()?;
    // Find the [remote "origin"] section and its url = ... line.
    let remote_url = parse_remote_origin_url(&content)?;
    let base = remote_to_web(&remote_url)?;

    match branch {
        GitBranch::Named(b) => Some(format!("{base}/tree/{b}")),
        GitBranch::Detached { short_sha } => Some(format!("{base}/commit/{short_sha}")),
        GitBranch::Unknown => Some(base),
    }
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

/// Transform a git remote URL into a web URL base.
///
/// Supported shapes (all trailing `.git` stripped):
/// - `git@github.com:owner/repo.git` → `https://github.com/owner/repo`
/// - `https://github.com/owner/repo.git` → `https://github.com/owner/repo`
/// - `ssh://git@github.com/owner/repo.git` → `https://github.com/owner/repo`
/// - `git@gitlab.com:owner/repo.git` → `https://gitlab.com/owner/repo`
fn remote_to_web(remote: &str) -> Option<String> {
    // Strip trailing .git
    let remote = remote.strip_suffix(".git").unwrap_or(remote);

    // SSH scp-style: git@host:owner/repo
    if let Some(rest) = remote.strip_prefix("git@")
        && let Some((host, path)) = rest.split_once(':')
    {
        return Some(format!("https://{host}/{path}"));
    }
    // HTTPS as-is
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

/// Wrap `text` in OSC 8 hyperlink escape codes targeting `url`.
/// Unsupported terminals display just `text` (the escapes are invisible).
///
/// NOTE: currently unused from the ratatui render path because `Paragraph`
/// strips raw ESC bytes. Kept for future use when a raw-terminal-write path
/// becomes available.
#[allow(dead_code)]
fn osc8_link(url: &str, text: &str) -> String {
    format!("\x1b]8;;{url}\x1b\\{text}\x1b]8;;\x1b\\")
}

impl MountKind {
    /// Short label for display next to a mount.
    pub fn label(&self) -> String {
        match self {
            Self::Missing => "missing".to_string(),
            Self::Folder => "folder".to_string(),
            Self::Git { branch, .. } => match branch {
                GitBranch::Named(b) => format!("git · {b}"),
                GitBranch::Detached { short_sha } => format!("git · detached {short_sha}"),
                GitBranch::Unknown => "git".to_string(),
            },
        }
    }

    /// Label with branch as OSC 8 hyperlink if possible.
    /// Falls back to plain label when no web URL resolved.
    ///
    /// NOTE: Ratatui's Paragraph widget may strip or mangle the ESC bytes
    /// from OSC 8 sequences when measuring/rendering text. If rendered output
    /// shows escape garbage, fall back to `label()` — we cannot fix this at
    /// the ratatui layer without raw terminal writes.
    ///
    /// Currently unused from the render path for the above reason; wired up
    /// for future use once a raw-terminal-write path exists.
    #[allow(dead_code)]
    pub fn labeled_hyperlink(&self) -> String {
        match self {
            Self::Git {
                branch: GitBranch::Named(b),
                web_url: Some(url),
            } => {
                format!("git · {}", osc8_link(url, b))
            }
            Self::Git {
                branch: GitBranch::Detached { short_sha },
                web_url: Some(url),
            } => {
                format!("git · detached {}", osc8_link(url, short_sha))
            }
            _ => self.label(),
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
            other => panic!("expected Git {{ branch: Named }}, got {:?}", other),
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
            other => panic!("expected Git {{ branch: Detached }}, got {:?}", other),
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
            other => panic!("expected submodule resolution, got {:?}", other),
        }
    }

    #[test]
    fn label_formats() {
        assert_eq!(MountKind::Missing.label(), "missing");
        assert_eq!(MountKind::Folder.label(), "folder");
        assert_eq!(
            MountKind::Git {
                branch: GitBranch::Named("main".into()),
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
                web_url: None,
            }
            .label(),
            "git · detached abc1234"
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
    fn remote_to_web_gitlab() {
        assert_eq!(
            remote_to_web("git@gitlab.com:owner/repo.git"),
            Some("https://gitlab.com/owner/repo".to_string())
        );
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
