//! Cache the agent binary version baked into a Docker image to skip redundant version probes.
//!
//! One cache file per image per agent under `~/.jackin/cache/image-<agent>-version/`.
//! Not responsible for downloading binaries or building images — only the
//! read/write of the cached version string.

use jackin_core::{Agent, JackinPaths};
use std::path::PathBuf;

/// File that records the Claude Code version baked into a given Docker image.
fn image_version_path(paths: &JackinPaths, image: &str) -> PathBuf {
    paths
        .cache_dir
        .join(format!("image-claude-version/{image}"))
}

/// File that records the `OpenCode` version baked into a given `Docker` image.
fn opencode_image_version_path(paths: &JackinPaths, image: &str) -> PathBuf {
    paths
        .cache_dir
        .join(format!("image-opencode-version/{image}"))
}

/// File that records the Kimi version baked into a given Docker image.
fn kimi_image_version_path(paths: &JackinPaths, image: &str) -> PathBuf {
    paths.cache_dir.join(format!("image-kimi-version/{image}"))
}

fn amp_image_version_path(paths: &JackinPaths, image: &str) -> PathBuf {
    paths.cache_dir.join(format!("image-amp-version/{image}"))
}

fn codex_image_version_path(paths: &JackinPaths, image: &str) -> PathBuf {
    paths.cache_dir.join(format!("image-codex-version/{image}"))
}

/// Read the Claude Code version we stored for a previously-built image.
pub fn stored_image_version(paths: &JackinPaths, image: &str) -> Option<String> {
    let path = image_version_path(paths, image);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Persist the Claude Code version that was just installed into an image.
pub fn store_image_version(paths: &JackinPaths, image: &str, version: &str) {
    let path = image_version_path(paths, image);
    let _ = write_cached(&path, version);
}

/// Read the `OpenCode` version we stored for a previously-built image.
pub fn stored_opencode_version(paths: &JackinPaths, image: &str) -> Option<String> {
    let path = opencode_image_version_path(paths, image);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Persist the `OpenCode` version that was just installed into an image.
pub fn store_opencode_version(paths: &JackinPaths, image: &str, version: &str) {
    let path = opencode_image_version_path(paths, image);
    let _ = write_cached(&path, version);
}

/// Read the Kimi version we stored for a previously-built image.
pub fn stored_kimi_version(paths: &JackinPaths, image: &str) -> Option<String> {
    let path = kimi_image_version_path(paths, image);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Persist the Kimi version that was just installed into an image.
pub fn store_kimi_version(paths: &JackinPaths, image: &str, version: &str) {
    let path = kimi_image_version_path(paths, image);
    let _ = write_cached(&path, version);
}

pub fn stored_amp_version(paths: &JackinPaths, image: &str) -> Option<String> {
    let path = amp_image_version_path(paths, image);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

pub fn store_amp_version(paths: &JackinPaths, image: &str, version: &str) {
    let path = amp_image_version_path(paths, image);
    let _ = write_cached(&path, version);
}

pub fn stored_codex_version(paths: &JackinPaths, image: &str) -> Option<String> {
    let path = codex_image_version_path(paths, image);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

pub fn store_codex_version(paths: &JackinPaths, image: &str, version: &str) {
    let path = codex_image_version_path(paths, image);
    let _ = write_cached(&path, version);
}

pub async fn needs_agent_update(paths: &JackinPaths, image: &str, agent: Agent) -> bool {
    let installed = match agent {
        Agent::Claude => stored_image_version(paths, image),
        Agent::Codex => stored_codex_version(paths, image),
        Agent::Amp => stored_amp_version(paths, image),
        Agent::Kimi => stored_kimi_version(paths, image),
        Agent::Opencode => stored_opencode_version(paths, image),
    };
    let Some(installed) = installed else {
        return false;
    };
    let Some(latest) = crate::agent_binary::latest_release(paths, agent).await else {
        return false;
    };
    installed != latest.version
}

/// File that records the last `JACKIN_CACHE_BUST` value used to build an image.
fn cache_bust_path(paths: &JackinPaths, image: &str) -> PathBuf {
    paths.cache_dir.join(format!("image-cache-bust/{image}"))
}

/// Read the last `JACKIN_CACHE_BUST` value used for an image build.
pub fn stored_cache_bust(paths: &JackinPaths, image: &str) -> Option<String> {
    let path = cache_bust_path(paths, image);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Persist the `JACKIN_CACHE_BUST` value used for a build so that
/// subsequent non-rebuild launches replay the same value and hit the
/// same Docker cache layer.
pub fn store_cache_bust(paths: &JackinPaths, image: &str, value: &str) {
    let path = cache_bust_path(paths, image);
    if let Err(e) = write_cached(&path, value) {
        eprintln!(
            "warning: failed to persist JACKIN_CACHE_BUST for {image}: {e}; \
             subsequent non-rebuild launches may replay the wrong cache layer"
        );
    }
}

/// Extract a bare semver string from `claude --version` output.
///
/// The command returns e.g. `"2.1.96 (Claude Code)"` but we only need the
/// `"2.1.96"` portion to compare against the npm registry.  Returns `None`
/// when the output doesn't look like a version string.
pub fn parse_claude_version(raw: &str) -> Option<&str> {
    let token = raw.split_whitespace().next()?;
    if token.split('.').count() < 2 || !token.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    Some(token)
}

/// Extract a bare semver string from `kimi --version` output.
///
/// The command returns e.g. `"kimi 1.2.3"` but we only need the `"1.2.3"`
/// portion. Returns `None` when the output doesn't look like a version string.
pub fn parse_kimi_version(raw: &str) -> Option<&str> {
    let mut tokens = raw.split_whitespace();
    let first = tokens.next()?;
    if first.split('.').count() >= 2 && first.starts_with(|c: char| c.is_ascii_digit()) {
        return Some(first);
    }
    let second = tokens.next()?;
    if second.split('.').count() >= 2 && second.starts_with(|c: char| c.is_ascii_digit()) {
        return Some(second);
    }
    None
}

/// Extract a bare semver string from `opencode --version` output.
///
/// The command returns e.g. `"1.14.48"` or `"v1.14.48"`. Strip a leading `v`
/// if present, then validate it looks like a semver.
pub fn parse_opencode_version(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    let token = trimmed.strip_prefix('v').unwrap_or(trimmed);
    if token.split('.').count() < 2 || !token.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    Some(token)
}

pub fn parse_amp_version(raw: &str) -> Option<&str> {
    raw.split_whitespace().find(|token| {
        token.split('.').count() >= 2 && token.starts_with(|c: char| c.is_ascii_digit())
    })
}

pub fn parse_codex_version(raw: &str) -> Option<&str> {
    raw.split_whitespace().find(|token| {
        token.split('.').count() >= 2 && token.starts_with(|c: char| c.is_ascii_digit())
    })
}

/// Write content to a cache file, creating parent directories as needed.
fn write_cached(path: &PathBuf, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

#[cfg(test)]
mod tests;
