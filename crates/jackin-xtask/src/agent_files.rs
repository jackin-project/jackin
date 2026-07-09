//! Per-directory agent-file gate.
//!
//! Every directory that owns contributor rules must carry `AGENTS.md` plus a
//! `CLAUDE.md` symlink pointing at it (the repo convention: every dir with
//! `AGENTS.md` has `CLAUDE.md` beside it). This gate enforces both halves:
//! presence of `AGENTS.md`, and that `CLAUDE.md` is a symlink (not a regular
//! file) whose target is exactly `AGENTS.md`.
//!
//! Checked dirs are the explicit top-level surfaces (repo root, `.github`,
//! `crates`, `docs`, `docker/construct`) plus **every workspace member crate**
//! under `crates/*/` that owns a `Cargo.toml`. The per-crate scan is what makes
//! each crate's `AGENTS.md`/`README.md` discipline enforceable rather than
//! aspirational: a new crate with a missing or malformed `CLAUDE.md` fails CI.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::docs::repo_root;

const AGENT_FILE_DIRS: &[&str] = &[".", ".github", "crates", "docs", "docker/construct"];

#[derive(Args, Debug)]
pub(crate) struct LintAgentFilesArgs {}

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; the lint report is its output"
)]
fn emit(line: &str) {
    println!("{line}");
}

pub(crate) fn enforce() -> Result<()> {
    run(LintAgentFilesArgs {})
}

pub(crate) fn run(_args: LintAgentFilesArgs) -> Result<()> {
    let root = repo_root()?;
    let mut dirs: Vec<String> = AGENT_FILE_DIRS.iter().map(|&s| s.to_owned()).collect();
    dirs.extend(crate_member_dirs(&root)?);
    let dir_refs: Vec<&str> = dirs.iter().map(String::as_str).collect();
    check(&root, &dir_refs)
}

/// Repo-relative paths of every workspace member crate under `crates/` that
/// owns a `Cargo.toml`. Each is required to carry `AGENTS.md` + a `CLAUDE.md`
/// symlink, so per-crate rules are enforced, not just the shared
/// `crates/AGENTS.md`.
fn crate_member_dirs(root: &Path) -> Result<Vec<String>> {
    let crates_root = root.join("crates");
    let entries =
        fs::read_dir(&crates_root).with_context(|| format!("reading {}", crates_root.display()))?;
    let mut dirs = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() && path.join("Cargo.toml").is_file() {
            let rel = path.strip_prefix(root).map_or_else(
                |_| path.to_string_lossy().into_owned(),
                |p| p.to_string_lossy().replace('\\', "/"),
            );
            dirs.push(rel);
        }
    }
    dirs.sort();
    Ok(dirs)
}

fn check(root: &Path, dirs: &[&str]) -> Result<()> {
    let mut problems = Vec::new();
    for dir in dirs {
        let base = root.join(dir);
        let agents = base.join("AGENTS.md");
        let claude = base.join("CLAUDE.md");
        if !agents.is_file() {
            problems.push(format!("{}: missing AGENTS.md", display(root, &agents)));
            continue;
        }
        match fs::symlink_metadata(&claude) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                check_symlink_target(root, &claude, &mut problems)?;
            }
            Ok(_) => problems.push(format!("{}: not a symlink", display(root, &claude))),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                problems.push(format!("{}: missing CLAUDE.md", display(root, &claude)));
            }
            Err(err) => return Err(err).with_context(|| format!("reading {}", claude.display())),
        }
    }

    if problems.is_empty() {
        emit(&format!(
            "agent-file symlink gate OK - {} CLAUDE.md symlink(s) checked",
            dirs.len()
        ));
        return Ok(());
    }

    bail!(
        "{} agent-file symlink violation(s):\n  {}",
        problems.len(),
        problems.join("\n  ")
    )
}

fn check_symlink_target(root: &Path, claude: &Path, problems: &mut Vec<String>) -> Result<()> {
    let target = fs::read_link(claude).with_context(|| format!("reading {}", claude.display()))?;
    if target == Path::new("AGENTS.md") {
        return Ok(());
    }
    problems.push(format!(
        "{}: symlink target is `{}`, expected `AGENTS.md`",
        display(root, claude),
        target.display()
    ));
    Ok(())
}

fn display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests;
