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
