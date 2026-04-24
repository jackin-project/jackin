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
    Git { branch: GitBranch },
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
        MountKind::Git { branch }
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

impl MountKind {
    /// Short label for display next to a mount.
    pub fn label(&self) -> String {
        match self {
            Self::Missing => "missing".to_string(),
            Self::Folder => "folder".to_string(),
            Self::Git { branch } => match branch {
                GitBranch::Named(b) => format!("git · {b}"),
                GitBranch::Detached { short_sha } => format!("git · detached {short_sha}"),
                GitBranch::Unknown => "git".to_string(),
            },
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
                branch: GitBranch::Named("main".into())
            }
            .label(),
            "git · main"
        );
        assert_eq!(
            MountKind::Git {
                branch: GitBranch::Detached {
                    short_sha: "abc1234".into()
                }
            }
            .label(),
            "git · detached abc1234"
        );
    }
}
