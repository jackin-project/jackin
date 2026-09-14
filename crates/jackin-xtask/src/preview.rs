// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Homebrew preview source-change classification.
#![allow(dead_code)] // exercised by unit tests; workflow template will call via xtask next

#[cfg(test)]
mod tests;

/// Whether a changed path can alter the shipped Homebrew preview binaries.
pub(crate) fn path_affects_preview(path: &str) -> bool {
    match path {
        ".github/workflows/preview.yml"
        | "Cargo.toml"
        | "Cargo.lock"
        | "rust-toolchain.toml"
        | "build.rs" => true,
        path if path.starts_with("src/") => true,
        path if path.starts_with("docker/runtime/") => true,
        path if path.starts_with("crates/") => true,
        "mise.toml" => false,
        _ => false,
    }
}

/// Whether a mise.toml diff affects release-relevant tool pins.
pub(crate) fn mise_release_tools_changed(base: &str, head: &str) -> bool {
    extract_release_mise(base) != extract_release_mise(head)
}

/// Classify whether any changed path requires a preview republish.
pub(crate) fn classify_preview_source(changed_paths: &[&str], mise_changed: bool) -> bool {
    changed_paths
        .iter()
        .any(|path| path_affects_preview(path) || (*path == "mise.toml" && mise_changed))
}

fn extract_release_mise(source: &str) -> Vec<String> {
    let mut section = "";
    let mut lines = Vec::new();
    for line in source.lines() {
        if line.trim_start().starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if line == "[tools]" {
            section = "tools";
            continue;
        }
        if line == "[tool_alias]" {
            section = "tool_alias";
            continue;
        }
        if let Some(rest) = line.strip_prefix("[tools.") {
            section = rest.trim_end_matches(']');
            continue;
        }
        if line.starts_with('[') {
            section = "";
            continue;
        }
        let key = line
            .split('=')
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches('"');
        if keep_release_mise_key(section, key) {
            lines.push(format!("{section}.{key}={line}"));
        }
    }
    lines.sort_unstable();
    lines
}

fn keep_release_mise_key(section: &str, key: &str) -> bool {
    matches!(
        key,
        "zig" | "cosign" | "syft" | "cargo-zigbuild" | "sccache"
    ) && (section == "tools" || section == "tool_alias" || !section.is_empty())
}

/// Parse the canonical source commit from a preview release body.
pub(crate) fn preview_commit_from_body(body: &str) -> Option<String> {
    for line in body.lines() {
        let Some((_, url)) = line.split_once("](") else {
            continue;
        };
        let url = url.trim_end_matches(|ch: char| !ch.is_ascii_hexdigit());
        let sha = url.rsplit('/').next()?;
        if sha.len() == 40 && sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Some(sha.to_ascii_lowercase());
        }
    }
    None
}
