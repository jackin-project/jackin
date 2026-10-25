// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum MountKind {
    Missing,
    Folder,
    Git {
        branch: GitBranch,
        origin: Option<GitOrigin>,
    },
}

#[derive(Debug, Clone)]
pub enum GitOrigin {
    Github { remote_url: String, web_url: String },
    Other { remote_url: String },
}

impl GitOrigin {
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
        return MountKind::Folder;
    }
    resolve_gitdirs(path).map_or(MountKind::Folder, |(work_dir, config_dir)| {
        let branch = parse_head(&work_dir);
        let origin = resolve_origin(&config_dir, &branch);
        MountKind::Git { branch, origin }
    })
}

fn resolve_gitdirs(workdir: &Path) -> Option<(PathBuf, PathBuf)> {
    let dotgit = workdir.join(".git");
    if dotgit.is_dir() {
        return Some((dotgit.clone(), dotgit));
    }
    if !dotgit.is_file() {
        return None;
    }
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

fn remote_points_at_github(remote: &str) -> bool {
    if let Some(rest) = remote.strip_prefix("git@") {
        return rest.starts_with("github.com:");
    }
    if let Some(rest) = remote.strip_prefix("ssh://") {
        let rest = rest.strip_prefix("git@").unwrap_or(rest);
        return rest.starts_with("github.com/");
    }
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
            let rest = rest.trim_start_matches([' ', '\t', '=']);
            return Some(rest.trim().to_owned());
        }
    }
    None
}

fn remote_to_web(remote: &str) -> Option<String> {
    if !remote_points_at_github(remote) {
        return None;
    }
    let remote = remote.strip_suffix(".git").unwrap_or(remote);

    if let Some(rest) = remote.strip_prefix("git@")
        && let Some((host, path)) = rest.split_once(':')
    {
        return Some(format!("https://{host}/{path}"));
    }
    if remote.starts_with("https://") || remote.starts_with("http://") {
        return Some(remote.to_owned());
    }
    if let Some(rest) = remote.strip_prefix("ssh://") {
        let rest = rest.strip_prefix("git@").unwrap_or(rest);
        if let Some((host, path)) = rest.split_once('/') {
            return Some(format!("https://{host}/{path}"));
        }
    }
    None
}

impl MountKind {
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
