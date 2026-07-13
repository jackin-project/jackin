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

use anyhow::{Context, Result};
use clap::Args;

use crate::docs::repo_root;
use crate::report::{Format, Report, Violation};

const AGENT_FILE_DIRS: &[&str] = &[".", ".github", "crates", "docs", "docker/construct"];
const RERUN: &str = "cargo xtask lint agents";

#[derive(Args, Debug)]
pub(crate) struct LintAgentFilesArgs {
    /// Output format (`human`, `json`, `github`). Defaults to human; under
    /// GitHub Actions selects `github` unless overridden.
    #[arg(long, value_enum)]
    format: Option<Format>,
}

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; the lint report is its output"
)]
fn emit(line: &str) {
    println!("{line}");
}

pub(crate) fn enforce() -> Result<()> {
    run(LintAgentFilesArgs { format: None })
}

pub(crate) fn run(args: LintAgentFilesArgs) -> Result<()> {
    let root = repo_root()?;
    let mut dirs: Vec<String> = AGENT_FILE_DIRS.iter().map(|&s| s.to_owned()).collect();
    dirs.extend(crate_member_dirs(&root)?);
    let dir_refs: Vec<&str> = dirs.iter().map(String::as_str).collect();
    let format = Format::detect(args.format);
    let violations = collect_violations(&root, &dir_refs)?;
    if violations.is_empty() && matches!(format, Format::Human) {
        emit(&format!(
            "agent-file symlink gate OK - {} CLAUDE.md symlink(s) checked",
            dirs.len()
        ));
        return Ok(());
    }
    Report::new("agents", violations).emit(format)
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

#[cfg(test)]
fn check(root: &Path, dirs: &[&str]) -> Result<()> {
    use anyhow::bail;
    let violations = collect_violations(root, dirs)?;
    if violations.is_empty() {
        emit(&format!(
            "agent-file symlink gate OK - {} CLAUDE.md symlink(s) checked",
            dirs.len()
        ));
        return Ok(());
    }
    let problems: Vec<String> = violations.into_iter().map(|v| v.why).collect();
    bail!(
        "{} agent-file symlink violation(s):\n  {}",
        problems.len(),
        problems.join("\n  ")
    )
}

fn collect_violations(root: &Path, dirs: &[&str]) -> Result<Vec<Violation>> {
    let mut violations = Vec::new();
    for dir in dirs {
        let base = root.join(dir);
        let agents = base.join("AGENTS.md");
        let claude = base.join("CLAUDE.md");
        if !agents.is_file() {
            let file = display(root, &agents);
            violations.push(Violation {
                rule: "agents",
                file: file.clone(),
                line: None,
                why: format!("{file}: missing AGENTS.md"),
                fix: format!(
                    "create `{file}` with crate/dir contributor rules (see crates/AGENTS.md)"
                ),
                rerun: RERUN.into(),
            });
            continue;
        }
        // Per-crate members must also carry README.md (crates/AGENTS.md hard
        // rule). Top-level agent dirs (repo root, docs, …) are not crate members.
        if dir.starts_with("crates/") && *dir != "crates" {
            let readme = base.join("README.md");
            if !readme.is_file() {
                let file = display(root, &readme);
                violations.push(Violation {
                    rule: "agents",
                    file: file.clone(),
                    line: None,
                    why: format!(
                        "{file}: missing README.md (crates/AGENTS.md hard rule: every crate carries README.md + AGENTS.md + CLAUDE.md)"
                    ),
                    fix: format!(
                        "add `{file}` describing purpose, tier, structure, and how to verify"
                    ),
                    rerun: RERUN.into(),
                });
            }
        }
        match fs::symlink_metadata(&claude) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                check_symlink_target(root, &claude, &mut violations)?;
            }
            Ok(_) => {
                let file = display(root, &claude);
                violations.push(Violation {
                    rule: "agents",
                    file: file.clone(),
                    line: None,
                    why: format!("{file}: not a symlink"),
                    fix: format!("rm `{file}` && ln -s AGENTS.md `{file}`"),
                    rerun: RERUN.into(),
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                let file = display(root, &claude);
                violations.push(Violation {
                    rule: "agents",
                    file: file.clone(),
                    line: None,
                    why: format!("{file}: missing CLAUDE.md"),
                    fix: format!("ln -s AGENTS.md `{file}`"),
                    rerun: RERUN.into(),
                });
            }
            Err(err) => return Err(err).with_context(|| format!("reading {}", claude.display())),
        }
    }
    Ok(violations)
}

fn check_symlink_target(root: &Path, claude: &Path, violations: &mut Vec<Violation>) -> Result<()> {
    let target = fs::read_link(claude).with_context(|| format!("reading {}", claude.display()))?;
    if target == Path::new("AGENTS.md") {
        return Ok(());
    }
    let file = display(root, claude);
    violations.push(Violation {
        rule: "agents",
        file: file.clone(),
        line: None,
        why: format!(
            "{file}: symlink target is `{}`, expected `AGENTS.md`",
            target.display()
        ),
        fix: format!("rm `{file}` && ln -s AGENTS.md `{file}`"),
        rerun: RERUN.into(),
    });
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
