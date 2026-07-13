//! Suppression reason-gate + per-crate bare-allow / per-lint expect ratchet.
//!
//! ```sh
//! cargo xtask lint suppressions             # enforce, fail on violation
//! cargo xtask lint suppressions --print-budget  # emit fresh ratchet family entries
//! ```
//!
//! Production enforcement is a thin shim over [`crate::ratchet`] for the
//! `bare-allow-per-crate` and `expect-per-lint-crate` families in `ratchet.toml`.
//! Measurement (`measure`) stays here for the ratchet providers. Pure
//! `Budget`/`check` helpers below exist only for unit characterization tests.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Args;
#[cfg(test)]
use serde::{Deserialize, Serialize};

use crate::health::{crate_name_from_path, parse_suppression_attrs, walk_rs_paths};
use crate::ratchet::{self, SUPPRESSION_FAMILIES};

const CRATES_GLOB: &str = "crates";
const RERUN: &str = "cargo xtask lint suppressions";

#[derive(Args, Debug)]
pub(crate) struct LintSuppressionsArgs {
    /// Emit regenerated `ratchet.toml` entries for the suppression families on
    /// stdout. Prefer `cargo xtask lint ratchet --print <family>` for one family.
    #[arg(long)]
    print_budget: bool,
}

/// Test-only budget shape (characterization fixtures parse this TOML themselves).
#[cfg(test)]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct Budget {
    #[serde(default, rename = "crate")]
    crates: Vec<CrateBudget>,
    #[serde(default, rename = "expect")]
    expects: Vec<ExpectBudget>,
}

#[cfg(test)]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct CrateBudget {
    name: String,
    bare_allow: usize,
}

#[cfg(test)]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct ExpectBudget {
    lint: String,
    #[serde(rename = "crate")]
    crate_name: String,
    count: usize,
}

/// Measured suppression inventory used by the gate and `--print-budget`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Measured {
    /// Bare `allow` attribute counts keyed by crate directory name.
    pub bare_by_crate: BTreeMap<String, usize>,
    /// `expect` counts keyed by `(lint, crate)`.
    pub expect_by_lint_crate: BTreeMap<(String, String), usize>,
}

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; the gate report is its output"
)]
fn emit(line: &str) {
    println!("{line}");
}

pub(crate) fn enforce() -> Result<()> {
    run(LintSuppressionsArgs {
        print_budget: false,
    })
}

pub(crate) fn run(args: LintSuppressionsArgs) -> Result<()> {
    if args.print_budget {
        return ratchet::print_families(SUPPRESSION_FAMILIES);
    }

    let outcome = ratchet::check_families_at_root(SUPPRESSION_FAMILIES)?;
    if outcome.problems.is_empty() {
        let root = crate::docs::repo_root()?;
        let measured = measure(&root)?;
        let bare_total: usize = measured.bare_by_crate.values().sum();
        let expect_total: usize = measured.expect_by_lint_crate.values().sum();
        emit(&format!(
            "suppression gate OK — {bare_total} bare allow(s) across {} crate(s), {expect_total} expect(s) across {} (lint,crate) pair(s) (ratchet.toml)",
            measured.bare_by_crate.len(),
            measured.expect_by_lint_crate.len()
        ));
        return Ok(());
    }

    let mut problems: Vec<&str> = outcome
        .problems
        .iter()
        .map(|p| p.message.as_str())
        .collect();
    problems.sort_unstable();
    bail!(
        "{} suppression-budget violation(s):\n  {}\n\nFix the listed rows, then re-run `{RERUN}`. To refresh the budget after an intentional shrink, run `{RERUN} --print-budget` (or `cargo xtask lint ratchet --print bare-allow-per-crate` / `expect-per-lint-crate`).",
        problems.len(),
        problems.join("\n  ")
    )
}

pub(crate) fn measure(root: &Path) -> Result<Measured> {
    let crates_dir = root.join(CRATES_GLOB);
    if !crates_dir.is_dir() {
        bail!("`{CRATES_GLOB}/` not found under {}", root.display());
    }
    let mut measured = Measured::default();
    for path in walk_rs_paths(&crates_dir)? {
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let crate_name = crate_name_from_path(root, &path);
        for (is_allow, lints, has_reason) in parse_suppression_attrs(&text) {
            if is_allow {
                if !has_reason {
                    *measured
                        .bare_by_crate
                        .entry(crate_name.clone())
                        .or_default() += 1;
                }
            } else {
                for lint in lints {
                    *measured
                        .expect_by_lint_crate
                        .entry((lint, crate_name.clone()))
                        .or_default() += 1;
                }
            }
        }
    }
    Ok(measured)
}

// --- Pure helpers kept for unit characterization tests only ---

#[cfg(test)]
fn check(budget: &Budget, measured: &Measured) -> Result<()> {
    let mut problems: Vec<String> = Vec::new();

    let mut budgeted_crates: BTreeMap<&str, usize> = BTreeMap::new();
    for row in &budget.crates {
        budgeted_crates.insert(row.name.as_str(), row.bare_allow);
    }
    for (name, &budgeted) in &budgeted_crates {
        let measured_n = measured.bare_by_crate.get(*name).copied().unwrap_or(0);
        if measured_n > budgeted {
            problems.push(format!(
                "crate {name}: bare_allow grew from {budgeted} to {measured_n} (delta +{}) — convert bare `#[allow]` to reasoned `#[expect(..., reason = \"…\")]` or shrink debt; regenerate: {RERUN} --print-budget",
                measured_n - budgeted
            ));
        } else if measured_n == 0 {
            problems.push(format!(
                "crate {name}: budgeted bare_allow={budgeted} but now 0 — delete the stale `[[crate]]` row from ratchet.toml; regenerate: {RERUN} --print-budget"
            ));
        } else if measured_n < budgeted {
            problems.push(format!(
                "crate {name}: bare_allow shrunk from {budgeted} to {measured_n} — update ratchet.toml to {measured_n} (shrink-only ratchet); regenerate: {RERUN} --print-budget"
            ));
        }
    }
    for (name, &measured_n) in &measured.bare_by_crate {
        if measured_n > 0 && !budgeted_crates.contains_key(name.as_str()) {
            problems.push(format!(
                "crate {name}: {measured_n} bare `#[allow]` not in ratchet.toml — add a shrink-only row or convert to reasoned expects; regenerate: {RERUN} --print-budget"
            ));
        }
    }

    let mut budgeted_expects: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for row in &budget.expects {
        budgeted_expects.insert((row.lint.as_str(), row.crate_name.as_str()), row.count);
    }
    for (&(lint, crate_name), &budgeted) in &budgeted_expects {
        let key = (lint.to_owned(), crate_name.to_owned());
        let measured_n = measured
            .expect_by_lint_crate
            .get(&key)
            .copied()
            .unwrap_or(0);
        if measured_n > budgeted {
            problems.push(format!(
                "expect {lint} in {crate_name}: grew from {budgeted} to {measured_n} (delta +{}) — remove the new `#[expect]` or justify and raise only via intentional budget update; regenerate: {RERUN} --print-budget",
                measured_n - budgeted
            ));
        } else if measured_n == 0 {
            problems.push(format!(
                "expect {lint} in {crate_name}: budgeted count={budgeted} but now 0 — delete the stale `[[expect]]` row from ratchet.toml; regenerate: {RERUN} --print-budget"
            ));
        } else if measured_n < budgeted {
            problems.push(format!(
                "expect {lint} in {crate_name}: shrunk from {budgeted} to {measured_n} — update ratchet.toml to {measured_n} (shrink-only ratchet); regenerate: {RERUN} --print-budget"
            ));
        }
    }
    for ((lint, crate_name), &measured_n) in &measured.expect_by_lint_crate {
        if measured_n > 0 && !budgeted_expects.contains_key(&(lint.as_str(), crate_name.as_str())) {
            problems.push(format!(
                "expect {lint} in {crate_name}: {measured_n} unbudgeted `#[expect]` — add a shrink-only row or remove the suppression; regenerate: {RERUN} --print-budget"
            ));
        }
    }

    if problems.is_empty() {
        let bare_total: usize = measured.bare_by_crate.values().sum();
        let expect_total: usize = measured.expect_by_lint_crate.values().sum();
        emit(&format!(
            "suppression gate OK — {bare_total} bare allow(s) across {} crate(s), {expect_total} expect(s) across {} (lint,crate) pair(s) (shrink-only)",
            measured.bare_by_crate.len(),
            measured.expect_by_lint_crate.len()
        ));
        return Ok(());
    }

    problems.sort_unstable();
    bail!(
        "{} suppression-budget violation(s):\n  {}\n\nFix the listed rows, then re-run `{RERUN}`. To refresh the budget after an intentional shrink, run `{RERUN} --print-budget`.",
        problems.len(),
        problems.join("\n  ")
    )
}

/// Serialize measured counts as a legacy-shaped budget TOML body (unit tests).
#[cfg(test)]
pub(crate) fn print_budget(measured: &Measured) -> String {
    let mut out = String::from(
        "# Suppression shrink-only budget (plan 011 reason-gate).\n\
         # Generated by `cargo xtask lint suppressions --print-budget`.\n\
         # [[crate]] rows: bare `#[allow]` / `#![allow]` counts (no reason =).\n\
         # [[expect]] rows: `#[expect]` / `#![expect]` counts per (lint, crate).\n\
         # The gate fails on growth, under-budget shrink without row update, or zero rows still listed.\n\n",
    );
    for (name, &bare_allow) in &measured.bare_by_crate {
        if bare_allow == 0 {
            continue;
        }
        out.push_str("[[crate]]\n");
        out.push_str(&format!("name = {name:?}\n"));
        out.push_str(&format!("bare_allow = {bare_allow}\n\n"));
    }
    for ((lint, crate_name), &count) in &measured.expect_by_lint_crate {
        if count == 0 {
            continue;
        }
        out.push_str("[[expect]]\n");
        out.push_str(&format!("lint = {lint:?}\n"));
        out.push_str(&format!("crate = {crate_name:?}\n"));
        out.push_str(&format!("count = {count}\n\n"));
    }
    out
}

#[cfg(test)]
mod tests;
