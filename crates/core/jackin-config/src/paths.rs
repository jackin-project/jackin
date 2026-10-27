// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
