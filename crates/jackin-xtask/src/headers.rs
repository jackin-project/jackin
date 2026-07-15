//! Ownership-header gate for every workspace `lib.rs` / binary `main.rs`.
//!
//! Contract (first 15 `//!` lines): owns line (`//! <crate>: …`),
//! `**Architecture Invariant:** T<n>` matching `arch::TIERS`, and
//! `Entry point: …`.
//!
//! ```sh
//! cargo xtask lint headers
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::arch::TIERS;
use crate::docs::repo_root;
use crate::report::{self, FormatArgs};

const RERUN: &str = "cargo xtask lint headers";

#[derive(Args, Debug)]
pub(crate) struct LintHeadersArgs {
    #[command(flatten)]
    output: FormatArgs,
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

pub(crate) fn enforce() -> Result<()> {
    run(LintHeadersArgs {
        output: FormatArgs::default(),
    })
}

pub(crate) fn run(args: LintHeadersArgs) -> Result<()> {
    report::run_gate(
        args.output.resolved(),
        "headers",
        "crates/",
        "restore the ownership, architecture invariant, and entry-point header fields",
        RERUN,
        run_inner,
    )
}

fn run_inner() -> Result<()> {
    let root = repo_root()?;
    let tier_map: std::collections::BTreeMap<&str, u8> = TIERS.iter().copied().collect();
    let roots = collect_roots(&root)?;
    let mut problems = Vec::new();

    for (crate_name, path) in &roots {
        // Workspace-excluded crates (e.g. jackin-lints) are not in TIERS.
        if !tier_map.contains_key(crate_name.as_str()) {
            continue;
        }
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let text = fs::read_to_string(path).with_context(|| format!("reading {rel}"))?;
        let header = leading_doc_lines(&text);
        let check = check_header(crate_name, &header, &tier_map);
        for p in check {
            problems.push(format!("{rel}: {p}"));
        }
    }

    if problems.is_empty() {
        emit(&format!(
            "headers gate OK — {} files checked, tiers consistent with arch gate",
            roots.len()
        ));
        return Ok(());
    }
    problems.sort();
    bail!(
        "{} ownership-header violation(s):\n  {}\n\nfix: leading `//!` block must include `//! <crate>: …`, `**Architecture Invariant:** T<n>` matching crates/jackin-xtask/src/arch.rs TIERS, and `Entry point: …`\nre-run: {RERUN}",
        problems.len(),
        problems.join("\n  ")
    )
}

fn collect_roots(root: &Path) -> Result<Vec<(String, PathBuf)>> {
    let crates = root.join("crates");
    let mut out = Vec::new();
    for entry in fs::read_dir(&crates).with_context(|| format!("reading {}", crates.display()))? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let dir = entry.path();
        let name = dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_owned();
        if name.is_empty() {
            continue;
        }
        let lib = dir.join("src/lib.rs");
        let main = dir.join("src/main.rs");
        if lib.is_file() {
            out.push((name.clone(), lib));
        }
        if main.is_file() {
            out.push((name, main));
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(out)
}

fn leading_doc_lines(text: &str) -> Vec<String> {
    let mut docs = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("//!") {
            docs.push(rest.trim().to_owned());
            if docs.len() >= 15 {
                break;
            }
        } else if line.trim().is_empty() {
            // blank before docs: skip; blank after docs started: keep scanning
            // only if more `//!` follow later.
        } else {
            break;
        }
    }
    docs
}

/// Pure contract check. Returns problem strings (empty = ok).
pub(crate) fn check_header(
    crate_name: &str,
    header: &[String],
    tiers: &std::collections::BTreeMap<&str, u8>,
) -> Vec<String> {
    let mut problems = Vec::new();
    if header.is_empty() {
        problems.push(format!(
            "missing `//!` ownership header — add `//! {crate_name}: <owns>`, `//! **Architecture Invariant:** T<n>`, and `//! Entry point: …`"
        ));
        return problems;
    }
    let first = &header[0];
    if !first.starts_with(&format!("{crate_name}:")) {
        problems.push(format!(
            "first doc line must be `//! {crate_name}: <one sentence owns>` (got {first:?})"
        ));
    }
    let mut found_tier: Option<u8> = None;
    for line in header {
        if let Some(rest) = line.split("**Architecture Invariant:**").nth(1)
            && let Some(cap) = rest.trim().strip_prefix('T')
        {
            let digits: String = cap.chars().take_while(char::is_ascii_digit).collect();
            if let Ok(n) = digits.parse::<u8>() {
                found_tier = Some(n);
            }
        }
    }
    match (found_tier, tiers.get(crate_name).copied()) {
        (None, _) => problems.push(format!(
            "missing `//! **Architecture Invariant:** T<n> …` line (expected T{} from arch::TIERS)",
            tiers
                .get(crate_name).map_or_else(|| "?".into(), ToString::to_string)
        )),
        (Some(got), Some(want)) if got != want => problems.push(format!(
            "header tier T{got} does not match arch::TIERS T{want} — update the header (or re-tier in arch.rs with justification)"
        )),
        (Some(_), None) => problems.push(format!(
            "{crate_name}: no tier in arch::TIERS — add it to crates/jackin-xtask/src/arch.rs"
        )),
        _ => {}
    }
    if !header.iter().any(|l| l.contains("Entry point:")) {
        problems.push(
            "missing `//! Entry point: [`item`] — <why>` line naming the one type/fn to copy"
                .into(),
        );
    }
    problems
}

#[cfg(test)]
mod tests;
