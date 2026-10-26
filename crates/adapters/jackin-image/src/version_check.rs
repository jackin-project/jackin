// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, JackinPaths};
use std::fmt::Arguments;
use std::io::Write as _;
use std::path::PathBuf;

fn stderr_line(args: Arguments<'_>) {
    let mut stderr = std::io::stderr().lock();
    drop(writeln!(stderr, "{args}"));
}

fn image_version_cache_path(paths: &JackinPaths, agent: Agent, image: &str) -> PathBuf {
    paths
        .cache_dir
        .join(format!("image-{}-version/{image}", agent.runtime().slug()))
}

pub fn stored_version(paths: &JackinPaths, agent: Agent, image: &str) -> Option<String> {
    let path = image_version_cache_path(paths, agent, image);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_owned())
}

pub fn store_version(paths: &JackinPaths, agent: Agent, image: &str, version: &str) {
    let path = image_version_cache_path(paths, agent, image);
    if let Err(e) = write_cached(&path, version) {
        stderr_line(format_args!(
            "warning: failed to cache {agent} version for {image}: {e}; \
             subsequent launch-time version checks will re-probe the image"
        ));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentVersionCheck {
    Stale,
    UpToDate,
    Unknown,
}

pub async fn needs_agent_update(
    paths: &JackinPaths,
    image: &str,
    agent: Agent,
) -> AgentVersionCheck {
    let Some(installed) = stored_version(paths, agent, image) else {
        return AgentVersionCheck::UpToDate;
    };
    let Some(latest) = crate::agent_binary::latest_release(paths, agent).await else {
        return AgentVersionCheck::Unknown;
    };
    if installed == latest.version {
        AgentVersionCheck::UpToDate
    } else {
        AgentVersionCheck::Stale
    }
}

fn cache_bust_path(paths: &JackinPaths, image: &str) -> PathBuf {
    paths.cache_dir.join(format!("image-cache-bust/{image}"))
}

pub fn stored_cache_bust(paths: &JackinPaths, image: &str) -> Option<String> {
    let path = cache_bust_path(paths, image);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_owned())
}

pub fn store_cache_bust(paths: &JackinPaths, image: &str, value: &str) {
    let path = cache_bust_path(paths, image);
    if let Err(e) = write_cached(&path, value) {
        stderr_line(format_args!(
            "warning: failed to persist JACKIN_CACHE_BUST for {image}: {e}; \
             subsequent non-rebuild launches may replay the wrong cache layer"
        ));
    }
}

fn write_cached(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}
