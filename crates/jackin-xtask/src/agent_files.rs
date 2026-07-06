use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::docs::repo_root;

const AGENT_FILE_DIRS: &[&str] = &[
    ".",
    ".github",
    "crates",
    "docs",
    "docker/construct",
    "crates/jackin-tui-lookbook",
];

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
    check(&root, AGENT_FILE_DIRS)
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
