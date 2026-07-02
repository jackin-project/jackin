//! File-size ratchet gate (Workstream B of codebase-health-enforcement).
//!
//! ```sh
//! cargo xtask lint files             # enforce, fail on violation
//! cargo xtask lint files --print-budget  # emit a fresh budget TOML to stdout
//! ```
//!
//! Two rules, both enforced from the budget file `file-size-budget.toml`:
//!
//!   1. Every production `crates/**/*.rs` (excluding `tests.rs`) must be at
//!      most `production_cap` lines. Today that is 2000L.
//!   2. Every `tests.rs` must be at most `test_cap` lines. Today that is
//!      10000L because the launch and daemon behavioural tests are large by
//!      design — see `roadmap/test-infra-behavioral-specs/` for the long-term
//!      fix.
//!
//! Files currently over their cap are grandfathered in the budget file with
//! their **current** line counts; the recorded count is the ratchet. The ratchet
//! is **shrink-only**: the gate fails if a listed file grows past its recorded
//! count, fails if any non-listed file exceeds the cap, and also fails on a
//! stale row — a budgeted file that no longer exists, has dropped to or under
//! its cap, or whose recorded count is higher than the current count. When a
//! file drops, shrink its recorded count to the new measurement or delete the
//! row entirely once the file is under its cap.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::{Deserialize, Serialize};

use crate::docs::repo_root;

const BUDGET_PATH: &str = "file-size-budget.toml";
const PRODUCTION_GLOB: &str = "crates";
const TEST_FILE_NAME: &str = "tests.rs";

#[derive(Args, Debug)]
pub(crate) struct LintFilesArgs {
    /// Emit the current per-file line counts as a fresh budget TOML on stdout
    /// and exit. Use this after a decomposition to refresh the budget: redirect
    /// the output over `file-size-budget.toml`, prune entries whose counts now
    /// sit under the cap, and commit the result.
    #[arg(long)]
    print_budget: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Budget {
    production_cap: usize,
    test_cap: usize,
    #[serde(default)]
    production: Vec<BudgetEntry>,
    #[serde(default)]
    test: Vec<BudgetEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct BudgetEntry {
    path: String,
    lines: usize,
}

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; the lint report is its output"
)]
fn emit(line: &str) {
    println!("{line}");
}

/// Run the file-size gate in enforce mode (no budget print). The umbrella
/// `cargo xtask lint` entry point uses this.
pub(crate) fn enforce() -> Result<()> {
    run(LintFilesArgs {
        print_budget: false,
    })
}

pub(crate) fn run(args: LintFilesArgs) -> Result<()> {
    let root = repo_root()?;
    let budget_path = root.join(BUDGET_PATH);
    let budget = read_budget(&budget_path)?;

    let counts = measure(&root)?;
    if args.print_budget {
        print_budget(&root, &counts, &budget);
        return Ok(());
    }
    check(&root, &budget, &counts)
}

/// Walk `crates/` and return every `.rs` file mapped to its line count.
/// Test files (basename `tests.rs`) and production files are returned together
/// so the budget-print path can label them consistently.
fn measure(root: &Path) -> Result<BTreeMap<PathBuf, usize>> {
    let crates_dir = root.join(PRODUCTION_GLOB);
    if !crates_dir.is_dir() {
        bail!("`{PRODUCTION_GLOB}/` not found under {}", root.display());
    }
    let mut out = BTreeMap::new();
    walk(&crates_dir, &mut out)?;
    Ok(out)
}

fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, usize>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            walk(&path, out)?;
            continue;
        }
        if path.extension().is_some_and(|ext| ext == "rs") {
            let text =
                fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
            // count physical lines (matches `wc -l` semantics, matches the
            // numbers maintainers see in editors and the budget file).
            let lines = text.lines().count();
            out.insert(path, lines);
        }
    }
    Ok(())
}

fn read_budget(path: &Path) -> Result<Budget> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("reading budget file {}", path.display()))?;
    let budget: Budget =
        toml::from_str(&text).with_context(|| format!("parsing budget file {}", path.display()))?;
    Ok(budget)
}

/// Enforce the budget. Returns `Err` listing every violation.
fn check(root: &Path, budget: &Budget, counts: &BTreeMap<PathBuf, usize>) -> Result<()> {
    let mut problems: Vec<String> = Vec::new();

    let prod_allowlist: BTreeMap<&str, usize> = budget
        .production
        .iter()
        .map(|e| (e.path.as_str(), e.lines))
        .collect();
    let test_allowlist: BTreeMap<&str, usize> = budget
        .test
        .iter()
        .map(|e| (e.path.as_str(), e.lines))
        .collect();

    // Repo-relative measured counts so budget rows (also repo-relative) and
    // measured files line up by the same key.
    let rel_counts: BTreeMap<String, usize> = counts
        .iter()
        .map(|(path, lines)| (relative(root, path), *lines))
        .collect();

    // Shrink-only ratchet: every budgeted row must still point at a real
    // over-cap file whose measured count exactly equals the recorded
    // high-water-mark. A row for a file that no longer exists, a file that
    // dropped to or under the cap, or a recorded count higher than the current
    // count is stale and must be deleted or shrunk — the gate rejects it
    // instead of silently accepting it.
    for (rel, budgeted) in &prod_allowlist {
        check_budget_entry(
            &mut problems,
            rel,
            *budgeted,
            budget.production_cap,
            &rel_counts,
        );
    }
    for (rel, budgeted) in &test_allowlist {
        check_budget_entry(&mut problems, rel, *budgeted, budget.test_cap, &rel_counts);
    }

    for (path, lines) in counts {
        let rel = relative(root, path);
        let is_test = path.file_name().is_some_and(|n| n == TEST_FILE_NAME);
        let (cap, allowlist) = if is_test {
            (budget.test_cap, &test_allowlist)
        } else {
            (budget.production_cap, &prod_allowlist)
        };
        if let Some(&budgeted) = allowlist.get(rel.as_str()) {
            // Growth past the recorded high-water-mark is still a hard failure;
            // the ratchet only ever shrinks. Steady-state and shrink cases are
            // handled by `check_budget_entry`.
            if *lines > budgeted {
                problems.push(format!(
                    "{rel}: grew from {budgeted} to {lines} lines (ratchet exceeded — refactor the file below {budgeted}, or shrink it under the {cap}-line cap)"
                ));
            }
        } else if *lines > cap {
            problems.push(format!(
                "{rel}: {lines} lines exceeds {cap}-line cap (refactor before merging, or add a budget row at {lines})"
            ));
        }
    }

    if problems.is_empty() {
        emit(&format!(
            "file-size budget OK — {} files measured, production cap = {}, test cap = {}",
            counts.len(),
            budget.production_cap,
            budget.test_cap,
        ));
        return Ok(());
    }
    problems.sort();
    bail!(
        "{} file-size violation(s):\n  {}",
        problems.len(),
        problems.join("\n  ")
    )
}

/// Reject a stale budget row: missing file, file now at/under the cap, or a
/// recorded count higher than the current measured count. A row exactly at the
/// measured count while still over the cap is the legitimate steady state.
fn check_budget_entry(
    problems: &mut Vec<String>,
    rel: &str,
    budgeted: usize,
    cap: usize,
    rel_counts: &BTreeMap<String, usize>,
) {
    let Some(&measured) = rel_counts.get(rel) else {
        problems.push(format!(
            "{rel}: budgeted at {budgeted} lines but the file no longer exists (delete the stale budget row)"
        ));
        return;
    };
    if measured <= cap {
        problems.push(format!(
            "{rel}: budgeted at {budgeted} lines but now {measured} lines, at or under the {cap}-line cap (delete the stale budget row — it no longer needs grandfathering)"
        ));
    } else if measured < budgeted {
        problems.push(format!(
            "{rel}: recorded at {budgeted} lines but now {measured} lines (shrink the budget row to {measured}, or refactor the file under the {cap}-line cap)"
        ));
    }
    // measured == budgeted (> cap): steady state, no problem.
    // measured > budgeted: growth, flagged by the counts loop.
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).map_or_else(
        |_| path.to_string_lossy().into_owned(),
        |p| p.to_string_lossy().into_owned(),
    )
}

/// Print a fresh budget TOML listing every file currently over its cap,
/// grouped production vs test. Files under their cap are not emitted. The
/// output is meant to be redirected over `file-size-budget.toml` after
/// pruning entries that should no longer be grandfathered.
#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; --print-budget writes the new budget file to stdout"
)]
fn print_budget(root: &Path, counts: &BTreeMap<PathBuf, usize>, budget: &Budget) {
    print!("{}", budget_report(root, counts, budget));
}

fn budget_report(root: &Path, counts: &BTreeMap<PathBuf, usize>, budget: &Budget) -> String {
    let mut prod: Vec<(&Path, usize)> = Vec::new();
    let mut test: Vec<(&Path, usize)> = Vec::new();
    for (path, lines) in counts {
        if path.file_name().is_some_and(|n| n == TEST_FILE_NAME) {
            if *lines > budget.test_cap {
                test.push((path.as_path(), *lines));
            }
        } else if *lines > budget.production_cap {
            prod.push((path.as_path(), *lines));
        }
    }
    prod.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    test.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));

    let mut out = String::new();
    out.push_str("# Regenerated by `cargo xtask lint files --print-budget`.\n");
    out.push_str(
        "# Numbers may only ever decrease; delete entries when a file drops under the cap.\n",
    );
    out.push_str(&format!("production_cap = {}\n", budget.production_cap));
    out.push_str(&format!("test_cap = {}\n\n", budget.test_cap));
    for (path, lines) in prod {
        out.push_str("[[production]]\n");
        out.push_str(&format!("path = \"{}\"\n", relative_for_print(root, path)));
        out.push_str(&format!("lines = {lines}\n\n"));
    }
    for (path, lines) in test {
        out.push_str("[[test]]\n");
        out.push_str(&format!("path = \"{}\"\n", relative_for_print(root, path)));
        out.push_str(&format!("lines = {lines}\n\n"));
    }
    out
}

fn relative_for_print(root: &Path, path: &Path) -> String {
    // Always print paths relative to the repo root with forward slashes so the
    // output is portable across platforms and matches the committed file.
    relative(root, path).replace('\\', "/")
}

#[cfg(test)]
mod tests;
