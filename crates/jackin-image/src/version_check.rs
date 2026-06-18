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
    if let Err(e) = write_cached(&path, version) {
        stderr_line(format_args!(
            "warning: failed to cache {agent} version for {image}: {e}; \
             subsequent launch-time version checks will re-probe the image"
        ));
    }
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

/// Write content to a cache file, creating parent directories as needed.
fn write_cached(path: &PathBuf, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

#[cfg(test)]
mod tests;
