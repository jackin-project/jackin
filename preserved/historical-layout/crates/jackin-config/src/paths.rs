// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Path helpers: tilde expansion, path normalization (without filesystem access).
//!
//! Not responsible for mount parsing or workspace config — purely string/path
//! manipulation. `expand_tilde` and `resolve_path` are the only entry points;
//! `normalize_path` is internal.

use std::path::{Component, Path, PathBuf};

fn home_dir() -> Option<String> {
    directories::BaseDirs::new().map(|b| b.home_dir().display().to_string())
}

/// Expand a leading `~` or `~/` to the operator's home directory.
pub fn expand_tilde(path: &str) -> String {
    if (path == "~" || path.starts_with("~/"))
        && let Some(home) = home_dir()
    {
        return path.replacen('~', &home, 1);
    }

    path.to_owned()
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

/// Resolve a path to one stable filesystem identity.
///
/// Existing components are resolved through symlinks. If the leaf or a
/// descendant does not exist yet, the nearest existing ancestor is resolved
/// and the remaining components are appended after lexical normalization.
/// This keeps supported paths usable before their directories are created
/// while making equivalent existing paths compare identically. Callers that
/// preserve configured path spelling should retain the original path.
pub(crate) fn canonical_path_identity(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }

    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_or_else(|_| path.to_path_buf(), |cwd| cwd.join(path))
    };
    let normalized = normalize_path(&absolute);
    let mut missing = Vec::new();
    let mut existing = normalized.as_path();

    loop {
        if let Ok(canonical) = existing.canonicalize() {
            let mut result = canonical;
            for component in missing.iter().rev() {
                result.push(component);
            }
            return result;
        }

        let Some(name) = existing.file_name() else {
            return normalized;
        };
        missing.push(name.to_owned());
        let Some(parent) = existing.parent() else {
            return normalized;
        };
        existing = parent;
    }
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
mod tests;
