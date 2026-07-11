//! Shrink-only gate: production `"/jackin` string literals.
//!
//! New container-side paths must go through `jackin_core::container_paths`.
//! Residual literals (Dockerfile template bodies, the chokepoint module
//! itself) are ledgered in `container-path-allowlist.toml` and may only shrink.
//!
//! ```sh
//! cargo xtask lint container-paths
//! cargo xtask lint container-paths --print-allowlist
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::Deserialize;

use crate::docs::repo_root;

const ALLOWLIST_PATH: &str = "container-path-allowlist.toml";
const RERUN: &str = "cargo xtask lint container-paths";

#[derive(Args, Debug)]
pub(crate) struct LintContainerPathsArgs {
    /// Print a regenerated allowlist TOML for the current residual set
    /// (does not write the file).
    #[arg(long)]
    print_allowlist: bool,
}

#[derive(Debug, Deserialize)]
struct Allowlist {
    #[serde(default)]
    file: Vec<AllowlistFile>,
}

#[derive(Debug, Deserialize)]
struct AllowlistFile {
    path: String,
    literals: usize,
    /// Human-readable residual rationale; not read by the gate.
    #[serde(default)]
    #[expect(dead_code, reason = "documentational allowlist field")]
    note: String,
}

#[expect(
    clippy::print_stdout,
    reason = "jackin-xtask is a CLI; the gate report is its output"
)]
fn emit(line: &str) {
    println!("{line}");
}

pub(crate) fn enforce() -> Result<()> {
    run(LintContainerPathsArgs {
        print_allowlist: false,
    })
}

pub(crate) fn run(args: LintContainerPathsArgs) -> Result<()> {
    let root = repo_root()?;
    let measured = measure_literals(&root)?;

    if args.print_allowlist {
        emit("# Regenerated residual `/jackin` literal ledger. Review, then write to");
        emit(&format!(
            "# {ALLOWLIST_PATH}. Shrink-only — never grow a row."
        ));
        emit("");
        for (path, n) in &measured {
            emit("[[file]]");
            emit(&format!("path = {path:?}"));
            emit(&format!("literals = {n}"));
            emit("note = \"\"");
            emit("");
        }
        return Ok(());
    }

    let allowlist = read_allowlist(&root)?;
    let mut recorded: BTreeMap<String, usize> = BTreeMap::new();
    for row in &allowlist.file {
        recorded.insert(row.path.clone(), row.literals);
    }

    let mut problems = Vec::new();

    // Stale rows: allowlisted path no longer has residual literals (or missing).
    for (path, budgeted) in &recorded {
        match measured.get(path) {
            None => problems.push(format!(
                "{path}: listed in {ALLOWLIST_PATH} with {budgeted} literals but no longer has any (remove the stale allowlist row)"
            )),
            Some(&n) if n < *budgeted => problems.push(format!(
                "{path}: shrunk from {budgeted} to {n} literals — update {ALLOWLIST_PATH} to {n} (shrink-only ratchet)"
            )),
            Some(&n) if n > *budgeted => problems.push(format!(
                "{path}: grew from {budgeted} to {n} `/jackin` literals — route new container paths through jackin_core::container_paths; regenerate: cargo xtask lint container-paths --print-allowlist"
            )),
            Some(_) => {}
        }
    }

    // New residual files not on the allowlist.
    for (path, n) in &measured {
        if !recorded.contains_key(path) {
            problems.push(format!(
                "{path}: {n} unallowlisted `/jackin` production literal(s) — route through jackin_core::container_paths; if residual is required (e.g. Dockerfile template), add a shrink-only row via: cargo xtask lint container-paths --print-allowlist"
            ));
        }
    }

    if problems.is_empty() {
        let total: usize = measured.values().sum();
        emit(&format!(
            "container-path gate OK — {} residual `/jackin` literal(s) across {} file(s) (shrink-only)",
            total,
            measured.len()
        ));
        return Ok(());
    }

    problems.sort();
    bail!(
        "{} container-path violation(s):\n  {}\n\nfix: route new container paths through jackin_core::container_paths; regenerate: cargo xtask lint container-paths --print-allowlist\nre-run: {RERUN}",
        problems.len(),
        problems.join("\n  ")
    )
}

fn measure_literals(root: &Path) -> Result<BTreeMap<String, usize>> {
    let crates_dir = root.join("crates");
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for entry in walkdir_rs_files(&crates_dir)? {
        let rel = entry
            .strip_prefix(root)
            .unwrap_or(&entry)
            .to_string_lossy()
            .replace('\\', "/");
        // Skip the gate crate itself (tooling, not a path emitter).
        if rel.contains("jackin-xtask/") {
            continue;
        }
        // Production sources only: under crates/*/src, not tests.rs / tests/.
        if !rel.contains("/src/") {
            continue;
        }
        let file_name = entry.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if file_name == "tests.rs" || rel.contains("/tests/") || rel.contains("/tests.rs/") {
            continue;
        }
        // Also skip paths whose parent is a `tests` directory component.
        if entry.components().any(|c| c.as_os_str() == "tests") {
            continue;
        }
        let text =
            fs::read_to_string(&entry).with_context(|| format!("reading {}", entry.display()))?;
        let n = text.matches("\"/jackin").count();
        if n > 0 {
            counts.insert(rel, n);
        }
    }
    Ok(counts)
}

fn walkdir_rs_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                out.push(path);
            }
        }
        Ok(())
    }
    walk(dir, &mut out)?;
    out.sort();
    Ok(out)
}

fn read_allowlist(root: &Path) -> Result<Allowlist> {
    let path = root.join(ALLOWLIST_PATH);
    if !path.exists() {
        bail!(
            "missing {ALLOWLIST_PATH} — create it with: cargo xtask lint container-paths --print-allowlist"
        );
    }
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

#[cfg(test)]
#[path = "container_paths_gate/tests.rs"]
mod tests;
