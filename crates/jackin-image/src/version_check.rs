//! Cache the agent binary version baked into a Docker image to skip redundant version probes.
//!
//! One cache file per image per agent under `~/.jackin/cache/image-<agent>-version/`.
//! Not responsible for downloading binaries or building images — only the
//! read/write of the cached version string.

use jackin_core::{Agent, JackinPaths};
use std::fmt::Arguments;
use std::io::Write as _;
use std::path::PathBuf;

fn stderr_line(args: Arguments<'_>) {
    let mut stderr = std::io::stderr().lock();
    drop(writeln!(stderr, "{args}"));
}

/// Canonical cache path for a given agent + image combination.
///
/// All five per-agent path variants collapse here because the only difference
/// between them was the agent slug embedded in the directory name.
fn image_version_cache_path(paths: &JackinPaths, agent: Agent, image: &str) -> PathBuf {
    paths
        .cache_dir
        .join(format!("image-{}-version/{image}", agent.runtime().slug()))
}

/// Read the cached agent version we stored for a previously-built image.
pub fn stored_version(paths: &JackinPaths, agent: Agent, image: &str) -> Option<String> {
    let path = image_version_cache_path(paths, agent, image);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_owned())
}

/// Persist the agent version that was just installed into an image.
pub fn store_version(paths: &JackinPaths, agent: Agent, image: &str, version: &str) {
    let path = image_version_cache_path(paths, agent, image);
    drop(write_cached(&path, version));
}

// ── Backward-compat shims — keep old names for callers and tests ──────────────

/// Read the Claude Code version we stored for a previously-built image.
pub fn stored_image_version(paths: &JackinPaths, image: &str) -> Option<String> {
    stored_version(paths, Agent::Claude, image)
}

/// Persist the Claude Code version that was just installed into an image.
pub fn store_image_version(paths: &JackinPaths, image: &str, version: &str) {
    store_version(paths, Agent::Claude, image, version);
}

/// Read the `OpenCode` version we stored for a previously-built image.
pub fn stored_opencode_version(paths: &JackinPaths, image: &str) -> Option<String> {
    stored_version(paths, Agent::Opencode, image)
}

/// Persist the `OpenCode` version that was just installed into an image.
pub fn store_opencode_version(paths: &JackinPaths, image: &str, version: &str) {
    store_version(paths, Agent::Opencode, image, version);
}

/// Read the Kimi version we stored for a previously-built image.
pub fn stored_kimi_version(paths: &JackinPaths, image: &str) -> Option<String> {
    stored_version(paths, Agent::Kimi, image)
}

/// Persist the Kimi version that was just installed into an image.
pub fn store_kimi_version(paths: &JackinPaths, image: &str, version: &str) {
    store_version(paths, Agent::Kimi, image, version);
}

pub async fn needs_agent_update(paths: &JackinPaths, image: &str, agent: Agent) -> bool {
    let Some(installed) = stored_version(paths, agent, image) else {
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
        .map(|s| s.trim().to_owned())
}

/// Persist the `JACKIN_CACHE_BUST` value used for a build so that
/// subsequent non-rebuild launches replay the same value and hit the
/// same Docker cache layer.
pub fn store_cache_bust(paths: &JackinPaths, image: &str, value: &str) {
    let path = cache_bust_path(paths, image);
    if let Err(e) = write_cached(&path, value) {
        stderr_line(format_args!(
            "warning: failed to persist JACKIN_CACHE_BUST for {image}: {e}; \
             subsequent non-rebuild launches may replay the wrong cache layer"
        ));
    }
}

// ── Version-string parsers ────────────────────────────────────────────────────
//
// Each delegates to the corresponding `AgentRuntime::parse_version` adapter so
// the parsing logic lives in one place (the adapter). The standalone public
// functions are kept for backward compatibility with callers and tests.

/// Extract a bare semver string from `claude --version` output.
///
/// The command returns e.g. `"2.1.96 (Claude Code)"` but we only need the
/// `"2.1.96"` portion to compare against the npm registry.  Returns `None`
/// when the output doesn't look like a version string.
pub fn parse_claude_version(raw: &str) -> Option<&str> {
    Agent::Claude.runtime().parse_version(raw)
}

/// Extract a bare semver string from `kimi --version` output.
///
/// The command returns e.g. `"kimi 1.2.3"` but we only need the `"1.2.3"`
/// portion. Returns `None` when the output doesn't look like a version string.
pub fn parse_kimi_version(raw: &str) -> Option<&str> {
    Agent::Kimi.runtime().parse_version(raw)
}

/// Extract a bare semver string from `opencode --version` output.
///
/// The command returns e.g. `"1.14.48"` or `"v1.14.48"`. Strip a leading `v`
/// if present, then validate it looks like a semver.
pub fn parse_opencode_version(raw: &str) -> Option<&str> {
    Agent::Opencode.runtime().parse_version(raw)
}

pub fn parse_amp_version(raw: &str) -> Option<&str> {
    Agent::Amp.runtime().parse_version(raw)
}

pub fn parse_codex_version(raw: &str) -> Option<&str> {
    Agent::Codex.runtime().parse_version(raw)
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
