use std::path::{Component, Path, PathBuf};

fn home_dir() -> Option<String> {
    directories::BaseDirs::new().map(|b| b.home_dir().display().to_string())
}

pub fn expand_tilde(path: &str) -> String {
    if (path == "~" || path.starts_with("~/"))
        && let Some(home) = home_dir()
    {
        return path.replacen('~', &home, 1);
    }

    path.to_string()
}

/// Normalize an absolute path by resolving `.` and `..` components without
/// touching the filesystem (unlike [`std::fs::canonicalize`]).
fn normalize_path(path: &Path) -> PathBuf {
    let mut parts: Vec<Component<'_>> = Vec::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                if let Some(Component::Normal(_)) = parts.last() {
                    parts.pop();
                }
            }
            Component::CurDir => {}
            c => parts.push(c),
        }
    }
    parts.iter().collect()
}

/// Expand tilde, resolve relative paths to absolute using the current working
/// directory, and normalize `.` / `..` components.
pub fn resolve_path(path: &str) -> String {
    let expanded = expand_tilde(path);
    let abs = if expanded.starts_with('/') {
        PathBuf::from(&expanded)
    } else if let Ok(cwd) = std::env::current_dir() {
        cwd.join(&expanded)
    } else {
        return expanded;
    };
    normalize_path(&abs).display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_path_resolves_relative_to_cwd() {
        let cwd = std::env::current_dir().unwrap();
        let resolved = resolve_path("my-project");

        assert_eq!(resolved, cwd.join("my-project").display().to_string());
        assert!(resolved.starts_with('/'));
    }

    #[test]
    fn resolve_path_leaves_absolute_unchanged() {
        assert_eq!(resolve_path("/workspace/project"), "/workspace/project");
    }

    #[test]
    fn resolve_path_normalizes_dot_to_cwd() {
        let cwd = std::env::current_dir().unwrap();
        let resolved = resolve_path(".");

        assert_eq!(resolved, cwd.display().to_string());
    }

    #[test]
    fn resolve_path_normalizes_parent_component() {
        let cwd = std::env::current_dir().unwrap();
        let resolved = resolve_path("../sibling");
        let expected = cwd.parent().unwrap().join("sibling");

        assert_eq!(resolved, expected.display().to_string());
        assert!(!resolved.contains(".."));
    }

    #[test]
    fn resolve_path_normalizes_absolute_with_dotdot() {
        assert_eq!(resolve_path("/a/b/../c"), "/a/c");
    }

    #[test]
    fn normalize_path_handles_multiple_parent_refs() {
        let path = Path::new("/a/b/c/../../d");
        assert_eq!(normalize_path(path), PathBuf::from("/a/d"));
    }

    #[test]
    fn normalize_path_preserves_root_on_excessive_parents() {
        let path = Path::new("/a/../../../b");
        assert_eq!(normalize_path(path), PathBuf::from("/b"));
    }
}
