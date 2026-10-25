use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::health::{crate_name_from_path, parse_suppression_attrs, walk_rs_paths};
use crate::ratchet::{self, SUPPRESSION_FAMILIES};
use crate::report::{self, FormatArgs};

const CRATES_GLOB: &str = "crates";
const RERUN: &str = "cargo xtask lint suppressions";

#[derive(Args, Debug)]
pub(crate) struct LintSuppressionsArgs {
    #[command(flatten)]
    output: FormatArgs,
    #[arg(long)]
    print_budget: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Measured {
    pub bare_by_crate: BTreeMap<String, usize>,
    pub expect_by_lint_crate: BTreeMap<(String, String), usize>,
}

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; the gate report is its output"
)]
fn emit(line: &str) {
    if report::human_output() {
        println!("{line}");
    }
}

pub(crate) fn run(args: LintSuppressionsArgs) -> Result<()> {
    let format = args.output.resolved();
    report::run_gate(
        format,
        "suppressions",
        "crates/",
        "add a reason or shrink the matching suppression budget row",
        RERUN,
        || run_inner(args),
    )
}

fn run_inner(args: LintSuppressionsArgs) -> Result<()> {
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
