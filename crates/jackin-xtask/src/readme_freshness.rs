//! README-freshness gate: structural `src/` layout changes must touch the crate README.
//!
//! Trigger: git name-status A/D/R* on `crates/<x>/src/**/*.rs` (content-only M does not
//! fire). Clear by any status on `crates/<x>/README.md` in the same range.
//!
//! ```sh
//! cargo xtask lint readme-freshness --base origin/main
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::docs::repo_root;

const RERUN: &str = "cargo xtask lint readme-freshness --base origin/main";

#[derive(Args, Debug)]
pub(crate) struct LintReadmeFreshnessArgs {
    /// Diff base ref (merge-base with HEAD). Defaults to `origin/main`.
    #[arg(long, default_value = "origin/main")]
    base: String,
}

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; the lint report is its output"
)]
fn emit(line: &str) {
    println!("{line}");
}

pub(crate) fn run(args: LintReadmeFreshnessArgs) -> Result<()> {
    let root = repo_root()?;
    let entries = git_name_status(&root, &args.base)?;
    let report = evaluate(&entries, &[]);
    if report.violations.is_empty() {
        emit(&format!(
            "readme-freshness OK - {} structural crate(s) checked against {}",
            report.structural_crates.len(),
            args.base
        ));
        return Ok(());
    }
    let mut lines = vec![format!(
        "{} crate(s) changed `src/` module layout without updating README.md:",
        report.violations.len()
    )];
    for v in &report.violations {
        lines.push(format!("  crates/{}/", v.crate_name));
        lines.push(
            "    rule: crates/AGENTS.md — structural src layout change (add/rename/delete .rs under src/) requires a same-PR README update".to_owned(),
        );
        for path in v.trigger_paths.iter().take(5) {
            lines.push(format!("    trigger: {path}"));
        }
        if v.trigger_paths.len() > 5 {
            lines.push(format!("    … +{} more path(s)", v.trigger_paths.len() - 5));
        }
        lines.push(format!(
            "    fix: update crates/{}/README.md — structure table and/or public API section — in this PR",
            v.crate_name
        ));
        lines.push(format!("    rerun: {RERUN}"));
    }
    bail!("{}", lines.join("\n"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NameStatusEntry {
    status: String,
    path: String,
    /// For renames, the destination path (R status).
    new_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Violation {
    crate_name: String,
    trigger_paths: Vec<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct FreshnessReport {
    structural_crates: BTreeSet<String>,
    readme_touched: BTreeSet<String>,
    violations: Vec<Violation>,
}

/// Pure bucketing over a parsed `git diff --name-status` list.
fn evaluate(entries: &[NameStatusEntry], allowlist: &[&str]) -> FreshnessReport {
    let allow: BTreeSet<&str> = allowlist.iter().copied().collect();
    let mut structural: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut readme_touched: BTreeSet<String> = BTreeSet::new();

    for entry in entries {
        let paths: Vec<&str> = match entry.new_path.as_deref() {
            Some(new_path) => vec![entry.path.as_str(), new_path],
            None => vec![entry.path.as_str()],
        };
        for path in paths {
            if let Some(crate_name) = crate_from_readme_path(path) {
                readme_touched.insert(crate_name.to_owned());
            }
            if is_structural_status(&entry.status)
                && let Some(crate_name) = crate_from_src_rs_path(path)
            {
                structural
                    .entry(crate_name.to_owned())
                    .or_default()
                    .push(format!("{} {path}", entry.status));
            }
        }
    }

    let mut violations = Vec::new();
    for (crate_name, triggers) in &structural {
        if allow.contains(crate_name.as_str()) {
            continue;
        }
        if readme_touched.contains(crate_name) {
            continue;
        }
        violations.push(Violation {
            crate_name: crate_name.clone(),
            trigger_paths: triggers.clone(),
        });
    }

    FreshnessReport {
        structural_crates: structural.keys().cloned().collect(),
        readme_touched,
        violations,
    }
}

fn is_structural_status(status: &str) -> bool {
    let code = status.chars().next().unwrap_or(' ');
    matches!(code, 'A' | 'D' | 'R' | 'C')
}

fn crate_from_src_rs_path(path: &str) -> Option<&str> {
    // crates/<name>/src/**/*.rs
    let rest = path.strip_prefix("crates/")?;
    let (crate_name, after) = rest.split_once('/')?;
    if after.starts_with("src/")
        && Path::new(after)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"))
    {
        Some(crate_name)
    } else {
        None
    }
}

fn crate_from_readme_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("crates/")?;
    let (crate_name, after) = rest.split_once('/')?;
    if after == "README.md" {
        Some(crate_name)
    } else {
        None
    }
}

fn git_name_status(root: &Path, base: &str) -> Result<Vec<NameStatusEntry>> {
    let merge_base = {
        let mut cmd = Command::new("git");
        cmd.args(["merge-base", base, "HEAD"]).current_dir(root);
        let out = crate::cmd::output_string(&mut cmd)
            .with_context(|| format!("git merge-base {base} HEAD"))?;
        out.trim().to_owned()
    };
    let mut cmd = Command::new("git");
    // -M enables rename detection explicitly (plan note).
    cmd.args(["diff", "--name-status", "-M", &merge_base, "HEAD"])
        .current_dir(root);
    let raw = crate::cmd::output_string(&mut cmd).context("git diff --name-status")?;
    Ok(parse_name_status(&raw))
}

fn parse_name_status(raw: &str) -> Vec<NameStatusEntry> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let Some(status) = parts.next() else {
            continue;
        };
        let Some(path) = parts.next() else {
            continue;
        };
        let new_path = parts.next().map(str::to_owned);
        out.push(NameStatusEntry {
            status: status.to_owned(),
            path: path.to_owned(),
            new_path,
        });
    }
    out
}

#[cfg(test)]
mod tests;
