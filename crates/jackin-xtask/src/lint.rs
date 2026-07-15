//! File-size ratchet gate.
//!
//! ```sh
//! cargo xtask lint files             # enforce, fail on violation
//! cargo xtask lint files --print-budget  # emit fresh ratchet family entries
//! ```
//!
//! Production enforcement is a thin shim over [`crate::ratchet`] for the
//! `file-size-production` and `file-size-test` families in `ratchet.toml`.
//! Measurement (`measure_lines`) stays here so the ratchet providers can call it.
//! Pure `Budget`/`check` helpers below exist only for unit characterization tests.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Args;
#[cfg(test)]
use serde::{Deserialize, Serialize};

use crate::docs::repo_root;
use crate::ratchet::{self, FILE_SIZE_FAMILIES};
use crate::report::{Format, Report, Violation};

const PRODUCTION_GLOB: &str = "crates";
#[cfg(test)]
const TEST_FILE_NAME: &str = "tests.rs";
const RERUN: &str = "cargo xtask lint files";

#[derive(Args, Debug)]
pub(crate) struct LintFilesArgs {
    /// Emit regenerated `ratchet.toml` entries for the file-size families on
    /// stdout (`file-size-production` then `file-size-test`). Prefer
    /// `cargo xtask lint ratchet --print <family>` for a single family.
    #[arg(long)]
    print_budget: bool,
    /// Output format (`human`, `json`, `github`). Defaults to human; under
    /// GitHub Actions selects `github` unless overridden.
    #[arg(long, value_enum)]
    format: Option<Format>,
}

/// Test-only budget shape (characterization fixtures write this TOML themselves).
#[cfg(test)]
#[derive(Debug, Clone, Deserialize, Serialize)]
struct Budget {
    production_cap: usize,
    test_cap: usize,
    #[serde(default)]
    production: Vec<BudgetEntry>,
    #[serde(default)]
    test: Vec<BudgetEntry>,
}

#[cfg(test)]
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
        format: None,
    })
}

pub(crate) fn run(args: LintFilesArgs) -> Result<()> {
    if args.print_budget {
        return ratchet::print_families(FILE_SIZE_FAMILIES);
    }

    let format = Format::detect(args.format);
    let outcome = ratchet::check_families_at_root(FILE_SIZE_FAMILIES)?;
    if outcome.problems.is_empty() {
        if matches!(format, Format::Human) {
            let root = repo_root()?;
            let counts = measure_lines(&root)?;
            let prod_cap = ratchet::family_cap("file-size-production")?;
            let test_cap = ratchet::family_cap("file-size-test")?;
            emit(&format!(
                "file-size budget OK — {} files measured, production cap = {}, test cap = {} (ratchet.toml)",
                counts.len(),
                prod_cap,
                test_cap,
            ));
        } else {
            Report::new("file-size", Vec::new()).emit(format)?;
        }
        return Ok(());
    }

    let violations: Vec<Violation> = outcome
        .problems
        .into_iter()
        .map(|p| Violation {
            rule: "file-size",
            file: p.key,
            line: None,
            message: p.message.clone(),
            fix: format!(
                "update `ratchet.toml` family `{}` (or refactor the source); regenerate: cargo xtask lint ratchet --print {}",
                p.family, p.family
            ),
            rerun: RERUN.into(),
        })
        .collect();
    Report::new("file-size", violations).emit(format)
}

/// Walk `crates/` and return every `.rs` file mapped to its line count.
/// Test files (basename `tests.rs`) and production files are returned together
/// so the budget-print path can label them consistently.
pub(crate) fn measure_lines(root: &Path) -> Result<BTreeMap<PathBuf, usize>> {
    let crates_dir = root.join(PRODUCTION_GLOB);
    if !crates_dir.is_dir() {
        bail!("`{PRODUCTION_GLOB}/` not found under {}", root.display());
    }
    let mut out = BTreeMap::new();
    walk(&crates_dir, &mut out)?;
    Ok(out)
}

fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, usize>) -> Result<()> {
    for entry in crate::fs_util::read_dir_sorted(dir)? {
        let path = entry.path();
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

// --- Pure helpers kept for unit characterization tests only ---

#[cfg(test)]
fn read_budget(path: &Path) -> Result<Budget> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("reading budget file {}", path.display()))?;
    let budget: Budget =
        toml::from_str(&text).with_context(|| format!("parsing budget file {}", path.display()))?;
    Ok(budget)
}

/// Enforce the budget. Returns `Err` listing every violation (test helper).
#[cfg(test)]
fn check(root: &Path, budget: &Budget, counts: &BTreeMap<PathBuf, usize>) -> Result<()> {
    let violations = collect_violations(root, budget, counts);
    if violations.is_empty() {
        emit(&format!(
            "file-size budget OK — {} files measured, production cap = {}, test cap = {}",
            counts.len(),
            budget.production_cap,
            budget.test_cap,
        ));
        return Ok(());
    }
    let mut problems: Vec<String> = violations.into_iter().map(|v| v.message).collect();
    problems.sort_unstable();
    bail!(
        "{} file-size violation(s):\n  {}",
        problems.len(),
        problems.join("\n  ")
    )
}

/// Pure violation builder for the file-size ratchet (testable without I/O emit).
#[cfg(test)]
fn collect_violations(
    root: &Path,
    budget: &Budget,
    counts: &BTreeMap<PathBuf, usize>,
) -> Vec<Violation> {
    let mut violations: Vec<Violation> = Vec::new();

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

    let rel_counts: BTreeMap<String, usize> = counts
        .iter()
        .map(|(path, lines)| (relative(root, path), *lines))
        .collect();

    for (rel, budgeted) in &prod_allowlist {
        push_budget_entry(
            &mut violations,
            rel,
            *budgeted,
            budget.production_cap,
            &rel_counts,
        );
    }
    for (rel, budgeted) in &test_allowlist {
        push_budget_entry(
            &mut violations,
            rel,
            *budgeted,
            budget.test_cap,
            &rel_counts,
        );
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
            if *lines > budgeted {
                let why = format!(
                    "{rel}: grew from {budgeted} to {lines} lines (ratchet exceeded — refactor the file below {budgeted}, or shrink it under the {cap}-line cap)"
                );
                violations.push(Violation {
                    rule: "file-size",
                    file: rel,
                    line: None,
                    message: why.clone(),
                    fix: format!(
                        "refactor `{why}` below {budgeted} lines, or under the {cap}-line cap and delete the budget row"
                    ),
                    rerun: RERUN.into(),
                });
            }
        } else if *lines > cap {
            let why = format!(
                "{rel}: {lines} lines exceeds {cap}-line cap (refactor before merging, or add a budget row at {lines})"
            );
            violations.push(Violation {
                rule: "file-size",
                file: rel.clone(),
                line: None,
                message: why.clone(),
                fix: format!(
                    "split `{rel}` under the {cap}-line cap, or add a budget row at {lines} in ratchet.toml"
                ),
                rerun: RERUN.into(),
            });
        }
    }

    violations.sort_by(|a, b| a.file.cmp(&b.file).then(a.message.cmp(&b.message)));
    violations
}

#[cfg(test)]
fn push_budget_entry(
    violations: &mut Vec<Violation>,
    rel: &str,
    budgeted: usize,
    cap: usize,
    rel_counts: &BTreeMap<String, usize>,
) {
    let Some(&measured) = rel_counts.get(rel) else {
        let why = format!(
            "{rel}: budgeted at {budgeted} lines but the file no longer exists (delete the stale budget row)"
        );
        violations.push(Violation {
            rule: "file-size",
            file: rel.to_owned(),
            line: None,
            message: why,
            fix: format!("delete the stale `{rel}` row from ratchet.toml"),
            rerun: RERUN.into(),
        });
        return;
    };
    if measured <= cap {
        let why = format!(
            "{rel}: budgeted at {budgeted} lines but now {measured} lines, at or under the {cap}-line cap (delete the stale budget row — it no longer needs grandfathering)"
        );
        violations.push(Violation {
            rule: "file-size",
            file: rel.to_owned(),
            line: None,
            message: why,
            fix: format!("delete the `{rel}` row from ratchet.toml (file is under the cap)"),
            rerun: RERUN.into(),
        });
    } else if measured < budgeted {
        let why = format!(
            "{rel}: recorded at {budgeted} lines but now {measured} lines (shrink the budget row to {measured}, or refactor the file under the {cap}-line cap)"
        );
        violations.push(Violation {
            rule: "file-size",
            file: rel.to_owned(),
            line: None,
            message: why,
            fix: format!("set `{rel}` budget lines = {measured} in ratchet.toml"),
            rerun: RERUN.into(),
        });
    }
}

#[cfg(test)]
fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).map_or_else(
        |_| path.to_string_lossy().into_owned(),
        |p| p.to_string_lossy().into_owned(),
    )
}

/// Pure budget-report helper (unit tests; production `--print-budget` uses ratchet).
#[cfg(test)]
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
        out.push_str(&format!(
            "path = \"{}\"\n",
            relative(root, path).replace('\\', "/")
        ));
        out.push_str(&format!("lines = {lines}\n\n"));
    }
    for (path, lines) in test {
        out.push_str("[[test]]\n");
        out.push_str(&format!(
            "path = \"{}\"\n",
            relative(root, path).replace('\\', "/")
        ));
        out.push_str(&format!("lines = {lines}\n\n"));
    }
    out
}

#[cfg(test)]
mod tests;
