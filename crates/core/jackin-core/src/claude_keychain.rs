// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

pub const CLAUDE_KEYCHAIN_SERVICE_BASE: &str = "Claude Code-credentials";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeKeychainScope {
    pub normalized_config_dir: PathBuf,

    pub service: String,

    pub is_default: bool,
}

pub fn claude_keychain_scope(
    config_dir: &Path,
    home: &Path,
    current_dir: &Path,
) -> Option<ClaudeKeychainScope> {
    let absolute = if config_dir.is_absolute() {
        config_dir.to_path_buf()
    } else {
        current_dir.join(config_dir)
    };
    let normalized = normalize_lexical(&absolute);
    let path_str = normalized.to_str()?;

    let default_dir = normalize_lexical(&home.join(".claude"));
    let is_default = normalized == default_dir;

    let service = if is_default {
        CLAUDE_KEYCHAIN_SERVICE_BASE.to_owned()
    } else {
        let digest = Sha256::digest(path_str.as_bytes());
        let mut suffix = hex::encode(digest);
        suffix.truncate(8);
        format!("{CLAUDE_KEYCHAIN_SERVICE_BASE}-{suffix}")
    };

    Some(ClaudeKeychainScope {
        normalized_config_dir: normalized,
        service,
        is_default,
    })
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                } else {
                    out.push(component.as_os_str());
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests;
