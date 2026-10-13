// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Claude Code's macOS Keychain service-name derivation, shared by instance
//! provisioning and the host usage probe so the two can never disagree about
//! which generic-password service backs a given `CLAUDE_CONFIG_DIR`.
//!
//! Pure string/path derivation only — no Keychain or filesystem I/O. The one
//! [`claude_keychain_scope`] entry normalizes the config dir and hashes that
//! same normalized value, so a caller cannot normalize and hash differently.

use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

/// Base macOS Keychain generic-password service Claude Code uses for the
/// default `~/.claude` config dir. A custom config dir uses this base plus a
/// `-<suffix>` where `<suffix>` is the first eight lowercase hex characters of
/// the SHA-256 of the normalized absolute config-dir path.
pub const CLAUDE_KEYCHAIN_SERVICE_BASE: &str = "Claude Code-credentials";

/// Resolved Keychain scope for one effective `CLAUDE_CONFIG_DIR`: the
/// normalized absolute config directory, the derived generic-password service,
/// and whether it is the default `~/.claude` scope. Secret-free.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeKeychainScope {
    /// Absolute, lexically normalized config directory (symlinks not resolved).
    pub normalized_config_dir: PathBuf,
    /// Derived macOS Keychain generic-password service name.
    pub service: String,
    /// `true` when this is the default `~/.claude` scope (bare base service).
    pub is_default: bool,
}

/// Derive the Claude Keychain scope for `config_dir`, resolving relative paths
/// against `current_dir` and comparing against `home`'s `.claude`.
///
/// Normalization makes the path absolute and collapses lexical `.`/`..`; it
/// never filesystem-canonicalizes symlinks. A non-UTF-8 normalized path yields
/// `None` rather than a guessed service. The default `~/.claude` maps to the
/// bare [`CLAUDE_KEYCHAIN_SERVICE_BASE`]; every other absolute path maps to the
/// base plus `-` and the first eight lowercase hex SHA-256 characters of the
/// normalized path bytes.
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

/// Collapse lexical `.` and `..` without resolving symlinks. Roots and prefixes
/// are preserved; a `..` with no normal component to pop is kept verbatim.
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
