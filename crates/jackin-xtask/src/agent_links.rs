//! No-cross-reference gate for `README.md` and `AGENTS.md`.
//!
//! An `AGENTS.md` is per-folder and **self-contained** — the agents.md
//! nearest-file-wins rule means an agent editing a file reads the closest
//! `AGENTS.md`, so that file must stand alone and explain only from its own
//! level. It must never point at another `AGENTS.md`, in any form — not a link,
//! not a prose "see `../AGENTS.md` / `.github/AGENTS.md`". A `README.md`
//! likewise never links to any `AGENTS.md`. Either file may still reference any
//! other markdown or source file (a design doc, a spec) as needed.
//!
//! This gate scans every `README.md` and `AGENTS.md` in the repo (skipping
//! fenced code blocks, where the convention doc shows template examples) and
//! fails on:
//!
//! - any markdown link — inline `[t](path)` or reference `[id]: path` — whose
//!   target is an `AGENTS.md` (in any `README.md` or `AGENTS.md`); and
//! - any path-reference to another `AGENTS.md` (a `…/AGENTS.md` mention) inside
//!   an `AGENTS.md`, except the convention doc `crates/AGENTS.md` that defines
//!   this very rule.
//!
//! ```sh
//! cargo xtask lint agent-links
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::docs::repo_root;
use crate::report::{self, FormatArgs};

const TARGET_BASENAME: &str = "AGENTS.md";
const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules"];
/// The convention doc that defines this rule references `AGENTS.md` files (and
/// shows templates in code fences); exempt it from the path-mention check.
const CONVENTION_DOC: &str = "crates/AGENTS.md";

#[derive(Args, Debug)]
pub(crate) struct LintAgentLinksArgs {
    #[command(flatten)]
    output: FormatArgs,
}

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; the lint report is its output"
)]
fn emit(line: &str) {
    if report::human_output() {
        println!("{line}");
    }
}

pub(crate) fn enforce() -> Result<()> {
    run(LintAgentLinksArgs {
        output: FormatArgs::default(),
    })
}

pub(crate) fn run(args: LintAgentLinksArgs) -> Result<()> {
    report::run_gate(
        args.output.resolved(),
        "agent-links",
        "AGENTS.md",
        "remove links or cross-references to AGENTS.md files",
        "cargo xtask lint agent-links",
        run_inner,
    )
}

fn run_inner() -> Result<()> {
    let root = repo_root()?;
    let mut files: Vec<PathBuf> = Vec::new();
    collect(&root, &mut files)?;
    files.sort();
    let mut problems: Vec<String> = Vec::new();
    for f in &files {
        check_file(&root, f, &mut problems)?;
    }
    if problems.is_empty() {
        emit(&format!(
            "agent-links gate OK — {} file(s) scanned",
            files.len()
        ));
        return Ok(());
    }
    problems.sort();
    bail!(
        "{} agent-links violation(s) — a README.md must not link to an AGENTS.md, and an AGENTS.md must not link to or mention another AGENTS.md:\n  {}",
        problems.len(),
        problems.join("\n  ")
    )
}

/// Recursively collect every `README.md` and `AGENTS.md`, skipping build/VCS
/// dirs.
fn collect(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in crate::fs_util::read_dir_sorted(root)? {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if SKIP_DIRS.contains(&name) {
                continue;
            }
            collect(&path, out)?;
            continue;
        }
        if name == "README.md" || name == "AGENTS.md" {
            out.push(path);
        }
    }
    Ok(())
}

fn check_file(root: &Path, path: &Path, problems: &mut Vec<String>) -> Result<()> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let is_agents = name == "AGENTS.md";
    let is_convention_doc = path == root.join(CONVENTION_DOC);
    // Path-mention check applies only to AGENTS.md files (a README may name a
    // workflow doc that happens to be an AGENTS.md), and never to the
    // convention doc that defines this rule.
    let check_mentions = is_agents && !is_convention_doc;
    let mut in_fence = false;
    for (idx, raw) in text.lines().enumerate() {
        if raw.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let mut line_flagged = false;
        for target in link_targets(raw) {
            let basename = target
                .rsplit('/')
                .next()
                .unwrap_or(&target)
                .split('#')
                .next()
                .unwrap_or(&target)
                .split('?')
                .next()
                .unwrap_or(&target);
            if basename == TARGET_BASENAME {
                problems.push(format!(
                    "{}:{}: links to `{}` (do not link to an AGENTS.md)",
                    display(root, path),
                    idx + 1,
                    target
                ));
                line_flagged = true;
            }
        }
        if check_mentions && !line_flagged && has_agents_path_mention(raw) {
            problems.push(format!(
                "{}:{}: mentions another AGENTS.md — an AGENTS.md is self-contained, do not point at another AGENTS.md (`{}`)",
                display(root, path),
                idx + 1,
                raw.trim()
            ));
        }
    }
    Ok(())
}

/// A path reference to another `AGENTS.md` — `…/AGENTS.md` or `./AGENTS.md`.
/// A bare `AGENTS.md` (self/convention) is allowed.
fn has_agents_path_mention(line: &str) -> bool {
    line.contains("/AGENTS.md") || line.contains("./AGENTS.md")
}

/// Extract markdown link targets from a single line, skipping inline code
/// spans. Handles inline `[label](target)` and reference-definition
/// `[id]: target`.
fn link_targets(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut in_code = false;
    while i < bytes.len() {
        let c = bytes[i];
        if c == '`' {
            in_code = !in_code;
            i += 1;
            continue;
        }
        if in_code {
            i += 1;
            continue;
        }
        if c == ']' && i + 1 < bytes.len() && bytes[i + 1] == '(' {
            let start = i + 2;
            let mut j = start;
            while j < bytes.len() && bytes[j] != ')' {
                j += 1;
            }
            if j < bytes.len() {
                let target: String = bytes[start..j]
                    .iter()
                    .collect::<String>()
                    .trim()
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .to_owned();
                if !target.is_empty() {
                    out.push(target);
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    // Reference definitions at the start of the line: `[id]: target "title"`.
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix('[')
        && let Some(close) = rest.find("]:")
    {
        let after = rest[close + 2..].trim();
        if let Some(token) = after.split_whitespace().next() {
            let target = token.trim_start_matches('<').trim_end_matches('>');
            if !target.is_empty() {
                out.push(target.to_owned());
            }
        }
    }
    out
}

fn display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests;
