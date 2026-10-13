//! Root agent-file gate.
//!
//! The repo carries a single consolidated `AGENTS.md` at the root plus a
//! `CLAUDE.md` symlink pointing at it. This gate enforces both halves:
//! presence of `AGENTS.md`, and that `CLAUDE.md` is a symlink (not a regular
//! file) whose target is exactly `AGENTS.md`.
//!
//! Only the repo root is checked. Per-directory and per-crate `AGENTS.md`
//! files were removed in #956 (consolidated into root `AGENTS.md`).

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use clap::Args;

use crate::docs::repo_root;
use crate::report::{Format, Report, Violation};

const AGENT_FILE_DIRS: &[&str] = &["."];
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
    let format = Format::detect(args.format);
    let violations = collect_violations(&root, AGENT_FILE_DIRS)?;
    if violations.is_empty() && matches!(format, Format::Human) {
        emit(&format!(
            "agent-file symlink gate OK - {} CLAUDE.md symlink(s) checked",
            AGENT_FILE_DIRS.len()
        ));
        return Ok(());
    }
    Report::new("agents", violations).emit(format)
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
    let problems: Vec<String> = violations.into_iter().map(|v| v.message).collect();
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
                message: format!("{file}: missing AGENTS.md"),
                fix: format!("create `{file}` with repo contributor rules (see AGENTS.md)"),
                rerun: RERUN.into(),
            });
            continue;
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
                    message: format!("{file}: not a symlink"),
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
                    message: format!("{file}: missing CLAUDE.md"),
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
        message: format!(
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
