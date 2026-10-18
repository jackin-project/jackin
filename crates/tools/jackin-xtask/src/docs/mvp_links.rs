// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::report;

pub(crate) const MVP_PACK_REL: &str = "plans/workflows/mvp";

pub(crate) fn check_mvp_links(root: &Path) -> Result<()> {
    let pack = root.join(MVP_PACK_REL);
    if !pack.is_dir() {
        bail!("MVP pack directory not found: {}", pack.display());
    }
    let mut files = Vec::new();
    collect_markdown(&pack, &mut files)?;
    let mut failures = Vec::new();
    for file in &files {
        check_mvp_links_file(root, file, &mut failures)?;
    }
    if failures.is_empty() {
        report_clean();
        return Ok(());
    }
    failures.sort();
    bail!(
        "MVP pack has {} broken local link(s):\n  {}",
        failures.len(),
        failures.join("\n  ")
    )
}

fn collect_markdown(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in crate::fs_util::read_dir_sorted(dir)? {
        let path = entry.path();
        if path.is_dir() {
            collect_markdown(&path, files)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            files.push(path);
        }
    }
    Ok(())
}

fn check_mvp_links_file(root: &Path, file: &Path, failures: &mut Vec<String>) -> Result<()> {
    let text = fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    let display = relative(root, file);
    let base = file.parent().unwrap_or(root);
    let mut in_fence = false;
    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        for target in inline_link_targets(line) {
            check_target(root, &display, base, idx + 1, target, failures);
        }
    }
    Ok(())
}

fn inline_link_targets(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let mut targets = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b']' && bytes[i + 1] == b'(' {
            if let Some(end) = line[i + 2..].find(')') {
                targets.push(line[i + 2..i + 2 + end].trim());
                i += end + 2;
                continue;
            }
            break;
        }
        i += 1;
    }
    targets
}

fn check_target(
    root: &Path,
    display: &str,
    base: &Path,
    line_no: usize,
    raw: &str,
    failures: &mut Vec<String>,
) {
    let mut target = raw.trim();
    if target.starts_with('<') && target.ends_with('>') && target.len() >= 2 {
        target = target[1..target.len() - 1].trim();
    }
    if let Some(first) = target.split_whitespace().next() {
        target = first;
    }
    if target.is_empty() || target.starts_with('#') || target.contains("://") {
        return;
    }
    if target.starts_with("mailto:") {
        return;
    }
    let path_part = target.split('#').next().unwrap_or("");
    if path_part.is_empty() {
        return;
    }
    let resolved = if let Some(rest) = path_part.strip_prefix('/') {
        root.join(rest)
    } else {
        base.join(path_part)
    };
    if fs::symlink_metadata(&resolved).is_err() {
        failures.push(format!("{display}:{line_no}: {target}"));
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn report_clean() {
    #[expect(
        clippy::print_stdout,
        reason = "jackin-xtask is a CLI; the audit result is its output"
    )]
    {
        if report::human_output() {
            println!("mvp links OK - MVP pack local links resolve.");
        }
    }
}

#[cfg(test)]
mod tests;
